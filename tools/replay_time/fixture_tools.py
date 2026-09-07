#!/usr/bin/env python3
"""Deterministic fixture generation and verification for QB-REPLAY-012."""

from __future__ import annotations

import argparse
from copy import deepcopy
from decimal import Decimal, InvalidOperation
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import sys
import uuid
import wave
import zlib
from typing import Any, BinaryIO, Iterable, Mapping, Sequence


SCHEMA_VERSION = 2
REPLAY_TICKS_PER_SECOND = 48_000_000
MAX_REPLAY_DURATION_SECONDS = 24 * 60 * 60
MAX_REPLAY_TICK = REPLAY_TICKS_PER_SECOND * MAX_REPLAY_DURATION_SECONDS
MAX_GAME_TICK = MAX_REPLAY_DURATION_SECONDS * 1_000_000
MIN_I64 = -(1 << 63)
MAX_I64 = (1 << 63) - 1
MAX_U32 = (1 << 32) - 1
MAX_U64 = (1 << 64) - 1
MAX_DECIMAL_LEN = 20
SAMPLE_RATE = 48_000
MAX_ERROR_CHARS = 2_000
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_BUNDLE_JSON_BYTES = 16 * 1024 * 1024
MAX_PROBE_JSON_BYTES = 64 * 1024 * 1024
MAX_RESULT_BYTES = 1024 * 1024
MAX_RAW_VIDEO_BYTES = 2 * 1024 * 1024 * 1024
MAX_WAV_BYTES = 512 * 1024 * 1024
MAX_BUNDLE_MEDIA_BYTES = 8 * 1024 * 1024 * 1024
MAX_STREAMS = 64
MAX_FRAMES = 1_000_000
MAX_PACKETS = 1_000_000
MAX_JSON_NODES = 2_000_000
MAX_JSON_DEPTH = 64
MAX_OBJECT_FIELDS = 512
MAX_ARRAY_ITEMS = 1_000_000
MAX_DIMENSION = 4096
MAX_GENERATED_FRAMES = 100_000
MAX_CREATED_FILES = 20
FILE_ATTRIBUTE_REPARSE_POINT = 0x400
INTEGER_RE = re.compile(r"-?(?:0|[1-9]\d*)$")
UNSIGNED_RE = re.compile(r"(?:0|[1-9]\d*)$")
FORBIDDEN_LEGACY_FIELDS = {
    "video_time_ms",
    "video_offset_ms",
    "duration_ms",
    "recording_fps",
}
VISUAL_MAGIC = 0xC3A5
VISUAL_COLUMNS = 8
VISUAL_ROWS = 8
VISUAL_BITS = 64
NATIVE_VISUAL_COLUMNS = 12
NATIVE_VISUAL_ROWS = 8
NATIVE_VISUAL_BITS = 96
NATIVE_VISUAL_TIMESTAMP_MASK = 0xFF_FFFF
NATIVE_VISUAL_PRE_EPOCH_STATE = 0x5A
NATIVE_VISUAL_LIVE_STATE = 0xA5
NATIVE_VISUAL_CHECKSUM_SEED = 0xA7
ZERO_LUMA = 16
ONE_LUMA = 235
NORMAL_BACKGROUND_LUMA = 48
IMPULSE_LENGTH_SAMPLES = 96
IMPULSE_AMPLITUDE = 30_000
IMPULSE_THRESHOLD = 6_000
MAX_AV_DISAGREEMENT_SAMPLES = SAMPLE_RATE // 20  # 50 ms
MAX_DRIFT_GROWTH_SAMPLES = SAMPLE_RATE // 200  # 5 ms
MAX_DECODED_AUDIO_TAIL_SAMPLES = 2_048
MAX_NATIVE_AUDIO_COVERAGE_DELTA_SAMPLES = 2_048


class FixtureError(ValueError):
    """A bounded, user-actionable fixture contract failure."""


def _bounded_message(error: BaseException) -> str:
    text = " ".join(str(error).split()) or error.__class__.__name__
    if len(text) > MAX_ERROR_CHARS:
        text = text[: MAX_ERROR_CHARS - 3] + "..."
    return text


def _is_reparse(path: Path) -> bool:
    try:
        stat_result = path.lstat()
    except OSError as error:
        raise FixtureError(f"cannot inspect path {path}: {error}") from error
    attributes = getattr(stat_result, "st_file_attributes", 0)
    return path.is_symlink() or bool(attributes & FILE_ATTRIBUTE_REPARSE_POINT)


def _reject_reparse_chain(path: Path, label: str) -> None:
    cursor = path
    while not os.path.lexists(cursor):
        parent = cursor.parent
        if parent == cursor:
            raise FixtureError(f"{label} has no existing ancestor")
        cursor = parent
    while True:
        if _is_reparse(cursor):
            raise FixtureError(f"{label} traverses a reparse/symlink path: {cursor}")
        parent = cursor.parent
        if parent == cursor:
            break
        cursor = parent


def _normalized_path(value: str, label: str) -> Path:
    if not value or "\x00" in value:
        raise FixtureError(f"{label} must be a nonempty path")
    supplied = Path(value)
    if any(part in {".", ".."} for part in supplied.parts):
        raise FixtureError(f"{label} must not contain dot segments")
    return supplied.absolute()


def _existing_file(value: str | Path, label: str, maximum_bytes: int) -> Path:
    path = _normalized_path(str(value), label)
    _reject_reparse_chain(path, label)
    if not path.is_file():
        raise FixtureError(f"{label} is not a regular file: {path}")
    size = path.stat().st_size
    if size < 0 or size > maximum_bytes:
        raise FixtureError(f"{label} exceeds its {maximum_bytes}-byte limit")
    return path


def _existing_directory(value: str | Path, label: str) -> Path:
    path = _normalized_path(str(value), label)
    _reject_reparse_chain(path, label)
    if not path.is_dir():
        raise FixtureError(f"{label} is not a directory: {path}")
    return path


def _new_file(value: str | Path, label: str) -> Path:
    path = _normalized_path(str(value), label)
    if os.path.lexists(path):
        raise FixtureError(f"{label} already exists: {path}")
    _reject_reparse_chain(path, label)
    if not path.parent.is_dir():
        raise FixtureError(f"{label} parent directory is missing: {path.parent}")
    return path


def _new_root(value: str | Path, label: str) -> Path:
    path = _normalized_path(str(value), label)
    if os.path.lexists(path):
        raise FixtureError(f"{label} must be fresh and not already exist: {path}")
    _reject_reparse_chain(path, label)
    if not path.parent.is_dir():
        raise FixtureError(f"{label} parent directory is missing: {path.parent}")
    return path


def _strictly_below(path: Path, root: Path) -> bool:
    try:
        return os.path.commonpath((str(path), str(root))) == str(root) and path != root
    except ValueError:
        return False


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise FixtureError(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _bound_json_shape(value: Any, label: str) -> None:
    stack: list[tuple[Any, int]] = [(value, 0)]
    nodes = 0
    while stack:
        current, depth = stack.pop()
        nodes += 1
        if nodes > MAX_JSON_NODES:
            raise FixtureError(f"{label} exceeds the {MAX_JSON_NODES}-node JSON limit")
        if depth > MAX_JSON_DEPTH:
            raise FixtureError(f"{label} exceeds the {MAX_JSON_DEPTH}-level JSON depth limit")
        if isinstance(current, dict):
            if len(current) > MAX_OBJECT_FIELDS:
                raise FixtureError(
                    f"{label} object exceeds the {MAX_OBJECT_FIELDS}-field limit"
                )
            stack.extend((item, depth + 1) for item in current.values())
        elif isinstance(current, list):
            if len(current) > MAX_ARRAY_ITEMS:
                raise FixtureError(
                    f"{label} array exceeds the {MAX_ARRAY_ITEMS}-item limit"
                )
            stack.extend((item, depth + 1) for item in current)


def _read_json(path: Path, label: str, maximum_bytes: int) -> dict[str, Any]:
    size = path.stat().st_size
    if size > maximum_bytes:
        raise FixtureError(f"{label} exceeds the {maximum_bytes}-byte JSON limit")
    try:
        raw = path.read_bytes()
        text = raw.decode("utf-8")
        value = json.loads(
            text,
            object_pairs_hook=_reject_duplicate_keys,
            parse_float=Decimal,
            parse_constant=lambda token: (_ for _ in ()).throw(
                FixtureError(f"{label} contains non-finite number {token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError, InvalidOperation, RecursionError) as error:
        raise FixtureError(f"{label} is not bounded valid UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise FixtureError(f"{label} must contain one JSON object")
    _bound_json_shape(value, label)
    return value


def _json_bytes(value: Any, label: str, maximum_bytes: int = MAX_RESULT_BYTES) -> bytes:
    try:
        encoded = (
            json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
        ).encode("utf-8")
    except (TypeError, ValueError, RecursionError) as error:
        raise FixtureError(f"cannot serialize {label}: {error}") from error
    if len(encoded) > maximum_bytes:
        raise FixtureError(f"{label} exceeds the {maximum_bytes}-byte output limit")
    return encoded


def _write_bytes_new(path: Path, data: bytes, label: str) -> None:
    try:
        with path.open("xb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    except FileExistsError as error:
        raise FixtureError(f"{label} already exists: {path}") from error


def _write_json_new(path: Path, value: Any, label: str) -> None:
    _write_bytes_new(path, _json_bytes(value, label), label)


def _only_fields(value: Mapping[str, Any], allowed: set[str], label: str) -> None:
    unexpected = sorted(set(value) - allowed)
    if unexpected:
        raise FixtureError(f"{label} has unsupported fields: {', '.join(unexpected)}")


def _require_fields(value: Mapping[str, Any], names: Iterable[str], label: str) -> None:
    missing = [name for name in names if name not in value]
    if missing:
        raise FixtureError(f"{label} is missing required fields: {', '.join(missing)}")
def _exact_object(
    value: Any,
    required: Iterable[str],
    optional: Iterable[str],
    label: str,
) -> Mapping[str, Any]:
    item = _mapping(value, label)
    required_fields = set(required)
    _only_fields(item, required_fields | set(optional), label)
    _require_fields(item, required_fields, label)
    return item


def _string(value: Any, label: str, *, nonempty: bool = False) -> str:
    if not isinstance(value, str):
        raise FixtureError(f"{label} must be a string")
    if nonempty and not value.strip():
        raise FixtureError(f"{label} must be nonempty")
    return value


def _boolean(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise FixtureError(f"{label} must be a boolean")
    return value


def _bounded_json_integer(
    value: Any, label: str, minimum: int, maximum: int
) -> int:
    result = _integer(value, label)
    if not minimum <= result <= maximum:
        raise FixtureError(f"{label} is outside its production integer range")
    return result


def _string_array(value: Any, label: str) -> list[Any]:
    items = _array(value, label, MAX_ARRAY_ITEMS)
    for index, item in enumerate(items):
        _string(item, f"{label}[{index}]")
    return items


def _mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise FixtureError(f"{label} must be an object")
    return value


def _array(value: Any, label: str, maximum: int) -> list[Any]:
    if not isinstance(value, list):
        raise FixtureError(f"{label} must be an array")
    if len(value) > maximum:
        raise FixtureError(f"{label} exceeds the {maximum}-item limit")
    return value


def _integer(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise FixtureError(f"{label} must be an integer")
    return value


def _probe_integer(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, (str, int)):
        raise FixtureError(f"{label} must be a canonical integer")
    text = str(value)
    digits = text.removeprefix("-")
    if (
        INTEGER_RE.fullmatch(text) is None
        or text == "-0"
        or len(digits) > MAX_DECIMAL_LEN
    ):
        raise FixtureError(f"{label} must be a canonical integer")
    return int(text)


def _wire_integer(
    value: Any,
    label: str,
    *,
    positive: bool = False,
    minimum: int = MIN_I64,
    maximum: int = MAX_I64,
) -> int:
    if not isinstance(value, str):
        raise FixtureError(f"{label} must be a canonical decimal wire string")
    digits = value.removeprefix("-")
    if (
        INTEGER_RE.fullmatch(value) is None
        or value == "-0"
        or len(digits) > MAX_DECIMAL_LEN
    ):
        raise FixtureError(f"{label} must be a canonical decimal wire string")
    result = int(value)
    if not minimum <= result <= maximum:
        raise FixtureError(f"{label} is outside its production integer range")
    if positive and result <= 0:
        raise FixtureError(f"{label} must be positive")
    return result


def _wire_unsigned(
    value: Any,
    label: str,
    *,
    positive: bool = False,
    maximum: int = MAX_U64,
) -> int:
    if (
        not isinstance(value, str)
        or UNSIGNED_RE.fullmatch(value) is None
        or len(value) > MAX_DECIMAL_LEN
    ):
        raise FixtureError(f"{label} must be a canonical unsigned decimal wire string")
    result = int(value)
    if result > maximum:
        raise FixtureError(f"{label} is outside its production integer range")
    if positive and result <= 0:
        raise FixtureError(f"{label} must be positive")
    return result


def _parse_fraction_text(value: Any, label: str, *, positive: bool = True) -> Fraction:
    if not isinstance(value, str) or value.count("/") != 1:
        raise FixtureError(f"{label} must be a canonical rational such as 60/1")
    numerator_text, denominator_text = value.split("/", 1)
    numerator = _probe_integer(numerator_text, f"{label} numerator")
    denominator = _probe_integer(denominator_text, f"{label} denominator")
    if not MIN_I64 <= numerator <= MAX_I64:
        raise FixtureError(f"{label} numerator is outside the signed i64 range")
    if not 0 < denominator <= MAX_U64:
        raise FixtureError(f"{label} denominator must be a positive u64")
    result = Fraction(numerator, denominator)
    if positive and result <= 0:
        raise FixtureError(f"{label} must be positive")
    if f"{result.numerator}/{result.denominator}" != value:
        raise FixtureError(f"{label} must be normalized")
    return result


def _wire_rational(value: Any, label: str, *, positive: bool = True) -> Fraction:
    item = _mapping(value, label)
    _only_fields(item, {"numerator", "denominator"}, label)
    _require_fields(item, ("numerator", "denominator"), label)
    numerator = _wire_integer(item["numerator"], f"{label}.numerator")
    denominator = _wire_unsigned(
        item["denominator"], f"{label}.denominator", positive=True
    )
    result = Fraction(numerator, denominator)
    if positive and result <= 0:
        raise FixtureError(f"{label} must be positive")
    if str(result.numerator) != item["numerator"] or str(result.denominator) != item[
        "denominator"
    ]:
        raise FixtureError(f"{label} must be normalized")
    return result


def _fraction_wire(value: Fraction) -> dict[str, str]:
    return {"numerator": str(value.numerator), "denominator": str(value.denominator)}


def _media_id(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise FixtureError(f"{label} must be a lowercase hyphenated non-nil UUID")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError) as error:
        raise FixtureError(
            f"{label} must be a lowercase hyphenated non-nil UUID"
        ) from error
    if parsed.int == 0 or str(parsed) != value:
        raise FixtureError(f"{label} must be a lowercase hyphenated non-nil UUID")
    return value


def _crc16_for_frame(frame_index: int) -> int:
    return zlib.crc32(struct.pack(">I", frame_index)) & 0xFFFF


def _visual_word(frame_index: int) -> int:
    return (VISUAL_MAGIC << 48) | (frame_index << 16) | _crc16_for_frame(frame_index)


def _marker_definitions(frame_count: int, samples_per_frame: int) -> list[dict[str, Any]]:
    indices = (frame_count // 6, frame_count // 2, (5 * frame_count) // 6)
    names = ("early", "middle", "late")
    lumas = (112, 152, 192)
    chroma = ((96, 176), (176, 96), (80, 80))
    markers: list[dict[str, Any]] = []
    for name, index, flash_luma, (u_value, v_value) in zip(
        names, indices, lumas, chroma
    ):
        markers.append(
            {
                "name": name,
                "frame_index": index,
                "sample_index": index * samples_per_frame,
                "time_seconds": _fraction_wire(Fraction(index * samples_per_frame, SAMPLE_RATE)),
                "flash_first_frame": index - 1,
                "flash_frame_count": 3,
                "flash_luma": flash_luma,
                "flash_u": u_value,
                "flash_v": v_value,
            }
        )
    return markers


def _build_manifest(
    width: int,
    height: int,
    frame_count: int,
    rate: Fraction,
    samples_per_frame: int,
    cell_size: int,
    cell_x: int,
    cell_y: int,
    probe_x: int,
    probe_y: int,
    markers: list[dict[str, Any]],
) -> dict[str, Any]:
    frame_bytes = width * height * 3 // 2
    sample_count = frame_count * samples_per_frame
    return {
        "schema_version": SCHEMA_VERSION,
        "fixture_kind": "synthetic-replay-time",
        "synthetic": True,
        "pixel_format": "yuv420p",
        "width": width,
        "height": height,
        "frame_count": frame_count,
        "frame_rate": _fraction_wire(rate),
        "frame_bytes": frame_bytes,
        "duration_seconds": _fraction_wire(Fraction(frame_count, 1) / rate),
        "visual_encoding": {
            "version": 1,
            "magic": f"{VISUAL_MAGIC:04x}",
            "bit_order": "msb-first",
            "payload": "magic16-frame_index32-crc32_low16",
            "columns": VISUAL_COLUMNS,
            "rows": VISUAL_ROWS,
            "cell_size": cell_size,
            "cell_x": cell_x,
            "cell_y": cell_y,
            "probe_x": probe_x,
            "probe_y": probe_y,
            "zero_luma": ZERO_LUMA,
            "one_luma": ONE_LUMA,
            "normal_background_luma": NORMAL_BACKGROUND_LUMA,
        },
        "audio": {
            "encoding": "pcm_s16le",
            "sample_rate": SAMPLE_RATE,
            "channels": 1,
            "sample_width_bytes": 2,
            "sample_count": sample_count,
            "samples_per_frame": samples_per_frame,
            "impulse_length_samples": IMPULSE_LENGTH_SAMPLES,
            "impulse_peak_amplitude": IMPULSE_AMPLITUDE,
        },
        "markers": markers,
    }


def _draw_cells(
    y_plane: bytearray,
    width: int,
    word: int,
    cell_x: int,
    cell_y: int,
    cell_size: int,
) -> None:
    for bit_index in range(VISUAL_BITS):
        bit = (word >> (VISUAL_BITS - 1 - bit_index)) & 1
        value = ONE_LUMA if bit else ZERO_LUMA
        column = bit_index % VISUAL_COLUMNS
        row = bit_index // VISUAL_COLUMNS
        left = cell_x + column * cell_size
        top = cell_y + row * cell_size
        fill = bytes([value]) * cell_size
        for y_value in range(top, top + cell_size):
            offset = y_value * width + left
            y_plane[offset : offset + cell_size] = fill


def _write_generated_video(
    path: Path,
    width: int,
    height: int,
    frame_count: int,
    cell_size: int,
    cell_x: int,
    cell_y: int,
    markers: list[dict[str, Any]],
) -> str:
    marker_by_frame: dict[int, dict[str, Any]] = {}
    for marker in markers:
        first = marker["flash_first_frame"]
        for index in range(first, first + marker["flash_frame_count"]):
            marker_by_frame[index] = marker
    chroma_size = width * height // 4
    digest = hashlib.sha256()
    try:
        with path.open("xb") as output:
            for frame_index in range(frame_count):
                marker = marker_by_frame.get(frame_index)
                background = (
                    int(marker["flash_luma"])
                    if marker is not None
                    else NORMAL_BACKGROUND_LUMA
                )
                y_plane = bytearray([background]) * (width * height)
                _draw_cells(
                    y_plane,
                    width,
                    _visual_word(frame_index),
                    cell_x,
                    cell_y,
                    cell_size,
                )
                u_value = int(marker["flash_u"]) if marker is not None else 128
                v_value = int(marker["flash_v"]) if marker is not None else 128
                frame = bytes(y_plane) + bytes([u_value]) * chroma_size + bytes(
                    [v_value]
                ) * chroma_size
                output.write(frame)
                digest.update(frame)
            output.flush()
            os.fsync(output.fileno())
    except FileExistsError as error:
        raise FixtureError(f"video output already exists: {path}") from error
    return digest.hexdigest()


def _impulse_value(offset: int) -> int:
    sign = 1 if (offset // 8) % 2 == 0 else -1
    envelope = IMPULSE_LENGTH_SAMPLES - offset
    return sign * (IMPULSE_AMPLITUDE * envelope // IMPULSE_LENGTH_SAMPLES)


def _write_generated_audio(
    path: Path, sample_count: int, markers: list[dict[str, Any]]
) -> str:
    marker_starts = [int(marker["sample_index"]) for marker in markers]
    digest = hashlib.sha256()
    try:
        with path.open("xb") as raw_output:
            with wave.open(raw_output, "wb") as output:
                output.setnchannels(1)
                output.setsampwidth(2)
                output.setframerate(SAMPLE_RATE)
                chunk_samples = 65_536
                for first in range(0, sample_count, chunk_samples):
                    count = min(chunk_samples, sample_count - first)
                    chunk = bytearray(count * 2)
                    for marker_start in marker_starts:
                        overlap_first = max(first, marker_start)
                        overlap_last = min(
                            first + count, marker_start + IMPULSE_LENGTH_SAMPLES
                        )
                        for sample_index in range(overlap_first, overlap_last):
                            value = _impulse_value(sample_index - marker_start)
                            struct.pack_into(
                                "<h", chunk, (sample_index - first) * 2, value
                            )
                    output.writeframesraw(chunk)
                output.writeframes(b"")
            raw_output.flush()
            os.fsync(raw_output.fileno())
    except FileExistsError as error:
        raise FixtureError(f"audio output already exists: {path}") from error
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def command_generate(arguments: argparse.Namespace) -> dict[str, Any]:
    video = _new_file(arguments.video, "video output")
    audio = _new_file(arguments.audio, "audio output")
    manifest_path = _new_file(arguments.manifest, "manifest output")
    result_path = _new_file(arguments.result, "result output") if arguments.result else None
    outputs = [video, audio, manifest_path] + ([result_path] if result_path else [])
    if len(set(outputs)) != len(outputs):
        raise FixtureError("generate output paths must be distinct")

    width = _integer(arguments.width, "width")
    height = _integer(arguments.height, "height")
    frame_count = _integer(arguments.frames, "frames")
    if width < 160 or height < 160 or width > MAX_DIMENSION or height > MAX_DIMENSION:
        raise FixtureError(f"dimensions must be even and between 160 and {MAX_DIMENSION}")
    if width % 2 or height % 2:
        raise FixtureError("yuv420p dimensions must be even")
    if frame_count < 12 or frame_count > MAX_GENERATED_FRAMES:
        raise FixtureError(f"frames must be between 12 and {MAX_GENERATED_FRAMES}")
    rate = _parse_fraction_text(arguments.rate, "rate")
    if rate > 1000:
        raise FixtureError("rate must not exceed 1000 frames per second")
    samples_per_frame_fraction = Fraction(SAMPLE_RATE, 1) / rate
    if samples_per_frame_fraction.denominator != 1:
        raise FixtureError("rate must place every frame boundary on an exact 48 kHz sample")
    samples_per_frame = samples_per_frame_fraction.numerator
    frame_bytes = width * height * 3 // 2
    raw_bytes = frame_bytes * frame_count
    sample_count = frame_count * samples_per_frame
    estimated_wav_bytes = 44 + sample_count * 2
    if raw_bytes > MAX_RAW_VIDEO_BYTES:
        raise FixtureError(
            f"generated video would exceed the {MAX_RAW_VIDEO_BYTES}-byte limit"
        )
    if estimated_wav_bytes > MAX_WAV_BYTES:
        raise FixtureError(f"generated WAV would exceed the {MAX_WAV_BYTES}-byte limit")

    cell_size = min(16, (width - 32) // VISUAL_COLUMNS, (height - 32) // VISUAL_ROWS)
    if cell_size < 8:
        raise FixtureError("dimensions are too small for robust visual bit cells")
    cell_x = 16
    cell_y = 16
    probe_x = min(width - 12, cell_x + VISUAL_COLUMNS * cell_size + 12)
    probe_y = min(height - 12, 12)
    markers = _marker_definitions(frame_count, samples_per_frame)
    manifest = _build_manifest(
        width,
        height,
        frame_count,
        rate,
        samples_per_frame,
        cell_size,
        cell_x,
        cell_y,
        probe_x,
        probe_y,
        markers,
    )
    video_sha256 = _write_generated_video(
        video,
        width,
        height,
        frame_count,
        cell_size,
        cell_x,
        cell_y,
        markers,
    )
    audio_sha256 = _write_generated_audio(audio, sample_count, markers)
    _write_json_new(manifest_path, manifest, "manifest")
    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "generate",
        "passed": True,
        "synthetic": True,
        "video": str(video),
        "audio": str(audio),
        "manifest": str(manifest_path),
        "video_bytes": video.stat().st_size,
        "audio_bytes": audio.stat().st_size,
        "video_sha256": video_sha256,
        "audio_sha256": audio_sha256,
        "frame_count": frame_count,
        "sample_count": sample_count,
        "markers": markers,
    }
    if result_path is not None:
        _write_json_new(result_path, result, "generate result")
    return result


def _selected_video_stream(probe: Mapping[str, Any]) -> Mapping[str, Any]:
    streams = _array(probe.get("streams"), "probe.streams", MAX_STREAMS)
    video_streams = [
        stream
        for stream in streams
        if isinstance(stream, dict) and stream.get("codec_type") == "video"
    ]
    if len(video_streams) != 1:
        raise FixtureError("probe must contain exactly one video stream")
    return video_streams[0]


def command_verify_grid(arguments: argparse.Namespace) -> dict[str, Any]:
    probe_path = _existing_file(arguments.probe, "ffprobe JSON", MAX_PROBE_JSON_BYTES)
    result_path = _new_file(arguments.result, "grid result")
    probe = _read_json(probe_path, "ffprobe JSON", MAX_PROBE_JSON_BYTES)
    expected_rate = _parse_fraction_text(arguments.expected_rate, "expected rate")
    if arguments.origin not in {"zero", "nonzero"}:
        raise FixtureError("origin must be zero or nonzero")

    stream = _selected_video_stream(probe)
    stream_index = _probe_integer(stream.get("index"), "video stream index")
    average_rate = _parse_fraction_text(
        stream.get("avg_frame_rate"), "video average frame rate"
    )
    nominal_rate = _parse_fraction_text(stream.get("r_frame_rate"), "video nominal frame rate")
    if average_rate != expected_rate or nominal_rate != expected_rate:
        raise FixtureError("selected video rates do not exactly match the expected rate")
    time_base = _parse_fraction_text(stream.get("time_base"), "video time base")
    frame_step_fraction = Fraction(1, 1) / expected_rate / time_base
    if frame_step_fraction.denominator != 1 or frame_step_fraction <= 0:
        raise FixtureError("video time base cannot represent an exact selected-rate frame step")
    frame_step = frame_step_fraction.numerator

    frames_value = probe.get("frames")
    packets_value = probe.get("packets")
    if frames_value is None and packets_value is None:
        combined = _array(
            probe.get("packets_and_frames"),
            "probe.packets_and_frames",
            MAX_ARRAY_ITEMS,
        )
        frames = []
        packets = []
        for index, raw_item in enumerate(combined):
            item = _mapping(raw_item, f"combined packet/frame {index}")
            item_type = item.get("type")
            if item_type == "frame":
                frames.append(item)
            elif item_type == "packet":
                packets.append(item)
            else:
                raise FixtureError(
                    f"combined packet/frame {index} has unsupported type"
                )
    elif frames_value is None or packets_value is None:
        raise FixtureError("probe must not mix combined and split packet/frame shapes")
    else:
        frames = _array(frames_value, "probe.frames", MAX_FRAMES)
        packets = _array(packets_value, "probe.packets", MAX_PACKETS)
    if not frames or not packets:
        raise FixtureError("probe must contain nonempty video frame and packet evidence")
    if len(frames) != len(packets):
        raise FixtureError("video frame and packet counts must agree exactly")

    frame_pts: list[int] = []
    for index, raw_frame in enumerate(frames):
        frame = _mapping(raw_frame, f"video frame {index}")
        if "stream_index" in frame and _probe_integer(
            frame["stream_index"], f"video frame {index} stream index"
        ) != stream_index:
            raise FixtureError(f"video frame {index} belongs to another stream")
        if "media_type" in frame and frame["media_type"] != "video":
            raise FixtureError(f"video frame {index} is not video evidence")
        pts = _probe_integer(frame.get("pts"), f"video frame {index} PTS")
        duration = _probe_integer(
            frame.get("duration"), f"video frame {index} duration"
        )
        if duration != frame_step:
            raise FixtureError(f"video frame {index} duration is not one exact frame step")
        if frame_pts and pts != frame_pts[-1] + frame_step:
            raise FixtureError(
                f"presentation-ordered video frame {index} is off the exact adjacent PTS grid"
            )
        frame_pts.append(pts)

    packet_pts: list[int] = []
    dts_values: list[int | None] = []
    for index, raw_packet in enumerate(packets):
        packet = _mapping(raw_packet, f"video packet {index}")
        if "stream_index" in packet and _probe_integer(
            packet["stream_index"], f"video packet {index} stream index"
        ) != stream_index:
            raise FixtureError(f"video packet {index} belongs to another stream")
        if "codec_type" in packet and packet["codec_type"] != "video":
            raise FixtureError(f"video packet {index} is not video evidence")
        packet_pts.append(
            _probe_integer(packet.get("pts"), f"video packet {index} PTS")
        )
        duration = _probe_integer(
            packet.get("duration"), f"video packet {index} duration"
        )
        if duration != frame_step:
            raise FixtureError(f"video packet {index} duration is not one exact frame step")
        raw_dts = packet.get("dts")
        if raw_dts in (None, "N/A"):
            dts_values.append(None)
        else:
            dts_values.append(_probe_integer(raw_dts, f"video packet {index} DTS"))

    if sorted(packet_pts) != frame_pts:
        raise FixtureError("packet PTS facts do not exactly match presentation frame PTS facts")
    if len(set(packet_pts)) != len(packet_pts):
        raise FixtureError("packet PTS facts contain duplicates")
    first_pts = frame_pts[0]
    frame_count = len(frame_pts)
    one_past_last_pts = first_pts + frame_count * frame_step
    if arguments.origin == "zero" and first_pts != 0:
        raise FixtureError("zero-origin evidence must begin at PTS zero")
    if arguments.origin == "nonzero" and first_pts == 0:
        raise FixtureError("nonzero-origin evidence must not begin at PTS zero")

    stream_start = _probe_integer(stream.get("start_pts"), "video stream start PTS")
    stream_duration = _probe_integer(
        stream.get("duration_ts"), "video stream duration timestamps"
    )
    read_frames = _probe_integer(
        stream.get("nb_read_frames"), "video stream decoded frame count"
    )
    read_packets = _probe_integer(
        stream.get("nb_read_packets"), "video stream packet count"
    )
    if stream_start != first_pts:
        raise FixtureError("stream start PTS disagrees with the first presentation PTS")
    if stream_duration != frame_count * frame_step:
        raise FixtureError("stream duration disagrees with the one-past-last frame boundary")
    if read_frames != frame_count or read_packets != frame_count:
        raise FixtureError("stream frame/packet counts disagree with scanned evidence")
    raw_nb_frames = stream.get("nb_frames")
    if raw_nb_frames not in (None, "N/A") and _probe_integer(
        raw_nb_frames, "video stream declared frame count"
    ) != frame_count:
        raise FixtureError("declared video frame count disagrees with scanned evidence")

    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "verify-grid",
        "passed": True,
        "stream_index": stream_index,
        "frame_rate": _fraction_wire(expected_rate),
        "time_base": _fraction_wire(time_base),
        "frame_step_pts": str(frame_step),
        "first_pts": str(first_pts),
        "frame_count": str(frame_count),
        "last_pts": str(frame_pts[-1]),
        "one_past_last_pts": str(one_past_last_pts),
        "duration_ts": str(frame_count * frame_step),
        "origin": arguments.origin,
        "packet_pts_match_presentation": True,
        "dts_reordering_allowed": True,
        "dts_present_count": sum(value is not None for value in dts_values),
    }
    _write_json_new(result_path, result, "grid result")
    return result


def _reject_legacy_and_floats(value: Any, label: str) -> None:
    stack: list[tuple[Any, str]] = [(value, label)]
    while stack:
        current, current_label = stack.pop()
        if isinstance(current, Decimal) or isinstance(current, float):
            raise FixtureError(f"{current_label} uses a non-wire JSON decimal number")
        if isinstance(current, dict):
            forbidden = sorted(set(current) & FORBIDDEN_LEGACY_FIELDS)
            if forbidden:
                raise FixtureError(
                    f"{current_label} contains forbidden legacy fields: {', '.join(forbidden)}"
                )
            if "numerator" in current or "denominator" in current:
                _wire_rational(current, current_label, positive=False)
            for key, child in current.items():
                stack.append((child, f"{current_label}.{key}"))
        elif isinstance(current, list):
            for index, child in enumerate(current):
                stack.append((child, f"{current_label}[{index}]"))


def _validate_capture_metadata(value: Any, label: str) -> Mapping[str, Any]:
    capture = _exact_object(
        value,
        (
            "schema_version",
            "backend",
            "diagnostics_abi",
            "support_label",
            "capture_adapter_luid",
            "encoder_adapter_luid",
            "capture_adapter_name",
            "capture_output",
            "encoder_backend",
            "encoder_interop",
            "media_runtime_id",
            "source_format",
            "converted_format",
            "host_readback",
            "gpu_stages",
            "frame_pool_capacity",
            "capture_output_pool_capacity",
            "filter_buffered_frame_limit",
            "encoder_depth",
            "progress_stall_timeout_seconds",
            "maximum_texture_bytes",
            "source_frames_surfaced",
            "source_frames_superseded",
            "encoded_frames",
            "muxed_bytes",
            "cfr_duplicates",
            "cfr_discards",
            "pool_recreations",
            "first_qpc_100ns",
            "latest_qpc_100ns",
            "terminal_progress",
        ),
        (),
        label,
    )
    _bounded_json_integer(capture["schema_version"], f"{label}.schema_version", 0, MAX_U32)
    _bounded_json_integer(capture["diagnostics_abi"], f"{label}.diagnostics_abi", 0, MAX_U32)
    for name in (
        "backend",
        "support_label",
        "capture_adapter_luid",
        "encoder_adapter_luid",
        "capture_adapter_name",
        "capture_output",
        "encoder_backend",
        "encoder_interop",
        "media_runtime_id",
        "source_format",
        "converted_format",
    ):
        _string(capture[name], f"{label}.{name}")
    _boolean(capture["host_readback"], f"{label}.host_readback")
    _string_array(capture["gpu_stages"], f"{label}.gpu_stages")
    for name in (
        "frame_pool_capacity",
        "capture_output_pool_capacity",
        "filter_buffered_frame_limit",
        "encoder_depth",
        "progress_stall_timeout_seconds",
    ):
        _bounded_json_integer(capture[name], f"{label}.{name}", 0, MAX_U32)
    for name in (
        "maximum_texture_bytes",
        "source_frames_surfaced",
        "source_frames_superseded",
        "encoded_frames",
        "muxed_bytes",
        "cfr_duplicates",
        "cfr_discards",
        "pool_recreations",
    ):
        _bounded_json_integer(capture[name], f"{label}.{name}", 0, MAX_U64)
    for name in ("first_qpc_100ns", "latest_qpc_100ns"):
        _bounded_json_integer(capture[name], f"{label}.{name}", MIN_I64, MAX_I64)
    _boolean(capture["terminal_progress"], f"{label}.terminal_progress")
    return capture


def _validate_media_timeline_shape(value: Any, label: str) -> Mapping[str, Any]:
    timeline = _exact_object(
        value,
        (
            "schema_version",
            "replay_ticks_per_second",
            "media_id",
            "video",
            "audio",
            "container",
            "producer",
        ),
        ("capture",),
        label,
    )
    _bounded_json_integer(timeline["schema_version"], f"{label}.schema_version", 0, MAX_U32)
    _wire_unsigned(
        timeline["replay_ticks_per_second"],
        f"{label}.replay_ticks_per_second",
        positive=True,
    )
    _media_id(timeline["media_id"], f"{label}.media_id")

    video_label = f"{label}.video"
    video = _exact_object(
        timeline["video"],
        (
            "codec",
            "time_base",
            "first_pts",
            "frame_rate",
            "frame_count",
            "one_past_last_pts",
            "replay_end",
            "exact_cfr",
        ),
        ("profile",),
        video_label,
    )
    _string(video["codec"], f"{video_label}.codec")
    if video.get("profile") is not None:
        _string(video["profile"], f"{video_label}.profile")
    _wire_rational(video["time_base"], f"{video_label}.time_base")
    _wire_integer(video["first_pts"], f"{video_label}.first_pts")
    _wire_rational(video["frame_rate"], f"{video_label}.frame_rate")
    _wire_unsigned(
        video["frame_count"],
        f"{video_label}.frame_count",
        maximum=MAX_REPLAY_TICK,
    )
    _wire_integer(video["one_past_last_pts"], f"{video_label}.one_past_last_pts")
    _wire_unsigned(
        video["replay_end"],
        f"{video_label}.replay_end",
        maximum=MAX_REPLAY_TICK,
    )
    _boolean(video["exact_cfr"], f"{video_label}.exact_cfr")

    audio_label = f"{label}.audio"
    audio = _exact_object(
        timeline["audio"],
        ("present",),
        ("codec", "sample_rate", "time_base", "first_pts", "replay_start", "replay_end"),
        audio_label,
    )
    audio_present = _boolean(audio["present"], f"{audio_label}.present")
    optional_audio = (
        "codec",
        "sample_rate",
        "time_base",
        "first_pts",
        "replay_start",
        "replay_end",
    )
    if audio_present and any(audio.get(name) is None for name in optional_audio):
        raise FixtureError(f"{audio_label} is missing required present-stream facts")
    if not audio_present and any(audio.get(name) is not None for name in optional_audio):
        raise FixtureError(f"{audio_label} absent stream must not carry facts")
    if audio.get("codec") is not None:
        _string(audio["codec"], f"{audio_label}.codec")
    if audio.get("sample_rate") is not None:
        _bounded_json_integer(
            audio["sample_rate"], f"{audio_label}.sample_rate", 0, MAX_U32
        )
    if audio.get("time_base") is not None:
        _wire_rational(audio["time_base"], f"{audio_label}.time_base")
    if audio.get("first_pts") is not None:
        _wire_integer(audio["first_pts"], f"{audio_label}.first_pts")
    for name in ("replay_start", "replay_end"):
        if audio.get(name) is not None:
            _wire_integer(
                audio[name],
                f"{audio_label}.{name}",
                minimum=-MAX_REPLAY_TICK,
                maximum=MAX_REPLAY_TICK,
            )

    container_label = f"{label}.container"
    container = _exact_object(
        timeline["container"],
        ("start_seconds", "duration_seconds"),
        (),
        container_label,
    )
    _wire_rational(
        container["start_seconds"], f"{container_label}.start_seconds", positive=False
    )
    _wire_rational(container["duration_seconds"], f"{container_label}.duration_seconds")

    producer_label = f"{label}.producer"
    producer = _exact_object(
        timeline["producer"],
        ("backend", "expected_frame_rate", "expected_frame_count", "media_runtime_id"),
        (),
        producer_label,
    )
    _string(producer["backend"], f"{producer_label}.backend")
    _wire_rational(
        producer["expected_frame_rate"], f"{producer_label}.expected_frame_rate"
    )
    _wire_unsigned(
        producer["expected_frame_count"],
        f"{producer_label}.expected_frame_count",
        maximum=MAX_REPLAY_TICK,
    )
    _string(producer["media_runtime_id"], f"{producer_label}.media_runtime_id")

    if timeline.get("capture") is not None:
        capture_label = f"{label}.capture"
        capture = _exact_object(
            timeline["capture"],
            ("first_qpc_100ns", "recorded_at"),
            (),
            capture_label,
        )
        _wire_integer(capture["first_qpc_100ns"], f"{capture_label}.first_qpc_100ns")
        _string(capture["recorded_at"], f"{capture_label}.recorded_at")
    return timeline


def _validate_metadata_shape(metadata: Any) -> Mapping[str, Any]:
    label = "metadata"
    document = _exact_object(
        metadata,
        (
            "schema_version",
            "media_id",
            "media_timeline",
            "recorded_at",
            "encoder_used",
            "recording_codec",
            "recording_profile",
            "recording_resolution",
            "capture_backend",
            "media_runtime_id",
            "capture_support_label",
            "source_frames_surfaced",
            "source_frames_superseded",
            "cfr_duplicates",
            "cfr_discards",
            "pool_recreations",
            "saved",
        ),
        (
            "game_mode",
            "local_player_summoner_name",
            "local_player_champion",
            "local_player_team",
            "capture_adapter_luid",
            "capture_adapter_name",
            "capture_output",
            "encoder_interop",
            "capture",
        ),
        label,
    )
    _bounded_json_integer(document["schema_version"], "metadata.schema_version", 0, MAX_U32)
    _media_id(document["media_id"], "metadata.media_id")
    _validate_media_timeline_shape(document["media_timeline"], "metadata.media_timeline")
    for name in (
        "recorded_at",
        "encoder_used",
        "recording_codec",
        "recording_profile",
        "recording_resolution",
        "capture_backend",
        "media_runtime_id",
        "capture_support_label",
    ):
        _string(document[name], f"metadata.{name}")
    for name in (
        "game_mode",
        "local_player_summoner_name",
        "local_player_champion",
        "local_player_team",
        "capture_adapter_luid",
        "capture_adapter_name",
        "capture_output",
        "encoder_interop",
    ):
        if document.get(name) is not None:
            _string(document[name], f"metadata.{name}")
    for name in (
        "source_frames_surfaced",
        "source_frames_superseded",
        "cfr_duplicates",
        "cfr_discards",
        "pool_recreations",
    ):
        _bounded_json_integer(document[name], f"metadata.{name}", 0, MAX_U64)
    _boolean(document["saved"], "metadata.saved")
    if document.get("capture") is not None:
        _validate_capture_metadata(document["capture"], "metadata.capture")
    return document


def _validate_game_calibration(value: Any, label: str) -> Mapping[str, Any]:
    calibration = _exact_object(
        value,
        (
            "status",
            "sample_count",
            "maximum_rtt_game_ticks",
            "maximum_residual_game_ticks",
            "uncertainty_game_ticks",
        ),
        ("first_game_tick", "last_game_tick", "replay_tick_at_game_zero"),
        label,
    )
    status = _string(calibration["status"], f"{label}.status")
    if status not in {"available", "temporarily_unavailable", "invalidated"}:
        raise FixtureError(f"{label}.status is unsupported")
    sample_count = _bounded_json_integer(
        calibration["sample_count"], f"{label}.sample_count", 0, MAX_U32
    )
    first_game_tick = None
    if calibration.get("first_game_tick") is not None:
        first_game_tick = _wire_integer(
            calibration["first_game_tick"],
            f"{label}.first_game_tick",
            minimum=-MAX_GAME_TICK,
            maximum=MAX_GAME_TICK,
        )
    last_game_tick = None
    if calibration.get("last_game_tick") is not None:
        last_game_tick = _wire_integer(
            calibration["last_game_tick"],
            f"{label}.last_game_tick",
            minimum=-MAX_GAME_TICK,
            maximum=MAX_GAME_TICK,
        )
    replay_tick_at_game_zero = None
    if calibration.get("replay_tick_at_game_zero") is not None:
        replay_tick_at_game_zero = _wire_integer(
            calibration["replay_tick_at_game_zero"],
            f"{label}.replay_tick_at_game_zero",
            minimum=-MAX_REPLAY_TICK,
            maximum=MAX_REPLAY_TICK,
        )
    for name in (
        "maximum_rtt_game_ticks",
        "maximum_residual_game_ticks",
        "uncertainty_game_ticks",
    ):
        _wire_unsigned(calibration[name], f"{label}.{name}")
    if status == "available":
        if (
            sample_count < 5
            or first_game_tick is None
            or last_game_tick is None
            or replay_tick_at_game_zero is None
            or first_game_tick >= last_game_tick
        ):
            raise FixtureError(f"{label} available state lacks a valid sample span")
    elif replay_tick_at_game_zero is not None:
        raise FixtureError(f"{label} unavailable state must not carry an affine intercept")
    return calibration


def _validate_snapshot_player(value: Any, label: str) -> None:
    player = _exact_object(
        value,
        ("summoner_name", "team", "champion", "cs", "level", "items"),
        ("gold", "hp", "hp_max", "summoner_spells", "keystone_id", "rune_ids"),
        label,
    )
    for name in ("summoner_name", "team", "champion"):
        _string(player[name], f"{label}.{name}")
    for name in ("gold", "hp", "hp_max"):
        if player.get(name) is not None:
            _bounded_json_integer(player[name], f"{label}.{name}", MIN_I64, MAX_I64)
    for name in ("cs", "level"):
        _bounded_json_integer(player[name], f"{label}.{name}", 0, MAX_U32)
    if player.get("keystone_id") is not None:
        _bounded_json_integer(
            player["keystone_id"], f"{label}.keystone_id", 0, MAX_U32
        )
    items = _array(player["items"], f"{label}.items", MAX_ARRAY_ITEMS)
    for index, raw_item in enumerate(items):
        item_label = f"{label}.items[{index}]"
        item = _exact_object(
            raw_item, ("item_id", "slot", "count"), (), item_label
        )
        for name in ("item_id", "slot", "count"):
            _bounded_json_integer(item[name], f"{item_label}.{name}", 0, MAX_U32)
    if player.get("summoner_spells") is not None:
        _string_array(player["summoner_spells"], f"{label}.summoner_spells")
    if player.get("rune_ids") is not None:
        rune_ids = _array(player["rune_ids"], f"{label}.rune_ids", MAX_ARRAY_ITEMS)
        for index, rune_id in enumerate(rune_ids):
            _bounded_json_integer(
                rune_id, f"{label}.rune_ids[{index}]", 0, MAX_U32
            )


def _validate_game_log_shape(game_log: Any) -> Mapping[str, Any]:
    label = "game log"
    document = _exact_object(
        game_log,
        ("schema_version", "media_id", "snapshots", "events", "snapshot_derived_changes"),
        ("calibration",),
        label,
    )
    _bounded_json_integer(document["schema_version"], f"{label}.schema_version", 0, MAX_U32)
    _media_id(document["media_id"], f"{label}.media_id")
    if document.get("calibration") is not None:
        _validate_game_calibration(document["calibration"], f"{label}.calibration")

    snapshots = _array(document["snapshots"], f"{label}.snapshots", MAX_ARRAY_ITEMS)
    for index, raw_snapshot in enumerate(snapshots):
        snapshot_label = f"{label}.snapshots[{index}]"
        snapshot = _exact_object(
            raw_snapshot, ("game_tick", "players"), (), snapshot_label
        )
        _wire_integer(
            snapshot["game_tick"],
            f"{snapshot_label}.game_tick",
            minimum=-MAX_GAME_TICK,
            maximum=MAX_GAME_TICK,
        )
        players = _array(
            snapshot["players"], f"{snapshot_label}.players", MAX_ARRAY_ITEMS
        )
        for player_index, player in enumerate(players):
            _validate_snapshot_player(
                player, f"{snapshot_label}.players[{player_index}]"
            )

    events = _array(document["events"], f"{label}.events", MAX_ARRAY_ITEMS)
    event_optional = (
        "killer",
        "victim",
        "assisters",
        "dragon_type",
        "stolen",
        "kill_streak",
        "acer",
        "acing_team",
        "turret",
        "inhibitor",
        "result",
    )
    for index, raw_event in enumerate(events):
        event_label = f"{label}.events[{index}]"
        event = _exact_object(
            raw_event, ("type", "game_tick"), event_optional, event_label
        )
        _string(event["type"], f"{event_label}.type")
        _wire_integer(
            event["game_tick"],
            f"{event_label}.game_tick",
            minimum=-MAX_GAME_TICK,
            maximum=MAX_GAME_TICK,
        )
        for name in (
            "killer",
            "victim",
            "dragon_type",
            "acer",
            "acing_team",
            "turret",
            "inhibitor",
            "result",
        ):
            if event.get(name) is not None:
                _string(event[name], f"{event_label}.{name}")
        if event.get("assisters") is not None:
            _string_array(event["assisters"], f"{event_label}.assisters")
        if event.get("stolen") is not None:
            _boolean(event["stolen"], f"{event_label}.stolen")
        if event.get("kill_streak") is not None:
            _bounded_json_integer(
                event["kill_streak"], f"{event_label}.kill_streak", 0, MAX_U32
            )

    changes = _array(
        document["snapshot_derived_changes"],
        f"{label}.snapshot_derived_changes",
        MAX_ARRAY_ITEMS,
    )
    for index, raw_change in enumerate(changes):
        change_label = f"{label}.snapshot_derived_changes[{index}]"
        change = _exact_object(
            raw_change,
            ("game_tick", "player", "change_type"),
            ("item_id", "new_level"),
            change_label,
        )
        _wire_integer(
            change["game_tick"],
            f"{change_label}.game_tick",
            minimum=-MAX_GAME_TICK,
            maximum=MAX_GAME_TICK,
        )
        _string(change["player"], f"{change_label}.player")
        change_type = _string(change["change_type"], f"{change_label}.change_type")
        if change_type not in {"ItemPurchased", "ItemSold", "LevelUp"}:
            raise FixtureError(f"{change_label}.change_type is unsupported")
        for name in ("item_id", "new_level"):
            if change.get(name) is not None:
                _bounded_json_integer(
                    change[name], f"{change_label}.{name}", 0, MAX_U32
                )
    return document


def _validate_bundle_documents(
    metadata: Mapping[str, Any], game_log: Mapping[str, Any], expected_media_id: str
) -> dict[str, Any]:
    _reject_legacy_and_floats(metadata, "metadata")
    _reject_legacy_and_floats(game_log, "game log")
    metadata = _validate_metadata_shape(metadata)
    game_log = _validate_game_log_shape(game_log)
    if metadata["schema_version"] != SCHEMA_VERSION:
        raise FixtureError("metadata schema_version must be exactly 2")
    if game_log["schema_version"] != SCHEMA_VERSION:
        raise FixtureError("game log schema_version must be exactly 2")
    timeline = _mapping(metadata["media_timeline"], "metadata media_timeline")
    if timeline["schema_version"] != SCHEMA_VERSION:
        raise FixtureError("metadata media_timeline schema_version must be exactly 2")
    identities = {
        _media_id(metadata["media_id"], "metadata media_id"),
        _media_id(timeline["media_id"], "metadata media_timeline media_id"),
        _media_id(game_log["media_id"], "game log media_id"),
        expected_media_id,
    }
    if len(identities) != 1:
        raise FixtureError("metadata, media_timeline, game log, and expected media identity differ")

    ticks_per_second = _wire_unsigned(
        timeline["replay_ticks_per_second"],
        "metadata media_timeline replay_ticks_per_second",
        positive=True,
    )
    if ticks_per_second != REPLAY_TICKS_PER_SECOND:
        raise FixtureError("metadata media_timeline replay scale must be exactly 48000000")

    video = _mapping(timeline["video"], "metadata media_timeline video")
    if video["exact_cfr"] is not True:
        raise FixtureError("metadata media_timeline video exact_cfr must be true")
    _string(video["codec"], "metadata media_timeline video.codec", nonempty=True)
    video_time_base = _wire_rational(
        video["time_base"], "metadata media_timeline video.time_base"
    )
    frame_rate = _wire_rational(
        video["frame_rate"], "metadata media_timeline video.frame_rate"
    )
    first_pts = _wire_integer(
        video["first_pts"], "metadata media_timeline video.first_pts"
    )
    frame_count = _wire_unsigned(
        video["frame_count"],
        "metadata media_timeline video.frame_count",
        positive=True,
        maximum=MAX_REPLAY_TICK,
    )
    one_past_last_pts = _wire_integer(
        video["one_past_last_pts"],
        "metadata media_timeline video.one_past_last_pts",
    )
    replay_end = _wire_unsigned(
        video["replay_end"],
        "metadata media_timeline video.replay_end",
        positive=True,
        maximum=MAX_REPLAY_TICK,
    )
    pts_delta = one_past_last_pts - first_pts
    if not MIN_I64 <= pts_delta <= MAX_I64:
        raise FixtureError("metadata video presentation interval exceeds signed i64")
    step = Fraction(1, 1) / frame_rate / video_time_base
    if step.denominator != 1 or one_past_last_pts != first_pts + frame_count * step:
        raise FixtureError("metadata video exact frame-grid wire facts are inconsistent")
    expected_replay_end = Fraction(
        frame_count * REPLAY_TICKS_PER_SECOND, 1
    ) / frame_rate
    if expected_replay_end.denominator != 1 or replay_end != expected_replay_end:
        raise FixtureError("metadata video replay_end wire fact is inconsistent")

    audio = _mapping(timeline["audio"], "metadata media_timeline audio")
    if audio["present"] is not True:
        raise FixtureError("metadata media_timeline audio must be present")
    _string(audio["codec"], "metadata media_timeline audio.codec", nonempty=True)
    _wire_rational(audio["time_base"], "metadata media_timeline audio.time_base")
    _wire_integer(audio["first_pts"], "metadata media_timeline audio.first_pts")
    audio_start = _wire_integer(
        audio["replay_start"],
        "metadata media_timeline audio.replay_start",
        minimum=-MAX_REPLAY_TICK,
        maximum=MAX_REPLAY_TICK,
    )
    audio_end = _wire_integer(
        audio["replay_end"],
        "metadata media_timeline audio.replay_end",
        minimum=-MAX_REPLAY_TICK,
        maximum=MAX_REPLAY_TICK,
    )
    if audio_end <= audio_start:
        raise FixtureError("metadata audio replay interval is empty or reversed")
    if _bounded_json_integer(
        audio["sample_rate"],
        "metadata media_timeline audio.sample_rate",
        0,
        MAX_U32,
    ) == 0:
        raise FixtureError("metadata audio sample_rate must be a positive integer")

    container = _mapping(timeline["container"], "metadata media_timeline container")
    _wire_rational(
        container["start_seconds"],
        "metadata media_timeline container.start_seconds",
        positive=False,
    )
    _wire_rational(
        container["duration_seconds"],
        "metadata media_timeline container.duration_seconds",
    )
    producer = _mapping(timeline["producer"], "metadata media_timeline producer")
    producer_backend = _string(
        producer["backend"],
        "metadata media_timeline producer.backend",
        nonempty=True,
    )
    producer_rate = _wire_rational(
        producer["expected_frame_rate"],
        "metadata media_timeline producer.expected_frame_rate",
    )
    producer_count = _wire_unsigned(
        producer["expected_frame_count"],
        "metadata media_timeline producer.expected_frame_count",
        positive=True,
        maximum=MAX_REPLAY_TICK,
    )
    producer_runtime_id = _string(
        producer["media_runtime_id"],
        "metadata media_timeline producer.media_runtime_id",
        nonempty=True,
    )
    if producer_rate != frame_rate or producer_count != frame_count:
        raise FixtureError("metadata producer exact CFR facts disagree with video facts")
    if (
        metadata["recording_codec"] != video["codec"]
        or metadata["capture_backend"] != producer_backend
        or metadata["media_runtime_id"] != producer_runtime_id
    ):
        raise FixtureError("metadata recorder facts disagree with media_timeline provenance")
    if metadata.get("capture") is not None:
        capture = _mapping(metadata["capture"], "metadata capture")
        if (
            capture["schema_version"] != 1
            or capture["backend"] != metadata["capture_backend"]
            or capture["media_runtime_id"] != metadata["media_runtime_id"]
        ):
            raise FixtureError("metadata capture facts disagree with recorder provenance")
    return {
        "media_id": expected_media_id,
        "frame_count": frame_count,
        "frame_rate": frame_rate,
        "first_pts": first_pts,
        "one_past_last_pts": one_past_last_pts,
        "replay_end": replay_end,
    }


def command_verify_bundle(arguments: argparse.Namespace) -> dict[str, Any]:
    metadata_path = _existing_file(
        arguments.metadata, "metadata JSON", MAX_BUNDLE_JSON_BYTES
    )
    game_log_path = _existing_file(
        arguments.game_log, "game log JSON", MAX_BUNDLE_JSON_BYTES
    )
    result_path = _new_file(arguments.result, "bundle result")
    expected_media_id = _media_id(arguments.expected_media_id, "expected media id")
    metadata = _read_json(metadata_path, "metadata JSON", MAX_BUNDLE_JSON_BYTES)
    game_log = _read_json(game_log_path, "game log JSON", MAX_BUNDLE_JSON_BYTES)
    facts = _validate_bundle_documents(metadata, game_log, expected_media_id)
    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "verify-bundle",
        "passed": True,
        "media_id": facts["media_id"],
        "frame_count": str(facts["frame_count"]),
        "frame_rate": _fraction_wire(facts["frame_rate"]),
        "first_pts": str(facts["first_pts"]),
        "one_past_last_pts": str(facts["one_past_last_pts"]),
        "replay_end": str(facts["replay_end"]),
        "schema_v2_only": True,
        "legacy_fields_absent": True,
        "exact_cfr": True,
    }
    _write_json_new(result_path, result, "bundle result")
    return result


def _validate_manifest(value: Mapping[str, Any]) -> dict[str, Any]:
    _only_fields(
        value,
        {
            "schema_version",
            "fixture_kind",
            "synthetic",
            "pixel_format",
            "width",
            "height",
            "frame_count",
            "frame_rate",
            "frame_bytes",
            "duration_seconds",
            "visual_encoding",
            "audio",
            "markers",
        },
        "manifest",
    )
    if value.get("schema_version") != SCHEMA_VERSION:
        raise FixtureError("manifest schema_version must be exactly 2")
    if value.get("fixture_kind") != "synthetic-replay-time" or value.get("synthetic") is not True:
        raise FixtureError("manifest must be explicitly labeled synthetic replay-time media")
    if value.get("pixel_format") != "yuv420p":
        raise FixtureError("manifest pixel_format must be yuv420p")
    width = _integer(value.get("width"), "manifest width")
    height = _integer(value.get("height"), "manifest height")
    frame_count = _integer(value.get("frame_count"), "manifest frame_count")
    frame_bytes = _integer(value.get("frame_bytes"), "manifest frame_bytes")
    if (
        width < 160
        or height < 160
        or width > MAX_DIMENSION
        or height > MAX_DIMENSION
        or width % 2
        or height % 2
    ):
        raise FixtureError("manifest dimensions are outside bounded even yuv420p limits")
    if frame_count < 12 or frame_count > MAX_GENERATED_FRAMES:
        raise FixtureError("manifest frame_count is outside its bounded range")
    if frame_bytes != width * height * 3 // 2:
        raise FixtureError("manifest frame_bytes disagrees with yuv420p dimensions")
    rate = _wire_rational(value.get("frame_rate"), "manifest frame_rate")
    duration = _wire_rational(value.get("duration_seconds"), "manifest duration_seconds")
    if duration != Fraction(frame_count, 1) / rate:
        raise FixtureError("manifest duration disagrees with frame count and rate")

    visual = _mapping(value.get("visual_encoding"), "manifest visual_encoding")
    _only_fields(
        visual,
        {
            "version",
            "magic",
            "bit_order",
            "payload",
            "columns",
            "rows",
            "cell_size",
            "cell_x",
            "cell_y",
            "probe_x",
            "probe_y",
            "zero_luma",
            "one_luma",
            "normal_background_luma",
        },
        "manifest visual_encoding",
    )
    expected_visual = {
        "version": 1,
        "magic": f"{VISUAL_MAGIC:04x}",
        "bit_order": "msb-first",
        "payload": "magic16-frame_index32-crc32_low16",
        "columns": VISUAL_COLUMNS,
        "rows": VISUAL_ROWS,
        "zero_luma": ZERO_LUMA,
        "one_luma": ONE_LUMA,
        "normal_background_luma": NORMAL_BACKGROUND_LUMA,
    }
    for name, expected in expected_visual.items():
        if visual.get(name) != expected:
            raise FixtureError(f"manifest visual_encoding.{name} is unsupported")
    cell_size = _integer(visual.get("cell_size"), "manifest visual cell_size")
    cell_x = _integer(visual.get("cell_x"), "manifest visual cell_x")
    cell_y = _integer(visual.get("cell_y"), "manifest visual cell_y")
    probe_x = _integer(visual.get("probe_x"), "manifest visual probe_x")
    probe_y = _integer(visual.get("probe_y"), "manifest visual probe_y")
    if cell_size < 8 or cell_size > 32:
        raise FixtureError("manifest visual cell_size is outside its robust range")
    if (
        cell_x < 0
        or cell_y < 0
        or cell_x + VISUAL_COLUMNS * cell_size > width
        or cell_y + VISUAL_ROWS * cell_size > height
        or not 0 <= probe_x < width
        or not 0 <= probe_y < height
    ):
        raise FixtureError("manifest visual geometry is outside the frame")

    audio = _mapping(value.get("audio"), "manifest audio")
    _only_fields(
        audio,
        {
            "encoding",
            "sample_rate",
            "channels",
            "sample_width_bytes",
            "sample_count",
            "samples_per_frame",
            "impulse_length_samples",
            "impulse_peak_amplitude",
        },
        "manifest audio",
    )
    if (
        audio.get("encoding") != "pcm_s16le"
        or audio.get("sample_rate") != SAMPLE_RATE
        or audio.get("channels") != 1
        or audio.get("sample_width_bytes") != 2
        or audio.get("impulse_length_samples") != IMPULSE_LENGTH_SAMPLES
        or audio.get("impulse_peak_amplitude") != IMPULSE_AMPLITUDE
    ):
        raise FixtureError("manifest audio encoding facts are unsupported")
    samples_per_frame = _integer(
        audio.get("samples_per_frame"), "manifest audio samples_per_frame"
    )
    sample_count = _integer(audio.get("sample_count"), "manifest audio sample_count")
    if Fraction(SAMPLE_RATE, 1) / rate != samples_per_frame:
        raise FixtureError("manifest samples_per_frame disagrees with frame rate")
    if sample_count != frame_count * samples_per_frame:
        raise FixtureError("manifest audio sample_count disagrees with coverage")

    marker_values = _array(value.get("markers"), "manifest markers", 16)
    if len(marker_values) != 3:
        raise FixtureError("manifest must contain exactly early, middle, and late markers")
    markers: list[dict[str, Any]] = []
    expected_markers = _marker_definitions(frame_count, samples_per_frame)
    for index, (raw_marker, expected_marker) in enumerate(zip(marker_values, expected_markers)):
        marker = _mapping(raw_marker, f"manifest marker {index}")
        _only_fields(marker, set(expected_marker), f"manifest marker {index}")
        for name, expected in expected_marker.items():
            if name == "time_seconds":
                if _wire_rational(marker.get(name), f"manifest marker {index}.{name}", positive=False) != _wire_rational(expected, "expected marker time", positive=False):
                    raise FixtureError(f"manifest marker {index} time is inconsistent")
            elif marker.get(name) != expected:
                raise FixtureError(f"manifest marker {index}.{name} is inconsistent")
        markers.append(dict(marker))
    return {
        "width": width,
        "height": height,
        "frame_count": frame_count,
        "frame_bytes": frame_bytes,
        "rate": rate,
        "cell_size": cell_size,
        "cell_x": cell_x,
        "cell_y": cell_y,
        "probe_x": probe_x,
        "probe_y": probe_y,
        "normal_background_luma": visual["normal_background_luma"],
        "samples_per_frame": samples_per_frame,
        "sample_count": sample_count,
        "markers": markers,
    }


def _cell_average(
    y_plane: bytes, width: int, left: int, top: int, cell_size: int
) -> int:
    inset = max(2, cell_size // 4)
    sample_left = left + inset
    sample_top = top + inset
    sample_size = cell_size - 2 * inset
    total = 0
    for y_value in range(sample_top, sample_top + sample_size):
        offset = y_value * width + sample_left
        total += sum(y_plane[offset : offset + sample_size])
    return total // (sample_size * sample_size)


def _decode_visual_word(
    frame: bytes, facts: Mapping[str, Any], offset: int
) -> tuple[int, int]:
    width = int(facts["width"])
    height = int(facts["height"])
    y_plane = frame[: width * height]
    cell_size = int(facts["cell_size"])
    cell_x = int(facts["cell_x"])
    cell_y = int(facts["cell_y"])
    word = 0
    visual_bits = int(facts.get("visual_bits", VISUAL_BITS))
    visual_columns = int(facts.get("visual_columns", VISUAL_COLUMNS))
    for bit_index in range(visual_bits):
        column = bit_index % visual_columns
        row = bit_index // visual_columns
        average = _cell_average(
            y_plane,
            width,
            cell_x + column * cell_size,
            cell_y + row * cell_size,
            cell_size,
        )
        if 90 < average < 160:
            raise FixtureError(
                f"decoded video frame {offset} has ambiguous visual bit cell {bit_index}"
            )
        word = (word << 1) | (1 if average >= 160 else 0)
    probe_x = int(facts["probe_x"])
    probe_y = int(facts["probe_y"])
    probe_width = min(8, width - probe_x)
    probe_height = min(8, height - probe_y)
    total = 0
    for y_value in range(probe_y, probe_y + probe_height):
        start = y_value * width + probe_x
        total += sum(y_plane[start : start + probe_width])
    probe_luma = total // (probe_width * probe_height)
    return word, probe_luma


def _decode_visual_frame(
    frame: bytes, facts: Mapping[str, Any], offset: int
) -> tuple[int, int]:
    word, probe_luma = _decode_visual_word(frame, facts, offset)
    magic = word >> 48
    frame_index = (word >> 16) & 0xFFFF_FFFF
    checksum = word & 0xFFFF
    if magic != VISUAL_MAGIC or checksum != _crc16_for_frame(frame_index):
        raise FixtureError(f"decoded video frame {offset} has an invalid visual payload")
    return frame_index, probe_luma


def _gray_to_binary(gray: int) -> int:
    value = gray
    while gray:
        gray >>= 1
        value ^= gray
    return value


def _native_visual_checksum(payload: int) -> int:
    checksum = NATIVE_VISUAL_CHECKSUM_SEED
    for _ in range(11):
        checksum ^= payload & 0xFF
        payload >>= 8
    return checksum


def _decode_native_visual_frame(
    frame: bytes, facts: Mapping[str, Any], offset: int, expected_generation: int
) -> tuple[str, int | None, int]:
    word, probe_luma = _decode_visual_word(frame, facts, offset)
    magic = word >> 80
    generation = (word >> 64) & 0xFFFF
    state = (word >> 56) & 0xFF
    first = (word >> 32) & NATIVE_VISUAL_TIMESTAMP_MASK
    duplicate = (word >> 8) & NATIVE_VISUAL_TIMESTAMP_MASK
    checksum = word & 0xFF
    if (
        magic != VISUAL_MAGIC
        or generation != expected_generation
        or first != duplicate
        or checksum != _native_visual_checksum(word >> 8)
    ):
        raise FixtureError(
            f"decoded native video frame {offset} has an invalid visual payload"
        )
    timestamp = _gray_to_binary(first)
    if state == NATIVE_VISUAL_PRE_EPOCH_STATE:
        reserved = NATIVE_VISUAL_TIMESTAMP_MASK ^ (
            NATIVE_VISUAL_TIMESTAMP_MASK >> 1
        )
        if first != reserved:
            raise FixtureError(
                f"decoded native video frame {offset} has an invalid pre-epoch word"
            )
        return "pre_epoch", None, probe_luma
    if state != NATIVE_VISUAL_LIVE_STATE:
        raise FixtureError(
            f"decoded native video frame {offset} has an unknown marker state"
        )
    return "live", timestamp, probe_luma


def _read_wave_facts(path: Path, expected_samples: int) -> tuple[wave.Wave_read, BinaryIO]:
    raw = path.open("rb")
    try:
        reader = wave.open(raw, "rb")
        if reader.getnchannels() != 1:
            raise FixtureError("decoded WAV must be mono")
        if reader.getsampwidth() != 2:
            raise FixtureError("decoded WAV must be 16-bit PCM")
        if reader.getframerate() != SAMPLE_RATE:
            raise FixtureError("decoded WAV must use exactly 48000 Hz")
        if reader.getcomptype() != "NONE":
            raise FixtureError("decoded WAV must contain uncompressed PCM")
        actual_samples = reader.getnframes()
        if actual_samples < expected_samples:
            raise FixtureError(
                f"decoded WAV has {actual_samples} samples; expected at least {expected_samples}"
            )
        if actual_samples - expected_samples > MAX_DECODED_AUDIO_TAIL_SAMPLES:
            raise FixtureError(
                "decoded WAV trailing codec padding exceeds the bounded 2048-sample allowance"
            )
        return reader, raw
    except Exception:
        raw.close()
        raise


def _measure_impulse(
    reader: wave.Wave_read, expected_sample: int, marker_name: str
) -> tuple[int, int]:
    search_first = max(0, expected_sample - MAX_AV_DISAGREEMENT_SAMPLES)
    search_last = min(reader.getnframes(), expected_sample + MAX_AV_DISAGREEMENT_SAMPLES + 1)
    if search_last <= search_first:
        raise FixtureError(f"marker {marker_name} has no bounded WAV search interval")
    reader.setpos(search_first)
    payload = reader.readframes(search_last - search_first)
    expected_bytes = (search_last - search_first) * 2
    if len(payload) != expected_bytes:
        raise FixtureError(f"decoded WAV ended while measuring marker {marker_name}")
    peak_value = -1
    peak_sample = -1
    for index in range(search_last - search_first):
        sample = abs(struct.unpack_from("<h", payload, index * 2)[0])
        if sample > peak_value:
            peak_value = sample
            peak_sample = search_first + index
    if peak_value < IMPULSE_THRESHOLD:
        raise FixtureError(f"decoded WAV has no marker impulse for {marker_name}")
    return peak_sample, peak_value


def _read_native_wave_facts(
    path: Path, expected_samples: int
) -> tuple[wave.Wave_read, BinaryIO]:
    raw = path.open("rb")
    try:
        reader = wave.open(raw, "rb")
        if (
            reader.getnchannels() != 1
            or reader.getsampwidth() != 2
            or reader.getframerate() != SAMPLE_RATE
            or reader.getcomptype() != "NONE"
        ):
            raise FixtureError(
                "decoded native marker WAV must be mono 48 kHz signed 16-bit PCM"
            )
        actual_samples = reader.getnframes()
        if (
            abs(actual_samples - expected_samples)
            > MAX_NATIVE_AUDIO_COVERAGE_DELTA_SAMPLES
        ):
            raise FixtureError(
                "decoded native marker WAV coverage differs from the requested "
                "interval by more than 2048 samples"
            )
        return reader, raw
    except Exception:
        raw.close()
        raise


def command_verify_media(arguments: argparse.Namespace) -> dict[str, Any]:
    video_path = _existing_file(arguments.video, "decoded yuv420p", MAX_RAW_VIDEO_BYTES)
    audio_path = _existing_file(arguments.audio, "decoded WAV", MAX_WAV_BYTES)
    manifest_path = _existing_file(arguments.manifest, "manifest JSON", MAX_MANIFEST_BYTES)
    result_path = _new_file(arguments.result, "media result")
    manifest = _read_json(manifest_path, "manifest JSON", MAX_MANIFEST_BYTES)
    facts = _validate_manifest(manifest)
    expected_start = _integer(arguments.expected_start_frame, "expected start frame")
    expected_count = _integer(arguments.expected_frame_count, "expected frame count")
    if expected_start < 0 or expected_count <= 0:
        raise FixtureError("expected frame interval must have a nonnegative start and positive count")
    if expected_count > MAX_FRAMES or expected_start + expected_count > facts["frame_count"]:
        raise FixtureError("expected frame interval is outside bounded manifest coverage")
    expected_video_bytes = expected_count * facts["frame_bytes"]
    if video_path.stat().st_size != expected_video_bytes:
        raise FixtureError(
            f"decoded video has {video_path.stat().st_size} bytes; expected {expected_video_bytes}"
        )

    marker_by_frame = {
        int(marker["frame_index"]): marker
        for marker in facts["markers"]
        if expected_start <= int(marker["frame_index"]) < expected_start + expected_count
    }
    visual_measurements: dict[int, int] = {}
    first_decoded_id: int | None = None
    last_decoded_id: int | None = None
    with video_path.open("rb") as video:
        for offset in range(expected_count):
            frame = video.read(facts["frame_bytes"])
            if len(frame) != facts["frame_bytes"]:
                raise FixtureError(f"decoded video ended at frame offset {offset}")
            frame_id, probe_luma = _decode_visual_frame(frame, facts, offset)
            wanted = expected_start + offset
            if frame_id != wanted:
                raise FixtureError(
                    f"decoded video frame offset {offset} carries source id {frame_id}; expected {wanted}"
                )
            if first_decoded_id is None:
                first_decoded_id = frame_id
            last_decoded_id = frame_id
            if frame_id in marker_by_frame:
                visual_measurements[frame_id] = probe_luma
        if video.read(1):
            raise FixtureError("decoded video contains bytes after the requested frame interval")

    expected_audio_samples = expected_count * facts["samples_per_frame"]
    reader, raw_audio = _read_wave_facts(audio_path, expected_audio_samples)
    decoded_audio_samples = reader.getnframes()
    measurements: list[dict[str, Any]] = []
    try:
        for frame_id in sorted(marker_by_frame):
            marker = marker_by_frame[frame_id]
            flash_threshold = (
                int(facts["normal_background_luma"]) + int(marker["flash_luma"])
            ) // 2
            measured_luma = visual_measurements.get(frame_id)
            if measured_luma is None or measured_luma < flash_threshold:
                raise FixtureError(f"decoded video marker flash {marker['name']} is missing")
            rebased_sample = (frame_id - expected_start) * facts["samples_per_frame"]
            impulse_sample, impulse_peak = _measure_impulse(
                reader, rebased_sample, str(marker["name"])
            )
            disagreement = rebased_sample - impulse_sample
            if abs(disagreement) > MAX_AV_DISAGREEMENT_SAMPLES:
                raise FixtureError(
                    f"marker {marker['name']} exceeds 50 ms A/V disagreement"
                )
            measurements.append(
                {
                    "name": marker["name"],
                    "source_frame_index": frame_id,
                    "decoded_frame_offset": frame_id - expected_start,
                    "visual_rebased_sample": rebased_sample,
                    "audio_impulse_sample": impulse_sample,
                    "audio_impulse_peak": impulse_peak,
                    "av_disagreement_samples": disagreement,
                    "av_disagreement_ms": _fraction_wire(
                        Fraction(disagreement * 1000, SAMPLE_RATE)
                    ),
                    "measured_flash_luma": measured_luma,
                }
            )
    finally:
        reader.close()
        raw_audio.close()

    if measurements:
        early_disagreement = int(measurements[0]["av_disagreement_samples"])
        for measurement in measurements:
            drift_from_early = (
                int(measurement["av_disagreement_samples"]) - early_disagreement
            )
            measurement["drift_from_early_samples"] = drift_from_early
            measurement["drift_from_early_ms"] = _fraction_wire(
                Fraction(drift_from_early * 1000, SAMPLE_RATE)
            )
            if abs(drift_from_early) > MAX_DRIFT_GROWTH_SAMPLES:
                raise FixtureError(
                    "marker A/V disagreement changes beyond the 5 ms drift bound"
                )

    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "verify-media",
        "passed": True,
        "synthetic": True,
        "expected_start_frame": expected_start,
        "expected_frame_count": expected_count,
        "first_decoded_source_frame": first_decoded_id,
        "last_decoded_source_frame": last_decoded_id,
        "decoded_video_bytes": expected_video_bytes,
        "expected_audio_samples": expected_audio_samples,
        "decoded_audio_samples": decoded_audio_samples,
        "ignored_trailing_audio_samples": decoded_audio_samples - expected_audio_samples,
        "markers_in_interval": len(measurements),
        "measurements": measurements,
        "maximum_av_disagreement_ms": 50,
        "maximum_drift_growth_ms": 5,
    }
    _write_json_new(result_path, result, "media result")
    return result

def _sha256_existing_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _native_video_pts(
    probe: Mapping[str, Any],
) -> tuple[Fraction, list[int]]:
    streams = _array(probe.get("streams"), "native marker probe streams", MAX_STREAMS)
    if len(streams) != 1:
        raise FixtureError("native marker probe must contain exactly one video stream")
    stream = _mapping(streams[0], "native marker probe video stream")
    if stream.get("codec_type") != "video":
        raise FixtureError("native marker probe stream must be video")
    time_base = _parse_fraction_text(
        stream.get("time_base"), "native marker video time_base"
    )

    frames = _array(probe.get("frames"), "native marker probe frames", MAX_FRAMES)
    if len(frames) < 12:
        raise FixtureError("native marker probe contains fewer than 12 video frames")
    pts_values: list[int] = []
    for index, raw_frame in enumerate(frames):
        frame = _mapping(raw_frame, f"native marker probe frame {index}")
        if frame.get("media_type") != "video":
            raise FixtureError(f"native marker probe frame {index} is not video")
        pts = _probe_integer(
            frame.get("best_effort_timestamp"),
            f"native marker probe frame {index} best_effort_timestamp",
        )
        if not MIN_I64 <= pts <= MAX_I64:
            raise FixtureError(
                f"native marker probe frame {index} PTS is outside signed i64"
            )
        if pts_values and pts <= pts_values[-1]:
            raise FixtureError(
                "native marker probe video PTS must be strictly presentation-ordered"
            )
        pts_values.append(pts)
    return time_base, pts_values


def command_verify_native_recorder_media(
    arguments: argparse.Namespace,
) -> dict[str, Any]:
    source_media_path = _existing_file(
        arguments.source_media, "native marker source media", MAX_BUNDLE_MEDIA_BYTES
    )
    video_path = _existing_file(
        arguments.video, "decoded native marker gray video", MAX_RAW_VIDEO_BYTES
    )
    audio_path = _existing_file(
        arguments.audio, "decoded native marker WAV", MAX_WAV_BYTES
    )
    probe_path = _existing_file(
        arguments.probe, "native marker frame probe JSON", MAX_PROBE_JSON_BYTES
    )
    manifest_path = _new_file(arguments.manifest, "native marker manifest")
    result_path = _new_file(arguments.result, "native marker media result")
    duration_seconds = _integer(
        arguments.duration_seconds, "native marker duration seconds"
    )
    if duration_seconds < 2 or duration_seconds > 7200:
        raise FixtureError(
            "native marker duration seconds must be between 2 and 7200"
        )
    expected_generation = _integer(
        arguments.expected_generation, "native marker expected generation"
    )
    if not 0 <= expected_generation <= 0xFFFF:
        raise FixtureError("native marker expected generation must fit u16")

    probe = _read_json(
        probe_path, "native marker frame probe JSON", MAX_PROBE_JSON_BYTES
    )
    time_base, pts_values = _native_video_pts(probe)

    width = 320
    height = 240
    frame_bytes = width * height
    video_bytes = video_path.stat().st_size
    if video_bytes % frame_bytes:
        raise FixtureError("decoded native marker video ends inside a gray frame")
    decoded_frame_count = video_bytes // frame_bytes
    if decoded_frame_count < 12 or decoded_frame_count > MAX_FRAMES:
        raise FixtureError("decoded native marker video frame count is outside its bound")
    if decoded_frame_count != len(pts_values):
        raise FixtureError(
            "decoded native marker video frame count disagrees with frame PTS evidence"
        )

    visual_tick_rate = 1000
    source_tick_count = duration_seconds * visual_tick_rate
    flash_half_width_ticks = 25
    marker_names = ("early", "middle", "late")
    marker_timestamps = (
        source_tick_count // 6,
        source_tick_count // 2,
        source_tick_count * 5 // 6,
    )
    marker_lumas = (112, 152, 192)
    markers = [
        {
            "name": name,
            "timestamp_ms": timestamp,
            "sample_index": timestamp * SAMPLE_RATE // visual_tick_rate,
            "time_seconds": _fraction_wire(Fraction(timestamp, visual_tick_rate)),
            "flash_first_timestamp_ms": timestamp - flash_half_width_ticks,
            "flash_duration_ms": flash_half_width_ticks * 2 + 1,
            "flash_luma": luma,
        }
        for name, timestamp, luma in zip(
            marker_names, marker_timestamps, marker_lumas
        )
    ]
    facts = {
        "width": width,
        "height": height,
        "frame_bytes": frame_bytes,
        "cell_size": 16,
        "cell_x": 16,
        "cell_y": 16,
        "probe_x": 240,
        "probe_y": 16,
        "normal_background_luma": NORMAL_BACKGROUND_LUMA,
        "visual_bits": NATIVE_VISUAL_BITS,
        "visual_columns": NATIVE_VISUAL_COLUMNS,
    }

    visual_records: list[tuple[int, int, int, int]] = []
    pre_epoch_frame_count = 0
    first_live_output_frame: int | None = None
    first_live_timestamp: int | None = None
    first_live_pts: int | None = None
    last_live_timestamp: int | None = None
    duplicate_visual_timestamps = 0
    maximum_visual_timestamp_step = 0
    invalid_pre_epoch_payload_frames = 0
    invalid_post_epoch_payload_frames = 0
    with video_path.open("rb") as video:
        for output_offset, pts in enumerate(pts_values):
            frame = video.read(frame_bytes)
            if len(frame) != frame_bytes:
                raise FixtureError(
                    f"decoded native marker video ended at frame offset {output_offset}"
                )
            try:
                state, visual_timestamp, probe_luma = _decode_native_visual_frame(
                    frame, facts, output_offset, expected_generation
                )
            except FixtureError as error:
                if first_live_output_frame is None:
                    invalid_pre_epoch_payload_frames += 1
                    phase = "before the shared epoch"
                else:
                    invalid_post_epoch_payload_frames += 1
                    phase = "after the shared epoch"
                raise FixtureError(
                    f"decoded native marker video has an invalid payload {phase} "
                    f"at frame {output_offset}; the strict allowance is zero"
                ) from error
            if state == "pre_epoch":
                if first_live_output_frame is not None:
                    raise FixtureError(
                        "decoded native marker video returned to pre-epoch state"
                    )
                pre_epoch_frame_count += 1
                continue
            if visual_timestamp is None:
                raise FixtureError("native live marker unexpectedly lacks a timestamp")
            if first_live_output_frame is None:
                first_live_pts = pts
                first_live_output_frame = output_offset
                first_live_timestamp = visual_timestamp
            if last_live_timestamp is not None:
                if visual_timestamp < last_live_timestamp:
                    raise FixtureError(
                        "decoded native marker timestamps moved backward"
                    )
                step = visual_timestamp - last_live_timestamp
                maximum_visual_timestamp_step = max(
                    maximum_visual_timestamp_step, step
                )
                if step == 0:
                    duplicate_visual_timestamps += 1
            last_live_timestamp = visual_timestamp
            visual_records.append(
                (output_offset, visual_timestamp, probe_luma, pts)
            )
        if video.read(1):
            raise FixtureError("decoded native marker video has trailing partial data")
    if (
        first_live_output_frame is None
        or first_live_timestamp is None
        or first_live_pts is None
        or len(visual_records) < 4
    ):
        raise FixtureError("decoded native marker video lacks live epoch coverage")
    video_epoch_time = first_live_pts * time_base - Fraction(
        first_live_timestamp, visual_tick_rate
    )
    if video_epoch_time > first_live_pts * time_base:
        raise FixtureError("native marker epoch resolves after the first live frame")

    expected_audio_samples = duration_seconds * SAMPLE_RATE
    reader, raw_audio = _read_native_wave_facts(audio_path, expected_audio_samples)
    measurements: list[dict[str, Any]] = []
    try:
        for marker in markers:
            name = str(marker["name"])
            center = int(marker["timestamp_ms"])

            lower: tuple[int, int, int] | None = None
            upper: tuple[int, int, int] | None = None
            for offset, timestamp, _luma, pts in visual_records:
                if timestamp < center:
                    lower = (offset, timestamp, pts)
                    continue
                if timestamp > center:
                    upper = (offset, timestamp, pts)
                    break
            if lower is None or upper is None:
                raise FixtureError(
                    f"decoded native marker {name} lacks a visual timestamp bracket"
                )
            lower_offset, lower_timestamp, lower_pts = lower
            upper_offset, upper_timestamp, upper_pts = upper
            timestamp_span = upper_timestamp - lower_timestamp
            if timestamp_span <= 0:
                raise FixtureError(
                    f"decoded native marker {name} has an invalid timestamp bracket"
                )
            lower_time = lower_pts * time_base
            upper_time = upper_pts * time_base
            visual_time_exact = lower_time + (
                Fraction(center - lower_timestamp, timestamp_span)
                * (upper_time - lower_time)
            )
            visual_sample_exact = (visual_time_exact - video_epoch_time) * SAMPLE_RATE
            if visual_sample_exact < 0:
                raise FixtureError(
                    f"decoded native marker {name} resolves before audio sample zero"
                )
            visual_sample = (
                2 * visual_sample_exact.numerator + visual_sample_exact.denominator
            ) // (2 * visual_sample_exact.denominator)
            impulse_sample, impulse_peak = _measure_impulse(
                reader, visual_sample, name
            )
            disagreement = visual_sample - impulse_sample
            if abs(disagreement) > MAX_AV_DISAGREEMENT_SAMPLES:
                raise FixtureError(
                    f"native marker {name} exceeds 50 ms A/V disagreement"
                )
            measurements.append(
                {
                    "name": name,
                    "source_timestamp_ms": center,
                    "source_sample": int(marker["sample_index"]),
                    "visual_lower_output_frame": lower_offset,
                    "visual_lower_timestamp_ms": lower_timestamp,
                    "visual_lower_pts": str(lower_pts),
                    "visual_lower_time_seconds": _fraction_wire(lower_time),
                    "visual_upper_output_frame": upper_offset,
                    "visual_upper_timestamp_ms": upper_timestamp,
                    "visual_rebased_time_seconds": _fraction_wire(
                        visual_time_exact - video_epoch_time
                    ),
                    "visual_upper_pts": str(upper_pts),
                    "visual_upper_time_seconds": _fraction_wire(upper_time),
                    "visual_sample": visual_sample,
                    "visual_sample_exact": _fraction_wire(visual_sample_exact),
                    "audio_impulse_sample": impulse_sample,
                    "audio_impulse_peak": impulse_peak,
                    "av_disagreement_samples": disagreement,
                    "av_disagreement_ms": _fraction_wire(
                        Fraction(disagreement * 1000, SAMPLE_RATE)
                    ),
                }
            )
        decoded_audio_samples = reader.getnframes()
    finally:
        reader.close()
        raw_audio.close()
    early_disagreement = int(measurements[0]["av_disagreement_samples"])
    maximum_observed_drift = 0
    for measurement in measurements:
        drift_from_early = (
            int(measurement["av_disagreement_samples"]) - early_disagreement
        )
        measurement["drift_from_early_samples"] = drift_from_early
        measurement["drift_from_early_ms"] = _fraction_wire(
            Fraction(drift_from_early * 1000, SAMPLE_RATE)
        )
        maximum_observed_drift = max(
            maximum_observed_drift, abs(drift_from_early)
        )


    input_files = {
        "source_media": {
            "path": str(source_media_path),
            "bytes": source_media_path.stat().st_size,
            "sha256": _sha256_existing_file(source_media_path),
        },
        "decoded_marker_video": {
            "path": str(video_path),
            "bytes": video_bytes,
            "sha256": _sha256_existing_file(video_path),
            "pixel_format": "gray8",
            "width": width,
            "height": height,
        },
        "decoded_audio": {
            "path": str(audio_path),
            "bytes": audio_path.stat().st_size,
            "sha256": _sha256_existing_file(audio_path),
            "encoding": "pcm_s16le",
            "channels": 1,
            "sample_rate": SAMPLE_RATE,
        },
        "frame_probe": {
            "path": str(probe_path),
            "bytes": probe_path.stat().st_size,
            "sha256": _sha256_existing_file(probe_path),
        },
    }
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "fixture_kind": "native-wgc-replay-time",
        "synthetic": False,
        "duration_seconds": duration_seconds,
        "generation": expected_generation,
        "input_files": input_files,
        "visual_encoding": {
            "version": 3,
            "magic": f"{VISUAL_MAGIC:04x}",
            "bit_order": "msb-first",
            "payload": (
                "magic16-generation16-state8-gray_elapsed_milliseconds24-"
                "duplicate24-xor_checksum8"
            ),
            "clock_epoch": "loopback_audio_sample_zero_commit",
            "clock_rate_hz": visual_tick_rate,
            "maximum_timestamp_ms": NATIVE_VISUAL_TIMESTAMP_MASK,
            "pre_epoch_state": NATIVE_VISUAL_PRE_EPOCH_STATE,
            "live_state": NATIVE_VISUAL_LIVE_STATE,
            "checksum_seed": NATIVE_VISUAL_CHECKSUM_SEED,
            "columns": NATIVE_VISUAL_COLUMNS,
            "rows": NATIVE_VISUAL_ROWS,
            "cell_size": 16,
            "cell_x": 16,
            "cell_y": 16,
            "probe_x": 240,
            "probe_y": 16,
            "normal_background_luma": NORMAL_BACKGROUND_LUMA,
            "atomic_publication": "single StretchDIBits call per complete word",
        },
        "video_timing": {
            "time_base": _fraction_wire(time_base),
            "decoded_frame_count": decoded_frame_count,
            "first_pts": str(pts_values[0]),
            "last_pts": str(pts_values[-1]),
            "authority": "ffprobe best_effort_timestamp in presentation order",
            "epoch_anchor_output_frame": first_live_output_frame,
            "epoch_anchor_timestamp_ms": first_live_timestamp,
            "epoch_anchor_pts": str(first_live_pts),
            "epoch_time_seconds": _fraction_wire(video_epoch_time),
        },
        "audio": {
            "encoding": "pcm_s16le",
            "sample_rate": SAMPLE_RATE,
            "source_channels": 2,
            "decoded_channels": 1,
            "impulse_length_samples": IMPULSE_LENGTH_SAMPLES,
            "impulse_peak_amplitude": IMPULSE_AMPLITUDE,
        },
        "markers": markers,
        "validation_contract": {
            "invalid_post_epoch_payload_frame_limit": 0,
            "maximum_av_disagreement_samples": MAX_AV_DISAGREEMENT_SAMPLES,
            "maximum_audio_coverage_delta_samples": (
                MAX_NATIVE_AUDIO_COVERAGE_DELTA_SAMPLES
            ),
            "visual_time_authority": "decoded video presentation PTS",
            "capture_latency_identifiable": False,
            "drift_gate_applied": False,
            "strict_drift_authority": "native-post-capture-mux-replay-time",
        },
    }
    _write_json_new(manifest_path, manifest, "native marker manifest")

    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "verify-native-recorder-media",
        "passed": True,
        "synthetic": False,
        "fixture_kind": "native-wgc-replay-time",
        "marker_duration_seconds": duration_seconds,
        "expected_generation": expected_generation,
        "decoded_frame_count": decoded_frame_count,
        "decoded_audio_samples": decoded_audio_samples,
        "expected_audio_samples": expected_audio_samples,
        "decoded_audio_coverage_delta_samples": (
            decoded_audio_samples - expected_audio_samples
        ),
        "probe_time_base": _fraction_wire(time_base),
        "first_video_pts": str(pts_values[0]),
        "last_video_pts": str(pts_values[-1]),
        "video_epoch_time_seconds": _fraction_wire(video_epoch_time),
        "pre_epoch_frame_count": pre_epoch_frame_count,
        "first_live_output_frame": first_live_output_frame,
        "first_live_timestamp_ms": first_live_timestamp,
        "last_live_timestamp_ms": last_live_timestamp,
        "duplicate_visual_timestamps": duplicate_visual_timestamps,
        "maximum_visual_timestamp_step_ms": maximum_visual_timestamp_step,
        "invalid_pre_epoch_payload_frames": invalid_pre_epoch_payload_frames,
        "invalid_post_epoch_payload_frames": invalid_post_epoch_payload_frames,
        "invalid_post_epoch_payload_frame_limit": 0,
        "markers_in_interval": len(measurements),
        "measurements": measurements,
        "maximum_av_disagreement_ms": 50,
        "maximum_observed_drift_samples": maximum_observed_drift,
        "capture_latency_identifiable": False,
        "drift_gate_applied": False,
        "timing_claim": "diagnostic-coarse-wgc-alignment-only",
        "strict_drift_authority": "native-post-capture-mux-replay-time",
        "input_files": input_files,
        "manifest": str(manifest_path),
    }
    _write_json_new(result_path, result, "native marker media result")
    return result


def _nearest_fraction_integer(value: Fraction) -> int:
    return (2 * value.numerator + value.denominator) // (2 * value.denominator)


def command_verify_native_mux_media(arguments: argparse.Namespace) -> dict[str, Any]:
    source_media_path = _existing_file(
        arguments.source_media, "native mux source media", MAX_BUNDLE_MEDIA_BYTES
    )
    video_path = _existing_file(
        arguments.video, "decoded native mux video", MAX_RAW_VIDEO_BYTES
    )
    audio_path = _existing_file(
        arguments.audio, "decoded native mux audio", MAX_WAV_BYTES
    )
    probe_path = _existing_file(
        arguments.probe, "native mux frame probe JSON", MAX_PROBE_JSON_BYTES
    )
    manifest_path = _existing_file(
        arguments.manifest, "native mux source manifest", MAX_MANIFEST_BYTES
    )
    result_path = _new_file(arguments.result, "native mux media result")
    input_timestamp_mode = _string(
        arguments.input_timestamp_mode,
        "native mux input timestamp mode",
        nonempty=True,
    )
    if input_timestamp_mode not in {"cfr", "wallclock"}:
        raise FixtureError(
            "native mux input timestamp mode must be exactly cfr or wallclock"
        )

    manifest = _read_json(
        manifest_path, "native mux source manifest", MAX_MANIFEST_BYTES
    )
    facts = _validate_manifest(manifest)
    if facts["rate"] != Fraction(60, 1):
        raise FixtureError("native mux source manifest must use exactly 60/1 FPS")

    probe = _read_json(
        probe_path, "native mux frame probe JSON", MAX_PROBE_JSON_BYTES
    )
    time_base, pts_values = _native_video_pts(probe)
    if len(pts_values) != facts["frame_count"]:
        raise FixtureError(
            "native mux decoded video frame count disagrees with source manifest"
        )
    frame_step_exact = Fraction(1, 1) / (facts["rate"] * time_base)
    if frame_step_exact.denominator != 1 or frame_step_exact.numerator <= 0:
        raise FixtureError(
            "native mux video time base cannot represent the exact declared frame rate"
        )
    frame_step = frame_step_exact.numerator
    if pts_values[0] != 0 or any(
        pts != index * frame_step for index, pts in enumerate(pts_values)
    ):
        observed_steps = sorted(
            {later - earlier for earlier, later in zip(pts_values, pts_values[1:])}
        )
        raise FixtureError(
            "native mux finalized video grid is not exact zero-based 60 Hz: "
            f"first={pts_values[0]} steps={observed_steps} expected={frame_step}"
        )
    exact_video_grid = {
        "frame_rate": _fraction_wire(facts["rate"]),
        "time_base": _fraction_wire(time_base),
        "first_pts": "0",
        "last_pts": str(pts_values[-1]),
        "step_pts": str(frame_step),
        "frame_count": len(pts_values),
        "one_past_last_pts": str(len(pts_values) * frame_step),
        "passed": True,
    }
    expected_video_bytes = facts["frame_count"] * facts["frame_bytes"]
    if video_path.stat().st_size != expected_video_bytes:
        raise FixtureError(
            "decoded native mux video byte count disagrees with source manifest"
        )

    marker_by_frame = {
        int(marker["frame_index"]): marker for marker in facts["markers"]
    }
    marker_pts: dict[int, int] = {}
    marker_lumas: dict[int, int] = {}
    with video_path.open("rb") as video:
        for output_offset, pts in enumerate(pts_values):
            frame = video.read(facts["frame_bytes"])
            if len(frame) != facts["frame_bytes"]:
                raise FixtureError(
                    f"decoded native mux video ended at frame {output_offset}"
                )
            frame_id, probe_luma = _decode_visual_frame(frame, facts, output_offset)
            if frame_id != output_offset:
                raise FixtureError(
                    "decoded native mux frame identity does not match presentation "
                    f"order at frame {output_offset}: found {frame_id}"
                )
            if frame_id in marker_by_frame:
                marker_pts[frame_id] = pts
                marker_lumas[frame_id] = probe_luma
        if video.read(1):
            raise FixtureError("decoded native mux video has trailing partial data")

    reader, raw_audio = _read_native_wave_facts(
        audio_path, facts["sample_count"]
    )
    measurements: list[dict[str, Any]] = []
    early_disagreement_exact: Fraction | None = None
    try:
        for marker in facts["markers"]:
            name = str(marker["name"])
            frame_id = int(marker["frame_index"])
            pts = marker_pts.get(frame_id)
            measured_luma = marker_lumas.get(frame_id)
            if pts is None or measured_luma is None:
                raise FixtureError(
                    f"decoded native mux video lacks marker frame {frame_id}"
                )
            flash_threshold = (
                int(facts["normal_background_luma"]) + int(marker["flash_luma"])
            ) // 2
            if measured_luma < flash_threshold:
                raise FixtureError(
                    f"decoded native mux video marker flash {name} is missing"
                )

            expected_audio_sample = int(marker["sample_index"])
            audio_sample, audio_peak = _measure_impulse(
                reader, expected_audio_sample, name
            )
            video_time_exact = pts * time_base
            video_sample_exact = video_time_exact * SAMPLE_RATE
            disagreement_exact = video_sample_exact - audio_sample
            if abs(disagreement_exact) > MAX_AV_DISAGREEMENT_SAMPLES:
                raise FixtureError(
                    f"native mux marker {name} exceeds 50 ms A/V disagreement"
                )
            if early_disagreement_exact is None:
                early_disagreement_exact = disagreement_exact
            drift_from_early_exact = disagreement_exact - early_disagreement_exact
            if abs(drift_from_early_exact) > MAX_DRIFT_GROWTH_SAMPLES:
                raise FixtureError(
                    "native mux A/V disagreement changes beyond the 5 ms drift bound"
                )

            measurements.append(
                {
                    "name": name,
                    "source_frame_index": frame_id,
                    "decoded_video_pts": str(pts),
                    "decoded_video_time_seconds": _fraction_wire(video_time_exact),
                    "decoded_video_sample_exact": _fraction_wire(video_sample_exact),
                    "decoded_video_sample": _nearest_fraction_integer(
                        video_sample_exact
                    ),
                    "expected_audio_sample": expected_audio_sample,
                    "decoded_audio_impulse_sample": audio_sample,
                    "decoded_audio_impulse_peak": audio_peak,
                    "av_disagreement_samples_exact": _fraction_wire(
                        disagreement_exact
                    ),
                    "av_disagreement_samples": _nearest_fraction_integer(
                        disagreement_exact
                    ),
                    "av_disagreement_ms": _fraction_wire(
                        disagreement_exact * 1000 / SAMPLE_RATE
                    ),
                    "drift_from_early_samples_exact": _fraction_wire(
                        drift_from_early_exact
                    ),
                    "drift_from_early_samples": _nearest_fraction_integer(
                        drift_from_early_exact
                    ),
                    "drift_from_early_ms": _fraction_wire(
                        drift_from_early_exact * 1000 / SAMPLE_RATE
                    ),
                    "measured_flash_luma": measured_luma,
                }
            )
        decoded_audio_samples = reader.getnframes()
    finally:
        reader.close()
        raw_audio.close()

    maximum_drift_exact = max(
        (
            abs(
                _wire_rational(
                    measurement["drift_from_early_samples_exact"],
                    "native mux drift measurement",
                    positive=False,
                )
            )
            for measurement in measurements
        ),
        default=Fraction(0, 1),
    )
    input_files = {
        "source_media": {
            "path": str(source_media_path),
            "bytes": source_media_path.stat().st_size,
            "sha256": _sha256_existing_file(source_media_path),
        },
        "decoded_video": {
            "path": str(video_path),
            "bytes": expected_video_bytes,
            "sha256": _sha256_existing_file(video_path),
            "pixel_format": "yuv420p",
        },
        "decoded_audio": {
            "path": str(audio_path),
            "bytes": audio_path.stat().st_size,
            "sha256": _sha256_existing_file(audio_path),
            "encoding": "pcm_s16le",
            "channels": 1,
            "sample_rate": SAMPLE_RATE,
        },
        "frame_probe": {
            "path": str(probe_path),
            "bytes": probe_path.stat().st_size,
            "sha256": _sha256_existing_file(probe_path),
        },
        "source_manifest": {
            "path": str(manifest_path),
            "bytes": manifest_path.stat().st_size,
            "sha256": _sha256_existing_file(manifest_path),
        },
    }
    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "verify-native-mux-media",
        "passed": True,
        "synthetic": True,
        "fixture_kind": "native-post-capture-mux-replay-time",
        "input_timestamp_mode": input_timestamp_mode,
        "decoded_frame_count": len(pts_values),
        "decoded_audio_samples": decoded_audio_samples,
        "expected_audio_samples": facts["sample_count"],
        "decoded_audio_coverage_delta_samples": (
            decoded_audio_samples - facts["sample_count"]
        ),
        "probe_time_base": _fraction_wire(time_base),
        "first_video_pts": str(pts_values[0]),
        "last_video_pts": str(pts_values[-1]),
        "exact_video_grid": exact_video_grid,
        "markers_in_interval": len(measurements),
        "measurements": measurements,
        "maximum_observed_drift_samples_exact": _fraction_wire(
            maximum_drift_exact
        ),
        "maximum_observed_drift_samples": _nearest_fraction_integer(
            maximum_drift_exact
        ),
        "maximum_av_disagreement_ms": 50,
        "maximum_drift_growth_ms": 5,
        "capture_path_in_scope": False,
        "timing_authority": (
            "decoded frame identity joined to decoded video presentation PTS "
            "and decoded audio impulse sample"
        ),
        "input_files": input_files,
    }
    _write_json_new(result_path, result, "native mux media result")
    return result


def _copy_new(source: Path, destination: Path, maximum_bytes: int) -> None:
    if os.path.lexists(destination):
        raise FixtureError(f"negative fixture target already exists: {destination}")
    size = source.stat().st_size
    if size > maximum_bytes:
        raise FixtureError(f"input {source.name} exceeds its {maximum_bytes}-byte copy limit")
    with source.open("rb") as input_file, destination.open("xb") as output_file:
        shutil.copyfileobj(input_file, output_file, length=1024 * 1024)
        output_file.flush()
        os.fsync(output_file.fileno())
    if destination.stat().st_size != size:
        raise FixtureError(f"copy size mismatch for {destination}")


def _copy_prefix_new(source: Path, destination: Path, byte_count: int) -> None:
    if os.path.lexists(destination):
        raise FixtureError(f"negative fixture target already exists: {destination}")
    with source.open("rb") as input_file, destination.open("xb") as output_file:
        remaining = byte_count
        while remaining:
            chunk = input_file.read(min(remaining, 64 * 1024))
            if not chunk:
                break
            output_file.write(chunk)
            remaining -= len(chunk)
        output_file.flush()
        os.fsync(output_file.fileno())


def _negative_case_record(
    name: str, directory: Path, root: Path, artifacts: Mapping[str, Path | None]
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "name": name,
        "relative_directory": directory.relative_to(root).as_posix(),
        "directory": str(directory),
    }
    for field, path in artifacts.items():
        record[field] = str(path) if path is not None else None
        record[f"relative_{field}"] = (
            path.relative_to(root).as_posix() if path is not None else None
        )
    return record


def command_make_negative(arguments: argparse.Namespace) -> dict[str, Any]:
    bundle = _existing_directory(arguments.bundle, "schema-v2 bundle")
    root = _new_root(arguments.root, "negative root")
    result_path = _normalized_path(arguments.result, "negative result")
    if _strictly_below(root, bundle) or _strictly_below(bundle, root):
        raise FixtureError("negative root and source bundle must not contain one another")
    if not _strictly_below(result_path, root):
        result_path = _new_file(result_path, "negative result")
    elif os.path.lexists(result_path):
        raise FixtureError(f"negative result already exists: {result_path}")

    video = _existing_file(bundle / "video.mp4", "bundle video", MAX_BUNDLE_MEDIA_BYTES)
    metadata_path = _existing_file(
        bundle / "metadata.json", "bundle metadata", MAX_BUNDLE_JSON_BYTES
    )
    game_log_path = _existing_file(
        bundle / "game_log.json", "bundle game log", MAX_BUNDLE_JSON_BYTES
    )
    metadata = _read_json(metadata_path, "bundle metadata", MAX_BUNDLE_JSON_BYTES)
    game_log = _read_json(game_log_path, "bundle game log", MAX_BUNDLE_JSON_BYTES)
    source_media_id = _media_id(metadata.get("media_id"), "bundle metadata media_id")
    _validate_bundle_documents(metadata, game_log, source_media_id)

    case_names = (
        "schema-v1",
        "malformed-v2",
        "identity-mismatch",
        "publication-failure",
        "crash-window",
    )
    if len(case_names) + 1 > MAX_CREATED_FILES:
        raise FixtureError("negative fixture case count exceeds its bound")
    root.mkdir()
    case_directories: dict[str, Path] = {}
    for name in case_names:
        directory = root / name
        if not _strictly_below(directory, root):
            raise FixtureError(f"negative case {name} is not strictly beneath its fresh root")
        directory.mkdir()
        case_directories[name] = directory

    schema_v1 = case_directories["schema-v1"]
    _copy_new(video, schema_v1 / "video.mp4", MAX_BUNDLE_MEDIA_BYTES)
    schema_v1_metadata = deepcopy(metadata)
    schema_v1_metadata["schema_version"] = 1
    schema_v1_metadata["media_timeline"]["schema_version"] = 1
    schema_v1_game_log = deepcopy(game_log)
    schema_v1_game_log["schema_version"] = 1
    _write_json_new(schema_v1 / "metadata.json", schema_v1_metadata, "schema-v1 metadata")
    _write_json_new(schema_v1 / "game_log.json", schema_v1_game_log, "schema-v1 game log")

    malformed_v2 = case_directories["malformed-v2"]
    _copy_new(video, malformed_v2 / "video.mp4", MAX_BUNDLE_MEDIA_BYTES)
    malformed_metadata = deepcopy(metadata)
    malformed_metadata["media_timeline"]["video"]["frame_rate"] = {
        "numerator": "060",
        "denominator": "1",
    }
    _write_json_new(
        malformed_v2 / "metadata.json", malformed_metadata, "malformed-v2 metadata"
    )
    _copy_new(game_log_path, malformed_v2 / "game_log.json", MAX_BUNDLE_JSON_BYTES)

    identity_mismatch = case_directories["identity-mismatch"]
    _copy_new(video, identity_mismatch / "video.mp4", MAX_BUNDLE_MEDIA_BYTES)
    _copy_new(metadata_path, identity_mismatch / "metadata.json", MAX_BUNDLE_JSON_BYTES)
    mismatch_id = "00000000-0000-4000-8000-000000000001"
    if mismatch_id == source_media_id:
        mismatch_id = "00000000-0000-4000-8000-000000000002"
    mismatch_game_log = deepcopy(game_log)
    mismatch_game_log["media_id"] = mismatch_id
    _write_json_new(
        identity_mismatch / "game_log.json",
        mismatch_game_log,
        "identity-mismatch game log",
    )

    publication_failure = case_directories["publication-failure"]
    partial_video = publication_failure / "video.mp4.partial"
    _copy_prefix_new(video, partial_video, min(video.stat().st_size, 64 * 1024))
    pending_metadata = publication_failure / "metadata.json.pending"
    _copy_new(metadata_path, pending_metadata, MAX_BUNDLE_JSON_BYTES)

    crash_window = case_directories["crash-window"]
    pending_directory = crash_window / "pending"
    pending_directory.mkdir()
    crash_metadata = pending_directory / "metadata.json"
    crash_game_log = pending_directory / "game_log.json"
    _copy_new(metadata_path, crash_metadata, MAX_BUNDLE_JSON_BYTES)
    _copy_new(game_log_path, crash_game_log, MAX_BUNDLE_JSON_BYTES)

    cases = [
        _negative_case_record(
            "schema-v1",
            schema_v1,
            root,
            {
                "video": schema_v1 / "video.mp4",
                "metadata": schema_v1 / "metadata.json",
                "game_log": schema_v1 / "game_log.json",
            },
        ),
        _negative_case_record(
            "malformed-v2",
            malformed_v2,
            root,
            {
                "video": malformed_v2 / "video.mp4",
                "metadata": malformed_v2 / "metadata.json",
                "game_log": malformed_v2 / "game_log.json",
            },
        ),
        _negative_case_record(
            "identity-mismatch",
            identity_mismatch,
            root,
            {
                "video": identity_mismatch / "video.mp4",
                "metadata": identity_mismatch / "metadata.json",
                "game_log": identity_mismatch / "game_log.json",
            },
        ),
        _negative_case_record(
            "publication-failure",
            publication_failure,
            root,
            {"video": partial_video, "metadata": pending_metadata, "game_log": None},
        ),
        _negative_case_record(
            "crash-window",
            crash_window,
            root,
            {"video": None, "metadata": crash_metadata, "game_log": crash_game_log},
        ),
    ]
    for case in cases[-2:]:
        directory = Path(case["directory"])
        coherent = all(
            (directory / name).is_file()
            for name in ("video.mp4", "metadata.json", "game_log.json")
        )
        if coherent:
            raise FixtureError(
                f"publication failure case {case['name']} contains a coherent canonical trio"
            )

    result = {
        "schema_version": SCHEMA_VERSION,
        "command": "make-negative",
        "passed": True,
        "source_bundle": str(bundle),
        "source_media_id": source_media_id,
        "root": str(root),
        "case_count": len(cases),
        "cases": cases,
        "publication_failure_canonical_trios": 0,
    }
    if _strictly_below(result_path, root):
        if not result_path.parent.exists():
            raise FixtureError("negative result parent must be the fresh root or a created case directory")
        _new_file(result_path, "negative result")
    _write_json_new(result_path, result, "negative result")
    return result


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Generate and verify bounded QB-REPLAY-012 fixture evidence."
    )
    commands = parser.add_subparsers(dest="command", required=True)

    generate = commands.add_parser("generate")
    generate.add_argument("--video", required=True)
    generate.add_argument("--audio", required=True)
    generate.add_argument("--manifest", required=True)
    generate.add_argument("--width", required=True, type=int)
    generate.add_argument("--height", required=True, type=int)
    generate.add_argument("--frames", required=True, type=int)
    generate.add_argument("--rate", required=True)
    generate.add_argument("--result")
    generate.set_defaults(handler=command_generate)

    grid = commands.add_parser("verify-grid")
    grid.add_argument("--probe", required=True)
    grid.add_argument("--expected-rate", required=True)
    grid.add_argument("--origin", required=True, choices=("zero", "nonzero"))
    grid.add_argument("--result", required=True)
    grid.set_defaults(handler=command_verify_grid)

    bundle = commands.add_parser("verify-bundle")
    bundle.add_argument("--metadata", required=True)
    bundle.add_argument("--game-log", required=True)
    bundle.add_argument("--expected-media-id", required=True)
    bundle.add_argument("--result", required=True)
    bundle.set_defaults(handler=command_verify_bundle)

    media = commands.add_parser("verify-media")
    media.add_argument("--video", required=True)
    media.add_argument("--audio", required=True)
    media.add_argument("--manifest", required=True)
    media.add_argument("--expected-start-frame", required=True, type=int)
    media.add_argument("--expected-frame-count", required=True, type=int)
    media.add_argument("--result", required=True)
    media.set_defaults(handler=command_verify_media)

    native_media = commands.add_parser("verify-native-recorder-media")
    native_media.add_argument("--source-media", required=True)
    native_media.add_argument("--video", required=True)
    native_media.add_argument("--audio", required=True)
    native_media.add_argument("--probe", required=True)
    native_media.add_argument("--duration-seconds", required=True, type=int)
    native_media.add_argument("--expected-generation", required=True, type=int)
    native_media.add_argument("--manifest", required=True)
    native_media.add_argument("--result", required=True)
    native_media.set_defaults(handler=command_verify_native_recorder_media)

    native_mux_media = commands.add_parser("verify-native-mux-media")
    native_mux_media.add_argument("--source-media", required=True)
    native_mux_media.add_argument("--video", required=True)
    native_mux_media.add_argument("--audio", required=True)
    native_mux_media.add_argument("--probe", required=True)
    native_mux_media.add_argument("--manifest", required=True)
    native_mux_media.add_argument(
        "--input-timestamp-mode", required=True, choices=("cfr", "wallclock")
    )
    native_mux_media.add_argument("--result", required=True)
    native_mux_media.set_defaults(handler=command_verify_native_mux_media)

    negative = commands.add_parser("make-negative")
    negative.add_argument("--bundle", required=True)
    negative.add_argument("--root", required=True)
    negative.add_argument("--result", required=True)
    negative.set_defaults(handler=command_make_negative)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    arguments = parser.parse_args(argv)
    try:
        arguments.handler(arguments)
        return 0
    except Exception as error:  # Every command must fail closed without an unbounded traceback.
        print(f"fixture_tools.py: error: {_bounded_message(error)}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
