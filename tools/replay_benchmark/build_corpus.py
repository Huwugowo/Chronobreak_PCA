#!/usr/bin/env python3
"""Build deterministic, non-personal replay bundles from explicit fixture media.

The builder is deliberately separate from prepare.ps1. It converts reviewed,
generated recorder outputs into complete QueueBack bundles; prepare.ps1 then
performs the immutable copy, full decode, and benchmark-manifest binding.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence


SCHEMA_VERSION = 1
SENTINEL_NAME = ".chronobreak-replay-benchmark"
MAX_SPEC_BYTES = 1024 * 1024
IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
GAME_ID_RE = re.compile(r"^\d+(?:-[1-9]\d{0,2})?$")
TRANSFORMS = {"copy", "remux_audio", "repeat", "hevc"}
FILE_ATTRIBUTE_REPARSE_POINT = 0x400
CREATE_NO_WINDOW = 0x08000000


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
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
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


def _positive_number(
    value: Any, label: str, *, minimum: float, maximum: float
) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise CorpusError(f"{label} must be numeric")
    result = float(value)
    if not minimum <= result <= maximum:
        raise CorpusError(f"{label} must be between {minimum:g} and {maximum:g}")
    return result


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


def _run(arguments: Sequence[str], label: str, timeout_seconds: int) -> str:
    try:
        result = subprocess.run(
            list(arguments),
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
            creationflags=CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise CorpusError(f"{label} could not complete: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.strip()[-4000:]
        raise CorpusError(f"{label} failed with exit {result.returncode}: {detail}")
    return result.stdout


def _probe(ffprobe: Path, media: Path) -> dict[str, Any]:
    raw = _run(
        [
            str(ffprobe),
            "-v",
            "error",
            "-show_entries",
            (
            "format=duration,size,format_name:"
                "stream=index,codec_type,codec_name,profile,width,height,"
                "avg_frame_rate,time_base,has_b_frames,sample_rate,channels,nb_read_packets"
            ),
            "-count_packets",
            "-of",
            "json",
            str(media),
        ],
        f"ffprobe {media.name}",
        300,
    )
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CorpusError(f"ffprobe returned malformed JSON for {media}") from error
    if not isinstance(value, dict):
        raise CorpusError(f"ffprobe returned a non-object for {media}")
    return value


def _stream(probe: Mapping[str, Any], kind: str) -> Mapping[str, Any] | None:
    streams = probe.get("streams")
    if not isinstance(streams, list):
        return None
    return next(
        (
            stream
            for stream in streams
            if isinstance(stream, dict) and stream.get("codec_type") == kind
        ),
        None,
    )


def _duration_seconds(probe: Mapping[str, Any]) -> float:
    format_value = probe.get("format")
    if not isinstance(format_value, dict):
        raise CorpusError("ffprobe output has no format object")
    try:
        duration = float(format_value["duration"])
    except (KeyError, TypeError, ValueError) as error:
        raise CorpusError("ffprobe output has no numeric duration") from error
    if duration <= 0:
        raise CorpusError("ffprobe duration must be positive")
    return duration


def parse_rate(value: Any) -> float:
    if not isinstance(value, str) or "/" not in value:
        raise CorpusError("video average frame rate is malformed")
    numerator, denominator = value.split("/", 1)
    try:
        result = int(numerator) / int(denominator)
    except (ValueError, ZeroDivisionError) as error:
        raise CorpusError("video average frame rate is malformed") from error
    if result <= 0:
        raise CorpusError("video average frame rate must be positive")
    return result


def build_game_log(
    duration_ms: int,
    *,
    event_interval_seconds: float,
    snapshot_interval_seconds: float,
) -> dict[str, Any]:
    """Return deterministic anonymized descriptive data bounded by media duration."""
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
    snapshot_step_ms = max(1, round(snapshot_interval_seconds * 1000))
    for game_time_ms in range(0, duration_ms + 1, snapshot_step_ms):
        progress = game_time_ms / max(1, duration_ms)
        snapshot_players = []
        for index, player in enumerate(players):
            updated = dict(player)
            updated["cs"] = int(progress * (220 + index * 4))
            updated["level"] = min(18, 1 + int(progress * 17))
            item_count = min(6, int(progress * 7))
            updated["items"] = [
                {"item_id": 1001 + ((index + slot) % 20), "slot": slot}
                for slot in range(item_count)
            ]
            snapshot_players.append(updated)
        snapshots.append({"game_time_ms": game_time_ms, "players": snapshot_players})

    events: list[dict[str, Any]] = []
    event_step_ms = max(1, round(event_interval_seconds * 1000))
    for sequence, game_time_ms in enumerate(
        range(event_step_ms, duration_ms, event_step_ms), start=1
    ):
        role = sequence % 3
        killer = local_player if role == 0 else f"QB-Red-{(sequence % 5) + 1}#TEST"
        victim = local_player if role == 1 else f"QB-Red-{((sequence + 1) % 5) + 1}#TEST"
        assisters = [local_player] if role == 2 else ["QB-Blue-2#TEST"]
        events.append(
            {
                "type": "ChampionKill",
                "game_time_ms": game_time_ms,
                "video_time_ms": game_time_ms,
                "killer": killer,
                "victim": victim,
                "assisters": assisters,
            }
        )
        if sequence % 30 == 0:
            events.append(
                {
                    "type": "DragonKill",
                    "game_time_ms": game_time_ms,
                    "video_time_ms": game_time_ms,
                    "killer": "QB-Blue-2#TEST",
                    "dragon_type": "Infernal",
                    "assisters": [local_player],
                }
            )
        if sequence % 18 == 0:
            events.append(
                {
                    "type": "TurretKilled",
                    "game_time_ms": game_time_ms,
                    "video_time_ms": game_time_ms,
                    "killer": local_player,
                    "turret": "OuterTurret",
                    "assisters": [],
                }
            )
    return {
        "game_start_video_offset_ms": 0,
        "snapshots": snapshots,
        "events": events,
    }


def _media_summary(probe: Mapping[str, Any]) -> dict[str, Any]:
    video = _stream(probe, "video")
    if video is None:
        raise CorpusError("fixture output has no video stream")
    audio = _stream(probe, "audio")
    format_value = probe.get("format")
    assert isinstance(format_value, dict)
    return {
        "duration_seconds": _duration_seconds(probe),
        "size_bytes": int(format_value.get("size", 0)),
        "format_name": format_value.get("format_name"),
        "video_codec": video.get("codec_name"),
        "video_profile": video.get("profile"),
        "width": video.get("width"),
        "height": video.get("height"),
        "average_frame_rate": video.get("avg_frame_rate"),
        "time_base": video.get("time_base"),
        "has_b_frames": video.get("has_b_frames"),
        "audio_codec": audio.get("codec_name") if audio else None,
        "audio_sample_rate": audio.get("sample_rate") if audio else None,
        "audio_channels": audio.get("channels") if audio else None,
        "audio_packets": int(audio.get("nb_read_packets", 0)) if audio else 0,
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
    if value["schema_version"] != SCHEMA_VERSION:
        raise CorpusError("schema_version must be 1")
    corpus_id = _identifier(value["corpus_id"], "corpus_id")
    sentinel_root = _absolute_path(value["sentinel_root"], "sentinel_root")
    if sentinel_root.name != SENTINEL_NAME:
        raise CorpusError(f"sentinel_root must end in {SENTINEL_NAME}")
    output_root = _absolute_path(value["output_root"], "output_root")
    if not _is_strictly_below(output_root, sentinel_root):
        raise CorpusError("output_root must be strictly below sentinel_root")
    if output_root.name != corpus_id:
        raise CorpusError("output_root leaf must equal corpus_id")
    if output_root.exists() or output_root.is_symlink():
        raise CorpusError("output_root already exists; corpus builds are immutable")
    _assert_nearest_ancestor_reparse_free(sentinel_root, "sentinel_root")
    _assert_nearest_ancestor_reparse_free(output_root, "output_root")

    fixtures_value = value["fixtures"]
    if not isinstance(fixtures_value, list) or not fixtures_value:
        raise CorpusError("fixtures must be a nonempty array")
    ids: set[str] = set()
    aliases: set[str] = set()
    game_ids: set[str] = set()
    fixtures: list[dict[str, Any]] = []
    allowed = {
        "id",
        "alias",
        "game_timestamp",
        "source_video",
        "transform",
        "backend",
        "expected_video_codec",
        "expected_audio_codec",
        "minimum_duration_seconds",
        "target_duration_seconds",
        "event_interval_seconds",
        "snapshot_interval_seconds",
    }
    required = (
        "id",
        "alias",
        "game_timestamp",
        "source_video",
        "transform",
        "backend",
        "expected_video_codec",
        "expected_audio_codec",
        "minimum_duration_seconds",
    )
    for index, item in enumerate(fixtures_value):
        if not isinstance(item, dict):
            raise CorpusError(f"fixture {index} must be an object")
        _only_fields(item, allowed, f"fixture {index}")
        _required(item, required, f"fixture {index}")
        fixture_id = _identifier(item["id"], f"fixture {index} id")
        alias = _identifier(item["alias"], f"fixture {fixture_id} alias")
        game_id = item["game_timestamp"]
        if not isinstance(game_id, str) or GAME_ID_RE.fullmatch(game_id) is None:
            raise CorpusError(f"fixture {fixture_id} game_timestamp is invalid")
        if fixture_id in ids or alias in aliases or game_id in game_ids:
            raise CorpusError("fixture ids, aliases, and game timestamps must be unique")
        ids.add(fixture_id)
        aliases.add(alias)
        game_ids.add(game_id)
        source_video = _absolute_path(
            item["source_video"], f"fixture {fixture_id} source_video"
        )
        source_video = _assert_reparse_free_existing(
            source_video, f"fixture {fixture_id} source_video"
        )
        if not source_video.is_file() or source_video.stat().st_size == 0:
            raise CorpusError(f"fixture {fixture_id} source_video must be a nonempty file")
        transform = item["transform"]
        if transform not in TRANSFORMS:
            raise CorpusError(f"fixture {fixture_id} transform is unsupported")
        backend = _identifier(item["backend"], f"fixture {fixture_id} backend")
        expected_video = item["expected_video_codec"]
        expected_audio = item["expected_audio_codec"]
        if expected_video not in {"h264", "hevc"} or expected_audio != "aac":
            raise CorpusError(f"fixture {fixture_id} expected codecs are unsupported")
        minimum_duration = _positive_number(
            item["minimum_duration_seconds"],
            f"fixture {fixture_id} minimum_duration_seconds",
            minimum=1,
            maximum=86_400,
        )
        target_duration = item.get("target_duration_seconds")
        if transform in {"repeat", "hevc"}:
            target_duration = _positive_number(
                target_duration,
                f"fixture {fixture_id} target_duration_seconds",
                minimum=1,
                maximum=86_400,
            )
            if target_duration < minimum_duration:
                raise CorpusError(
                    f"fixture {fixture_id} target duration is below its minimum"
                )
        elif target_duration is not None:
            raise CorpusError(f"fixture {fixture_id} copy transform has no target duration")
        event_interval = _positive_number(
            item.get("event_interval_seconds", 5),
            f"fixture {fixture_id} event_interval_seconds",
            minimum=1,
            maximum=300,
        )
        snapshot_interval = _positive_number(
            item.get("snapshot_interval_seconds", 30),
            f"fixture {fixture_id} snapshot_interval_seconds",
            minimum=1,
            maximum=600,
        )
        fixtures.append(
            {
                **item,
                "id": fixture_id,
                "alias": alias,
                "game_timestamp": game_id,
                "source_video": source_video,
                "transform": transform,
                "backend": backend,
                "minimum_duration_seconds": minimum_duration,
                "target_duration_seconds": target_duration,
                "event_interval_seconds": event_interval,
                "snapshot_interval_seconds": snapshot_interval,
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
    if not isinstance(runtime_id, str) or not runtime_id:
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
        return summary
    if summary["video_codec"] != fixture["expected_video_codec"]:
        raise CorpusError(f"fixture {fixture['id']} output video codec is wrong")
    if summary["audio_codec"] != fixture["expected_audio_codec"]:
        raise CorpusError(f"fixture {fixture['id']} output audio codec is wrong")
    if summary["audio_packets"] <= 0:
        raise CorpusError(f"fixture {fixture['id']} output has no decodable audio packets")
    if summary["duration_seconds"] + 0.5 < fixture["minimum_duration_seconds"]:
        raise CorpusError(f"fixture {fixture['id']} output is shorter than required")
    if summary["width"] != 1920 or summary["height"] != 1080:
        raise CorpusError(f"fixture {fixture['id']} output must remain 1920x1080")
    rate = parse_rate(summary["average_frame_rate"])
    if not 59.0 <= rate <= 61.0:
        raise CorpusError(f"fixture {fixture['id']} output must remain approximately 60 FPS")
    return summary


def repeat_count(target_duration_seconds: float, source_duration_seconds: float) -> int:
    if target_duration_seconds <= 0 or source_duration_seconds <= 0:
        raise CorpusError("repeat durations must be positive")
    return max(1, math.ceil(target_duration_seconds / source_duration_seconds))


def _create_media(
    ffmpeg: Path,
    fixture: Mapping[str, Any],
    destination: Path,
    source_duration_seconds: float,
) -> None:
    source = fixture["source_video"]
    transform = fixture["transform"]
    if transform == "copy":
        shutil.copy2(source, destination, follow_symlinks=False)
        return
    output_duration = (
        fixture["target_duration_seconds"]
        if fixture["target_duration_seconds"] is not None
        else source_duration_seconds
    )
    target = f"{output_duration:.3f}"
    audio_source = f"sine=frequency=880:sample_rate=48000:duration={target}"
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
            "-t",
            target,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            str(destination),
        ]
    elif transform == "repeat":
        repeats = repeat_count(fixture["target_duration_seconds"], source_duration_seconds)
        concat_path = destination.parent.parent / f".{fixture['id']}.ffconcat"
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
            "-t",
            target,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-avoid_negative_ts",
            "make_zero",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            str(destination),
        ]
    else:
        arguments = [
            *common,
            "-i",
            str(source),
            "-f",
            "lavfi",
            "-i",
            audio_source,
            "-t",
            target,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
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
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-af",
            audio_filter,
            "-movflags",
            "+faststart",
            str(destination),
        ]
    _run(arguments, f"{transform} fixture {fixture['id']}", 7200)
    if transform == "repeat":
        concat_path.unlink()


def _build_metadata(
    fixture: Mapping[str, Any], media: Mapping[str, Any], runtime_id: str
) -> dict[str, Any]:
    fps = round(parse_rate(media["average_frame_rate"]))
    return {
        "recorded_at": "2026-08-25T00:00:00Z",
        "duration_ms": round(media["duration_seconds"] * 1000),
        "game_mode": "PRACTICETOOL",
        "local_player_summoner_name": "QB-Blue-1#TEST",
        "local_player_champion": "Ahri",
        "local_player_team": "ORDER",
        "video_offset_ms": 0,
        "encoder_used": "derived-corpus" if fixture["transform"] != "copy" else "source",
        "recording_codec": media["video_codec"],
        "recording_profile": media["video_profile"] or "unknown",
        "recording_resolution": f"{media['width']}x{media['height']}",
        "recording_fps": fps,
        "capture_backend": fixture["backend"],
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
        "benchmark_fixture": {
            "schema_version": 1,
            "fixture_id": fixture["id"],
            "alias": fixture["alias"],
            "transform": fixture["transform"],
            "personal_data": False,
        },
    }


def build_corpus(
    specification: Mapping[str, Any], runtime_root: Path, *, preflight_only: bool
) -> dict[str, Any]:
    validated = _validate_spec(specification)
    ffmpeg, ffprobe, runtime_id = _resolve_tools(runtime_root)
    sources: list[dict[str, Any]] = []
    for fixture in validated["fixtures"]:
        source_hash = _sha256(fixture["source_video"])
        source_media = _validate_media(_probe(ffprobe, fixture["source_video"]), fixture, source=True)
        sources.append(
            {
                "fixture_id": fixture["id"],
                "source_name": fixture["source_video"].name,
                "source_sha256": source_hash,
                "source_media": source_media,
            }
        )
    if preflight_only:
        return {
            "schema_version": 1,
            "corpus_id": validated["corpus_id"],
            "runtime_id": runtime_id,
            "preflight_only": True,
            "fixtures": sources,
        }

    sentinel_root: Path = validated["sentinel_root"]
    if not sentinel_root.exists():
        sentinel_root.mkdir(exist_ok=False)
    output_root: Path = validated["output_root"]
    output_root.parent.mkdir(parents=True, exist_ok=True)
    output_root.mkdir(exist_ok=False)
    built: list[dict[str, Any]] = []
    for fixture, source_evidence in zip(validated["fixtures"], sources):
        bundle = output_root / fixture["id"]
        bundle.mkdir()
        media_path = bundle / "video.mp4"
        _create_media(
            ffmpeg,
            fixture,
            media_path,
            float(source_evidence["source_media"]["duration_seconds"]),
        )
        source_hash_after = _sha256(fixture["source_video"])
        if source_hash_after != source_evidence["source_sha256"]:
            raise CorpusError(f"fixture {fixture['id']} source changed during construction")
        media = _validate_media(_probe(ffprobe, media_path), fixture, source=False)
        duration_ms = round(media["duration_seconds"] * 1000)
        metadata = _build_metadata(fixture, media, runtime_id)
        game_log = build_game_log(
            duration_ms,
            event_interval_seconds=fixture["event_interval_seconds"],
            snapshot_interval_seconds=fixture["snapshot_interval_seconds"],
        )
        _write_json_new(bundle / "metadata.json", metadata)
        _write_json_new(bundle / "game_log.json", game_log)
        built.append(
            {
                **source_evidence,
                "source_sha256_after": source_hash_after,
                "alias": fixture["alias"],
                "game_timestamp": fixture["game_timestamp"],
                "backend": fixture["backend"],
                "transform": fixture["transform"],
                "bundle_relative_path": fixture["id"],
                "output_media": media,
                "output_media_sha256": _sha256(media_path),
                "metadata_sha256": _sha256(bundle / "metadata.json"),
                "game_log_sha256": _sha256(bundle / "game_log.json"),
                "snapshot_count": len(game_log["snapshots"]),
                "event_count": len(game_log["events"]),
                "personal_data": False,
            }
        )
    receipt = {
        "schema_version": 1,
        "corpus_id": validated["corpus_id"],
        "runtime_id": runtime_id,
        "source_policy": "explicit generated fixture media; sources opened read-only and hash-checked before/after",
        "automatic_cleanup": False,
        "fixtures": built,
    }
    _write_json_new(output_root / "corpus.json", receipt)
    return receipt


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Build deterministic QueueBack replay bundles from explicit generated media."
    )
    parser.add_argument("--spec", required=True, help="Corpus schema-v1 specification")
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
