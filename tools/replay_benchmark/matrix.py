#!/usr/bin/env python3
"""Deterministic planner and completeness verifier for replay benchmarks.

This tool deliberately does not launch QueueBack, hash prepared media, or cool
the machine. It expands an already prepared manifest into immutable one-process
launch manifests and later proves expansion, result order, cooldown, and the
last trial's matrix-final source-integrity evidence.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import ntpath
import os
import re
import stat
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence


SCHEMA_VERSION = 1
SENTINEL_NAME = ".chronobreak-replay-benchmark"
IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
HASH_RE = re.compile(r"^[a-f0-9]{64}$")
SUCCESS_STATUSES = {"complete", "completed", "success", "passed"}
MAX_JSON_BYTES = 16 * 1024 * 1024
FILE_ATTRIBUTE_REPARSE_POINT = 0x400

SCENARIO_KINDS = {
    "app_idle",
    "cold_open",
    "warm_open",
    "play_pause",
    "rate",
    "seek",
    "scrub",
    "layout",
    "lifecycle",
    "export",
}
SCENARIO_FIELDS = {
    "id",
    "kind",
    "fixture_ids",
    "trial_id",
    "seek_reason",
    "seek_playback_mode",
    "seed",
    "warmup_seconds",
    "duration_seconds",
    "idle_seconds",
    "target_times_ms",
    "rates",
    "request_rate_hz",
    "iterations",
    "distance_classes",
    "export_presets",
    "clip_start_ms",
    "clip_end_ms",
    "expected_duration_ms",
    "duration_tolerance_ms",
    "expected_video_codec",
    "expected_audio_codec",
    "music_mode",
    "gain_mode",
    "built_in_music_filename",
    "music_modes",
    "gain_modes",
    "endpoint_alignment",
    "observer_control",
}
RESERVED_WINDOWS_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{index}" for index in range(1, 10)),
    *(f"LPT{index}" for index in range(1, 10)),
}


class MatrixError(ValueError):
    """Raised when a matrix input, plan, or result set is unsafe or incomplete."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise MatrixError(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _read_json(path: Path, label: str) -> tuple[dict[str, Any], bytes]:
    if not path.is_file():
        raise MatrixError(f"{label} is missing: {path}")
    size = path.stat().st_size
    if size > MAX_JSON_BYTES:
        raise MatrixError(f"{label} exceeds the {MAX_JSON_BYTES}-byte safety limit")
    raw = path.read_bytes()
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise MatrixError(f"{label} is not valid UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise MatrixError(f"{label} must contain a JSON object")
    return value, raw


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def _canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise MatrixError(f"{label} must be an object")
    return value


def _array(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise MatrixError(f"{label} must be an array")
    return value


def _only_fields(value: Mapping[str, Any], allowed: set[str], label: str) -> None:
    unexpected = sorted(set(value) - allowed)
    if unexpected:
        raise MatrixError(f"{label} contains unsupported field(s): {', '.join(unexpected)}")


def _required(value: Mapping[str, Any], names: Sequence[str], label: str) -> None:
    missing = [name for name in names if name not in value]
    if missing:
        raise MatrixError(f"{label} is missing required field(s): {', '.join(missing)}")


def _identifier(value: Any, label: str) -> str:
    if not isinstance(value, str) or IDENTIFIER_RE.fullmatch(value) is None:
        raise MatrixError(f"{label} must be a safe 1-80 character identifier")
    return value


def _integer(value: Any, label: str, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise MatrixError(f"{label} must be an integer")
    if not minimum <= value <= maximum:
        raise MatrixError(f"{label} must be between {minimum} and {maximum}")
    return value


def _number(value: Any, label: str, minimum: float, maximum: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise MatrixError(f"{label} must be numeric")
    result = float(value)
    if not minimum <= result <= maximum:
        raise MatrixError(f"{label} must be between {minimum:g} and {maximum:g}")
    return result


def _windows_path(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value or "\x00" in value:
        raise MatrixError(f"{label} must be a nonempty absolute Windows path")
    candidate = value.replace("/", "\\")
    if candidate.startswith("\\\\") or candidate.startswith("\\?\\"):
        raise MatrixError(f"{label} must not be a UNC or device path")
    drive, tail = ntpath.splitdrive(candidate)
    if re.fullmatch(r"[A-Za-z]:", drive) is None or not tail.startswith("\\"):
        raise MatrixError(f"{label} must be an absolute drive-letter Windows path")
    components = [component for component in tail.split("\\") if component]
    for component in components:
        if component in {".", ".."}:
            raise MatrixError(f"{label} contains an unsafe traversal component")
        if component.endswith((" ", ".")) or ":" in component:
            raise MatrixError(f"{label} contains an ambiguous Windows component")
        stem = component.split(".", 1)[0].upper()
        if stem in RESERVED_WINDOWS_NAMES:
            raise MatrixError(f"{label} contains reserved Windows name {component!r}")
    normalized = ntpath.normpath(candidate)
    if normalized != candidate:
        raise MatrixError(f"{label} must already be normalized: {normalized}")
    return normalized


def _same_path(left: str, right: str) -> bool:
    return ntpath.normcase(left) == ntpath.normcase(right)


def _inside(root: str, candidate: str, *, strict: bool = True) -> bool:
    try:
        common = ntpath.commonpath([root, candidate])
    except ValueError:
        return False
    equal = _same_path(common, root)
    return equal and (not strict or not _same_path(root, candidate))


def _require_inside(root: str, candidate: str, label: str) -> None:
    if not _inside(root, candidate):
        raise MatrixError(f"{label} must be a strict descendant of sentinel_root")


def _paths_overlap(left: str, right: str) -> bool:
    return _same_path(left, right) or _inside(left, right) or _inside(right, left)


def _native_absolute(path: Path, label: str, *, must_exist: bool) -> Path:
    if not path.is_absolute():
        raise MatrixError(f"{label} must be an absolute filesystem path")
    _assert_reparse_free(path, label)
    try:
        return path.resolve(strict=must_exist)
    except OSError as error:
        raise MatrixError(f"cannot resolve {label}: {error}") from error


def _assert_reparse_free(path: Path, label: str) -> None:
    absolute = path.absolute()
    parts = absolute.parts
    if not parts:
        raise MatrixError(f"{label} is empty")
    current = Path(parts[0])
    for component in parts[1:]:
        current /= component
        if not current.exists() and not current.is_symlink():
            break
        try:
            metadata = os.lstat(current)
        except OSError as error:
            raise MatrixError(f"cannot inspect {label} component {current}: {error}") from error
        attributes = getattr(metadata, "st_file_attributes", 0)
        if stat.S_ISLNK(metadata.st_mode) or attributes & FILE_ATTRIBUTE_REPARSE_POINT:
            raise MatrixError(f"{label} crosses a symlink/reparse point: {current}")


def _path_from_declared(value: str) -> Path:
    if os.name != "nt":
        raise MatrixError("writing Windows replay matrix plans requires Windows")
    return Path(value)


def _validate_prepared_filesystem(template: Mapping[str, Any]) -> None:
    """Inspect prepared roots only; deliberately never traverse fixture files."""
    directories = {
        "sentinel_root": template["sentinel_root"],
        "library_root": template["library_root"],
        "app_data_root": template["app_data_root"],
        "scratch_root": template["scratch_root"],
        "ddragon.cache_root": _object(template["ddragon"], "template ddragon")["cache_root"],
        "result_root parent": ntpath.dirname(str(template["result_root"])),
    }
    for label, declared in directories.items():
        path = _native_absolute(_path_from_declared(str(declared)), label, must_exist=True)
        if not path.is_dir():
            raise MatrixError(f"prepared {label} must be an existing directory")
    config = _native_absolute(
        _path_from_declared(str(template["config_path"])), "config_path", must_exist=True
    )
    if not config.is_file():
        raise MatrixError("prepared config_path must be an existing file")


def _validate_scenario(
    raw: Any,
    fixture_ids: set[str],
    label: str,
    *,
    allow_trial_id: bool,
    matrix_seed: int | None,
) -> dict[str, Any]:
    scenario = copy.deepcopy(_object(raw, label))
    _only_fields(scenario, SCENARIO_FIELDS, label)
    _required(scenario, ("id", "kind", "fixture_ids"), label)
    scenario_id = _identifier(scenario["id"], f"{label}.id")
    kind = scenario["kind"]
    if kind not in SCENARIO_KINDS:
        raise MatrixError(f"{label}.kind is unsupported")
    references = _array(scenario["fixture_ids"], f"{label}.fixture_ids")
    if not references:
        raise MatrixError(f"{label}.fixture_ids must not be empty")
    normalized_refs = [_identifier(item, f"{label}.fixture_ids item") for item in references]
    if len(set(normalized_refs)) != len(normalized_refs):
        raise MatrixError(f"{label}.fixture_ids contains duplicates")
    missing = sorted(set(normalized_refs) - fixture_ids)
    if missing:
        raise MatrixError(f"{label} references unknown fixture(s): {', '.join(missing)}")
    if kind != "lifecycle" and len(normalized_refs) != 1:
        raise MatrixError(f"{label} must reference exactly one fixture")
    if "trial_id" in scenario:
        if not allow_trial_id:
            raise MatrixError(f"{label}.trial_id is generated by the planner and must be omitted")
        _identifier(scenario["trial_id"], f"{label}.trial_id")
    if matrix_seed is not None:
        if "seed" in scenario and scenario["seed"] != matrix_seed:
            raise MatrixError(f"{label}.seed must equal the matrix seed")
        scenario["seed"] = matrix_seed
    elif "seed" in scenario:
        _integer(scenario["seed"], f"{label}.seed", 0, 2**63 - 1)
    if "seek_reason" in scenario and scenario["seek_reason"] not in {
        "benchmark",
        "event-jump",
        "endpoint-edit",
    }:
        raise MatrixError(f"{label}.seek_reason is unsupported")
    if "seek_playback_mode" in scenario and scenario["seek_playback_mode"] not in {
        "playing",
        "paused",
    }:
        raise MatrixError(f"{label}.seek_playback_mode is unsupported")
    if "observer_control" in scenario and not isinstance(scenario["observer_control"], bool):
        raise MatrixError(f"{label}.observer_control must be boolean")
    for field, limits in {
        "warmup_seconds": (0, 3600),
        "duration_seconds": (0.000001, 86400),
        "idle_seconds": (0, 3600),
        "request_rate_hz": (0.000001, 1000),
    }.items():
        if field in scenario:
            _number(scenario[field], f"{label}.{field}", *limits)
    if "iterations" in scenario:
        _integer(scenario["iterations"], f"{label}.iterations", 1, 10000)
    for field in ("target_times_ms", "rates", "distance_classes", "export_presets"):
        if field in scenario:
            values = _array(scenario[field], f"{label}.{field}")
            if len(values) != len({_canonical_bytes(item) for item in values}):
                raise MatrixError(f"{label}.{field} contains duplicates")
    if kind == "export":
        required = (
            "clip_start_ms",
            "clip_end_ms",
            "expected_duration_ms",
            "expected_video_codec",
            "expected_audio_codec",
            "export_presets",
        )
        _required(scenario, required, label)
        if scenario["expected_video_codec"] != "h264" or scenario["expected_audio_codec"] != "aac":
            raise MatrixError(f"{label} must declare the H.264/AAC export contract")
    scenario["id"] = scenario_id
    scenario["fixture_ids"] = normalized_refs
    return scenario


def _validate_template(template: dict[str, Any]) -> dict[str, Any]:
    required = (
        "schema_version",
        "run_id",
        "sentinel_root",
        "library_root",
        "config_path",
        "app_data_root",
        "result_root",
        "scratch_root",
        "observer_profile",
        "ddragon",
        "fixtures",
        "scenarios",
    )
    _required(template, required, "template manifest")
    if template["schema_version"] != SCHEMA_VERSION:
        raise MatrixError("template manifest schema_version must be 1")
    run_id = _identifier(template["run_id"], "template manifest run_id")
    sentinel = _windows_path(template["sentinel_root"], "template sentinel_root")
    if ntpath.basename(sentinel).lower() != SENTINEL_NAME:
        raise MatrixError(f"sentinel_root must itself end with {SENTINEL_NAME}")
    normalized_paths: dict[str, str] = {"sentinel_root": sentinel}
    for field in ("library_root", "config_path", "app_data_root", "result_root", "scratch_root"):
        normalized = _windows_path(template[field], f"template {field}")
        _require_inside(sentinel, normalized, f"template {field}")
        normalized_paths[field] = normalized
    if ntpath.basename(normalized_paths["config_path"]).lower() != "config.toml":
        raise MatrixError("template config_path must end with config.toml")
    if ntpath.basename(normalized_paths["result_root"]) != run_id:
        raise MatrixError("template result_root leaf must equal template run_id")
    if template["observer_profile"] not in {"minimal", "full"}:
        raise MatrixError("template observer_profile must be minimal or full")
    ddragon = _object(template["ddragon"], "template ddragon")
    _required(ddragon, ("mode", "cache_root", "cache_fingerprint"), "template ddragon")
    if ddragon["mode"] != "offline":
        raise MatrixError("template Data Dragon mode must be offline")
    cache_root = _windows_path(ddragon["cache_root"], "template ddragon.cache_root")
    expected_cache = ntpath.join(normalized_paths["app_data_root"], "ddragon")
    if not _same_path(cache_root, expected_cache):
        raise MatrixError("template ddragon.cache_root must equal app_data_root\\ddragon")
    fingerprint = ddragon["cache_fingerprint"]
    if not isinstance(fingerprint, str) or re.fullmatch(r"sha256:[a-f0-9]{64}", fingerprint) is None:
        raise MatrixError("template ddragon.cache_fingerprint is invalid")
    fixtures = _array(template["fixtures"], "template fixtures")
    if not fixtures:
        raise MatrixError("template fixtures must not be empty")
    fixture_ids: set[str] = set()
    for index, raw_fixture in enumerate(fixtures):
        fixture = _object(raw_fixture, f"template fixture {index}")
        fixture_id = _identifier(fixture.get("id"), f"template fixture {index}.id")
        if fixture_id in fixture_ids:
            raise MatrixError(f"template fixture id {fixture_id!r} is duplicated")
        fixture_ids.add(fixture_id)
    scenarios = _array(template["scenarios"], "template scenarios")
    if len(scenarios) != 1:
        raise MatrixError("template must contain exactly one prepared launch scenario")
    _validate_scenario(
        scenarios[0], fixture_ids, "template scenario", allow_trial_id=True, matrix_seed=None
    )
    result_parent = ntpath.dirname(normalized_paths["result_root"])
    _require_inside(sentinel, result_parent, "template result parent")
    mutable_roots = {
        "library_root": normalized_paths["library_root"],
        "config parent": ntpath.dirname(normalized_paths["config_path"]),
        "app_data_root": normalized_paths["app_data_root"],
        "result parent": result_parent,
        "scratch_root": normalized_paths["scratch_root"],
    }
    mutable_items = list(mutable_roots.items())
    for index, (left_label, left) in enumerate(mutable_items):
        for right_label, right in mutable_items[index + 1 :]:
            if _paths_overlap(left, right):
                raise MatrixError(
                    f"template mutable roots overlap: {left_label} and {right_label}"
                )
    return {
        "run_id": run_id,
        "sentinel_root": sentinel,
        "result_parent": result_parent,
        "fixture_ids": fixture_ids,
        "mutable_roots": mutable_roots,
        **normalized_paths,
    }


def _compact_identifier(value: str) -> str:
    if len(value) <= 80:
        return value
    digest = _sha256(value.encode("utf-8"))[:12]
    return f"{value[:67]}-{digest}"


def _compile(
    template: dict[str, Any],
    template_bytes: bytes,
    template_path: Path,
    specification: dict[str, Any],
    specification_bytes: bytes,
    specification_path: Path,
) -> tuple[dict[str, Any], list[tuple[str, dict[str, Any], bytes]]]:
    context = _validate_template(template)
    _only_fields(
        specification,
        {
            "schema_version",
            "matrix_id",
            "seed",
            "cooldown_seconds",
            "timeout_seconds",
            "plan_root",
            "arms",
            "order",
        },
        "matrix specification",
    )
    _required(
        specification,
        ("schema_version", "matrix_id", "seed", "cooldown_seconds", "plan_root", "arms", "order"),
        "matrix specification",
    )
    if specification["schema_version"] != SCHEMA_VERSION:
        raise MatrixError("matrix specification schema_version must be 1")
    matrix_id = _identifier(specification["matrix_id"], "matrix_id")
    seed = _integer(specification["seed"], "seed", 0, 2**63 - 1)
    cooldown = _number(specification["cooldown_seconds"], "cooldown_seconds", 0, 86400)
    timeout_override = (
        _integer(specification["timeout_seconds"], "timeout_seconds", 30, 86400)
        if "timeout_seconds" in specification
        else None
    )
    plan_root = _windows_path(specification["plan_root"], "plan_root")
    _require_inside(context["sentinel_root"], plan_root, "plan_root")
    if ntpath.basename(plan_root) != matrix_id:
        raise MatrixError("plan_root leaf must equal matrix_id")
    for label, mutable_root in context["mutable_roots"].items():
        if _paths_overlap(plan_root, mutable_root):
            raise MatrixError(f"plan_root overlaps template {label}")

    arms_raw = _array(specification["arms"], "arms")
    if not arms_raw:
        raise MatrixError("arms must not be empty")
    arms: dict[str, dict[str, Any]] = {}
    groups: dict[str, list[dict[str, Any]]] = {}
    normalized_arms: list[dict[str, Any]] = []
    for index, raw_arm in enumerate(arms_raw):
        label = f"arm {index}"
        arm = _object(raw_arm, label)
        _only_fields(
            arm,
            {
                "id",
                "observer_profile",
                "repetitions",
                "timeout_seconds",
                "scenario",
                "observer_control",
            },
            label,
        )
        _required(arm, ("id", "observer_profile", "repetitions", "scenario"), label)
        arm_id = _identifier(arm["id"], f"{label}.id")
        if arm_id in arms:
            raise MatrixError(f"arm id {arm_id!r} is duplicated")
        profile = arm["observer_profile"]
        if profile not in {"minimal", "full"}:
            raise MatrixError(f"{label}.observer_profile must be minimal or full")
        repetitions = _integer(arm["repetitions"], f"{label}.repetitions", 1, 10000)
        arm_timeout_override = (
            _integer(
                arm["timeout_seconds"],
                f"{label}.timeout_seconds",
                30,
                86400,
            )
            if "timeout_seconds" in arm
            else None
        )
        scenario = _validate_scenario(
            arm["scenario"],
            context["fixture_ids"],
            f"{label}.scenario",
            allow_trial_id=False,
            matrix_seed=seed,
        )
        control: dict[str, str] | None = None
        if "observer_control" in arm:
            control_raw = _object(arm["observer_control"], f"{label}.observer_control")
            _only_fields(control_raw, {"group_id", "role"}, f"{label}.observer_control")
            _required(control_raw, ("group_id", "role"), f"{label}.observer_control")
            group_id = _identifier(control_raw["group_id"], f"{label}.observer_control.group_id")
            role = control_raw["role"]
            if role not in {"minimal", "full"} or role != profile:
                raise MatrixError(f"{label} observer-control role must equal observer_profile")
            if scenario.get("observer_control") is not True:
                raise MatrixError(f"{label}.scenario must set observer_control=true")
            control = {"group_id": group_id, "role": role}
        elif scenario.get("observer_control") is True:
            raise MatrixError(f"{label} sets observer_control=true without pairing metadata")
        identity_material = {
            "observer_profile": profile,
            "scenario": scenario,
            "observer_control": control,
        }
        if arm_timeout_override is not None:
            identity_material["timeout_seconds"] = arm_timeout_override
        normalized = {
            "id": arm_id,
            "observer_profile": profile,
            "repetitions": repetitions,
            "arm_identity_sha256": _sha256(_canonical_bytes(identity_material)),
            "scenario": scenario,
        }
        if arm_timeout_override is not None:
            normalized["timeout_seconds"] = arm_timeout_override
        if control is not None:
            normalized["observer_control"] = control
            groups.setdefault(control["group_id"], []).append(normalized)
        arms[arm_id] = normalized
        normalized_arms.append(normalized)

    for group_id, paired_arms in groups.items():
        if len(paired_arms) != 2 or {arm["observer_profile"] for arm in paired_arms} != {"minimal", "full"}:
            raise MatrixError(
                f"observer-control group {group_id!r} must contain exactly one minimal and one full arm"
            )
        if paired_arms[0]["repetitions"] != paired_arms[1]["repetitions"]:
            raise MatrixError(f"observer-control group {group_id!r} repetitions do not match")
        paired_timeouts = {
            arm.get("timeout_seconds", timeout_override) for arm in paired_arms
        }
        if len(paired_timeouts) != 1:
            raise MatrixError(f"observer-control group {group_id!r} timeouts do not match")
        if _canonical_bytes(paired_arms[0]["scenario"]) != _canonical_bytes(paired_arms[1]["scenario"]):
            raise MatrixError(f"observer-control group {group_id!r} scenarios are not identical")

    order_raw = _array(specification["order"], "order")
    if not order_raw:
        raise MatrixError("order must not be empty")
    counts = {arm_id: 0 for arm_id in arms}
    seen_arm_trials: set[tuple[str, str]] = set()
    normalized_order: list[dict[str, Any]] = []
    pair_entries: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for index, raw_entry in enumerate(order_raw, 1):
        label = f"order entry {index}"
        entry = _object(raw_entry, label)
        _only_fields(entry, {"arm_id", "trial_id", "pair_id"}, label)
        _required(entry, ("arm_id", "trial_id"), label)
        arm_id = _identifier(entry["arm_id"], f"{label}.arm_id")
        trial_id = _identifier(entry["trial_id"], f"{label}.trial_id")
        if arm_id not in arms:
            raise MatrixError(f"{label} references unknown arm {arm_id!r}")
        if (arm_id, trial_id) in seen_arm_trials:
            raise MatrixError(f"{label} duplicates arm/trial {arm_id}/{trial_id}")
        seen_arm_trials.add((arm_id, trial_id))
        counts[arm_id] += 1
        normalized_entry: dict[str, Any] = {
            "sequence": index,
            "arm_id": arm_id,
            "trial_id": trial_id,
        }
        control = arms[arm_id].get("observer_control")
        if control is None:
            if "pair_id" in entry:
                raise MatrixError(f"{label}.pair_id is only valid for observer-control arms")
        else:
            if "pair_id" not in entry:
                raise MatrixError(f"{label} is missing pair_id for an observer-control arm")
            pair_id = _identifier(entry["pair_id"], f"{label}.pair_id")
            normalized_entry["pair_id"] = pair_id
            normalized_entry["observer_control_group"] = control["group_id"]
            normalized_entry["observer_control_role"] = control["role"]
            pair_entries.setdefault((control["group_id"], pair_id), []).append(normalized_entry)
        normalized_order.append(normalized_entry)

    for arm_id, arm in arms.items():
        if counts[arm_id] != arm["repetitions"]:
            raise MatrixError(
                f"arm {arm_id!r} declares {arm['repetitions']} repetitions but order contains {counts[arm_id]}"
            )

    leading_roles: dict[str, list[tuple[int, str]]] = {}
    for (group_id, pair_id), entries in pair_entries.items():
        if len(entries) != 2 or {entry["observer_control_role"] for entry in entries} != {"minimal", "full"}:
            raise MatrixError(
                f"observer-control pair {group_id}/{pair_id} must contain one minimal and one full trial"
            )
        if entries[0]["trial_id"] != entries[1]["trial_id"]:
            raise MatrixError(f"observer-control pair {group_id}/{pair_id} trial ids do not match")
        entries.sort(key=lambda item: item["sequence"])
        if entries[1]["sequence"] != entries[0]["sequence"] + 1:
            raise MatrixError(f"observer-control pair {group_id}/{pair_id} must be adjacent in order")
        leading_roles.setdefault(group_id, []).append(
            (entries[0]["sequence"], entries[0]["observer_control_role"])
        )
    for group_id, paired_arms in groups.items():
        expected = paired_arms[0]["repetitions"]
        actual_pairs = [key for key in pair_entries if key[0] == group_id]
        if len(actual_pairs) != expected:
            raise MatrixError(
                f"observer-control group {group_id!r} requires {expected} complete pairs, found {len(actual_pairs)}"
            )
        roles = [role for _, role in sorted(leading_roles.get(group_id, []))]
        if any(left == right for left, right in zip(roles, roles[1:])):
            raise MatrixError(
                f"observer-control group {group_id!r} must alternate minimal/full leading order"
            )

    template_absolute = _windows_path(str(template_path), "template manifest path")
    specification_absolute = _windows_path(str(specification_path), "matrix specification path")
    _require_inside(context["sentinel_root"], template_absolute, "template manifest path")
    _require_inside(context["sentinel_root"], specification_absolute, "matrix specification path")
    for source_label, source_path in (
        ("template manifest path", template_absolute),
        ("matrix specification path", specification_absolute),
    ):
        for mutable_label, mutable_root in context["mutable_roots"].items():
            if _inside(mutable_root, source_path) or _same_path(mutable_root, source_path):
                raise MatrixError(f"{source_label} is inside mutable {mutable_label}")

    launches: list[dict[str, Any]] = []
    manifest_outputs: list[tuple[str, dict[str, Any], bytes]] = []
    run_ids: set[str] = set()
    result_roots: set[str] = set()
    for entry in normalized_order:
        arm = arms[entry["arm_id"]]
        raw_run_id = (
            f"{matrix_id}-{entry['sequence']:03d}-{entry['arm_id']}-"
            f"{entry['trial_id']}-{arm['observer_profile']}"
        )
        run_id = _compact_identifier(raw_run_id)
        if IDENTIFIER_RE.fullmatch(run_id) is None or run_id in run_ids:
            raise MatrixError("deterministic run_id generation produced a collision")
        run_ids.add(run_id)
        result_root = ntpath.join(context["result_parent"], run_id)
        _require_inside(context["sentinel_root"], result_root, "generated result_root")
        if ntpath.normcase(result_root) in result_roots:
            raise MatrixError("deterministic result_root generation produced a collision")
        result_roots.add(ntpath.normcase(result_root))
        scenario = copy.deepcopy(arm["scenario"])
        scenario["trial_id"] = entry["trial_id"]
        launch_manifest = copy.deepcopy(template)
        launch_manifest["run_id"] = run_id
        launch_manifest["result_root"] = result_root
        launch_manifest["observer_profile"] = arm["observer_profile"]
        launch_manifest["scenarios"] = [scenario]
        launch_timeout = arm.get("timeout_seconds", timeout_override)
        if launch_timeout is not None:
            launch_manifest["timeout_seconds"] = launch_timeout
        manifest_bytes = _json_bytes(launch_manifest)
        manifest_name = f"{entry['sequence']:04d}-{run_id}.json"
        manifest_path = ntpath.join(plan_root, "launches", manifest_name)
        launch = {
            **entry,
            "arm_identity_sha256": arm["arm_identity_sha256"],
            "observer_profile": arm["observer_profile"],
            "run_id": run_id,
            "manifest_path": manifest_path,
            "manifest_sha256": _sha256(manifest_bytes),
            "result_root": result_root,
        }
        launches.append(launch)
        manifest_outputs.append((manifest_path, launch_manifest, manifest_bytes))

    plan = {
        "schema_version": SCHEMA_VERSION,
        "document_type": "queueback-replay-matrix-plan",
        "matrix_id": matrix_id,
        "seed": seed,
        "cooldown_seconds": cooldown,
        "sentinel_root": context["sentinel_root"],
        "plan_root": plan_root,
        "template": {
            "path": template_absolute,
            "sha256": _sha256(template_bytes),
            "prepared_run_id": context["run_id"],
        },
        "specification": {"path": specification_absolute, "sha256": _sha256(specification_bytes)},
        "result_parent": context["result_parent"],
        "expected_arm_count": len(normalized_arms),
        "expected_trial_count": len(normalized_order),
        "arms": normalized_arms,
        "order": normalized_order,
        "launches": launches,
        "limitations": [
            "This plan does not execute trials or wait for cooldown; verify --require-results rejects result timestamps that do not prove the declared order and cooldown.",
            "run.ps1 currently performs out-of-window integrity hashes per trial; the final launch proof is bound to the matrix, but those earlier reads can warm the OS cache.",
        ],
    }
    if timeout_override is not None:
        plan["timeout_seconds"] = timeout_override
    return plan, manifest_outputs


def _source_paths(template_path: Path, specification_path: Path) -> tuple[Path, Path]:
    template = _native_absolute(template_path, "template manifest path", must_exist=True)
    specification = _native_absolute(specification_path, "matrix specification path", must_exist=True)
    return template, specification


def plan_matrix(template_path: Path, specification_path: Path) -> Path:
    template_path, specification_path = _source_paths(template_path, specification_path)
    template, template_bytes = _read_json(template_path, "template manifest")
    specification, specification_bytes = _read_json(specification_path, "matrix specification")
    plan, manifests = _compile(
        template,
        template_bytes,
        template_path,
        specification,
        specification_bytes,
        specification_path,
    )
    _validate_prepared_filesystem(template)
    plan_root = _path_from_declared(plan["plan_root"])
    parent = _native_absolute(plan_root.parent, "plan_root parent", must_exist=True)
    if not parent.is_dir():
        raise MatrixError("plan_root parent must be an existing directory")
    if plan_root.exists() or plan_root.is_symlink():
        raise MatrixError(f"plan_root already exists; refusing overwrite: {plan_root}")
    for launch in plan["launches"]:
        result_root = _path_from_declared(launch["result_root"])
        _assert_reparse_free(result_root.parent, "generated result_root parent")
        if not result_root.parent.is_dir():
            raise MatrixError(f"generated result parent is missing: {result_root.parent}")
        if result_root.exists() or result_root.is_symlink():
            raise MatrixError(f"generated result_root already exists; refusing reuse: {result_root}")

    plan_root.mkdir()
    launches_root = plan_root / "launches"
    launches_root.mkdir()
    for declared_path, _, raw in manifests:
        destination = _path_from_declared(declared_path)
        with destination.open("xb") as output:
            output.write(raw)
            output.flush()
            os.fsync(output.fileno())
    plan_path = plan_root / "matrix-plan.json"
    with plan_path.open("xb") as output:
        output.write(_json_bytes(plan))
        output.flush()
        os.fsync(output.fileno())
    return plan_path


def _utc_timestamp(value: Any, label: str) -> datetime:
    if not isinstance(value, str) or not value:
        raise MatrixError(f"{label} must be an ISO-8601 UTC timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise MatrixError(f"{label} must be an ISO-8601 UTC timestamp") from error
    if parsed.tzinfo is None or parsed.utcoffset() != timedelta(0):
        raise MatrixError(f"{label} must include a UTC offset")
    return parsed.astimezone(timezone.utc)


def _verify_result(
    launch: Mapping[str, Any], expected_manifest: Mapping[str, Any]
) -> tuple[datetime, datetime]:
    result_root = _path_from_declared(str(launch["result_root"]))
    _assert_reparse_free(result_root, f"result {launch['run_id']}")
    if not result_root.is_dir():
        raise MatrixError(f"required result is missing: {result_root}")
    manifest, _ = _read_json(result_root / "manifest.json", "preserved result manifest")
    if manifest != expected_manifest:
        raise MatrixError(f"result manifest does not match planned launch {launch['run_id']}")
    terminal, _ = _read_json(result_root / "terminal.json", "result terminal")
    scenario = expected_manifest["scenarios"][0]
    if terminal.get("schema_version") != 1 or terminal.get("run_id") != launch["run_id"]:
        raise MatrixError(f"result terminal identity is wrong for {launch['run_id']}")
    if terminal.get("scenario_id") != scenario["id"] or str(terminal.get("trial_id")) != scenario["trial_id"]:
        raise MatrixError(f"result terminal scenario/trial is wrong for {launch['run_id']}")
    if str(terminal.get("status", "")).lower() not in SUCCESS_STATUSES:
        raise MatrixError(f"result terminal is unsuccessful for {launch['run_id']}")
    for field in ("finished", "telemetry_complete", "media_valid", "source_hash_unchanged"):
        if terminal.get(field) is not True:
            raise MatrixError(f"result terminal {field} is not true for {launch['run_id']}")
    metadata, _ = _read_json(result_root / "runner-metadata.json", "runner metadata")
    if metadata.get("run_id") != launch["run_id"]:
        raise MatrixError(f"runner metadata identity is wrong for {launch['run_id']}")
    started = _utc_timestamp(
        metadata.get("started_utc"), f"runner start for {launch['run_id']}"
    )
    runner, _ = _read_json(result_root / "runner-result.json", "runner result")
    if runner.get("run_id") != launch["run_id"] or str(runner.get("status", "")).lower() not in SUCCESS_STATUSES:
        raise MatrixError(f"runner result is incomplete for {launch['run_id']}")
    completed = _utc_timestamp(
        runner.get("completed_utc"), f"runner completion for {launch['run_id']}"
    )
    if completed < started:
        raise MatrixError(f"runner completion precedes start for {launch['run_id']}")
    return started, completed


def _verify_matrix_final_integrity(
    launch: Mapping[str, Any], expected_manifest: Mapping[str, Any]
) -> None:
    result_root = _path_from_declared(str(launch["result_root"]))
    evidence, _ = _read_json(result_root / "post-hashes.json", "matrix-final post hashes")
    if evidence.get("schema_version") != 1 or evidence.get("all_match") is not True:
        raise MatrixError("matrix-final post hashes do not report all_match=true")
    checked = _utc_timestamp(evidence.get("checked_utc"), "matrix-final hash timestamp")
    runner, _ = _read_json(result_root / "runner-result.json", "matrix-final runner result")
    metadata, _ = _read_json(result_root / "runner-metadata.json", "matrix-final metadata")
    started = _utc_timestamp(metadata.get("started_utc"), "matrix-final runner start")
    completed = _utc_timestamp(runner.get("completed_utc"), "matrix-final runner completion")
    if checked < started or checked > completed:
        raise MatrixError("matrix-final hash timestamp is outside the final launch lifetime")

    config = _object(evidence.get("benchmark_config"), "matrix-final benchmark_config")
    if config.get("exists") is not True or config.get("match") is not True:
        raise MatrixError("matrix-final benchmark config identity does not match")
    expected_config = config.get("expected_sha256")
    actual_config = config.get("actual_sha256")
    if (
        not isinstance(expected_config, str)
        or HASH_RE.fullmatch(expected_config) is None
        or actual_config != expected_config
    ):
        raise MatrixError("matrix-final benchmark config SHA-256 is invalid")

    ddragon = _object(evidence.get("ddragon_cache"), "matrix-final ddragon_cache")
    expected_ddragon = _object(
        expected_manifest.get("ddragon"), "planned Data Dragon condition"
    ).get("cache_fingerprint")
    if (
        ddragon.get("exists") is not True
        or ddragon.get("match") is not True
        or ddragon.get("expected_fingerprint") != expected_ddragon
        or ddragon.get("actual_fingerprint") != expected_ddragon
    ):
        raise MatrixError("matrix-final Data Dragon fingerprint does not match the plan")

    expected_files: dict[tuple[str, str], str] = {}
    for fixture in _array(expected_manifest.get("fixtures"), "planned fixtures"):
        fixture_object = _object(fixture, "planned fixture")
        fixture_id = _identifier(fixture_object.get("id"), "planned fixture id")
        for file_identity in _array(fixture_object.get("files"), "planned fixture files"):
            file_object = _object(file_identity, "planned fixture file")
            relative_path = file_object.get("relative_path")
            digest = file_object.get("sha256")
            if not isinstance(relative_path, str) or not isinstance(digest, str):
                raise MatrixError("planned fixture file identity is malformed")
            expected_files[(fixture_id, relative_path.casefold())] = digest
    actual_files: dict[tuple[str, str], str] = {}
    for row in _array(evidence.get("files"), "matrix-final fixture hashes"):
        item = _object(row, "matrix-final fixture hash")
        fixture_id = _identifier(item.get("fixture_id"), "matrix-final fixture id")
        relative_path = item.get("relative_path")
        expected = item.get("expected_sha256")
        actual = item.get("actual_sha256")
        if (
            not isinstance(relative_path, str)
            or item.get("exists") is not True
            or item.get("match") is not True
            or not isinstance(expected, str)
            or HASH_RE.fullmatch(expected) is None
            or actual != expected
        ):
            raise MatrixError("matrix-final fixture hash row is invalid")
        key = (fixture_id, relative_path.casefold())
        if key in actual_files:
            raise MatrixError("matrix-final fixture hash rows contain a duplicate")
        actual_files[key] = expected
    if actual_files != expected_files:
        raise MatrixError("matrix-final fixture hashes do not exactly cover the planned corpus")


def verify_matrix(plan_path: Path, *, require_results: bool = False) -> dict[str, int]:
    plan_path = _native_absolute(plan_path, "matrix plan path", must_exist=True)
    stored_plan, _ = _read_json(plan_path, "matrix plan")
    if stored_plan.get("document_type") != "queueback-replay-matrix-plan":
        raise MatrixError("matrix plan document_type is invalid")
    plan_root = _windows_path(stored_plan.get("plan_root"), "matrix plan plan_root")
    expected_plan_path = ntpath.join(plan_root, "matrix-plan.json")
    if not _same_path(_windows_path(str(plan_path), "matrix plan path"), expected_plan_path):
        raise MatrixError("matrix plan file is not the declared plan_root\\matrix-plan.json")
    template_path = _path_from_declared(
        _windows_path(_object(stored_plan.get("template"), "matrix plan template").get("path"), "template path")
    )
    specification_path = _path_from_declared(
        _windows_path(
            _object(stored_plan.get("specification"), "matrix plan specification").get("path"),
            "specification path",
        )
    )
    template_path, specification_path = _source_paths(template_path, specification_path)
    template, template_bytes = _read_json(template_path, "template manifest")
    specification, specification_bytes = _read_json(specification_path, "matrix specification")
    expected_plan, manifests = _compile(
        template,
        template_bytes,
        template_path,
        specification,
        specification_bytes,
        specification_path,
    )
    _validate_prepared_filesystem(template)
    if stored_plan != expected_plan:
        raise MatrixError("matrix plan no longer matches its template/specification")
    expected_names: set[str] = set()
    expected_by_run: dict[str, dict[str, Any]] = {}
    for (declared_path, manifest, raw), launch in zip(manifests, expected_plan["launches"]):
        manifest_path = _path_from_declared(declared_path)
        _assert_reparse_free(manifest_path, "generated launch manifest")
        if not manifest_path.is_file():
            raise MatrixError(f"generated launch manifest is missing: {manifest_path}")
        actual = manifest_path.read_bytes()
        if _sha256(actual) != launch["manifest_sha256"] or actual != raw:
            raise MatrixError(f"generated launch manifest changed: {manifest_path}")
        expected_names.add(manifest_path.name)
        expected_by_run[launch["run_id"]] = manifest
    launches_root = _path_from_declared(ntpath.join(plan_root, "launches"))
    actual_names = {child.name for child in launches_root.iterdir()}
    if actual_names != expected_names:
        raise MatrixError("launches directory has missing or unexpected entries")
    if require_results:
        previous_completed: datetime | None = None
        cooldown = timedelta(seconds=float(expected_plan["cooldown_seconds"]))
        for launch in expected_plan["launches"]:
            started, completed = _verify_result(
                launch, expected_by_run[launch["run_id"]]
            )
            if previous_completed is not None and started < previous_completed + cooldown:
                raise MatrixError(
                    f"result order/cooldown is invalid before sequence {launch['sequence']}"
                )
            previous_completed = completed
        final_launch = expected_plan["launches"][-1]
        _verify_matrix_final_integrity(
            final_launch, expected_by_run[final_launch["run_id"]]
        )
    return {
        "arm_count": expected_plan["expected_arm_count"],
        "trial_count": expected_plan["expected_trial_count"],
        "result_count": expected_plan["expected_trial_count"] if require_results else 0,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    plan_parser = subparsers.add_parser("plan", help="expand an immutable matrix plan")
    plan_parser.add_argument("--template", required=True, type=Path)
    plan_parser.add_argument("--spec", required=True, type=Path)
    verify_parser = subparsers.add_parser("verify", help="verify plan completeness and identities")
    verify_parser.add_argument("--plan", required=True, type=Path)
    verify_parser.add_argument(
        "--require-results",
        action="store_true",
        help="also require a successful immutable result bundle for every planned trial",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        if arguments.command == "plan":
            path = plan_matrix(arguments.template, arguments.spec)
            print(f"QB-REPLAY-MATRIX-PLAN-OK: {path}")
        else:
            counts = verify_matrix(arguments.plan, require_results=arguments.require_results)
            print(
                "QB-REPLAY-MATRIX-VERIFY-OK: "
                f"{counts['arm_count']} arm(s), {counts['trial_count']} trial(s), "
                f"{counts['result_count']} required result(s)"
            )
        return 0
    except (MatrixError, OSError) as error:
        print(f"QB-REPLAY-MATRIX-ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
