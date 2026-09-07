#!/usr/bin/env python3
"""Build deterministic, non-personal replay bundles from explicit fixture media.

The builder is deliberately separate from prepare.ps1. It converts reviewed,
generated recorder outputs into complete QueueBack bundles; prepare.ps1 then
performs the immutable copy, full decode, and benchmark-manifest binding.
"""

from __future__ import annotations

import argparse
from decimal import Decimal, InvalidOperation
from fractions import Fraction
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path
from typing import Any, Mapping, Sequence


SCHEMA_VERSION = 2
SENTINEL_NAME = ".chronobreak-replay-time"
MAX_SPEC_BYTES = 1024 * 1024
MAX_SUMMARY_PROBE_BYTES = 1024 * 1024
MAX_TOOL_DIAGNOSTIC_BYTES = 1024 * 1024
MAX_FRAME_SCAN_BYTES = 512 * 1024 * 1024
MAX_FIXTURES = 64
MAX_I64 = (1 << 63) - 1
MIN_I64 = -(1 << 63)
MAX_U64 = (1 << 64) - 1
IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
GAME_ID_RE = re.compile(r"^\d+(?:-[1-9]\d{0,2})?$")
UNSIGNED_DECIMAL_RE = re.compile(r"^(?:0|[1-9]\d*)$")
TRANSFORMS = {"copy", "remux_audio", "repeat", "normalize", "hevc"}
PRODUCER_BACKENDS = {
    transform: f"replay-corpus-{transform.replace('_', '-')}"
    for transform in TRANSFORMS
}
LEGACY_TIME_FIELDS = frozenset(
    {
        "duration_ms",
        "game_start_video_offset_ms",
        "game_time_ms",
        "recording_fps",
        "video_offset_ms",
        "video_time_ms",
    }
)
REPLAY_TICKS_PER_SECOND = 48_000_000
GAME_TICKS_PER_SECOND = 1_000_000
REPLAY_TICKS_PER_GAME_TICK = REPLAY_TICKS_PER_SECOND // GAME_TICKS_PER_SECOND
MAX_REPLAY_DURATION_SECONDS = 24 * 60 * 60
MAX_REPLAY_TICK = REPLAY_TICKS_PER_SECOND * MAX_REPLAY_DURATION_SECONDS
FILE_ATTRIBUTE_REPARSE_POINT = 0x400
CREATE_NO_WINDOW = 0x08000000

_OUTPUT_DRAIN_SECONDS = 5.0


if os.name == "nt":
    import _winapi
    import ctypes
    import msvcrt
    from ctypes import wintypes

    _CREATE_SUSPENDED = 0x00000004
    _JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000
    _JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
    _WAIT_OBJECT_0 = 0
    _WAIT_TIMEOUT = 258

    class _JobObjectBasicLimitInformation(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]

    class _IoCounters(ctypes.Structure):
        _fields_ = [
            ("ReadOperationCount", ctypes.c_ulonglong),
            ("WriteOperationCount", ctypes.c_ulonglong),
            ("OtherOperationCount", ctypes.c_ulonglong),
            ("ReadTransferCount", ctypes.c_ulonglong),
            ("WriteTransferCount", ctypes.c_ulonglong),
            ("OtherTransferCount", ctypes.c_ulonglong),
        ]

    class _JobObjectExtendedLimitInformation(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", _JobObjectBasicLimitInformation),
            ("IoInfo", _IoCounters),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]

    _kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _create_job_object = _kernel32.CreateJobObjectW
    _create_job_object.argtypes = (wintypes.LPVOID, wintypes.LPCWSTR)
    _create_job_object.restype = wintypes.HANDLE
    _set_job_information = _kernel32.SetInformationJobObject
    _set_job_information.argtypes = (
        wintypes.HANDLE,
        ctypes.c_int,
        wintypes.LPVOID,
        wintypes.DWORD,
    )
    _set_job_information.restype = wintypes.BOOL
    _assign_process_to_job = _kernel32.AssignProcessToJobObject
    _assign_process_to_job.argtypes = (wintypes.HANDLE, wintypes.HANDLE)
    _assign_process_to_job.restype = wintypes.BOOL
    _terminate_job = _kernel32.TerminateJobObject
    _terminate_job.argtypes = (wintypes.HANDLE, wintypes.UINT)
    _terminate_job.restype = wintypes.BOOL
    _terminate_process = _kernel32.TerminateProcess
    _terminate_process.argtypes = (wintypes.HANDLE, wintypes.UINT)
    _terminate_process.restype = wintypes.BOOL
    _resume_thread = _kernel32.ResumeThread
    _resume_thread.argtypes = (wintypes.HANDLE,)
    _resume_thread.restype = wintypes.DWORD
    _close_handle = _kernel32.CloseHandle
    _close_handle.argtypes = (wintypes.HANDLE,)
    _close_handle.restype = wintypes.BOOL

    def _last_windows_error(action: str) -> OSError:
        error = ctypes.get_last_error()
        return OSError(error, f"{action}: {ctypes.FormatError(error).strip()}")

    class _WindowsJobProcess:
        """Suspended child assigned to a kill-on-close job before it can run."""

        def __init__(self, arguments: Sequence[str]) -> None:
            self.args = list(arguments)
            self.stdout: Any = None
            self._job = 0
            self._process = 0
            self._tree_lock = threading.Lock()
            read_fd = write_fd = null_fd = -1
            thread_handle = 0
            assigned = False
            try:
                self._job = _create_job_object(None, None)
                if not self._job:
                    raise _last_windows_error("CreateJobObjectW failed")
                limits = _JobObjectExtendedLimitInformation()
                limits.BasicLimitInformation.LimitFlags = (
                    _JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                )
                if not _set_job_information(
                    self._job,
                    _JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    ctypes.byref(limits),
                    ctypes.sizeof(limits),
                ):
                    raise _last_windows_error("SetInformationJobObject failed")

                read_fd, write_fd = os.pipe()
                null_fd = os.open(os.devnull, os.O_RDONLY)
                os.set_inheritable(read_fd, False)
                os.set_inheritable(write_fd, True)
                os.set_inheritable(null_fd, True)
                write_handle = msvcrt.get_osfhandle(write_fd)
                null_handle = msvcrt.get_osfhandle(null_fd)
                startup = subprocess.STARTUPINFO()
                startup.dwFlags |= subprocess.STARTF_USESTDHANDLES
                startup.hStdInput = null_handle
                startup.hStdOutput = write_handle
                startup.hStdError = write_handle
                startup.lpAttributeList = {
                    "handle_list": [null_handle, write_handle],
                }
                (
                    self._process,
                    thread_handle,
                    _process_id,
                    _thread_id,
                ) = _winapi.CreateProcess(
                    None,
                    subprocess.list2cmdline(self.args),
                    None,
                    None,
                    True,
                    CREATE_NO_WINDOW | _CREATE_SUSPENDED,
                    None,
                    None,
                    startup,
                )
                if not _assign_process_to_job(self._job, self._process):
                    raise _last_windows_error("AssignProcessToJobObject failed")
                assigned = True
                if _resume_thread(thread_handle) == 0xFFFFFFFF:
                    raise _last_windows_error("ResumeThread failed")
                self.stdout = os.fdopen(read_fd, "rb", buffering=0)
                read_fd = -1
            except Exception:
                if self._process:
                    if assigned:
                        self.terminate_tree()
                    else:
                        _terminate_process(self._process, 0xEEEE0003)
                self.close()
                raise
            finally:
                if thread_handle:
                    _close_handle(thread_handle)
                for descriptor in (read_fd, write_fd, null_fd):
                    if descriptor >= 0:
                        os.close(descriptor)

        def wait(self, timeout: float) -> int:
            milliseconds = min(
                0xFFFFFFFE,
                max(0, int(timeout * 1000 + 0.999)),
            )
            result = _winapi.WaitForSingleObject(self._process, milliseconds)
            if result == _WAIT_TIMEOUT:
                raise subprocess.TimeoutExpired(self.args, timeout)
            if result != _WAIT_OBJECT_0:
                raise OSError(f"WaitForSingleObject returned {result}")
            return ctypes.c_int32(_winapi.GetExitCodeProcess(self._process)).value

        def terminate_tree(self) -> None:
            with self._tree_lock:
                job = self._job
                self._job = 0
                if job:
                    try:
                        _terminate_job(job, 0xEEEE0001)
                    finally:
                        _close_handle(job)

        def close_tree(self) -> None:
            with self._tree_lock:
                job = self._job
                self._job = 0
                if job:
                    _close_handle(job)

        def close(self) -> None:
            self.close_tree()
            if self._process:
                _close_handle(self._process)
                self._process = 0


class CorpusError(ValueError):
    """Raised when corpus input, output, or media evidence is unsafe."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise CorpusError(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _read_json(path: Path, label: str) -> dict[str, Any]:
    if not path.is_file():
        raise CorpusError(f"{label} is missing: {path}")
    if path.stat().st_size > MAX_SPEC_BYTES:
        raise CorpusError(f"{label} exceeds the {MAX_SPEC_BYTES}-byte safety limit")

    def reject_constant(value: str) -> None:
        raise CorpusError(f"{label} contains non-finite number {value}")

    try:
        value = json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=_reject_duplicate_keys,
            parse_float=Decimal,
            parse_constant=reject_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, InvalidOperation) as error:
        raise CorpusError(f"{label} is not valid UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise CorpusError(f"{label} must contain an object")
    return value


def _only_fields(value: Mapping[str, Any], allowed: set[str], label: str) -> None:
    unexpected = sorted(set(value) - allowed)
    if unexpected:
        raise CorpusError(f"{label} contains unsupported fields: {', '.join(unexpected)}")


def _required(value: Mapping[str, Any], names: Sequence[str], label: str) -> None:
    missing = [name for name in names if name not in value]
    if missing:
        raise CorpusError(f"{label} is missing required fields: {', '.join(missing)}")


def _identifier(value: Any, label: str) -> str:
    if not isinstance(value, str) or IDENTIFIER_RE.fullmatch(value) is None:
        raise CorpusError(f"{label} must be a safe 1-80 character identifier")
    return value


def _producer_backend(transform: Any, label: str = "transform") -> str:
    if not isinstance(transform, str) or transform not in PRODUCER_BACKENDS:
        raise CorpusError(f"{label} is unsupported")
    return PRODUCER_BACKENDS[transform]


def _media_id(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise CorpusError(f"{label} must be a lowercase hyphenated non-nil UUID")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError) as error:
        raise CorpusError(
            f"{label} must be a lowercase hyphenated non-nil UUID"
        ) from error
    if parsed.int == 0 or str(parsed) != value:
        raise CorpusError(f"{label} must be a lowercase hyphenated non-nil UUID")
    return value


def _positive_rational(value: Any, label: str) -> Fraction:
    if not isinstance(value, dict):
        raise CorpusError(f"{label} must be a rational object")
    _only_fields(value, {"numerator", "denominator"}, label)
    _required(value, ("numerator", "denominator"), label)
    numerator = value["numerator"]
    denominator = value["denominator"]
    if (
        not isinstance(numerator, str)
        or UNSIGNED_DECIMAL_RE.fullmatch(numerator) is None
        or not isinstance(denominator, str)
        or UNSIGNED_DECIMAL_RE.fullmatch(denominator) is None
    ):
        raise CorpusError(f"{label} must use canonical unsigned decimal strings")
    if denominator == "0":
        raise CorpusError(f"{label} denominator must be positive")
    if len(numerator) > 19 or len(denominator) > 20:
        raise CorpusError(f"{label} is outside the schema-v2 rational range")
    numerator_value = int(numerator)
    denominator_value = int(denominator)
    if numerator_value > MAX_I64 or denominator_value > MAX_U64:
        raise CorpusError(f"{label} is outside the schema-v2 rational range")
    result = Fraction(numerator_value, denominator_value)
    if result <= 0:
        raise CorpusError(f"{label} must be positive")
    if str(result.numerator) != numerator or str(result.denominator) != denominator:
        raise CorpusError(f"{label} must be normalized")
    return result


def _positive_decimal(
    value: Any, label: str, *, minimum: int, maximum: int
) -> Fraction:
    if isinstance(value, bool) or not isinstance(value, (int, float, Decimal)):
        raise CorpusError(f"{label} must be numeric")
    try:
        result = Fraction(str(value))
    except (ValueError, ZeroDivisionError) as error:
        raise CorpusError(f"{label} must be a finite decimal number") from error
    if not Fraction(minimum, 1) <= result <= Fraction(maximum, 1):
        raise CorpusError(f"{label} must be between {minimum} and {maximum}")
    return result


def _seconds_to_game_ticks(value: Any, label: str, *, maximum: int) -> int:
    seconds = _positive_decimal(value, label, minimum=1, maximum=maximum)
    ticks = seconds * GAME_TICKS_PER_SECOND
    if ticks.denominator != 1:
        raise CorpusError(f"{label} must address an exact integer microsecond")
    return ticks.numerator


def _absolute_path(value: Any, label: str) -> Path:
    if not isinstance(value, str) or not value or "\x00" in value:
        raise CorpusError(f"{label} must be a nonempty absolute path")
    path = Path(value)
    if not path.is_absolute():
        raise CorpusError(f"{label} must be absolute")
    if any(part in {".", ".."} for part in path.parts):
        raise CorpusError(f"{label} must not contain dot segments")
    return path


def _has_reparse_attribute(path: Path) -> bool:
    attributes = getattr(path.lstat(), "st_file_attributes", 0)
    return bool(attributes & FILE_ATTRIBUTE_REPARSE_POINT) or path.is_symlink()


def _assert_reparse_free_existing(path: Path, label: str) -> Path:
    if not path.exists():
        raise CorpusError(f"{label} is missing: {path}")
    resolved = path.resolve(strict=True)
    cursor = path
    while True:
        if _has_reparse_attribute(cursor):
            raise CorpusError(f"{label} traverses a reparse point: {cursor}")
        if cursor.parent == cursor:
            break
        cursor = cursor.parent
    return resolved


def _assert_nearest_ancestor_reparse_free(path: Path, label: str) -> None:
    cursor = path
    while not cursor.exists():
        if cursor.parent == cursor:
            raise CorpusError(f"{label} has no existing ancestor")
        cursor = cursor.parent
    _assert_reparse_free_existing(cursor, label)


def _is_strictly_below(path: Path, root: Path) -> bool:
    try:
        return os.path.commonpath((str(path), str(root))) == str(root) and path != root
    except ValueError:
        return False


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _write_json_new(path: Path, value: Any) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(value, output, indent=2, ensure_ascii=False)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def _write_text_new(path: Path, value: str) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as output:
        output.write(value)
        output.flush()
        os.fsync(output.fileno())


def _run(
    arguments: Sequence[str],
    label: str,
    timeout_seconds: int,
    *,
    max_output_bytes: int,
) -> str:
    if max_output_bytes <= 0:
        raise CorpusError(f"{label} output limit must be positive")
    chunks: list[bytes] = []
    output_size = 0
    output_exceeded = threading.Event()
    reader_errors: list[OSError] = []
    try:
        if os.name == "nt":
            process = _WindowsJobProcess(arguments)
        else:
            process = subprocess.Popen(
                list(arguments),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
    except OSError as error:
        raise CorpusError(f"{label} could not start: {error}") from error
    assert process.stdout is not None

    def terminate_tree() -> None:
        if os.name == "nt":
            process.terminate_tree()
        else:
            try:
                process.kill()
            except OSError:
                pass

    def read_output() -> None:
        nonlocal output_size
        try:
            while chunk := process.stdout.read(64 * 1024):
                retained = min(len(chunk), max_output_bytes - output_size)
                if retained > 0:
                    chunks.append(chunk[:retained])
                output_size += len(chunk)
                if output_size > max_output_bytes:
                    output_exceeded.set()
                    terminate_tree()
                    return
        except OSError as error:
            reader_errors.append(error)
        finally:
            process.stdout.close()

    reader = threading.Thread(target=read_output, daemon=True)
    reader_started = False
    return_code: int | None = None
    timeout_error: subprocess.TimeoutExpired | None = None
    wait_error: OSError | None = None
    try:
        reader.start()
        reader_started = True
        deadline = time.monotonic() + timeout_seconds
        while return_code is None:
            if output_exceeded.is_set():
                terminate_tree()
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                timeout_error = subprocess.TimeoutExpired(arguments, timeout_seconds)
                terminate_tree()
                break
            try:
                return_code = process.wait(timeout=min(0.1, remaining))
            except subprocess.TimeoutExpired:
                continue
            except OSError as error:
                wait_error = error
                terminate_tree()
                break

        if os.name == "nt":
            process.close_tree()

        cleanup_deadline = time.monotonic() + _OUTPUT_DRAIN_SECONDS
        if return_code is None:
            try:
                process.wait(timeout=max(0.0, cleanup_deadline - time.monotonic()))
            except (OSError, subprocess.TimeoutExpired):
                pass
        reader.join(max(0.0, cleanup_deadline - time.monotonic()))
        drain_incomplete = reader.is_alive()
        if timeout_error is not None:
            raise CorpusError(
                f"{label} exceeded its {timeout_seconds}-second timeout"
            ) from timeout_error
        if wait_error is not None:
            raise CorpusError(f"{label} could not be waited for: {wait_error}")
        if output_exceeded.is_set():
            raise CorpusError(f"{label} output exceeds the {max_output_bytes}-byte limit")
        if drain_incomplete:
            raise CorpusError(
                f"{label} output pipe did not close within "
                f"{_OUTPUT_DRAIN_SECONDS:g} seconds after process termination"
            )
        if reader_errors:
            raise CorpusError(f"{label} output could not be read: {reader_errors[0]}")
        output = b"".join(chunks).decode("utf-8", errors="replace")
        assert return_code is not None
        if return_code != 0:
            detail = output.strip()[-4000:]
            raise CorpusError(f"{label} failed with exit {return_code}: {detail}")
        return output
    finally:
        terminate_tree()
        if not reader_started:
            process.stdout.close()
        if os.name == "nt":
            process.close()


def _probe(ffprobe: Path, media: Path) -> dict[str, Any]:
    raw = _run(
        [
            str(ffprobe),
            "-v",
            "error",
            "-show_entries",
            (
                "format=start_time,duration,size,format_name:"
                "stream=index,codec_type,codec_name,profile,width,height,"
                "avg_frame_rate,time_base,start_pts,duration_ts,has_b_frames,"
                "sample_rate,channels,nb_frames,nb_read_frames,nb_read_packets"
            ),
            "-count_frames",
            "-count_packets",
            "-of",
            "json",
            str(media),
        ],
        f"ffprobe {media.name}",
        300,
        max_output_bytes=MAX_SUMMARY_PROBE_BYTES,
    )
    if len(raw.encode("utf-8")) > MAX_SUMMARY_PROBE_BYTES:
        raise CorpusError(f"ffprobe summary exceeds safety limit for {media}")
    try:
        value = json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
    except json.JSONDecodeError as error:
        raise CorpusError(f"ffprobe returned malformed JSON for {media}") from error
    if not isinstance(value, dict):
        raise CorpusError(f"ffprobe returned a non-object for {media}")
    return value


def _probe_integer(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, (str, int)):
        raise CorpusError(f"{label} is not an integer")
    text = str(value)
    if (
        re.fullmatch(r"-?(?:0|[1-9]\d*)", text) is None
        or text == "-0"
        or len(text.removeprefix("-")) > 20
    ):
        raise CorpusError(f"{label} is not a canonical bounded integer")
    result = int(text)
    if not MIN_I64 <= result <= MAX_U64:
        raise CorpusError(f"{label} is outside the supported integer range")
    return result


def _schema_i64(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise CorpusError(f"{label} must be an integer")
    if not MIN_I64 <= value <= MAX_I64:
        raise CorpusError(f"{label} is outside the schema-v2 signed integer range")
    return value


def _probe_rational(value: Any, label: str, *, positive: bool) -> Fraction:
    if not isinstance(value, str) or value.count("/") != 1:
        raise CorpusError(f"{label} is malformed")
    numerator_text, denominator_text = value.split("/", 1)
    numerator = _probe_integer(numerator_text, f"{label} numerator")
    denominator = _probe_integer(denominator_text, f"{label} denominator")
    if denominator <= 0:
        raise CorpusError(f"{label} denominator must be positive")
    result = Fraction(numerator, denominator)
    if positive and result <= 0:
        raise CorpusError(f"{label} must be positive")
    return result


def _probe_decimal(value: Any, label: str, *, positive: bool) -> Fraction:
    if not isinstance(value, str) or re.fullmatch(
        r"-?(?:0|[1-9]\d*)(?:\.\d+)?", value
    ) is None:
        raise CorpusError(f"{label} is not a canonical decimal")
    unsigned = value.removeprefix("-")
    whole, _, fractional = unsigned.partition(".")
    if len(whole) > 20 or len(fractional) > 9:
        raise CorpusError(f"{label} is outside the bounded probe decimal grammar")
    result = Fraction(value)
    if positive and result <= 0:
        raise CorpusError(f"{label} must be positive")
    return result


def _wire_rational(value: Fraction, label: str = "rational") -> dict[str, str]:
    if (
        not MIN_I64 <= value.numerator <= MAX_I64
        or not 0 < value.denominator <= MAX_U64
    ):
        raise CorpusError(f"{label} is outside the schema-v2 rational range")
    return {
        "numerator": str(value.numerator),
        "denominator": str(value.denominator),
    }


def _target_frame_count(duration_seconds: Fraction, frame_rate: Fraction) -> int:
    count = duration_seconds * frame_rate
    if count.denominator != 1 or count <= 0:
        raise CorpusError(
            "target_duration_seconds must address an exact positive frame boundary"
        )
    if count > MAX_REPLAY_TICK:
        raise CorpusError("target frame count exceeds the schema-v2 frame boundary")
    return count.numerator


def _format_seconds(value: Fraction) -> str:
    if value <= 0:
        raise CorpusError("FFmpeg duration must be positive")
    scale = 1_000_000_000
    scaled = (value.numerator * scale + value.denominator - 1) // value.denominator
    whole, fractional = divmod(scaled, scale)
    if fractional == 0:
        return str(whole)
    return f"{whole}.{fractional:09d}".rstrip("0")


def _scan_video_grid(
    ffprobe: Path,
    media: Path,
    *,
    time_base: Fraction,
    frame_rate: Fraction,
    expected_first_pts: int,
    expected_duration_ts: int,
    expected_frame_count: int,
) -> dict[str, int]:
    raw = _run(
        [
            str(ffprobe),
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pts,duration",
            "-show_frames",
            "-of",
            "json",
            str(media),
        ],
        f"video frame scan {media.name}",
        7_200,
        max_output_bytes=MAX_FRAME_SCAN_BYTES,
    )
    try:
        document = json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
    except json.JSONDecodeError as error:
        raise CorpusError(f"video frame scan returned malformed JSON for {media}") from error
    frames = document.get("frames") if isinstance(document, dict) else None
    if not isinstance(frames, list) or not frames:
        raise CorpusError(f"{media.name} has no decoded video frames")
    presentation: list[tuple[int, int]] = []
    for index, frame in enumerate(frames):
        if not isinstance(frame, dict):
            raise CorpusError(f"{media.name} video frame {index} is malformed")
        presentation.append(
            (
                _probe_integer(frame.get("pts"), f"video frame {index} PTS"),
                _probe_integer(
                    frame.get("duration"), f"video frame {index} duration"
                ),
            )
        )
    frame_step = Fraction(1, 1) / frame_rate / time_base
    if frame_step.denominator != 1 or frame_step <= 0:
        raise CorpusError(
            f"{media.name} time base cannot represent its exact frame boundaries"
        )
    step = frame_step.numerator
    if len(presentation) != expected_frame_count:
        raise CorpusError(
            f"{media.name} has {len(presentation)} decoded video frames; "
            f"expected {expected_frame_count}"
        )
    for index, (pts, duration) in enumerate(presentation):
        expected_pts = expected_first_pts + index * step
        if pts != expected_pts or duration != step:
            raise CorpusError(
                f"{media.name} is off the exact frame grid at decoded frame {index}"
            )
    frame_count = len(presentation)
    duration_ts = frame_count * step
    if duration_ts != expected_duration_ts:
        raise CorpusError(
            f"{media.name} video duration disagrees with its exact frame grid"
        )
    return {
        "frame_count": frame_count,
        "first_pts": expected_first_pts,
        "one_past_last_pts": expected_first_pts + duration_ts,
        "duration_ts": duration_ts,
        "frame_step_pts": step,
    }


def build_game_log(
    media_id: str,
    replay_end_tick: int,
    *,
    event_interval_game_ticks: int,
    snapshot_interval_game_ticks: int,
) -> dict[str, Any]:
    """Return deterministic anonymized schema-v2 data bounded by media coverage."""
    media_id = _media_id(media_id, "game log media_id")
    if (
        isinstance(replay_end_tick, bool)
        or not isinstance(replay_end_tick, int)
        or not 0 < replay_end_tick <= MAX_REPLAY_TICK
    ):
        raise CorpusError("replay_end_tick must be a positive bounded integer")
    for value, label in (
        (event_interval_game_ticks, "event_interval_game_ticks"),
        (snapshot_interval_game_ticks, "snapshot_interval_game_ticks"),
    ):
        if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
            raise CorpusError(f"{label} must be a positive integer")
    duration_game_ticks = replay_end_tick // REPLAY_TICKS_PER_GAME_TICK
    if duration_game_ticks < 2:
        raise CorpusError("media coverage is too short for a game-clock fixture")
    local_player = "QB-Blue-1#TEST"
    players = [
        {
            "summoner_name": f"QB-{team}-{index}#TEST",
            "team": "ORDER" if team == "Blue" else "CHAOS",
            "champion": (
                ["Ahri", "Garen", "LeeSin", "Jinx", "Thresh"]
                if team == "Blue"
                else ["Annie", "Darius", "Vi", "Ashe", "Leona"]
            )[index - 1],
            "gold": None,
            "hp": None,
            "hp_max": None,
            "cs": 0,
            "level": 1,
            "items": [],
            "summoner_spells": ["Flash", "Teleport"],
            "keystone_id": 8214,
        }
        for team in ("Blue", "Red")
        for index in range(1, 6)
    ]
    snapshots: list[dict[str, Any]] = []
    for game_tick in range(
        0, duration_game_ticks, snapshot_interval_game_ticks
    ):
        snapshot_players = []
        for index, player in enumerate(players):
            updated = dict(player)
            updated["cs"] = game_tick * (220 + index * 4) // duration_game_ticks
            updated["level"] = min(
                18, 1 + game_tick * 17 // duration_game_ticks
            )
            item_count = min(6, game_tick * 7 // duration_game_ticks)
            updated["items"] = [
                {
                    "item_id": 1001 + ((index + slot) % 20),
                    "slot": slot,
                    "count": 1,
                }
                for slot in range(item_count)
            ]
            snapshot_players.append(updated)
        snapshots.append({"game_tick": str(game_tick), "players": snapshot_players})

    events: list[dict[str, Any]] = []
    for sequence, game_tick in enumerate(
        range(
            event_interval_game_ticks,
            duration_game_ticks,
            event_interval_game_ticks,
        ),
        start=1,
    ):
        role = sequence % 3
        killer = (
            local_player if role == 0 else f"QB-Red-{(sequence % 5) + 1}#TEST"
        )
        victim = (
            local_player
            if role == 1
            else f"QB-Red-{((sequence + 1) % 5) + 1}#TEST"
        )
        assisters = [local_player] if role == 2 else ["QB-Blue-2#TEST"]
        events.append(
            {
                "type": "ChampionKill",
                "game_tick": str(game_tick),
                "killer": killer,
                "victim": victim,
                "assisters": assisters,
            }
        )
        if sequence % 30 == 0:
            events.append(
                {
                    "type": "DragonKill",
                    "game_tick": str(game_tick),
                    "killer": "QB-Blue-2#TEST",
                    "dragon_type": "Infernal",
                    "assisters": [local_player],
                }
            )
        if sequence % 18 == 0:
            events.append(
                {
                    "type": "TurretKilled",
                    "game_tick": str(game_tick),
                    "killer": local_player,
                    "turret": "OuterTurret",
                    "assisters": [],
                }
            )
    calibration_last = min(duration_game_ticks - 1, GAME_TICKS_PER_SECOND)
    return {
        "schema_version": SCHEMA_VERSION,
        "media_id": media_id,
        "calibration": {
            "status": "available",
            "sample_count": 5,
            "first_game_tick": "0",
            "last_game_tick": str(calibration_last),
            "replay_tick_at_game_zero": "0",
            "maximum_rtt_game_ticks": "0",
            "maximum_residual_game_ticks": "0",
            "uncertainty_game_ticks": "0",
        },
        "snapshots": snapshots,
        "events": events,
        "snapshot_derived_changes": [],
    }


def _media_summary(probe: Mapping[str, Any]) -> dict[str, Any]:
    streams = probe.get("streams")
    if not isinstance(streams, list):
        raise CorpusError("ffprobe output has no stream array")
    video_streams = [
        stream
        for stream in streams
        if isinstance(stream, dict) and stream.get("codec_type") == "video"
    ]
    audio_streams = [
        stream
        for stream in streams
        if isinstance(stream, dict) and stream.get("codec_type") == "audio"
    ]
    if len(video_streams) != 1 or len(audio_streams) != 1 or len(streams) != 2:
        raise CorpusError("fixture media must contain exactly one video and one audio stream")
    video = video_streams[0]
    audio = audio_streams[0]
    format_value = probe.get("format")
    if not isinstance(format_value, dict):
        raise CorpusError("ffprobe output has no format object")

    video_codec = video.get("codec_name")
    video_profile = video.get("profile")
    audio_codec = audio.get("codec_name")
    format_name = format_value.get("format_name")
    if not isinstance(video_codec, str) or not video_codec.strip():
        raise CorpusError("video codec is missing")
    if video_profile is not None and (
        not isinstance(video_profile, str) or not video_profile
    ):
        raise CorpusError("video profile is malformed")
    if not isinstance(audio_codec, str) or not audio_codec.strip():
        raise CorpusError("audio codec is missing")
    if not isinstance(format_name, str) or "mp4" not in format_name.split(","):
        raise CorpusError("fixture media must use an MP4 container")

    frame_rate = _probe_rational(
        video.get("avg_frame_rate"), "video average frame rate", positive=True
    )
    video_time_base = _probe_rational(
        video.get("time_base"), "video time base", positive=True
    )
    video_start_pts = _schema_i64(
        _probe_integer(video.get("start_pts"), "video start PTS"),
        "video start PTS",
    )
    video_duration_ts = _schema_i64(
        _probe_integer(video.get("duration_ts"), "video duration timestamp"),
        "video duration timestamp",
    )
    video_frames = _probe_integer(
        video.get("nb_read_frames"), "decoded video frame count"
    )
    video_packets = _probe_integer(
        video.get("nb_read_packets"), "video packet count"
    )
    if (
        video_duration_ts <= 0
        or video_frames <= 0
        or video_frames > MAX_REPLAY_TICK
        or video_packets != video_frames
        or video_start_pts + video_duration_ts > MAX_I64
    ):
        raise CorpusError("video frame, packet, or duration evidence is inconsistent")

    audio_time_base = _probe_rational(
        audio.get("time_base"), "audio time base", positive=True
    )
    audio_start_pts = _schema_i64(
        _probe_integer(audio.get("start_pts"), "audio start PTS"),
        "audio start PTS",
    )
    audio_duration_ts = _schema_i64(
        _probe_integer(audio.get("duration_ts"), "audio duration timestamp"),
        "audio duration timestamp",
    )
    audio_packets = _probe_integer(
        audio.get("nb_read_packets"), "audio packet count"
    )
    audio_sample_rate = _probe_integer(
        audio.get("sample_rate"), "audio sample rate"
    )
    audio_channels = _probe_integer(audio.get("channels"), "audio channel count")
    if (
        audio_duration_ts <= 0
        or audio_packets <= 0
        or not 0 < audio_sample_rate <= 384_000
        or audio_channels <= 0
        or audio_start_pts + audio_duration_ts > MAX_I64
    ):
        raise CorpusError("audio packet, duration, or format evidence is inconsistent")

    width = _probe_integer(video.get("width"), "video width")
    height = _probe_integer(video.get("height"), "video height")
    has_b_frames = _probe_integer(video.get("has_b_frames"), "video B-frame depth")
    size_bytes = _probe_integer(format_value.get("size"), "media size")
    if width <= 0 or height <= 0 or has_b_frames < 0 or size_bytes <= 0:
        raise CorpusError("video dimensions, B-frame depth, or media size is invalid")
    container_start = _probe_decimal(
        format_value.get("start_time"), "container start time", positive=False
    )
    container_duration = _probe_decimal(
        format_value.get("duration"), "container duration", positive=True
    )
    return {
        "size_bytes": size_bytes,
        "format_name": format_name,
        "video_codec": video_codec,
        "video_profile": video_profile,
        "width": width,
        "height": height,
        "video_start_pts": video_start_pts,
        "video_duration_ts": video_duration_ts,
        "video_frames": video_frames,
        "video_packets": video_packets,
        "has_b_frames": has_b_frames,
        "audio_codec": audio_codec,
        "audio_sample_rate": audio_sample_rate,
        "audio_channels": audio_channels,
        "audio_start_pts": audio_start_pts,
        "audio_duration_ts": audio_duration_ts,
        "audio_packets": audio_packets,
        "exact_container_start": _wire_rational(
            container_start, "container start time"
        ),
        "exact_container_duration": _wire_rational(
            container_duration, "container duration"
        ),
        "exact_frame_rate": _wire_rational(frame_rate, "video frame rate"),
        "exact_video_time_base": _wire_rational(
            video_time_base, "video time base"
        ),
        "exact_audio_time_base": _wire_rational(
            audio_time_base, "audio time base"
        ),
    }


def _validate_spec(value: Mapping[str, Any]) -> dict[str, Any]:
    _only_fields(
        value,
        {"schema_version", "corpus_id", "sentinel_root", "output_root", "fixtures"},
        "corpus specification",
    )
    _required(
        value,
        ("schema_version", "corpus_id", "sentinel_root", "output_root", "fixtures"),
        "corpus specification",
    )
    if type(value["schema_version"]) is not int or value["schema_version"] != SCHEMA_VERSION:
        raise CorpusError(f"schema_version must be integer {SCHEMA_VERSION}")
    corpus_id = _identifier(value["corpus_id"], "corpus_id")
    sentinel_root = _absolute_path(value["sentinel_root"], "sentinel_root")
    if sentinel_root.name != SENTINEL_NAME:
        raise CorpusError(f"sentinel_root must end in {SENTINEL_NAME}")
    sentinel_root = _assert_reparse_free_existing(sentinel_root, "sentinel_root")
    if not sentinel_root.is_dir():
        raise CorpusError("sentinel_root must be an existing directory")
    output_root = _absolute_path(value["output_root"], "output_root")
    if not _is_strictly_below(output_root, sentinel_root):
        raise CorpusError("output_root must be strictly below sentinel_root")
    if output_root.name != corpus_id:
        raise CorpusError("output_root leaf must equal corpus_id")
    if output_root.exists() or output_root.is_symlink():
        raise CorpusError("output_root already exists; corpus builds are immutable")
    _assert_nearest_ancestor_reparse_free(output_root, "output_root")

    fixtures_value = value["fixtures"]
    if (
        not isinstance(fixtures_value, list)
        or not fixtures_value
        or len(fixtures_value) > MAX_FIXTURES
    ):
        raise CorpusError(f"fixtures must contain between 1 and {MAX_FIXTURES} items")
    ids: set[str] = set()
    aliases: set[str] = set()
    game_ids: set[str] = set()
    media_ids: set[str] = set()
    fixtures: list[dict[str, Any]] = []
    allowed = {
        "id",
        "media_id",
        "alias",
        "game_timestamp",
        "source_video",
        "transform",
        "expected_video_codec",
        "expected_audio_codec",
        "expected_frame_rate",
        "minimum_duration_seconds",
        "target_duration_seconds",
        "event_interval_seconds",
        "snapshot_interval_seconds",
    }
    required = (
        "id",
        "media_id",
        "alias",
        "game_timestamp",
        "source_video",
        "transform",
        "expected_video_codec",
        "expected_audio_codec",
        "expected_frame_rate",
        "minimum_duration_seconds",
    )
    for index, item in enumerate(fixtures_value):
        if not isinstance(item, dict):
            raise CorpusError(f"fixture {index} must be an object")
        _only_fields(item, allowed, f"fixture {index}")
        _required(item, required, f"fixture {index}")
        fixture_id = _identifier(item["id"], f"fixture {index} id")
        alias = _identifier(item["alias"], f"fixture {fixture_id} alias")
        media_id = _media_id(item["media_id"], f"fixture {fixture_id} media_id")
        game_id = item["game_timestamp"]
        if not isinstance(game_id, str) or GAME_ID_RE.fullmatch(game_id) is None:
            raise CorpusError(f"fixture {fixture_id} game_timestamp is invalid")
        if (
            fixture_id in ids
            or alias in aliases
            or game_id in game_ids
            or media_id in media_ids
        ):
            raise CorpusError(
                "fixture ids, aliases, game timestamps, and media IDs must be unique"
            )
        ids.add(fixture_id)
        aliases.add(alias)
        game_ids.add(game_id)
        media_ids.add(media_id)
        source_video = _absolute_path(
            item["source_video"], f"fixture {fixture_id} source_video"
        )
        source_video = _assert_reparse_free_existing(
            source_video, f"fixture {fixture_id} source_video"
        )
        if not source_video.is_file() or source_video.stat().st_size == 0:
            raise CorpusError(f"fixture {fixture_id} source_video must be a nonempty file")
        transform = item["transform"]
        producer_backend = _producer_backend(
            transform, f"fixture {fixture_id} transform"
        )
        expected_video = item["expected_video_codec"]
        expected_audio = item["expected_audio_codec"]
        if (
            not isinstance(expected_video, str)
            or not isinstance(expected_audio, str)
            or expected_video not in {"h264", "hevc"}
            or expected_audio != "aac"
        ):
            raise CorpusError(f"fixture {fixture_id} expected codecs are unsupported")
        expected_frame_rate = _positive_rational(
            item["expected_frame_rate"],
            f"fixture {fixture_id} expected_frame_rate",
        )
        if transform == "hevc" and expected_video != "hevc":
            raise CorpusError(f"fixture {fixture_id} HEVC transform must expect HEVC")
        if transform == "normalize" and expected_video != "h264":
            raise CorpusError(f"fixture {fixture_id} normalize transform must expect H.264")
        minimum_duration = _positive_decimal(
            item["minimum_duration_seconds"],
            f"fixture {fixture_id} minimum_duration_seconds",
            minimum=1,
            maximum=86_400,
        )
        target_duration = item.get("target_duration_seconds")
        if transform in {"repeat", "normalize", "hevc"}:
            target_duration = _positive_rational(
                target_duration,
                f"fixture {fixture_id} target_duration_seconds",
            )
            if target_duration > MAX_REPLAY_DURATION_SECONDS:
                raise CorpusError(
                    f"fixture {fixture_id} target duration exceeds 24 hours"
                )
            if target_duration < minimum_duration:
                raise CorpusError(
                    f"fixture {fixture_id} target duration is below its minimum"
                )
            _target_frame_count(target_duration, expected_frame_rate)
        elif target_duration is not None:
            raise CorpusError(
                f"fixture {fixture_id} {transform} transform has no target duration"
            )
        event_interval_game_ticks = _seconds_to_game_ticks(
            item.get("event_interval_seconds", 5),
            f"fixture {fixture_id} event_interval_seconds",
            maximum=300,
        )
        snapshot_interval_game_ticks = _seconds_to_game_ticks(
            item.get("snapshot_interval_seconds", 30),
            f"fixture {fixture_id} snapshot_interval_seconds",
            maximum=600,
        )
        fixtures.append(
            {
                **item,
                "id": fixture_id,
                "media_id": media_id,
                "alias": alias,
                "game_timestamp": game_id,
                "source_video": source_video,
                "transform": transform,
                "producer_backend": producer_backend,
                "expected_frame_rate": expected_frame_rate,
                "minimum_duration_seconds": minimum_duration,
                "target_duration_seconds": target_duration,
                "event_interval_game_ticks": event_interval_game_ticks,
                "snapshot_interval_game_ticks": snapshot_interval_game_ticks,
            }
        )
    return {
        "schema_version": SCHEMA_VERSION,
        "corpus_id": corpus_id,
        "sentinel_root": sentinel_root,
        "output_root": output_root,
        "fixtures": fixtures,
    }


def _resolve_tools(runtime_root: Path) -> tuple[Path, Path, str]:
    runtime_root = _assert_reparse_free_existing(runtime_root, "media runtime root")
    if not runtime_root.is_dir():
        raise CorpusError("media runtime root must be a directory")
    identity = _read_json(runtime_root / "runtime-manifest.json", "runtime manifest")
    runtime_id = identity.get("runtime_id")
    if not isinstance(runtime_id, str) or not runtime_id.strip():
        raise CorpusError("runtime manifest has no runtime_id")
    ffmpeg = _assert_reparse_free_existing(runtime_root / "bin" / "ffmpeg.exe", "ffmpeg")
    ffprobe = _assert_reparse_free_existing(runtime_root / "bin" / "ffprobe.exe", "ffprobe")
    if not ffmpeg.is_file() or not ffprobe.is_file():
        raise CorpusError("media runtime tools must be files")
    return ffmpeg, ffprobe, runtime_id


def _validate_media(
    probe: Mapping[str, Any], fixture: Mapping[str, Any], *, source: bool
) -> dict[str, Any]:
    summary = _media_summary(probe)
    if source:
        if summary["video_codec"] not in {"h264", "hevc"}:
            raise CorpusError(f"fixture {fixture['id']} source video codec is unsupported")
        if summary["audio_codec"] != "aac":
            raise CorpusError(f"fixture {fixture['id']} source must contain AAC audio")
        if summary["width"] != 1920 or summary["height"] != 1080:
            raise CorpusError(f"fixture {fixture['id']} source must be 1920x1080")
        if fixture["transform"] in {"copy", "remux_audio", "repeat"}:
            if summary["video_codec"] != fixture["expected_video_codec"]:
                raise CorpusError(
                    f"fixture {fixture['id']} source video codec does not match "
                    "the stream-copy transform expectation"
                )
            source_rate = _positive_rational(
                summary["exact_frame_rate"],
                "source video average frame rate",
            )
            if source_rate != fixture["expected_frame_rate"]:
                raise CorpusError(
                    f"fixture {fixture['id']} source frame rate does not match "
                    "expected_frame_rate"
                )
        return summary
    if summary["video_codec"] != fixture["expected_video_codec"]:
        raise CorpusError(f"fixture {fixture['id']} output video codec is wrong")
    if summary["audio_codec"] != fixture["expected_audio_codec"]:
        raise CorpusError(f"fixture {fixture['id']} output audio codec is wrong")
    output_rate = _positive_rational(
        summary["exact_frame_rate"], "output video average frame rate"
    )
    video_duration = Fraction(summary["video_frames"], 1) / output_rate
    if video_duration < fixture["minimum_duration_seconds"]:
        raise CorpusError(f"fixture {fixture['id']} output is shorter than required")
    if summary["width"] != 1920 or summary["height"] != 1080:
        raise CorpusError(f"fixture {fixture['id']} output must remain 1920x1080")
    if output_rate != fixture["expected_frame_rate"]:
        raise CorpusError(
            f"fixture {fixture['id']} output frame rate does not match "
            "expected_frame_rate"
        )
    return summary


def repeat_count(
    target_duration_seconds: Fraction, source_duration_seconds: Fraction
) -> int:
    try:
        target = Fraction(str(target_duration_seconds))
        source = Fraction(str(source_duration_seconds))
    except (ValueError, ZeroDivisionError) as error:
        raise CorpusError("repeat durations must be finite numbers") from error
    if target <= 0 or source <= 0:
        raise CorpusError("repeat durations must be positive")
    ratio = target / source
    return max(1, (ratio.numerator + ratio.denominator - 1) // ratio.denominator)


def _create_media(
    ffmpeg: Path,
    fixture: Mapping[str, Any],
    destination: Path,
    source_grid: Mapping[str, int] | None,
) -> int:
    source = fixture["source_video"]
    transform = fixture["transform"]
    frame_rate: Fraction = fixture["expected_frame_rate"]
    if transform in {"copy", "remux_audio", "repeat"} and source_grid is None:
        raise CorpusError(f"{transform} requires exact source-grid evidence")
    if transform in {"copy", "remux_audio"}:
        if source_grid is None:
            raise CorpusError(f"{transform} requires exact source-grid evidence")
        expected_frame_count = source_grid["frame_count"]
    else:
        expected_frame_count = _target_frame_count(
            fixture["target_duration_seconds"], frame_rate
        )
    if transform == "copy":
        shutil.copyfile(source, destination, follow_symlinks=False)
        return expected_frame_count

    output_duration = Fraction(expected_frame_count, 1) / frame_rate
    audio_duration = output_duration + 1
    audio_source = (
        "sine=frequency=880:sample_rate=48000:duration="
        f"{_format_seconds(audio_duration)}"
    )
    audio_filter = "volume=if(lt(mod(t\\,5)\\,0.08)\\,0.25\\,0):eval=frame"
    common = [str(ffmpeg), "-hide_banner", "-nostdin", "-v", "error", "-n"]
    if transform == "remux_audio":
        arguments = [
            *common,
            "-i",
            str(source),
            "-f",
            "lavfi",
            "-i",
            audio_source,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-frames:v",
            str(expected_frame_count),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-shortest",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            str(destination),
        ]
    elif transform == "repeat":
        if source_grid is None:
            raise CorpusError("repeat requires exact source-grid evidence")
        source_duration = Fraction(source_grid["frame_count"], 1) / frame_rate
        repeats = repeat_count(output_duration, source_duration)
        concat_path = destination.with_suffix(".ffconcat")
        source_path = source.as_posix()
        if "'" in source_path:
            raise CorpusError("repeat fixture source path may not contain a single quote")
        _write_text_new(
            concat_path,
            "ffconcat version 1.0\n" + (f"file '{source_path}'\n" * repeats),
        )
        arguments = [
            *common,
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(concat_path),
            "-f",
            "lavfi",
            "-i",
            audio_source,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-frames:v",
            str(expected_frame_count),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-shortest",
            "-avoid_negative_ts",
            "make_zero",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            str(destination),
        ]
    else:
        video_codec_arguments = (
            [
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high",
                "-pix_fmt",
                "yuv420p",
                "-g",
                "120",
                "-bf",
                "0",
            ]
            if transform == "normalize"
            else [
                "-c:v",
                "hevc_nvenc",
                "-preset",
                "p4",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                "28",
                "-b:v",
                "0",
                "-g",
                "120",
                "-bf",
                "0",
                "-tag:v",
                "hvc1",
            ]
        )
        rate = f"{frame_rate.numerator}/{frame_rate.denominator}"
        arguments = [
            *common,
            "-i",
            str(source),
            "-f",
            "lavfi",
            "-i",
            audio_source,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-vf",
            f"fps=fps={rate}:start_time=0",
            "-frames:v",
            str(expected_frame_count),
            *video_codec_arguments,
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-shortest",
            "-movflags",
            "+faststart",
            str(destination),
        ]
    _run(
        arguments,
        f"{transform} fixture {fixture['id']}",
        7_200,
        max_output_bytes=MAX_TOOL_DIAGNOSTIC_BYTES,
    )
    if transform == "repeat":
        concat_path.unlink()
    return expected_frame_count


def _map_pts_to_replay_tick(
    pts: int,
    time_base: Fraction,
    video_first_pts: int,
    video_time_base: Fraction,
) -> int:
    mapped = (
        Fraction(pts, 1) * time_base
        - Fraction(video_first_pts, 1) * video_time_base
    ) * REPLAY_TICKS_PER_SECOND
    if mapped.denominator != 1:
        raise CorpusError("media PTS cannot be mapped exactly to replay ticks")
    return mapped.numerator


def _build_media_timeline(
    fixture: Mapping[str, Any],
    media: Mapping[str, Any],
    grid: Mapping[str, int],
    runtime_id: str,
) -> dict[str, Any]:
    media_id = _media_id(fixture.get("media_id"), "fixture media_id")
    if not isinstance(runtime_id, str) or not runtime_id.strip():
        raise CorpusError("media runtime identity must be nonempty")
    producer_backend = _producer_backend(
        fixture.get("transform"), "fixture transform"
    )
    supplied_backend = fixture.get("producer_backend", producer_backend)
    if supplied_backend != producer_backend:
        raise CorpusError("fixture producer backend contradicts its transform")
    frame_rate = _positive_rational(
        media.get("exact_frame_rate"), "video average frame rate"
    )
    video_time_base = _positive_rational(
        media.get("exact_video_time_base"), "video time base"
    )
    audio_time_base = _positive_rational(
        media.get("exact_audio_time_base"), "audio time base"
    )
    if frame_rate != fixture.get("expected_frame_rate"):
        raise CorpusError("probed frame rate does not match producer expectation")

    frame_count = grid.get("frame_count")
    first_pts = grid.get("first_pts")
    one_past_last_pts = grid.get("one_past_last_pts")
    duration_ts = grid.get("duration_ts")
    frame_step_pts = grid.get("frame_step_pts")
    if any(
        isinstance(value, bool) or not isinstance(value, int)
        for value in (
            frame_count,
            first_pts,
            one_past_last_pts,
            duration_ts,
            frame_step_pts,
        )
    ):
        raise CorpusError("video grid evidence must use exact integers")
    if (
        frame_count <= 0
        or frame_count > MAX_REPLAY_TICK
        or duration_ts <= 0
        or frame_step_pts <= 0
        or not MIN_I64 <= first_pts <= MAX_I64
        or not MIN_I64 <= one_past_last_pts <= MAX_I64
        or first_pts != media.get("video_start_pts")
        or duration_ts != media.get("video_duration_ts")
        or frame_count != media.get("video_frames")
        or one_past_last_pts != first_pts + duration_ts
        or duration_ts != frame_count * frame_step_pts
    ):
        raise CorpusError("video grid evidence contradicts the probed stream summary")
    expected_step = Fraction(1, 1) / frame_rate / video_time_base
    if expected_step.denominator != 1 or expected_step.numerator != frame_step_pts:
        raise CorpusError("video grid cadence contradicts its rate or time base")

    replay_end = Fraction(frame_count, 1) / frame_rate
    replay_end *= REPLAY_TICKS_PER_SECOND
    pts_replay_end = (
        Fraction(one_past_last_pts - first_pts, 1)
        * video_time_base
        * REPLAY_TICKS_PER_SECOND
    )
    if (
        replay_end.denominator != 1
        or replay_end != pts_replay_end
        or not 0 < replay_end <= MAX_REPLAY_TICK
    ):
        raise CorpusError("video coverage cannot be represented by the replay contract")

    audio_start_pts = media.get("audio_start_pts")
    audio_duration_ts = media.get("audio_duration_ts")
    if (
        isinstance(audio_start_pts, bool)
        or not isinstance(audio_start_pts, int)
        or isinstance(audio_duration_ts, bool)
        or not isinstance(audio_duration_ts, int)
        or audio_duration_ts <= 0
        or not MIN_I64 <= audio_start_pts <= MAX_I64
        or not MIN_I64 <= audio_start_pts + audio_duration_ts <= MAX_I64
    ):
        raise CorpusError("audio PTS evidence must use exact bounded integers")
    audio_end_pts = audio_start_pts + audio_duration_ts
    audio_replay_start = _map_pts_to_replay_tick(
        audio_start_pts,
        audio_time_base,
        first_pts,
        video_time_base,
    )
    audio_replay_end = _map_pts_to_replay_tick(
        audio_end_pts,
        audio_time_base,
        first_pts,
        video_time_base,
    )
    if (
        audio_replay_end <= audio_replay_start
        or abs(audio_replay_start) > MAX_REPLAY_TICK
        or abs(audio_replay_end) > MAX_REPLAY_TICK
    ):
        raise CorpusError("audio coverage is empty, reversed, or outside replay bounds")
    return {
        "schema_version": SCHEMA_VERSION,
        "replay_ticks_per_second": str(REPLAY_TICKS_PER_SECOND),
        "media_id": media_id,
        "video": {
            "codec": media["video_codec"],
            "profile": media["video_profile"],
            "time_base": _wire_rational(video_time_base),
            "first_pts": str(first_pts),
            "frame_rate": _wire_rational(frame_rate),
            "frame_count": str(frame_count),
            "one_past_last_pts": str(one_past_last_pts),
            "replay_end": str(replay_end.numerator),
            "exact_cfr": True,
        },
        "audio": {
            "present": True,
            "codec": media["audio_codec"],
            "sample_rate": media["audio_sample_rate"],
            "time_base": _wire_rational(audio_time_base),
            "first_pts": str(audio_start_pts),
            "replay_start": str(audio_replay_start),
            "replay_end": str(audio_replay_end),
        },
        "container": {
            "start_seconds": media["exact_container_start"],
            "duration_seconds": media["exact_container_duration"],
        },
        "producer": {
            "backend": producer_backend,
            "expected_frame_rate": _wire_rational(frame_rate, "video frame rate"),
            "expected_frame_count": str(frame_count),
            "media_runtime_id": runtime_id,
        },
        "capture": None,
    }


def _build_metadata(
    fixture: Mapping[str, Any],
    media: Mapping[str, Any],
    grid: Mapping[str, int],
    runtime_id: str,
) -> dict[str, Any]:
    media_timeline = _build_media_timeline(fixture, media, grid, runtime_id)
    return {
        "schema_version": SCHEMA_VERSION,
        "media_id": fixture["media_id"],
        "media_timeline": media_timeline,
        "recorded_at": "2026-08-27T00:00:00Z",
        "game_mode": "PRACTICETOOL",
        "local_player_summoner_name": "QB-Blue-1#TEST",
        "local_player_champion": "Ahri",
        "local_player_team": "ORDER",
        "encoder_used": "derived-corpus" if fixture["transform"] != "copy" else "source",
        "recording_codec": media["video_codec"],
        "recording_profile": media["video_profile"] or "unknown",
        "recording_resolution": f"{media['width']}x{media['height']}",
        "capture_backend": media_timeline["producer"]["backend"],
        "capture_adapter_luid": None,
        "capture_adapter_name": "benchmark-generated",
        "capture_output": "benchmark-generated",
        "encoder_interop": "derived" if fixture["transform"] != "copy" else "preserved",
        "media_runtime_id": runtime_id,
        "capture_support_label": "benchmark-generated-non-personal",
        "source_frames_surfaced": 0,
        "source_frames_superseded": 0,
        "cfr_duplicates": 0,
        "cfr_discards": 0,
        "pool_recreations": 0,
        "saved": True,
    }


def _assert_bundle_contract(
    metadata: Mapping[str, Any], game_log: Mapping[str, Any]
) -> None:
    timeline = metadata.get("media_timeline")
    if not isinstance(timeline, Mapping):
        raise CorpusError("metadata media_timeline is missing")
    identities = (
        metadata.get("media_id"),
        timeline.get("media_id"),
        game_log.get("media_id"),
    )
    canonical = [_media_id(value, "bundle media_id") for value in identities]
    if len(set(canonical)) != 1:
        raise CorpusError("metadata, media timeline, and game log media IDs do not match")
    versions = (
        metadata.get("schema_version"),
        timeline.get("schema_version"),
        game_log.get("schema_version"),
    )
    if any(type(version) is not int or version != SCHEMA_VERSION for version in versions):
        raise CorpusError("generated bundle must use schema version 2 throughout")

    def reject_legacy_fields(value: Any) -> None:
        if isinstance(value, Mapping):
            legacy = sorted(LEGACY_TIME_FIELDS.intersection(value))
            if legacy:
                raise CorpusError(
                    f"generated bundle contains legacy fields: {', '.join(legacy)}"
                )
            for child in value.values():
                reject_legacy_fields(child)
        elif isinstance(value, list):
            for child in value:
                reject_legacy_fields(child)

    reject_legacy_fields(metadata)
    reject_legacy_fields(game_log)


def build_corpus(
    specification: Mapping[str, Any], runtime_root: Path, *, preflight_only: bool
) -> dict[str, Any]:
    validated = _validate_spec(specification)
    ffmpeg, ffprobe, runtime_id = _resolve_tools(runtime_root)
    sources: list[dict[str, Any]] = []
    for fixture in validated["fixtures"]:
        source_hash_before = _sha256(fixture["source_video"])
        source_media = _validate_media(
            _probe(ffprobe, fixture["source_video"]), fixture, source=True
        )
        source_grid = None
        if fixture["transform"] in {"copy", "remux_audio", "repeat"}:
            source_grid = _scan_video_grid(
                ffprobe,
                fixture["source_video"],
                time_base=_positive_rational(
                    source_media["exact_video_time_base"],
                    "source video time base",
                ),
                frame_rate=fixture["expected_frame_rate"],
                expected_first_pts=source_media["video_start_pts"],
                expected_duration_ts=source_media["video_duration_ts"],
                expected_frame_count=source_media["video_frames"],
            )
        source_hash_after = _sha256(fixture["source_video"])
        if source_hash_after != source_hash_before:
            raise CorpusError(
                f"fixture {fixture['id']} source changed during preflight"
            )
        sources.append(
            {
                "fixture_id": fixture["id"],
                "media_id": fixture["media_id"],
                "producer_backend": _producer_backend(fixture["transform"]),
                "source_name": fixture["source_video"].name,
                "source_sha256_before": source_hash_before,
                "source_sha256_after_preflight": source_hash_after,
                "source_media": source_media,
                "source_grid": source_grid,
            }
        )
    if preflight_only:
        return {
            "schema_version": SCHEMA_VERSION,
            "corpus_id": validated["corpus_id"],
            "runtime_id": runtime_id,
            "preflight_only": True,
            "fixtures": sources,
        }

    sentinel_root: Path = _assert_reparse_free_existing(
        validated["sentinel_root"], "sentinel_root"
    )
    output_root: Path = validated["output_root"]
    output_root.parent.mkdir(parents=True, exist_ok=True)
    output_parent = _assert_reparse_free_existing(
        output_root.parent, "output_root parent"
    )
    if output_parent != sentinel_root and not _is_strictly_below(
        output_parent, sentinel_root
    ):
        raise CorpusError("output_root parent escaped sentinel_root")
    output_root.mkdir(exist_ok=False)
    output_root = _assert_reparse_free_existing(output_root, "output_root")
    if not _is_strictly_below(output_root, sentinel_root):
        raise CorpusError("created output_root escaped sentinel_root")
    staging_root = output_root / ".partial"
    staging_root.mkdir()
    _assert_reparse_free_existing(staging_root, "staging_root")
    built: list[dict[str, Any]] = []
    for fixture, source_evidence in zip(validated["fixtures"], sources):
        bundle = staging_root / fixture["id"]
        bundle.mkdir()
        _assert_reparse_free_existing(bundle, f"fixture {fixture['id']} staging root")
        media_partial = bundle / "video.partial.mp4"
        expected_frame_count = _create_media(
            ffmpeg,
            fixture,
            media_partial,
            source_evidence["source_grid"],
        )
        source_hash_after_build = _sha256(fixture["source_video"])
        if source_hash_after_build != source_evidence["source_sha256_before"]:
            raise CorpusError(
                f"fixture {fixture['id']} source changed during construction"
            )
        media = _validate_media(
            _probe(ffprobe, media_partial), fixture, source=False
        )
        grid = _scan_video_grid(
            ffprobe,
            media_partial,
            time_base=_positive_rational(
                media["exact_video_time_base"], "output video time base"
            ),
            frame_rate=fixture["expected_frame_rate"],
            expected_first_pts=media["video_start_pts"],
            expected_duration_ts=media["video_duration_ts"],
            expected_frame_count=expected_frame_count,
        )
        metadata = _build_metadata(fixture, media, grid, runtime_id)
        replay_end_tick = int(metadata["media_timeline"]["video"]["replay_end"])
        game_log = build_game_log(
            fixture["media_id"],
            replay_end_tick,
            event_interval_game_ticks=fixture["event_interval_game_ticks"],
            snapshot_interval_game_ticks=fixture["snapshot_interval_game_ticks"],
        )
        _assert_bundle_contract(metadata, game_log)
        metadata_partial = bundle / "metadata.partial.json"
        game_log_partial = bundle / "game_log.partial.json"
        _write_json_new(metadata_partial, metadata)
        _write_json_new(game_log_partial, game_log)
        media_hash = _sha256(media_partial)
        metadata_hash = _sha256(metadata_partial)
        game_log_hash = _sha256(game_log_partial)
        media_partial.rename(bundle / "video.mp4")
        metadata_partial.rename(bundle / "metadata.json")
        game_log_partial.rename(bundle / "game_log.json")
        built.append(
            {
                **source_evidence,
                "source_sha256_after_build": source_hash_after_build,
                "alias": fixture["alias"],
                "game_timestamp": fixture["game_timestamp"],
                "transform": fixture["transform"],
                "bundle_relative_path": fixture["id"],
                "output_media": media,
                "output_grid": grid,
                "output_media_sha256": media_hash,
                "metadata_sha256": metadata_hash,
                "game_log_sha256": game_log_hash,
                "snapshot_count": len(game_log["snapshots"]),
                "event_count": len(game_log["events"]),
                "personal_data": False,
            }
        )
    receipt = {
        "schema_version": SCHEMA_VERSION,
        "corpus_id": validated["corpus_id"],
        "runtime_id": runtime_id,
        "source_policy": (
            "explicit generated fixture media; sources opened read-only and "
            "hash-checked before/after"
        ),
        "publication_policy": (
            "fixtures remain below .partial until all media and schema-v2 "
            "bundle facts are validated; corpus.json publishes last"
        ),
        "automatic_cleanup": False,
        "fixtures": built,
    }
    for fixture in validated["fixtures"]:
        (staging_root / fixture["id"]).rename(output_root / fixture["id"])
    staging_root.rmdir()
    receipt_partial = output_root / "corpus.partial.json"
    _write_json_new(receipt_partial, receipt)
    receipt_partial.rename(output_root / "corpus.json")
    return receipt


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Build deterministic QueueBack replay bundles from explicit generated media."
    )
    parser.add_argument("--spec", required=True, help="Corpus schema-v2 specification")
    parser.add_argument("--media-runtime-root", required=True)
    parser.add_argument(
        "--preflight-only",
        action="store_true",
        help="Validate and hash sources without creating the sentinel or corpus",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        specification = _read_json(Path(arguments.spec), "corpus specification")
        receipt = build_corpus(
            specification,
            Path(arguments.media_runtime_root),
            preflight_only=arguments.preflight_only,
        )
    except (CorpusError, OSError) as error:
        print(f"QB-REPLAY-CORPUS-ERROR: {error}", file=sys.stderr)
        print("Partial corpus artifacts are preserved; nothing is cleaned automatically.", file=sys.stderr)
        return 2
    fixture_count = len(receipt["fixtures"])
    if arguments.preflight_only:
        print(f"QB-REPLAY-CORPUS-PREFLIGHT-OK: {fixture_count} source fixture(s)")
    else:
        print(f"QB-REPLAY-CORPUS-OK: {fixture_count} bundle(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
