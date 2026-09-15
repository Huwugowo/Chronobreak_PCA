#!/usr/bin/env python3
"""Deterministic analyzer for QB-REPLAY-008 replay benchmark bundles.

The analyzer intentionally uses only the Python standard library.  Raw run
bundles may contain additional fields, but the identity, timing, accounting,
and terminal fields defined here are strict because losing any of them would
make a replay-performance conclusion ambiguous.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import statistics
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping, Sequence


SCHEMA_VERSION = 1
BENCHMARK_ID = "QB-REPLAY-008"
SENTINEL_NAME = ".chronobreak-replay-benchmark"
ARTIFACTS = (
    "manifest.json",
    "runner-metadata.json",
    "collection-result.json",
    "events.jsonl",
    "server_requests.jsonl",
    "process_samples.jsonl",
    "terminal.json",
)
MAX_COLLECTOR_FINALIZATION_DELAY_MS = 15_000.0
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
HASH_RE = re.compile(r"^[0-9a-fA-F]{64}$")
FAILURE_KINDS = {
    "media_error",
    "scenario_failed",
    "seek_timeout",
    "scenario_timeout",
    "recovery_exhausted",
    "degraded",
    "preview_degraded",
    "stale_result",
    "source_mutation",
    "event_loss",
    "export_invalid",
    "action_failed",
    "run_failed",
}
SCENARIO_START_KINDS = {"scenario_start", "scenario_started"}
SCENARIO_END_KINDS = {"scenario_end", "scenario_complete", "scenario_completed"}
ACTION_REQUEST_KINDS = {
    "action_requested",
    "seek_requested",
    "rate_requested",
    "export_requested",
    "play_requested",
    "pause_requested",
    "open_requested",
}
ACTION_COMPLETE_KINDS = {
    "action_complete",
    "action_completed",
    "seek_settled",
    "rate_applied",
    "export_complete",
    "export_completed",
    "play_complete",
    "play_completed",
    "pause_complete",
    "pause_completed",
    "open_complete",
    "open_completed",
}
ACTION_SHORT_CIRCUIT_KINDS = {
    "seek_coalesced",
    "seek_deduped",
    "action_coalesced",
    "action_deduped",
    "action_cancelled_superseded",
    "seek_pending_replaced",
}
SEEK_DISPATCH_KINDS = {"seek_dispatched", "action_dispatched"}
SEEKED_KINDS = {"seeked", "seek_native_complete"}
PRESENTED_KINDS = {
    "seek_presented",
    "first_presented_frame",
    "authoritative_frame_presented",
}


class InvalidData(ValueError):
    """Raised when a run cannot support benchmark conclusions."""


@dataclass(frozen=True, order=True)
class TrialKey:
    scenario_id: str
    trial_id: str


@dataclass
class RunBundle:
    alias: str
    manifest: dict[str, Any]
    runner_metadata: dict[str, Any]
    collection: dict[str, Any]
    expected: dict[TrialKey, dict[str, Any]]
    events: list[dict[str, Any]]
    requests: list[dict[str, Any]]
    samples: list[dict[str, Any]]
    terminal: dict[str, Any]
    identity_by_scenario: dict[str, str]
    control_identity_by_scenario: dict[str, str]
    subject_by_scenario: dict[str, str]
    export_metrics_by_trial: dict[TrialKey, dict[str, list[float]]]


def _canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _digest(value: Any) -> str:
    return hashlib.sha256(_canonical_json(value).encode("utf-8")).hexdigest()


def _kind(record: Mapping[str, Any]) -> str:
    return str(record.get("kind", "")).strip().lower().replace("-", "_")


def _field(record: Mapping[str, Any], name: str, default: Any = None) -> Any:
    if name in record:
        return record[name]
    payload = record.get("payload")
    if isinstance(payload, Mapping):
        return payload.get(name, default)
    return default


def _identifier(value: Any, label: str) -> str:
    if isinstance(value, bool) or not isinstance(value, (str, int)):
        raise InvalidData(f"{label} must be a string or integer identifier")
    text = str(value)
    if not ID_RE.fullmatch(text):
        raise InvalidData(f"{label} is not a safe benchmark identifier")
    return text


def _finite_number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise InvalidData(f"{label} must be numeric")
    result = float(value)
    if not math.isfinite(result):
        raise InvalidData(f"{label} must be finite")
    return result


def _nonnegative_number(value: Any, label: str) -> float:
    result = _finite_number(value, label)
    if result < 0:
        raise InvalidData(f"{label} must be nonnegative")
    return result


def _schema(record: Mapping[str, Any], label: str) -> None:
    if record.get("schema_version") != SCHEMA_VERSION:
        raise InvalidData(f"{label} schema_version must be {SCHEMA_VERSION}")


def _read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise InvalidData(f"could not read {label}: {error}") from error
    if not isinstance(value, dict):
        raise InvalidData(f"{label} must contain a JSON object")
    return value


def _read_jsonl(path: Path, label: str) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    try:
        with path.open("r", encoding="utf-8") as stream:
            for line_number, line in enumerate(stream, 1):
                if not line.strip():
                    raise InvalidData(f"{label} line {line_number} is blank")
                try:
                    value = json.loads(line)
                except json.JSONDecodeError as error:
                    raise InvalidData(f"{label} line {line_number} is malformed: {error}") from error
                if not isinstance(value, dict):
                    raise InvalidData(f"{label} line {line_number} must be an object")
                records.append(value)
    except OSError as error:
        raise InvalidData(f"could not read {label}: {error}") from error
    return records


def _inside(root: Path, candidate: Path) -> bool:
    try:
        candidate.relative_to(root)
        return True
    except ValueError:
        return False


def _validate_roots(manifest: Mapping[str, Any], bundle_dir: Path) -> None:
    required = (
        "sentinel_root",
        "library_root",
        "config_path",
        "app_data_root",
        "result_root",
        "scratch_root",
    )
    paths: dict[str, Path] = {}
    for name in required:
        raw = manifest.get(name)
        if not isinstance(raw, str) or not raw:
            raise InvalidData(f"manifest {name} must be a nonempty absolute path")
        path = Path(raw)
        if not path.is_absolute():
            raise InvalidData(f"manifest {name} must be absolute")
        try:
            paths[name] = path.resolve(strict=name in {"sentinel_root", "result_root"})
        except OSError as error:
            raise InvalidData(f"manifest {name} cannot be resolved: {error}") from error
    sentinel = paths["sentinel_root"]
    if not sentinel.is_dir() or sentinel.name != SENTINEL_NAME:
        raise InvalidData(f"sentinel_root must be a directory named {SENTINEL_NAME}")
    for name, path in paths.items():
        if name != "sentinel_root" and not _inside(sentinel, path):
            raise InvalidData(f"manifest {name} escapes sentinel_root")
    if not _inside(paths["result_root"], bundle_dir.resolve()):
        raise InvalidData("run bundle is outside manifest result_root")


def _normalize_scenarios(manifest: Mapping[str, Any]) -> dict[TrialKey, dict[str, Any]]:
    scenarios = manifest.get("scenarios")
    if not isinstance(scenarios, list) or not scenarios:
        raise InvalidData("manifest scenarios must be a nonempty array")
    expected: dict[TrialKey, dict[str, Any]] = {}
    for index, scenario in enumerate(scenarios):
        if not isinstance(scenario, Mapping):
            raise InvalidData(f"manifest scenario {index} must be an object")
        scenario_id = _identifier(
            scenario.get("scenario_id", scenario.get("id")), f"scenario {index} id"
        )
        declared_trial_id = scenario.get("trial_id")
        raw_trials = (
            [declared_trial_id]
            if declared_trial_id is not None
            else scenario.get(
                "trial_ids",
                scenario.get(
                    "trials", scenario.get("trial_count", scenario.get("repetitions"))
                ),
            )
        )
        trial_specs: list[tuple[str, Mapping[str, Any]]] = []
        if isinstance(raw_trials, int) and not isinstance(raw_trials, bool) and raw_trials > 0:
            trial_specs = [(str(number), {}) for number in range(1, raw_trials + 1)]
        elif isinstance(raw_trials, list) and raw_trials:
            for trial in raw_trials:
                if isinstance(trial, Mapping):
                    trial_id = _identifier(
                        trial.get("trial_id", trial.get("id")),
                        f"scenario {scenario_id} trial id",
                    )
                    trial_specs.append((trial_id, trial))
                else:
                    trial_specs.append(
                        (_identifier(trial, f"scenario {scenario_id} trial id"), {})
                    )
        else:
            raise InvalidData(f"scenario {scenario_id} must declare trials")
        for trial_id, trial in trial_specs:
            key = TrialKey(scenario_id, trial_id)
            if key in expected:
                raise InvalidData(f"duplicate expected trial {scenario_id}/{trial_id}")
            merged = dict(scenario)
            merged.update(trial)
            action_count = merged.get(
                "expected_action_count", merged.get("action_count", merged.get("actions"))
            )
            if isinstance(action_count, list):
                action_count = len(action_count)
            if action_count is not None and (
                isinstance(action_count, bool)
                or not isinstance(action_count, int)
                or action_count < 0
            ):
                raise InvalidData(
                    f"scenario {scenario_id}/{trial_id} action count must be nonnegative"
                )
            merged["expected_action_count"] = action_count
            merged["max_telemetry_gap_ms"] = merged.get(
                "max_telemetry_gap_ms",
                manifest.get("max_telemetry_gap_ms", 2_500),
            )
            _nonnegative_number(
                merged["max_telemetry_gap_ms"],
                f"scenario {scenario_id}/{trial_id} max telemetry gap",
            )
            expected[key] = merged
    return expected


def _validate_fixtures(manifest: Mapping[str, Any]) -> set[str]:
    fixtures = manifest.get("fixtures")
    if not isinstance(fixtures, list) or not fixtures:
        raise InvalidData("manifest fixtures must be a nonempty array")
    identities: set[str] = set()
    sentinel = Path(str(manifest["sentinel_root"])).resolve()
    for index, fixture in enumerate(fixtures):
        if not isinstance(fixture, Mapping):
            raise InvalidData(f"manifest fixture {index} must be an object")
        fixture_id = _identifier(
            fixture.get("fixture_id", fixture.get("id")), f"manifest fixture {index} id"
        )
        if fixture_id in identities:
            raise InvalidData(f"duplicate manifest fixture {fixture_id}")
        identities.add(fixture_id)
        direct_digest = fixture.get("media_sha256", fixture.get("sha256"))
        direct_size = fixture.get("size_bytes")
        files = fixture.get("files")
        if direct_digest is not None or direct_size is not None:
            if not isinstance(direct_digest, str) or not HASH_RE.fullmatch(direct_digest):
                raise InvalidData(f"manifest fixture {fixture_id} must have a SHA-256 identity")
            if isinstance(direct_size, bool) or not isinstance(direct_size, int) or direct_size <= 0:
                raise InvalidData(f"manifest fixture {fixture_id} must have a positive size_bytes")
        elif isinstance(files, list) and files:
            for file_index, file_identity in enumerate(files):
                if not isinstance(file_identity, Mapping):
                    raise InvalidData(f"manifest fixture {fixture_id} file {file_index} must be an object")
                digest = file_identity.get("sha256")
                size = file_identity.get("size_bytes")
                if not isinstance(digest, str) or not HASH_RE.fullmatch(digest):
                    raise InvalidData(
                        f"manifest fixture {fixture_id} file {file_index} must have a SHA-256 identity"
                    )
                if isinstance(size, bool) or not isinstance(size, int) or size < 0:
                    raise InvalidData(
                        f"manifest fixture {fixture_id} file {file_index} has invalid size_bytes"
                    )
                _validate_fixture_path(
                    file_identity.get("relative_path"),
                    sentinel,
                    f"manifest fixture {fixture_id} file {file_index} relative_path",
                    relative=True,
                )
        else:
            raise InvalidData(f"manifest fixture {fixture_id} must identify its files")
        relative_path = fixture.get("relative_path")
        if relative_path is not None:
            _validate_fixture_path(
                relative_path,
                sentinel,
                f"manifest fixture {fixture_id} relative_path",
                relative=True,
            )
        for key, value in fixture.items():
            lowered = str(key).lower()
            if "path" not in lowered or value is None or lowered == "relative_path":
                continue
            _validate_fixture_path(
                value,
                sentinel,
                f"manifest fixture {fixture_id} {key}",
                relative=False,
            )
    return identities


def _validate_fixture_path(value: Any, sentinel: Path, label: str, *, relative: bool) -> None:
    if not isinstance(value, str) or not value:
        raise InvalidData(f"{label} must be a nonempty path")
    path = Path(value)
    if relative:
        if path.is_absolute() or ".." in path.parts:
            raise InvalidData(f"{label} must remain relative to sentinel_root")
        candidate = (sentinel / path).resolve()
    else:
        if not path.is_absolute():
            raise InvalidData(f"{label} must be absolute")
        candidate = path.resolve()
    if not _inside(sentinel, candidate):
        raise InvalidData(f"{label} escapes sentinel_root")


def _stable_identity(value: Any) -> Any:
    """Remove location/run-local fields before fingerprint comparison.

    Raw manifests retain paths for safety checks, but relocating a copied fixture
    root must not make an otherwise matched before/after arm incomparable.
    """
    if isinstance(value, Mapping):
        result: dict[str, Any] = {}
        for raw_key, child in value.items():
            key = str(raw_key)
            lowered = key.lower()
            if any(
                token in lowered
                for token in (
                    "path",
                    "root",
                    "command",
                    "token",
                    "username",
                    "summoner",
                    "player_name",
                    "hostname",
                    "run_id",
                    "started_at",
                    "timestamp",
                )
            ):
                continue
            result[key] = _stable_identity(child)
        return result
    if isinstance(value, list):
        return [_stable_identity(child) for child in value]
    return value


def _comparison_identity(value: Any) -> Any:
    """Remove the implementation-under-test identity for before/after matching."""
    if isinstance(value, Mapping):
        result: dict[str, Any] = {}
        for raw_key, child in value.items():
            key = str(raw_key)
            lowered = key.lower()
            if (
                lowered in {"app", "application", "subject"}
                or (
                    "app" in lowered
                    and any(
                        token in lowered
                        for token in ("sha", "hash", "revision", "dirty", "version")
                    )
                )
                or lowered in {"revision", "git_revision", "dirty", "binary_sha256"}
            ):
                continue
            result[key] = _comparison_identity(child)
        return result
    if isinstance(value, list):
        return [_comparison_identity(child) for child in value]
    return value


def _runner_environment_material(metadata: Mapping[str, Any]) -> dict[str, Any]:
    return {
        "config_sha256": metadata.get("config_sha256"),
        "observer_cadence_ms": metadata.get("observer_cadence_ms"),
        "logical_processors": metadata.get("logical_processors"),
        "cpu_accounting_quantum_ms": metadata.get("cpu_accounting_quantum_ms"),
        "cpu_accounting_quantum_method": metadata.get("cpu_accounting_quantum_method"),
        "cpu_accounting_reported_counter_unit_ms": metadata.get(
            "cpu_accounting_reported_counter_unit_ms"
        ),
        "cpu_accounting_limitation": metadata.get("cpu_accounting_limitation"),
        "gpu_collection": metadata.get("gpu_collection"),
        "process_tree_collection_method": metadata.get("process_tree_collection_method"),
        "process_tree_assignment_limitation": metadata.get("process_tree_assignment_limitation"),
        "environment": _stable_identity(metadata.get("environment")),
        "media_runtime_id": metadata.get("media_runtime_id"),
        "webview2_runtime_versions": metadata.get("webview2_runtime_versions"),
        "webview2_runtime_limitation": metadata.get("webview2_runtime_limitation"),
    }


def _identity_material(
    manifest: Mapping[str, Any],
    scenario: Mapping[str, Any],
    runner_metadata: Mapping[str, Any],
) -> dict[str, Any]:
    fixture_refs = scenario.get("fixture_ids")
    if not isinstance(fixture_refs, list):
        fixture_ref = scenario.get("fixture_id", scenario.get("fixture"))
        fixture_refs = [] if fixture_ref is None else [fixture_ref]
    fixture_identities: list[Any] = []
    fixtures = manifest.get("fixtures")
    if isinstance(fixtures, list):
        for fixture in fixtures:
            if not isinstance(fixture, Mapping):
                continue
            if fixture.get("id", fixture.get("fixture_id")) not in fixture_refs:
                continue
            fixture_identities.append(
                _stable_identity(
                    {
                        key: fixture[key]
                        for key in sorted(fixture)
                        if key
                        in {
                            "id",
                            "fixture_id",
                            "sha256",
                            "media_sha256",
                            "size_bytes",
                            "files",
                            "codec",
                            "profile",
                            "duration_ms",
                        }
                    }
                )
            )
    scenario_identity = {
        key: value
        for key, value in scenario.items()
        if key
        not in {
            "trial_id",
            "trial_ids",
            "trials",
            "trial_count",
            "repetitions",
            "max_telemetry_gap_ms",
        }
    }
    return {
        "fingerprints": _comparison_identity(_stable_identity(manifest.get("fingerprints", {}))),
        "environment": _stable_identity(manifest.get("environment", {})),
        "observer_profile": manifest.get("observer_profile"),
        "ddragon": _stable_identity(manifest.get("ddragon")),
        "media_tools": _stable_identity(manifest.get("media_tools", {})),
        "runner_environment": _runner_environment_material(runner_metadata),
        "fixtures": sorted(fixture_identities, key=_canonical_json),
        "scenario": _stable_identity(scenario_identity),
    }


def _subject_material(
    manifest: Mapping[str, Any], runner_metadata: Mapping[str, Any]
) -> Any:
    fingerprints = _stable_identity(manifest.get("fingerprints", {}))
    manifest_subject = (
        {
            key: value
            for key, value in fingerprints.items()
            if "app" in key.lower()
            or key.lower() in {"revision", "git_revision", "dirty", "binary_sha256"}
        }
        if isinstance(fingerprints, Mapping)
        else fingerprints
    )
    return {
        "manifest": manifest_subject,
        "app_binary_sha256": runner_metadata.get("app_binary_sha256"),
        "source": _stable_identity(runner_metadata.get("source")),
    }


def _control_identity_material(
    manifest: Mapping[str, Any],
    scenario: Mapping[str, Any],
    runner_metadata: Mapping[str, Any],
) -> dict[str, Any]:
    material = _identity_material(manifest, scenario, runner_metadata)
    material.pop("observer_profile", None)
    return material


def _validate_envelope(
    record: Mapping[str, Any],
    label: str,
    run_id: str,
    expected: Mapping[TrialKey, Mapping[str, Any]],
) -> TrialKey:
    _schema(record, label)
    if record.get("run_id") != run_id:
        raise InvalidData(f"{label} run_id does not match manifest")
    scenario_id = _identifier(record.get("scenario_id"), f"{label} scenario_id")
    trial_id = _identifier(record.get("trial_id"), f"{label} trial_id")
    key = TrialKey(scenario_id, trial_id)
    if key not in expected:
        raise InvalidData(f"{label} references undeclared trial {scenario_id}/{trial_id}")
    _nonnegative_number(record.get("monotonic_ms"), f"{label} monotonic_ms")
    _identifier(record.get("source"), f"{label} source")
    _identifier(record.get("kind"), f"{label} kind")
    payload = record.get("payload")
    if payload is not None and not isinstance(payload, Mapping):
        raise InvalidData(f"{label} payload must be an object")
    generation = _field(record, "generation")
    if generation is not None and (
        isinstance(generation, bool) or not isinstance(generation, int) or generation < 0
    ):
        raise InvalidData(f"{label} generation must be a nonnegative integer")
    action_id = _field(record, "action_id")
    if action_id is not None:
        _identifier(action_id, f"{label} action_id")
    return key


def _validate_log_order(records: Sequence[Mapping[str, Any]], label: str) -> None:
    previous = -math.inf
    for index, record in enumerate(records, 1):
        timestamp = _finite_number(record["monotonic_ms"], f"{label} record {index} time")
        if timestamp < previous:
            raise InvalidData(f"{label} monotonic timestamp regressed at record {index}")
        previous = timestamp


def _counter(mapping: Mapping[str, Any], section: str, name: str) -> int:
    nested = _field(mapping, section)
    value = nested.get(name) if isinstance(nested, Mapping) else None
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise InvalidData(f"terminal {section}.{name} must be a nonnegative integer")
    return value


def _validate_terminal(
    terminal: Mapping[str, Any], run_id: str, counts: Mapping[str, int]
) -> None:
    _schema(terminal, "terminal")
    if terminal.get("run_id") != run_id:
        raise InvalidData("terminal run_id does not match manifest")
    status = str(_field(terminal, "status", "")).lower()
    if status not in {"complete", "completed", "success", "passed"}:
        raise InvalidData("terminal status is unfinished or unsuccessful")
    if _field(terminal, "completed", True) is not True:
        raise InvalidData("terminal completion flag is false")
    for name, actual in counts.items():
        if _counter(terminal, "record_counts", name) != actual:
            raise InvalidData(f"terminal record count for {name} does not match artifact")
        if _counter(terminal, "dropped_counts", name) != 0:
            raise InvalidData(f"terminal reports lost {name} records")
    if _field(terminal, "source_hash_unchanged") is not True:
        raise InvalidData("terminal does not prove source hashes remained unchanged")
    for field in ("media_integrity_ok", "export_integrity_ok"):
        value = _field(terminal, field)
        if value is not None and value is not True:
            raise InvalidData(f"terminal {field} is false")
    queue = _field(terminal, "event_queue")
    if isinstance(queue, Mapping):
        dropped = queue.get("dropped_records")
        if dropped is not None and _nonnegative_number(dropped, "terminal queue drops") > 0:
            raise InvalidData("terminal event queue reports lost records")
        capacity = queue.get("capacity")
        high_water = queue.get("high_water_mark")
        if capacity is not None and high_water is not None:
            if _nonnegative_number(high_water, "terminal queue high-water mark") > _nonnegative_number(
                capacity, "terminal queue capacity"
            ):
                raise InvalidData("terminal event queue high-water mark exceeds capacity")


def _validate_collection(
    collection: Mapping[str, Any], run_id: str, terminal: Mapping[str, Any]
) -> None:
    _schema(collection, "collection result")
    if collection.get("run_id") != run_id:
        raise InvalidData("collection result run_id does not match manifest")
    duration_ms = _nonnegative_number(
        collection.get("duration_ms"), "collection duration_ms"
    )
    _nonnegative_number(
        collection.get("post_hash_elapsed_ms"), "collection post_hash_elapsed_ms"
    )
    app_pid = collection.get("app_pid")
    if isinstance(app_pid, bool) or not isinstance(app_pid, int) or app_pid < 1:
        raise InvalidData("collection app_pid must be a positive integer")
    if collection.get("app_exit_code") != 0:
        raise InvalidData("collection app exit code is not zero")
    if collection.get("timed_out") is not False:
        raise InvalidData("collection reports a finite-timeout failure")
    if collection.get("startup_timed_out") is not False:
        raise InvalidData("collection reports a frontend-startup timeout")
    if collection.get("frontend_startup_observed") is not True:
        raise InvalidData("collection did not observe frontend startup")
    if collection.get("fixture_hashes_match") is not True:
        raise InvalidData("collection fixture post-hashes do not match")
    if collection.get("forced_termination_reason") not in {None, ""}:
        raise InvalidData("collection required forced process-tree termination")

    accounting = collection.get("job_accounting")
    if not isinstance(accounting, Mapping):
        raise InvalidData("collection job_accounting must be an object")
    total = _nonnegative_integer(
        accounting.get("total_processes"), "collection total Job processes"
    )
    active = _nonnegative_integer(
        accounting.get("active_processes"), "collection active Job processes"
    )
    unobserved = _nonnegative_integer(
        accounting.get("unobserved_process_count"),
        "collection unobserved Job processes",
    )
    notifications = _nonnegative_integer(
        accounting.get("new_process_notification_count"),
        "collection Job new-process notifications",
    )
    if total < 1 or active != 0 or unobserved != 0 or notifications != total:
        raise InvalidData("collection Job process accounting is incomplete")

    terminal_elapsed_ms = _nonnegative_number(
        terminal.get("elapsed_ms"), "terminal elapsed_ms"
    )
    finalization_delay_ms = duration_ms - terminal_elapsed_ms
    if finalization_delay_ms < 0:
        raise InvalidData("collection duration precedes terminal completion")
    if finalization_delay_ms > MAX_COLLECTOR_FINALIZATION_DELAY_MS:
        raise InvalidData(
            "collector finalization exceeded the 15-second post-terminal bound"
        )


def _validate_scenario_evidence(
    key: TrialKey,
    spec: Mapping[str, Any],
    trial_events: Sequence[Mapping[str, Any]],
) -> None:
    scenario_kind = str(spec.get("kind", "")).lower()
    if not scenario_kind:
        # Legacy analyzer fixtures predate the production launch-manifest shape.
        return
    kinds = [_kind(event) for event in trial_events]

    def require(*required: str) -> None:
        missing = [kind for kind in required if kind not in kinds]
        if missing:
            raise InvalidData(
                f"trial {key.scenario_id}/{key.trial_id} {scenario_kind} missing required "
                f"evidence: {', '.join(missing)}"
            )

    require("library_requested", "library_useful")
    if scenario_kind == "app_idle":
        return
    if scenario_kind == "export":
        require("export_requested", "export_completed", "scenario_completed")
        completed = next(event for event in trial_events if _kind(event) == "export_completed")
        if (
            _field(completed, "strategy") != "full_reencode"
            or _field(completed, "copy_strategy") != "not_implemented"
            or _field(completed, "hybrid_strategy") != "not_implemented"
            or _field(completed, "benchmark_scope") != "backend_command"
            or _field(completed, "ui_workflow_included") is not False
        ):
            raise InvalidData("export evidence does not truthfully identify the measured strategy/scope")
        return

    require(
        "replay_requested",
        "playback_payload_ready",
        "viewer_mounted",
        "media_loadstart",
        "metadata_ready",
        "media_canplay",
        "first_presented_frame",
        "scenario_completed",
    )
    if scenario_kind in {"cold_open", "warm_open"}:
        require(
            "play_requested",
            "play_complete",
            "steady_playback",
            "pause_requested",
            "pause_complete",
        )
    if scenario_kind == "play_pause":
        require("play_requested", "play_complete", "pause_requested", "pause_complete")
        if kinds.count("play_requested") < 2 or kinds.count("pause_requested") < 2:
            raise InvalidData("play_pause evidence lacks both deterministic transition cycles")
    if scenario_kind == "rate":
        require("rate_requested", "rate_applied", "rate_observed")
        declared_rates = spec.get("rates", [0.25, 0.5, 1, 2, 4, 8])
        expected_rates = {float(rate) for rate in declared_rates}
        observed_rates = {
            float(_field(event, "requested_rate"))
            for event in trial_events
            if _kind(event) == "rate_observed"
            and isinstance(_field(event, "requested_rate"), (int, float))
            and not isinstance(_field(event, "requested_rate"), bool)
        }
        if not expected_rates.issubset(observed_rates):
            raise InvalidData("rate evidence does not cover the declared capability ladder")
    if scenario_kind == "seek":
        require("seek_requested", "seek_dispatched", "seeked", "seek_presented")
        targets = spec.get("target_times_ms")
        reason = str(spec.get("seek_reason", "benchmark"))
        requested_for_reason = sum(
            1
            for event in trial_events
            if _kind(event) == "seek_requested"
            and str(_field(event, "reason")) == reason
        )
        if isinstance(targets, list) and requested_for_reason != len(targets):
            raise InvalidData("seek evidence requested count does not match target_times_ms")
        if reason == "event-jump":
            require("event_jump_selected")
        elif reason == "endpoint-edit":
            require("clip_endpoint_updated")
    if scenario_kind == "scrub":
        require("scrub_started", "scrub_settled", "seek_requested")
        started_count = sum(
            int(_nonnegative_number(_field(event, "request_count"), "scrub request_count"))
            for event in trial_events
            if _kind(event) == "scrub_started"
        )
        settled_count = sum(
            int(_nonnegative_number(_field(event, "request_count"), "scrub request_count"))
            for event in trial_events
            if _kind(event) == "scrub_settled"
        )
        requested_count = kinds.count("seek_requested")
        if started_count != settled_count or started_count != requested_count:
            raise InvalidData("scrub requested/settled/actual seek counts do not reconcile")
    if scenario_kind == "layout":
        require("fullscreen_requested", "fullscreen_applied", "clip_mode_applied", "seek_dispatched")
        for kind in ("fullscreen_applied", "clip_mode_applied"):
            states = {
                _field(event, "enabled")
                for event in trial_events
                if _kind(event) == kind
            }
            if not {False, True}.issubset(states):
                raise InvalidData(f"layout evidence does not cover both {kind} states")
        layout_generations = {
            _field(event, "generation")
            for event in trial_events
            if _kind(event)
            in {"fullscreen_requested", "fullscreen_applied", "clip_mode_applied", "seek_dispatched"}
        }
        layout_generations.discard(None)
        if len(layout_generations) != 1:
            raise InvalidData("layout evidence does not preserve one media generation")
    if scenario_kind == "warm_open":
        expected_cycles = int(spec.get("iterations", 6))
        if (
            kinds.count("viewer_cycle_completed") != expected_cycles
            or kinds.count("viewer_reopen_requested") != expected_cycles - 1
        ):
            raise InvalidData("warm_open viewer cycle counts do not match the manifest")
    if scenario_kind == "lifecycle":
        if spec.get("duration_seconds") is None:
            expected_cycles = int(spec.get("iterations", 20))
            if (
                kinds.count("viewer_cycle_completed") != expected_cycles
                or kinds.count("viewer_reopen_requested") != expected_cycles - 1
                or kinds.count("viewer_close_requested") != 1
            ):
                raise InvalidData("lifecycle viewer open/close counts do not match the manifest")
        else:
            require("seek_requested", "rate_observed")


def _validate_events(
    events: Sequence[dict[str, Any]],
    expected: Mapping[TrialKey, Mapping[str, Any]],
) -> dict[TrialKey, dict[str, list[dict[str, Any]]]]:
    by_trial: dict[TrialKey, list[dict[str, Any]]] = defaultdict(list)
    for event in events:
        by_trial[TrialKey(str(event["scenario_id"]), str(event["trial_id"]))].append(event)
        if _kind(event) in FAILURE_KINDS:
            raise InvalidData(f"trial emitted reliability failure {_kind(event)}")
        if _kind(event) in PRESENTED_KINDS and _field(event, "authoritative") is not True:
            raise InvalidData(
                f"trial uses non-authoritative presentation evidence in {_kind(event)}"
            )
        if _field(event, "event_loss", False) is True or _field(event, "stale", False) is True:
            raise InvalidData("trial reports event loss or a stale result")
        for field in (
            "local_dropped",
            "remote_dropped",
            "overwritten_records",
            "dropped_records",
            "event_loss_count",
        ):
            value = _field(event, field)
            if value is not None and _nonnegative_number(value, f"event {field}") > 0:
                raise InvalidData(f"trial reports event loss in {field}")

    actions_by_trial: dict[TrialKey, dict[str, list[dict[str, Any]]]] = {}
    for key, spec in expected.items():
        trial_events = by_trial.get(key, [])
        starts = [event for event in trial_events if _kind(event) in SCENARIO_START_KINDS]
        ends = [event for event in trial_events if _kind(event) in SCENARIO_END_KINDS]
        if len(starts) != 1 or len(ends) != 1:
            raise InvalidData(f"trial {key.scenario_id}/{key.trial_id} is missing one start/end")
        if starts[0]["monotonic_ms"] > ends[0]["monotonic_ms"]:
            raise InvalidData(f"trial {key.scenario_id}/{key.trial_id} ends before it starts")
        _validate_scenario_evidence(key, spec, trial_events)
        action_events: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for event in trial_events:
            action_id = _field(event, "action_id")
            if action_id is not None:
                action_events[str(action_id)].append(event)
        requested = 0
        requested_generations: list[tuple[float, int]] = []
        for action_id, records in sorted(action_events.items()):
            kinds = [_kind(record) for record in records]
            requests = [record for record in records if _kind(record) in ACTION_REQUEST_KINDS]
            if len(requests) != 1:
                raise InvalidData(
                    f"action {key.scenario_id}/{key.trial_id}/{action_id} must have one request"
                )
            requested += 1
            generations = {
                _field(record, "generation")
                for record in records
                if _field(record, "generation") is not None
            }
            if len(generations) > 1:
                raise InvalidData(f"action {action_id} crosses media generations")
            action_kind = str(
                _field(requests[0], "action_kind", _kind(requests[0]).removesuffix("_requested"))
            ).lower()
            if action_kind == "seek" or _kind(requests[0]) == "seek_requested":
                if not generations:
                    raise InvalidData(f"seek action {action_id} has no generation")
                generation = next(iter(generations))
                assert isinstance(generation, int)
                requested_generations.append((float(requests[0]["monotonic_ms"]), generation))
                if any(kind in ACTION_SHORT_CIRCUIT_KINDS for kind in kinds):
                    continue
                phases = [
                    next((i for i, record in enumerate(records) if _kind(record) in phase), None)
                    for phase in (SEEK_DISPATCH_KINDS, SEEKED_KINDS, PRESENTED_KINDS)
                ]
                # RVFC and native seeked are independent browser observations. The
                # controller supports either completion order, but both must follow
                # dispatch and both remain required for a completed seek.
                if any(index is None for index in phases) or not (
                    phases[0] < phases[1] and phases[0] < phases[2]
                ):
                    raise InvalidData(f"seek action {action_id} has incomplete/impossible phases")
            elif not any(
                kind in ACTION_COMPLETE_KINDS or kind in ACTION_SHORT_CIRCUIT_KINDS
                for kind in kinds
            ):
                raise InvalidData(f"action {action_id} has no terminal event")
        chronological_generations = [
            generation for _, generation in sorted(requested_generations)
        ]
        if chronological_generations != sorted(chronological_generations):
            raise InvalidData(f"trial {key.scenario_id}/{key.trial_id} generation ordering regressed")
        expected_action_count = spec["expected_action_count"]
        if expected_action_count is not None and requested != expected_action_count:
            raise InvalidData(
                f"trial {key.scenario_id}/{key.trial_id} has {requested} actions, expected "
                f"{expected_action_count}"
            )
        actions_by_trial[key] = dict(action_events)
    return actions_by_trial


def _validate_requests(
    requests: Sequence[dict[str, Any]],
    actions: Mapping[TrialKey, Mapping[str, Sequence[Mapping[str, Any]]]],
) -> None:
    seen: set[str] = set()
    for request in requests:
        request_id = _identifier(_field(request, "request_id"), "server request_id")
        if request_id in seen:
            raise InvalidData(f"duplicate server request_id {request_id}")
        seen.add(request_id)
        _identifier(_field(request, "route_class"), f"request {request_id} route_class")
        outcome = str(_field(request, "outcome", "")).lower()
        if outcome not in {"completed", "cancelled"}:
            raise InvalidData(f"request {request_id} has invalid/error outcome")
        started = _nonnegative_number(
            _field(request, "started_ms", request["monotonic_ms"]),
            f"request {request_id} started_ms",
        )
        first_byte = _field(request, "first_byte_ms")
        completed = _field(request, "completed_ms")
        if first_byte is not None and _nonnegative_number(first_byte, "first_byte_ms") < started:
            raise InvalidData(f"request {request_id} first byte precedes start")
        if completed is None or _nonnegative_number(completed, "completed_ms") < started:
            raise InvalidData(f"request {request_id} lacks a valid completion time")
        declared = _field(request, "declared_bytes")
        delivered = _field(request, "delivered_bytes")
        if declared is not None:
            declared = _nonnegative_number(declared, "declared_bytes")
        if delivered is not None:
            delivered = _nonnegative_number(delivered, "delivered_bytes")
        if declared is not None and delivered is not None and delivered > declared:
            raise InvalidData(f"request {request_id} delivered more than declared")
        method = str(_field(request, "method", "GET")).upper()
        if outcome == "completed" and declared is not None:
            if method == "HEAD" and delivered != 0:
                raise InvalidData(f"completed HEAD request {request_id} delivered body bytes")
            if method != "HEAD" and delivered != declared:
                raise InvalidData(f"completed request {request_id} byte accounting disagrees")
        key = TrialKey(str(request["scenario_id"]), str(request["trial_id"]))
        action_id = _field(request, "action_id")
        if action_id is not None:
            action_id = str(action_id)
            if action_id not in actions.get(key, {}):
                raise InvalidData(f"request {request_id} references unknown action {action_id}")
            request_generation = _field(request, "generation")
            action_generations = {
                _field(event, "generation")
                for event in actions[key][action_id]
                if _field(event, "generation") is not None
            }
            if request_generation is not None and action_generations != {request_generation}:
                raise InvalidData(f"request {request_id} generation disagrees with action")


def _validate_samples(
    samples: Sequence[dict[str, Any]], expected: Mapping[TrialKey, Mapping[str, Any]]
) -> None:
    by_trial: dict[TrialKey, list[dict[str, Any]]] = defaultdict(list)
    for sample in samples:
        key = TrialKey(str(sample["scenario_id"]), str(sample["trial_id"]))
        by_trial[key].append(sample)
        if _field(sample, "unexpected_process_disappearance", False) is True:
            raise InvalidData("process sampler reports an unexpected disappearance")
        if _field(sample, "cadence_gap", False) is True:
            raise InvalidData("process sampler reports a telemetry gap")
        unobserved = _field(sample, "job_unobserved_process_count", 0)
        if _nonnegative_number(unobserved, "unobserved Job Object process count") > 0:
            raise InvalidData("process sampler missed one or more Job Object members")
        process_count = _field(sample, "process_count", _field(sample, "process_tree_count"))
        if process_count is None:
            raise InvalidData("process sample is missing process_count")
        if _nonnegative_number(process_count, "process_count") < 1:
            raise InvalidData("process sampler lost the complete process tree")
        cpu = next(
            (
                _field(sample, field)
                for field in ("process_tree_cpu_percent", "normalized_cpu_percent", "cpu_percent")
                if _field(sample, field) is not None
            ),
            None,
        )
        memory = next(
            (
                _field(sample, field)
                for field in ("process_tree_private_bytes", "private_bytes", "working_set_bytes")
                if _field(sample, field) is not None
            ),
            None,
        )
        if cpu is None or memory is None:
            raise InvalidData("process sample is missing core CPU/private-memory telemetry")
        _nonnegative_number(cpu, "process sample CPU")
        _nonnegative_number(memory, "process sample private memory")
    for key, spec in expected.items():
        trial_samples = by_trial.get(key, [])
        if len(trial_samples) < 2:
            raise InvalidData(f"trial {key.scenario_id}/{key.trial_id} has insufficient process samples")
        maximum_gap = _finite_number(spec["max_telemetry_gap_ms"], "maximum telemetry gap")
        for left, right in zip(trial_samples, trial_samples[1:]):
            gap = float(right["monotonic_ms"]) - float(left["monotonic_ms"])
            if gap > maximum_gap:
                raise InvalidData(
                    f"trial {key.scenario_id}/{key.trial_id} telemetry gap {gap:g} ms exceeds "
                    f"{maximum_gap:g} ms"
                )


def _validate_manifest(manifest: dict[str, Any], bundle_dir: Path) -> tuple[str, dict[TrialKey, dict[str, Any]]]:
    _schema(manifest, "manifest")
    if manifest.get("benchmark_id", BENCHMARK_ID) != BENCHMARK_ID:
        raise InvalidData(f"manifest benchmark_id must be {BENCHMARK_ID}")
    run_id = _identifier(manifest.get("run_id"), "manifest run_id")
    observer = _identifier(manifest.get("observer_profile"), "manifest observer_profile")
    if observer not in {"minimal", "full"}:
        raise InvalidData("manifest observer_profile must be minimal or full")
    if not isinstance(manifest.get("ddragon"), Mapping):
        raise InvalidData("manifest ddragon must be an object")
    _validate_roots(manifest, bundle_dir)
    fixture_ids = _validate_fixtures(manifest)
    expected = _normalize_scenarios(manifest)
    for key, scenario in expected.items():
        fixtures = scenario.get("fixture_ids")
        if not isinstance(fixtures, list):
            fixture = scenario.get("fixture_id", scenario.get("fixture"))
            fixtures = [] if fixture is None else [fixture]
        if not fixtures:
            raise InvalidData(f"scenario {key.scenario_id} must reference at least one fixture")
        for fixture in fixtures:
            if str(fixture) not in fixture_ids:
                raise InvalidData(f"scenario {key.scenario_id} references an unknown fixture")
    return run_id, expected


def _optional_limitation(value: Any, label: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str) or not value.strip():
        raise InvalidData(f"{label} must be null or a nonempty string")
    return value


def _validate_runner_metadata(
    metadata: dict[str, Any],
    manifest: Mapping[str, Any],
    manifest_path: Path,
    run_id: str,
) -> None:
    _schema(metadata, "runner metadata")
    required_fields = {
        "run_id",
        "manifest_sha256",
        "config_sha256",
        "app_binary_sha256",
        "observer_profile",
        "observer_cadence_ms",
        "logical_processors",
        "cpu_accounting_quantum_ms",
        "cpu_accounting_quantum_method",
        "cpu_accounting_reported_counter_unit_ms",
        "cpu_accounting_limitation",
        "gpu_collection",
        "process_tree_collection_method",
        "process_tree_assignment_limitation",
        "environment",
        "source",
        "media_runtime_id",
        "webview2_runtime_versions",
        "webview2_runtime_limitation",
    }
    missing = sorted(required_fields.difference(metadata))
    if missing:
        raise InvalidData(f"runner metadata is missing required field {missing[0]}")
    if metadata.get("run_id") != run_id:
        raise InvalidData("runner metadata run_id does not match manifest")
    _sha256(metadata.get("app_binary_sha256"), "runner app binary")
    manifest_digest = _sha256(metadata.get("manifest_sha256"), "runner manifest")
    try:
        actual_manifest_digest = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    except OSError as error:
        raise InvalidData(f"could not hash manifest for runner identity: {error}") from error
    if manifest_digest != actual_manifest_digest:
        raise InvalidData("runner manifest SHA-256 does not match manifest.json")
    config_digest = _sha256(metadata.get("config_sha256"), "runner config")
    config_path = Path(str(manifest.get("config_path", "")))
    try:
        actual_config_digest = hashlib.sha256(config_path.read_bytes()).hexdigest()
    except OSError as error:
        raise InvalidData(f"could not hash benchmark config for runner identity: {error}") from error
    if config_digest != actual_config_digest:
        raise InvalidData("runner config SHA-256 does not match benchmark config")
    if metadata.get("observer_profile") != manifest.get("observer_profile"):
        raise InvalidData("runner observer profile does not match manifest")
    if _nonnegative_number(
        metadata.get("observer_cadence_ms"), "runner observer cadence"
    ) <= 0:
        raise InvalidData("runner observer cadence must be positive")
    logical_processors = metadata.get("logical_processors")
    if (
        isinstance(logical_processors, bool)
        or not isinstance(logical_processors, int)
        or logical_processors < 1
    ):
        raise InvalidData("runner logical processor count must be a positive integer")
    if _nonnegative_number(
        metadata.get("cpu_accounting_quantum_ms"), "runner CPU accounting quantum"
    ) <= 0:
        raise InvalidData("runner CPU accounting quantum must be positive")
    if _nonnegative_number(
        metadata.get("cpu_accounting_reported_counter_unit_ms"),
        "runner CPU counter unit",
    ) <= 0:
        raise InvalidData("runner CPU counter unit must be positive")
    for name in (
        "cpu_accounting_quantum_method",
        "cpu_accounting_limitation",
        "gpu_collection",
        "process_tree_collection_method",
        "process_tree_assignment_limitation",
    ):
        if not isinstance(metadata.get(name), str) or not str(metadata[name]).strip():
            raise InvalidData(f"runner metadata {name} must be a nonempty string")

    environment = metadata.get("environment")
    if not isinstance(environment, Mapping):
        raise InvalidData("runner environment identity must be an object")
    environment_fields = {
        "os",
        "computer",
        "processors",
        "video_controllers",
        "powershell_version",
        "limitations",
    }
    missing_environment = sorted(environment_fields.difference(environment))
    if missing_environment:
        raise InvalidData(
            f"runner environment is missing required field {missing_environment[0]}"
        )
    for name in ("os", "computer"):
        value = environment.get(name)
        if value is not None and not isinstance(value, Mapping):
            raise InvalidData(f"runner environment {name} must be an object or null")
    for name in ("processors", "video_controllers"):
        value = environment.get(name)
        if not isinstance(value, list) or any(not isinstance(item, Mapping) for item in value):
            raise InvalidData(f"runner environment {name} must be an array of objects")
    if not isinstance(environment.get("powershell_version"), str) or not str(
        environment["powershell_version"]
    ).strip():
        raise InvalidData("runner environment PowerShell version must be nonempty")
    limitations = environment.get("limitations")
    if not isinstance(limitations, list) or any(
        not isinstance(item, str) or not item.strip() for item in limitations
    ):
        raise InvalidData("runner environment limitations must be an array of strings")
    if (
        environment.get("os") is None
        or environment.get("computer") is None
        or not environment.get("processors")
        or not environment.get("video_controllers")
    ) and not limitations:
        raise InvalidData("runner environment omissions require an explicit limitation")

    source = metadata.get("source")
    if not isinstance(source, Mapping):
        raise InvalidData("runner source identity must be an object")
    missing_source = sorted({"revision", "dirty", "limitation"}.difference(source))
    if missing_source:
        raise InvalidData(f"runner source is missing required field {missing_source[0]}")
    revision = source.get("revision")
    dirty = source.get("dirty")
    source_limitation = _optional_limitation(source.get("limitation"), "source limitation")
    if revision is not None and (not isinstance(revision, str) or not revision.strip()):
        raise InvalidData("runner source revision must be null or nonempty")
    if dirty is not None and not isinstance(dirty, bool):
        raise InvalidData("runner source dirty state must be boolean or null")
    if (revision is None or dirty is None) and source_limitation is None:
        raise InvalidData("missing runner source identity requires an explicit limitation")

    if "media_runtime_id" not in metadata:
        raise InvalidData("runner metadata is missing media_runtime_id")
    runtime_id = metadata.get("media_runtime_id")
    if runtime_id is not None and (not isinstance(runtime_id, str) or not runtime_id.strip()):
        raise InvalidData("runner media_runtime_id must be null or nonempty")
    media_tools = manifest.get("media_tools")
    if isinstance(media_tools, Mapping) and runtime_id != media_tools.get("runtime_id"):
        raise InvalidData("runner media runtime identity disagrees with manifest")

    versions = metadata.get("webview2_runtime_versions")
    if not isinstance(versions, list) or any(
        not isinstance(version, str) or not version.strip() for version in versions
    ):
        raise InvalidData("runner WebView2 runtime versions must be an array of strings")
    webview_limitation = _optional_limitation(
        metadata.get("webview2_runtime_limitation"), "WebView2 runtime limitation"
    )
    if not versions and webview_limitation is None:
        raise InvalidData("missing WebView2 runtime identity requires an explicit limitation")


def _safe_export_relative_path(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise InvalidData(f"{label} must be a nonempty sentinel-relative path")
    normalized = value.replace("\\", "/")
    if (
        normalized.startswith("/")
        or normalized.startswith("//")
        or re.match(r"^[A-Za-z]:", normalized)
    ):
        raise InvalidData(f"{label} must be sentinel-relative")
    parts = normalized.split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise InvalidData(f"{label} is not a safe sentinel-relative path")
    if parts[0].lower() != "clips":
        raise InvalidData(f"{label} must remain in the sentinel-owned clips directory")
    return "/".join(parts)


def _sha256(value: Any, label: str) -> str:
    if not isinstance(value, str) or not HASH_RE.fullmatch(value):
        raise InvalidData(f"{label} must be a SHA-256 identity")
    return value.lower()


def _nonnegative_integer(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise InvalidData(f"{label} must be a nonnegative integer")
    return value


def _validate_export_validation(
    bundle_dir: Path,
    manifest: Mapping[str, Any],
    expected: Mapping[TrialKey, Mapping[str, Any]],
    run_id: str,
) -> dict[TrialKey, dict[str, list[float]]]:
    export_trials = [
        (key, scenario)
        for key, scenario in expected.items()
        if str(scenario.get("kind", "")).lower() == "export"
    ]
    if not export_trials:
        return {}
    if len(export_trials) != 1:
        raise InvalidData("each export run bundle must declare exactly one export trial")
    path = bundle_dir / "export-validation.json"
    if not path.is_file():
        raise InvalidData("export run is missing required artifact export-validation.json")
    validation = _read_json(path, "export validation")
    _schema(validation, "export validation")
    key, scenario = export_trials[0]
    if validation.get("run_id") != run_id:
        raise InvalidData("export validation run_id does not match manifest")
    if validation.get("scenario_id") != key.scenario_id:
        raise InvalidData("export validation scenario_id does not match manifest")
    if str(validation.get("trial_id")) != key.trial_id:
        raise InvalidData("export validation trial_id does not match manifest")
    if str(validation.get("clips_root", "")).replace("/", "\\").lower() != (
        "library_root\\clips"
    ):
        raise InvalidData("export validation clips_root is not the sentinel library clips root")
    if validation.get("all_valid") is not True:
        raise InvalidData("export validation all_valid is not true")
    errors = validation.get("errors")
    if not isinstance(errors, list) or errors:
        raise InvalidData("export validation must contain zero errors")
    validation_elapsed = _nonnegative_number(
        validation.get("validation_elapsed_ms"), "export validation elapsed time"
    )

    presets = scenario.get("export_presets")
    if not isinstance(presets, list) or not presets:
        raise InvalidData("export scenario must declare export_presets")
    expected_presets = [_identifier(preset, "export preset") for preset in presets]
    if any(preset not in {"horizontal", "vertical", "discord"} for preset in expected_presets):
        raise InvalidData("export scenario contains an unsupported preset")
    if len(set(expected_presets)) != len(expected_presets):
        raise InvalidData("export scenario presets must be unique")
    expected_count = _nonnegative_integer(
        validation.get("expected_output_count"), "export expected output count"
    )
    if expected_count != len(expected_presets):
        raise InvalidData("export validation expected output count disagrees with manifest")
    outputs = validation.get("outputs")
    if not isinstance(outputs, list) or len(outputs) != expected_count:
        raise InvalidData("export validation output count is not exact")

    expected_duration = _nonnegative_number(
        scenario.get("expected_duration_ms"), "export expected duration"
    )
    if expected_duration <= 0:
        raise InvalidData("export expected duration must be positive")
    if _nonnegative_number(
        validation.get("expected_duration_ms"), "validated expected duration"
    ) != expected_duration:
        raise InvalidData("export validation expected duration disagrees with manifest")
    manifest_tolerance = _nonnegative_number(
        scenario.get("duration_tolerance_ms", 1_000), "export duration tolerance"
    )
    if manifest_tolerance > 5_000:
        raise InvalidData("export duration tolerance exceeds five seconds")
    if _nonnegative_number(
        validation.get("duration_tolerance_ms"), "validated duration tolerance"
    ) != manifest_tolerance:
        raise InvalidData("export validation duration tolerance disagrees with manifest")
    expected_video = scenario.get("expected_video_codec")
    expected_audio = scenario.get("expected_audio_codec")
    if expected_video != "h264" or expected_audio != "aac":
        raise InvalidData("export scenario must declare the H.264/AAC stream contract")
    stream_contract = validation.get("expected_streams")
    if not isinstance(stream_contract, Mapping) or (
        stream_contract.get("video_count") != 1
        or stream_contract.get("audio_count") != 1
        or stream_contract.get("video_codec") != expected_video
        or stream_contract.get("audio_codec") != expected_audio
    ):
        raise InvalidData("export validation stream contract disagrees with manifest")
    discord_limit = _nonnegative_integer(
        validation.get("discord_limit_bytes_exclusive"), "Discord byte limit"
    )
    if discord_limit <= 0:
        raise InvalidData("Discord byte limit must be positive")
    if discord_limit != 10_000_000:
        raise InvalidData("Discord byte limit does not match the strict product contract")

    seen_presets: set[str] = set()
    seen_paths: set[str] = set()
    allowed_entries: set[str] = set()
    metrics: dict[str, list[float]] = defaultdict(list)
    metrics["export_validation_elapsed_ms"].append(validation_elapsed)
    for index, output in enumerate(outputs):
        if not isinstance(output, Mapping):
            raise InvalidData(f"export validation output {index} must be an object")
        preset = _identifier(output.get("preset"), f"export output {index} preset")
        if preset not in expected_presets or preset in seen_presets:
            raise InvalidData("export validation presets are missing, duplicate, or unexpected")
        seen_presets.add(preset)
        filename = _identifier(output.get("filename"), f"export output {index} filename")
        relative_path = _safe_export_relative_path(
            output.get("relative_path"), f"export output {index} path"
        )
        thumbnail_path = _safe_export_relative_path(
            output.get("thumbnail_relative_path"), f"export output {index} thumbnail path"
        )
        if not relative_path.lower().endswith(f"/{filename.lower()}.mp4"):
            raise InvalidData("export output path does not match its filename")
        if not thumbnail_path.lower().endswith(f"/{filename.lower()}.jpg"):
            raise InvalidData("export thumbnail path does not match its filename")
        relative_key = relative_path.casefold()
        thumbnail_key = thumbnail_path.casefold()
        if relative_key in seen_paths or thumbnail_key in seen_paths:
            raise InvalidData("export validation contains duplicate output paths")
        seen_paths.update((relative_key, thumbnail_key))
        allowed_entries.update((relative_key, thumbnail_key))
        size_bytes = _nonnegative_integer(
            output.get("size_bytes"), f"export output {index} size"
        )
        if size_bytes <= 0:
            raise InvalidData("export output size must be positive")
        _sha256(output.get("sha256"), f"export output {index}")
        _sha256(output.get("thumbnail_sha256"), f"export output {index} thumbnail")
        duration_ms = _nonnegative_number(
            output.get("duration_ms"), f"export output {index} duration"
        )
        if duration_ms <= 0:
            raise InvalidData("export output duration must be positive")
        if abs(duration_ms - expected_duration) > manifest_tolerance:
            raise InvalidData("export output duration is outside the declared tolerance")
        if output.get("full_single_thread_decode_ok") is not True:
            raise InvalidData("export output lacks successful full decode evidence")
        if not isinstance(output.get("decode_stderr"), str):
            raise InvalidData("export output full decode stderr evidence must be text")
        if output.get("valid") is not True:
            raise InvalidData("export output is not individually valid")

        probe = output.get("ffprobe")
        if not isinstance(probe, Mapping):
            raise InvalidData("export output lacks ffprobe evidence")
        streams = probe.get("streams")
        if not isinstance(streams, list) or len(streams) != 2:
            raise InvalidData("export output must have exactly two streams")
        video_streams = [
            stream
            for stream in streams
            if isinstance(stream, Mapping) and stream.get("codec_type") == "video"
        ]
        audio_streams = [
            stream
            for stream in streams
            if isinstance(stream, Mapping) and stream.get("codec_type") == "audio"
        ]
        if (
            len(video_streams) != 1
            or video_streams[0].get("codec_name") != expected_video
            or len(audio_streams) != 1
            or audio_streams[0].get("codec_name") != expected_audio
        ):
            raise InvalidData("export output codec/stream evidence is invalid")
        probe_format = probe.get("format")
        if not isinstance(probe_format, Mapping):
            raise InvalidData("export output lacks ffprobe format evidence")
        try:
            probe_duration_ms = float(probe_format.get("duration")) * 1_000.0
            probe_size = int(probe_format.get("size"))
        except (TypeError, ValueError, OverflowError) as error:
            raise InvalidData("export output ffprobe duration/size evidence is malformed") from error
        if not math.isfinite(probe_duration_ms) or probe_duration_ms < 0:
            raise InvalidData("export output ffprobe duration must be finite and nonnegative")
        if abs(probe_duration_ms - duration_ms) > 1.0 or probe_size != size_bytes:
            raise InvalidData("export output ffprobe duration/size evidence disagrees")
        discord_valid = output.get("discord_size_valid")
        if preset == "discord":
            if discord_valid is not True or size_bytes >= discord_limit:
                raise InvalidData("Discord export lacks valid strict-size evidence")
        elif discord_valid is not None:
            raise InvalidData("non-Discord export has unexpected Discord-size evidence")

        metrics["export_output_size_bytes"].append(float(size_bytes))
        metrics["export_output_duration_ms"].append(duration_ms)
        metrics[f"export_{preset}_output_size_bytes"].append(float(size_bytes))
        metrics[f"export_{preset}_output_duration_ms"].append(duration_ms)

    if seen_presets != set(expected_presets):
        raise InvalidData("export validation presets do not exactly match the manifest")
    entries = validation.get("newly_created_entries")
    if not isinstance(entries, list):
        raise InvalidData("export validation newly_created_entries must be an array")
    entry_paths: set[str] = set()
    for index, entry in enumerate(entries):
        if not isinstance(entry, Mapping) or entry.get("kind") != "file":
            raise InvalidData("export validation contains a non-file output entry")
        entry_path = _safe_export_relative_path(
            entry.get("relative_path"), f"export created entry {index} path"
        )
        entry_size = _nonnegative_integer(
            entry.get("size_bytes"), f"export created entry {index} size"
        )
        if entry_size <= 0:
            raise InvalidData("export validation contains an empty created output")
        entry_key = entry_path.casefold()
        if entry_key in entry_paths:
            raise InvalidData("export validation contains duplicate created entries")
        entry_paths.add(entry_key)
    if entry_paths != allowed_entries:
        raise InvalidData("export validation created entries do not exactly match outputs")
    _nonnegative_integer(
        validation.get("preexisting_entry_count"), "export preexisting entry count"
    )
    return {key: dict(metrics)}


def load_bundle(bundle_dir: Path, alias: str) -> RunBundle:
    for artifact in ARTIFACTS:
        if not (bundle_dir / artifact).is_file():
            raise InvalidData(f"{alias} is missing required artifact {artifact}")
    manifest = _read_json(bundle_dir / "manifest.json", f"{alias} manifest")
    run_id, expected = _validate_manifest(manifest, bundle_dir)
    runner_metadata = _read_json(
        bundle_dir / "runner-metadata.json", f"{alias} runner metadata"
    )
    _validate_runner_metadata(
        runner_metadata, manifest, bundle_dir / "manifest.json", run_id
    )
    collection = _read_json(
        bundle_dir / "collection-result.json", f"{alias} collection result"
    )
    events = _read_jsonl(bundle_dir / "events.jsonl", f"{alias} events")
    requests = _read_jsonl(bundle_dir / "server_requests.jsonl", f"{alias} server requests")
    samples = _read_jsonl(bundle_dir / "process_samples.jsonl", f"{alias} process samples")
    terminal = _read_json(bundle_dir / "terminal.json", f"{alias} terminal")
    for label, records in (
        ("events", events),
        ("server requests", requests),
        ("process samples", samples),
    ):
        for index, record in enumerate(records, 1):
            _validate_envelope(record, f"{alias} {label} record {index}", run_id, expected)
    # Frontend batches and concurrent Rust/server producers can reach the bounded
    # writer after a later-timestamped record. Monotonic time, not cross-producer
    # append order, is authoritative. Collector samples have one producer and
    # must remain append-ordered so cadence loss cannot be hidden by sorting.
    _validate_log_order(samples, f"{alias} process samples")
    events.sort(key=lambda record: float(record["monotonic_ms"]))
    requests.sort(key=lambda record: float(record["monotonic_ms"]))
    _validate_terminal(
        terminal,
        run_id,
        {"events": len(events), "server_requests": len(requests), "process_samples": len(samples)},
    )
    _validate_collection(collection, run_id, terminal)
    actions = _validate_events(events, expected)
    _validate_requests(requests, actions)
    _validate_samples(samples, expected)
    export_metrics = _validate_export_validation(
        bundle_dir, manifest, expected, run_id
    )
    identities: dict[str, str] = {}
    control_identities: dict[str, str] = {}
    subjects: dict[str, str] = {}
    for key, scenario in expected.items():
        identities.setdefault(
            key.scenario_id,
            _digest(_identity_material(manifest, scenario, runner_metadata)),
        )
        control_identities.setdefault(
            key.scenario_id,
            _digest(_control_identity_material(manifest, scenario, runner_metadata)),
        )
        subjects.setdefault(
            key.scenario_id, _digest(_subject_material(manifest, runner_metadata))
        )
    return RunBundle(
        alias=alias,
        manifest=manifest,
        runner_metadata=runner_metadata,
        collection=collection,
        expected=expected,
        events=events,
        requests=requests,
        samples=samples,
        terminal=terminal,
        identity_by_scenario=identities,
        control_identity_by_scenario=control_identities,
        subject_by_scenario=subjects,
        export_metrics_by_trial=export_metrics,
    )


def discover_bundles(input_path: Path) -> list[Path]:
    if not input_path.is_dir():
        raise InvalidData("--input must be a run bundle or matrix directory")
    if (input_path / "manifest.json").is_file():
        return [input_path]
    bundles = sorted({path.parent for path in input_path.rglob("manifest.json")})
    if not bundles:
        raise InvalidData("input contains no replay benchmark run bundles")
    return bundles


def percentile_type7(values: Sequence[float], percentile: float) -> float:
    if not values:
        raise InvalidData("cannot summarize an empty metric")
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * percentile
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def distribution(values: Sequence[float]) -> dict[str, float | int]:
    ordered = sorted(values)
    median = statistics.median(ordered)
    deviations = [abs(value - median) for value in ordered]
    return {
        "count": len(ordered),
        "min": ordered[0],
        "median": median,
        "p95": percentile_type7(ordered, 0.95),
        "max": ordered[-1],
        "mad": statistics.median(deviations),
    }


def _add_metric(metrics: dict[str, list[float]], name: str, value: Any) -> None:
    if not ID_RE.fullmatch(name):
        return
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return
    number = float(value)
    if math.isfinite(number):
        metrics[name].append(number)


def _generic_metrics(record: Mapping[str, Any], metrics: dict[str, list[float]]) -> None:
    payload = record.get("payload")
    if not isinstance(payload, Mapping):
        return
    values = payload.get("metrics")
    if isinstance(values, Mapping):
        for name, value in values.items():
            _add_metric(metrics, str(name), value)
    metric_name = payload.get("metric")
    if isinstance(metric_name, str) and "value" in payload:
        _add_metric(metrics, metric_name, payload["value"])


KNOWN_EVENT_FIELDS: dict[str, tuple[tuple[str, str], ...]] = {
    "benchmark_session_started": (
        ("harness_initialization_ms", "harness_initialization_ms"),
    ),
    "library_games_ready": (
        ("elapsed_ms", "library_games_backend_elapsed_ms"),
        ("count", "library_games_count"),
    ),
    "library_clips_ready": (
        ("elapsed_ms", "library_clips_backend_elapsed_ms"),
        ("count", "library_clips_count"),
    ),
    "library_storage_ready": (
        ("elapsed_ms", "library_storage_backend_elapsed_ms"),
    ),
    "library_useful": (
        ("game_count", "library_useful_game_count"),
        ("clip_count", "library_useful_clip_count"),
    ),
    "playback_payload_backend_ready": (
        ("elapsed_ms", "playback_payload_backend_elapsed_ms"),
    ),
    "playback_payload_ready": (
        ("event_count", "playback_payload_event_count"),
        ("participant_count", "playback_payload_participant_count"),
        ("duration_ms", "playback_payload_duration_ms"),
    ),
    "viewer_mounted": (
        ("duration_ms", "viewer_recording_duration_ms"),
        ("recording_fps", "viewer_recording_fps"),
    ),
    "metadata_ready": (
        ("duration_ms", "media_duration_ms"),
        ("video_width", "media_video_width"),
        ("video_height", "media_video_height"),
    ),
    "rate_requested": (("rate", "rate_requested_value"),),
    "rate_applied": (
        ("requested_rate", "rate_applied_requested_value"),
        ("actual_rate", "rate_applied_actual_value"),
    ),
    "rate_observed": (
        ("requested_rate", "rate_observed_requested_value"),
        ("actual_rate", "rate_observed_actual_value"),
        ("effective_rate", "effective_playback_rate"),
        ("dropped_frames", "rate_observed_dropped_frames"),
    ),
    "scrub_started": (("request_count", "scrub_request_count"),),
    "scrub_settled": (
        ("request_count", "scrub_settled_request_count"),
        ("media_time_ms", "scrub_settled_media_time_ms"),
    ),
    "scenario_end": (
        ("playback_rate", "scenario_playback_rate"),
        ("total_frames", "scenario_total_frames"),
        ("dropped_frames", "scenario_dropped_frames"),
    ),
    "scenario_complete": (
        ("playback_rate", "scenario_playback_rate"),
        ("total_frames", "scenario_total_frames"),
        ("dropped_frames", "scenario_dropped_frames"),
    ),
    "scenario_completed": (
        ("playback_rate", "scenario_playback_rate"),
        ("total_frames", "scenario_total_frames"),
        ("dropped_frames", "scenario_dropped_frames"),
    ),
    "viewer_cycle_completed": (
        ("playback_rate", "scenario_playback_rate"),
        ("total_frames", "scenario_total_frames"),
        ("dropped_frames", "scenario_dropped_frames"),
    ),
    "export_progress": (
        ("percent", "export_progress_percent"),
        ("completed_outputs", "export_progress_completed_outputs"),
        ("total_outputs", "export_progress_total_outputs"),
    ),
    "export_backend_completed": (
        ("elapsed_ms", "export_backend_elapsed_ms"),
        ("setup_elapsed_ms", "export_backend_setup_elapsed_ms"),
        ("source_probe_elapsed_ms", "export_backend_source_probe_elapsed_ms"),
        ("finalize_elapsed_ms", "export_backend_finalize_elapsed_ms"),
        ("observed_command_ms", "export_backend_observed_command_ms"),
        ("total_file_size_bytes", "export_backend_total_file_size_bytes"),
    ),
    "export_completed": (
        ("elapsed_ms", "export_elapsed_ms"),
        ("setup_elapsed_ms", "export_setup_elapsed_ms"),
        ("source_probe_elapsed_ms", "export_source_probe_elapsed_ms"),
        ("finalize_elapsed_ms", "export_finalize_elapsed_ms"),
        ("total_file_size_bytes", "export_total_file_size_bytes"),
        ("throughput_bytes_per_second", "export_throughput_bytes_per_second"),
    ),
}


def _known_event_metrics(
    event: Mapping[str, Any], metrics: dict[str, list[float]]
) -> None:
    """Extract only reviewed numeric fields from the production event contract."""
    payload = event.get("payload")
    if not isinstance(payload, Mapping):
        return
    kind = _kind(event)
    for field_name, metric_name in KNOWN_EVENT_FIELDS.get(kind, ()):
        _add_metric(metrics, metric_name, payload.get(field_name))
    if kind == "playback_payload_ready" and "elapsed_ms" in payload:
        source = str(event.get("source", "")).lower()
        phase = "backend" if source in {"app", "backend", "tauri"} else "frontend"
        _add_metric(metrics, f"playback_payload_{phase}_elapsed_ms", payload["elapsed_ms"])
    if kind not in {"export_backend_completed", "export_completed"}:
        return
    outputs = payload.get("outputs")
    if not isinstance(outputs, list):
        return
    prefix = "export_backend_output" if kind == "export_backend_completed" else "export_output"
    for output in outputs:
        if not isinstance(output, Mapping):
            continue
        for field_name, metric_name in (
            ("file_size_bytes", f"{prefix}_file_size_bytes"),
            ("encode_elapsed_ms", f"{prefix}_encode_elapsed_ms"),
            ("thumbnail_elapsed_ms", f"{prefix}_thumbnail_elapsed_ms"),
            ("retry_count", f"{prefix}_retry_count"),
        ):
            _add_metric(metrics, metric_name, output.get(field_name))


def _event_pair_metrics(
    events: Sequence[Mapping[str, Any]], metrics: dict[str, list[float]]
) -> None:
    ordered = sorted(events, key=lambda event: float(event["monotonic_ms"]))
    first_by_kind: dict[str, Mapping[str, Any]] = {}
    by_kind: dict[str, list[Mapping[str, Any]]] = defaultdict(list)
    export_progress_stages: dict[str, int] = defaultdict(int)
    scrub_started: Mapping[str, Any] | None = None
    for event in ordered:
        kind = _kind(event)
        first_by_kind.setdefault(kind, event)
        by_kind[kind].append(event)
        if kind == "export_progress":
            stage = _field(event, "stage")
            if stage in {"encoding", "thumbnail", "complete"}:
                export_progress_stages[str(stage)] += 1
        if kind == "scrub_started":
            scrub_started = event
        elif kind == "scrub_settled" and scrub_started is not None:
            _add_metric(
                metrics,
                "scrub_settle_ms",
                float(event["monotonic_ms"]) - float(scrub_started["monotonic_ms"]),
            )
            scrub_started = None
    for stage, count in sorted(export_progress_stages.items()):
        _add_metric(metrics, f"export_progress_{stage}_event_count", count)

    library_request_events = [
        first_by_kind[kind]
        for kind in (
            "library_requested",
            "library_games_requested",
            "library_clips_requested",
            "library_storage_requested",
        )
        if kind in first_by_kind
    ]
    library_requested = (
        min(library_request_events, key=lambda event: float(event["monotonic_ms"]))
        if library_request_events
        else None
    )
    library_useful = first_by_kind.get("library_useful")
    if library_requested is not None and library_useful is not None:
        _add_metric(
            metrics,
            "library_request_to_useful_ms",
            float(library_useful["monotonic_ms"])
            - float(library_requested["monotonic_ms"]),
        )

    replay_requested = first_by_kind.get("replay_requested")
    if replay_requested is None:
        return
    requested_at = float(replay_requested["monotonic_ms"])
    shared_payload_ready = by_kind.get("playback_payload_ready", [])
    backend_payload_ready = first_by_kind.get("playback_payload_backend_ready")
    if backend_payload_ready is None:
        backend_payload_ready = next(
            (
                event
                for event in shared_payload_ready
                if str(event.get("source", "")).lower() in {"app", "backend", "tauri"}
            ),
            None,
        )
    frontend_payload_ready = next(
        (
            event
            for event in shared_payload_ready
            if str(event.get("source", "")).lower() in {"frontend", "viewer", "webview"}
            or _field(event, "event_count") is not None
        ),
        None,
    )
    milestones = (
        (
            first_by_kind.get("playback_payload_requested"),
            "replay_request_to_payload_backend_request_ms",
        ),
        (backend_payload_ready, "replay_request_to_payload_backend_ready_ms"),
        (frontend_payload_ready, "replay_request_to_payload_ready_ms"),
    )
    for event, metric_name in milestones:
        if event is not None:
            _add_metric(metrics, metric_name, float(event["monotonic_ms"]) - requested_at)
    for milestone, metric_name in (
        ("viewer_mounted", "replay_request_to_viewer_mounted_ms"),
        ("media_loadstart", "replay_request_to_media_loadstart_ms"),
        ("metadata_ready", "replay_request_to_metadata_ready_ms"),
        ("media_canplay", "replay_request_to_media_canplay_ms"),
        ("first_presented_frame", "replay_request_to_first_presented_frame_ms"),
    ):
        event = first_by_kind.get(milestone)
        if event is not None:
            _add_metric(metrics, metric_name, float(event["monotonic_ms"]) - requested_at)


def _trial_metrics(
    key: TrialKey,
    events: Sequence[Mapping[str, Any]],
    requests: Sequence[Mapping[str, Any]],
    samples: Sequence[Mapping[str, Any]],
) -> dict[str, list[float]]:
    metrics: dict[str, list[float]] = defaultdict(list)
    action_events: dict[str, list[Mapping[str, Any]]] = defaultdict(list)
    for event in events:
        _generic_metrics(event, metrics)
        _known_event_metrics(event, metrics)
        action_id = _field(event, "action_id")
        if action_id is not None:
            action_events[str(action_id)].append(event)
    _event_pair_metrics(events, metrics)
    action_counts = defaultdict(int)
    for records in action_events.values():
        kinds = [_kind(record) for record in records]
        requested = next((record for record in records if _kind(record) in ACTION_REQUEST_KINDS), None)
        if requested is None:
            continue
        action_counts["requested"] += 1
        request_time = float(requested["monotonic_ms"])
        dispatch = next((record for record in records if _kind(record) in SEEK_DISPATCH_KINDS), None)
        seeked = next((record for record in records if _kind(record) in SEEKED_KINDS), None)
        presented = next((record for record in records if _kind(record) in PRESENTED_KINDS), None)
        if dispatch is not None:
            action_counts["dispatched"] += 1
            _add_metric(metrics, "request_to_dispatch_ms", float(dispatch["monotonic_ms"]) - request_time)
        if seeked is not None:
            _add_metric(metrics, "request_to_seeked_ms", float(seeked["monotonic_ms"]) - request_time)
            if dispatch is not None:
                _add_metric(
                    metrics,
                    "dispatch_to_seeked_ms",
                    float(seeked["monotonic_ms"]) - float(dispatch["monotonic_ms"]),
                )
        if presented is not None:
            _add_metric(
                metrics,
                "request_to_presented_ms",
                float(presented["monotonic_ms"]) - request_time,
            )
            target = _field(requested, "target_ms")
            actual = _field(
                presented,
                "media_time_ms",
                _field(presented, "presented_media_time_ms"),
            )
            if target is not None and actual is not None:
                _add_metric(metrics, "target_error_ms", abs(float(actual) - float(target)))
        terminal = presented or next(
            (
                record
                for record in records
                if _kind(record) in ACTION_COMPLETE_KINDS
                or _kind(record) in ACTION_SHORT_CIRCUIT_KINDS
            ),
            None,
        )
        if terminal is not None:
            action_latency = float(terminal["monotonic_ms"]) - request_time
            _add_metric(
                metrics,
                "action_latency_ms",
                action_latency,
            )
            action_kind = str(
                _field(
                    requested,
                    "action_kind",
                    _kind(requested).removesuffix("_requested"),
                )
            )
            _add_metric(metrics, f"{action_kind}_action_latency_ms", action_latency)
        if any(kind in {"seek_coalesced", "action_coalesced", "action_cancelled_superseded"} for kind in kinds):
            action_counts["coalesced"] += 1
        if any(kind in {"seek_deduped", "action_deduped"} for kind in kinds):
            action_counts["deduped"] += 1
    for name, count in action_counts.items():
        _add_metric(metrics, f"actions_{name}_count", count)

    completed = cancelled = request_errors = ranges = bytes_delivered = 0
    peak_active = 0.0
    for request in requests:
        _generic_metrics(request, metrics)
        outcome = str(_field(request, "outcome", "")).lower()
        completed += int(outcome == "completed")
        cancelled += int(outcome == "cancelled")
        request_errors += int(outcome not in {"completed", "cancelled"})
        status = _field(request, "status")
        ranges += int(status == 206 or _field(request, "range_start") is not None)
        delivered = _field(request, "delivered_bytes", 0)
        if isinstance(delivered, (int, float)) and not isinstance(delivered, bool):
            bytes_delivered += int(delivered)
        active = _field(request, "peak_active_streams", _field(request, "active_streams", 0))
        if isinstance(active, (int, float)) and not isinstance(active, bool):
            peak_active = max(peak_active, float(active))
    for name, value in {
        "server_requests_count": len(requests),
        "server_completed_count": completed,
        "server_cancelled_count": cancelled,
        "server_error_count": request_errors,
        "server_ranges_count": ranges,
        "server_delivered_bytes": bytes_delivered,
        "server_peak_active_streams": peak_active,
    }.items():
        _add_metric(metrics, name, value)

    numeric_samples: dict[str, list[float]] = defaultdict(list)
    gpu_available_samples = 0
    gpu_unavailable_samples = 0
    for sample in samples:
        _generic_metrics(sample, metrics)
        payload = sample.get("payload")
        sample_values = {
            name: value
            for name, value in sample.items()
            if name
            not in {
                "schema_version",
                "run_id",
                "scenario_id",
                "trial_id",
                "monotonic_ms",
                "source",
                "kind",
                "generation",
                "action_id",
                "payload",
            }
        }
        if isinstance(payload, Mapping):
            sample_values.update(payload)
        gpu = sample.get("gpu")
        if isinstance(gpu, Mapping):
            if gpu.get("available") is True:
                gpu_available_samples += 1
                engines = gpu.get("engines")
                if isinstance(engines, list):
                    utilization = sum(
                        float(item["utilization_percent"])
                        for item in engines
                        if isinstance(item, Mapping)
                        and isinstance(item.get("utilization_percent"), (int, float))
                        and not isinstance(item.get("utilization_percent"), bool)
                        and math.isfinite(float(item["utilization_percent"]))
                    )
                    numeric_samples["process_tree_gpu_engine_utilization_percent"].append(
                        utilization
                    )
                memory = gpu.get("memory")
                if isinstance(memory, list):
                    for field, metric_name in (
                        ("dedicated_bytes", "process_tree_gpu_dedicated_memory_bytes"),
                        ("shared_bytes", "process_tree_gpu_shared_memory_bytes"),
                    ):
                        total = sum(
                            int(item[field])
                            for item in memory
                            if isinstance(item, Mapping)
                            and isinstance(item.get(field), int)
                            and not isinstance(item.get(field), bool)
                            and int(item[field]) >= 0
                        )
                        numeric_samples[metric_name].append(float(total))
            else:
                gpu_unavailable_samples += 1
        for name, value in sample_values.items():
            if isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value)):
                numeric_samples[str(name)].append(float(value))
    for name, values in numeric_samples.items():
        safe_name = name.lower()
        if "cpu_time" in safe_name:
            _add_metric(metrics, f"{name}_delta", values[-1] - values[0])
        elif "cpu" in safe_name:
            _add_metric(metrics, f"{name}_mean", statistics.fmean(values))
            _add_metric(metrics, f"{name}_max", max(values))
        elif "utilization" in safe_name and "percent" in safe_name:
            _add_metric(metrics, f"{name}_mean", statistics.fmean(values))
            _add_metric(metrics, f"{name}_max", max(values))
        elif any(token in safe_name for token in ("private", "working_set", "memory", "handles", "threads")):
            _add_metric(metrics, f"{name}_max", max(values))
            _add_metric(metrics, f"{name}_growth", values[-1] - values[0])
            if len(values) >= 3:
                sustained_growth = all(
                    right >= left for left, right in zip(values, values[1:])
                ) and values[-1] > values[0]
                _add_metric(
                    metrics,
                    f"{name}_sustained_monotonic_growth_flag",
                    int(sustained_growth),
                )
        elif any(token in safe_name for token in ("read_bytes", "write_bytes", "io_bytes")):
                _add_metric(metrics, f"{name}_delta", values[-1] - values[0])
    _add_metric(metrics, "gpu_available_sample_count", gpu_available_samples)
    _add_metric(metrics, "gpu_unavailable_sample_count", gpu_unavailable_samples)
    return dict(metrics)


def _outcome_digest(
    events: Sequence[Mapping[str, Any]], requests: Sequence[Mapping[str, Any]]
) -> str:
    action_records: dict[str, list[Mapping[str, Any]]] = defaultdict(list)
    for event in events:
        action_id = _field(event, "action_id")
        if action_id is not None:
            action_records[str(action_id)].append(event)
    ordered_actions: list[tuple[float, dict[str, Any]]] = []
    for records in action_records.values():
        requested = next(
            (record for record in records if _kind(record) in ACTION_REQUEST_KINDS), None
        )
        if requested is None:
            continue
        relevant_kinds = sorted(
            _kind(record)
            for record in records
            if (
                _kind(record) in ACTION_REQUEST_KINDS
                or _kind(record) in ACTION_COMPLETE_KINDS
                or _kind(record) in ACTION_SHORT_CIRCUIT_KINDS
                or _kind(record) in SEEK_DISPATCH_KINDS
                or _kind(record) in SEEKED_KINDS
                or _kind(record) in PRESENTED_KINDS
                or _kind(record) in FAILURE_KINDS
            )
        )
        ordered_actions.append(
            (
                float(requested["monotonic_ms"]),
                {
                    "action_kind": _field(
                        requested,
                        "action_kind",
                        _kind(requested).removesuffix("_requested"),
                    ),
                    "generation": _field(requested, "generation"),
                    "target_ms": _field(
                        requested,
                        "target_ms",
                        _field(requested, "requested_preview_ms"),
                    ),
                    "outcomes": relevant_kinds,
                },
            )
        )
    actions = [action for _, action in sorted(ordered_actions, key=lambda item: item[0])]
    settled_request_classes: set[tuple[Any, Any, Any, Any, bool]] = set()
    cancelled_request_classes: set[tuple[Any, Any]] = set()
    for request in requests:
        route_class = _field(request, "route_class")
        method = _field(request, "method")
        outcome = _field(request, "outcome")
        if outcome == "cancelled":
            # WebView2's cancelled range boundaries, partial delivery, and whether the
            # response was constructed before cancellation are transport-scheduling
            # observations. Their counts/bytes remain report metrics, but hashing them
            # would make two semantically identical playback runs incomparable.
            cancelled_request_classes.add((route_class, method))
            continue
        settled_request_classes.add(
            (
                route_class,
                method,
                _field(request, "status"),
                outcome,
                _field(request, "status") == 206
                or _field(request, "range_start") is not None,
            )
        )
    settled_request_outcomes = [
        {
            "route_class": route_class,
            "method": method,
            "status": status,
            "outcome": outcome,
            "range_response": range_response,
        }
        for route_class, method, status, outcome, range_response in sorted(
            settled_request_classes,
            key=lambda item: _canonical_json(
                {
                    "route_class": item[0],
                    "method": item[1],
                    "status": item[2],
                    "outcome": item[3],
                    "range_response": item[4],
                }
            ),
        )
    ]
    request_outcomes = {
        "settled": settled_request_outcomes,
        "cancelled_classes": [
            {"route_class": route_class, "method": method}
            for route_class, method in sorted(
                cancelled_request_classes,
                key=lambda item: _canonical_json(
                    {"route_class": item[0], "method": item[1]}
                ),
            )
        ],
    }
    recovery_counts: dict[str, int] = defaultdict(int)
    error_counts: dict[str, int] = defaultdict(int)
    behavior_events: list[str] = []
    for event in events:
        kind = _kind(event)
        if (
            kind in ACTION_REQUEST_KINDS
            or kind in ACTION_COMPLETE_KINDS
            or kind in ACTION_SHORT_CIRCUIT_KINDS
            or kind in SEEK_DISPATCH_KINDS
            or kind in SEEKED_KINDS
            or kind in PRESENTED_KINDS
        ):
            behavior_events.append(kind)
        if "recovery" in kind or "degraded" in kind:
            recovery_counts[kind] += 1
        if any(token in kind for token in ("error", "failed", "timeout", "exhausted")):
            error_counts[kind] += 1
    return _digest(
        {
            "actions": actions,
            "behavior_events": behavior_events,
            "requests": request_outcomes,
            "errors": dict(sorted(error_counts.items())),
            "recoveries": dict(sorted(recovery_counts.items())),
        }
    )


def _numeric_field(record: Mapping[str, Any], names: Sequence[str]) -> float | None:
    for name in names:
        value = _field(record, name)
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            number = float(value)
            if math.isfinite(number):
                return number
    return None


def _observer_control_evidence(
    manifest: Mapping[str, Any],
    events: Sequence[Mapping[str, Any]],
    samples: Sequence[Mapping[str, Any]],
    metrics: Mapping[str, Sequence[float]],
) -> dict[str, Any]:
    starts = [event for event in events if _kind(event) in SCENARIO_START_KINDS]
    ends = [event for event in events if _kind(event) in SCENARIO_END_KINDS]
    duration_ms = float(ends[0]["monotonic_ms"]) - float(starts[0]["monotonic_ms"])
    private_values = [
        value
        for sample in samples
        if (
            value := _numeric_field(
                sample,
                ("process_tree_private_bytes", "private_bytes", "working_set_bytes"),
            )
        )
        is not None
    ]
    process_counts = [
        value
        for sample in samples
        if (value := _numeric_field(sample, ("process_count", "process_tree_count")))
        is not None
    ]
    cpu_values: list[float] = []
    cpu_scale = 1.0
    for names, scale in (
        (("process_tree_cpu_time_ms", "cpu_time_ms"), 1.0),
        (("process_tree_cpu_time_100ns", "cpu_time_100ns"), 1.0 / 10_000.0),
        (("process_tree_cpu_time_seconds", "cpu_time_seconds"), 1_000.0),
    ):
        candidate = [
            value
            for sample in samples
            if (value := _numeric_field(sample, names)) is not None
        ]
        if len(candidate) == len(samples):
            cpu_values = candidate
            cpu_scale = scale
            break
    cpu_time_ms: float | None = None
    if cpu_values:
        delta = (cpu_values[-1] - cpu_values[0]) * cpu_scale
        if delta >= 0:
            cpu_time_ms = delta
    quantum_ms: float | None = None
    for sample in samples:
        quantum_ms = _numeric_field(
            sample, ("cpu_accounting_quantum_ms", "accounting_quantum_ms")
        )
        if quantum_ms is not None:
            break
    if quantum_ms is None:
        for container in (
            manifest,
            manifest.get("environment", {}),
            manifest.get("fingerprints", {}),
        ):
            if isinstance(container, Mapping):
                quantum_ms = _numeric_field(
                    container,
                    ("cpu_accounting_quantum_ms", "windows_cpu_accounting_quantum_ms"),
                )
                if quantum_ms is not None:
                    break
    def metric_value(name: str) -> float | None:
        values = metrics.get(name, [])
        if len(values) != 1:
            return None
        value = values[0]
        if not isinstance(value, (int, float)) or isinstance(value, bool):
            return None
        number = float(value)
        return number if math.isfinite(number) and number >= 0 else None

    total_frames = metric_value("scenario_total_frames")
    dropped_frames = metric_value("scenario_dropped_frames")
    dropped_frame_rate = None
    if total_frames is not None and total_frames > 0 and dropped_frames is not None:
        dropped_frame_rate = dropped_frames / total_frames
    seek_action_latencies = sorted(
        float(value) for value in metrics.get("seek_action_latency_ms", [])
    )
    return {
        "duration_ms": duration_ms,
        "action_latency_scope": "seek",
        "action_latency_ms": seek_action_latencies,
        "max_private_bytes": max(private_values),
        "aggregate_cpu_time_ms": cpu_time_ms,
        "max_process_count": int(max(process_counts)),
        "cpu_accounting_quantum_ms": quantum_ms,
        "server_request_count": metric_value("server_requests_count"),
        "server_completed_count": metric_value("server_completed_count"),
        "server_error_count": metric_value("server_error_count"),
        "server_range_count": metric_value("server_ranges_count"),
        "server_cancelled_count": metric_value("server_cancelled_count"),
        "server_delivered_bytes": metric_value("server_delivered_bytes"),
        "scenario_total_frames": total_frames,
        "scenario_dropped_frames": dropped_frames,
        "scenario_dropped_frame_rate": dropped_frame_rate,
    }


def _metric_unit(name: str) -> str:
    lowered = name.lower()
    if lowered.endswith("_ms") or "latency" in lowered:
        return "ms"
    if "percent" in lowered:
        return "percentage_point"
    if "throughput" in lowered and "bytes" in lowered:
        return "bytes_per_second"
    if "bytes" in lowered or "memory" in lowered or "private" in lowered or "working_set" in lowered:
        return "bytes"
    if "ratio" in lowered or "rate" in lowered:
        return "ratio"
    return "count"


def _direction(name: str) -> str:
    lowered = name.lower()
    if (
        "effective_media_rate" in lowered
        or "effective_playback_rate" in lowered
        or "throughput" in lowered
    ):
        return "higher"
    if any(
        token in lowered
        for token in (
            "playback_payload_duration_ms",
            "recording_duration_ms",
            "media_duration_ms",
            "output_duration_ms",
            "settled_media_time_ms",
        )
    ):
        return "neutral"
    if any(
        token in lowered
        for token in (
            "latency",
            "_ms",
            "error",
            "cancelled",
            "dropped",
            "cpu",
            "utilization",
            "memory",
            "private",
            "working_set",
            "growth",
            "bytes",
            "requests",
            "ranges",
            "handles",
            "threads",
        )
    ):
        return "lower"
    return "neutral"


def analyze_many(input_paths: Sequence[Path]) -> dict[str, Any]:
    if not input_paths:
        raise InvalidData("at least one --input is required")
    bundle_paths: list[Path] = []
    seen_bundle_paths: set[Path] = set()
    for input_path in input_paths:
        for bundle_path in discover_bundles(input_path):
            resolved = bundle_path.resolve()
            if resolved in seen_bundle_paths:
                raise InvalidData(f"duplicate run bundle selected by --input: {resolved}")
            seen_bundle_paths.add(resolved)
            bundle_paths.append(resolved)
    bundle_paths.sort(key=lambda path: str(path).casefold())
    bundles = [load_bundle(path, f"run-{index:03d}") for index, path in enumerate(bundle_paths, 1)]
    seen_trials: set[TrialKey] = set()
    identities: dict[str, str] = {}
    subjects: dict[str, str] = {}
    trials: list[dict[str, Any]] = []
    scenario_trial_values: dict[tuple[str, str, str], list[float]] = defaultdict(list)
    scenario_observations: dict[tuple[str, str], int] = defaultdict(int)
    for bundle in bundles:
        events_by_trial: dict[TrialKey, list[dict[str, Any]]] = defaultdict(list)
        requests_by_trial: dict[TrialKey, list[dict[str, Any]]] = defaultdict(list)
        samples_by_trial: dict[TrialKey, list[dict[str, Any]]] = defaultdict(list)
        for record in bundle.events:
            events_by_trial[TrialKey(str(record["scenario_id"]), str(record["trial_id"]))].append(record)
        for record in bundle.requests:
            requests_by_trial[TrialKey(str(record["scenario_id"]), str(record["trial_id"]))].append(record)
        for record in bundle.samples:
            samples_by_trial[TrialKey(str(record["scenario_id"]), str(record["trial_id"]))].append(record)
        for key in sorted(bundle.expected):
            if key in seen_trials:
                raise InvalidData(f"duplicate preserved trial {key.scenario_id}/{key.trial_id}")
            seen_trials.add(key)
            identity = bundle.identity_by_scenario[key.scenario_id]
            if key.scenario_id in identities and identities[key.scenario_id] != identity:
                raise InvalidData(f"scenario {key.scenario_id} has inconsistent fingerprints")
            identities[key.scenario_id] = identity
            subject = bundle.subject_by_scenario[key.scenario_id]
            if key.scenario_id in subjects and subjects[key.scenario_id] != subject:
                raise InvalidData(f"scenario {key.scenario_id} changes app identity within one arm")
            subjects[key.scenario_id] = subject
            raw_metrics = _trial_metrics(
                key,
                events_by_trial[key],
                requests_by_trial[key],
                samples_by_trial[key],
            )
            for metric_name, values in bundle.export_metrics_by_trial.get(key, {}).items():
                raw_metrics.setdefault(metric_name, []).extend(values)
            control_evidence = _observer_control_evidence(
                bundle.manifest,
                events_by_trial[key],
                samples_by_trial[key],
                raw_metrics,
            )
            metric_report: dict[str, dict[str, float | int]] = {}
            for metric_name, values in sorted(raw_metrics.items()):
                metric_report[metric_name] = distribution(values)
                scenario_observations[(key.scenario_id, metric_name)] += len(values)
                for statistic in ("median", "p95"):
                    scenario_trial_values[(key.scenario_id, metric_name, statistic)].append(
                        float(metric_report[metric_name][statistic])
                    )
            trials.append(
                {
                    "scenario_id": key.scenario_id,
                    "trial_alias": f"trial-{len(trials) + 1:03d}",
                    "trial_identity_sha256": _digest(
                        {"scenario_id": key.scenario_id, "trial_id": key.trial_id}
                    ),
                    "run_alias": bundle.alias,
                    "fingerprint_sha256": identity,
                    "control_fingerprint_sha256": bundle.control_identity_by_scenario[
                        key.scenario_id
                    ],
                    "subject_sha256": subject,
                    "observer_profile": bundle.manifest["observer_profile"],
                    "outcome_sha256": _outcome_digest(
                        events_by_trial[key], requests_by_trial[key]
                    ),
                    "observer_control_evidence": control_evidence,
                    "metrics": metric_report,
                }
            )
    summaries: list[dict[str, Any]] = []
    for (scenario_id, metric_name, statistic), values in sorted(scenario_trial_values.items()):
        observation_count = scenario_observations[(scenario_id, metric_name)]
        summaries.append(
            {
                "scenario_id": scenario_id,
                "metric": metric_name,
                "trial_statistic": statistic,
                "unit": _metric_unit(metric_name),
                "direction": _direction(metric_name),
                "fingerprint_sha256": identities[scenario_id],
                "subject_sha256": subjects[scenario_id],
                "eligible_trials": len(values),
                "observation_count": observation_count,
                "percentile_eligible": observation_count >= 40,
                "distribution": distribution(values),
            }
        )
    return {
        "schema_version": SCHEMA_VERSION,
        "benchmark_id": BENCHMARK_ID,
        "status": "valid",
        "run_count": len(bundles),
        "trial_count": len(trials),
        "runs": [{"run_alias": bundle.alias} for bundle in bundles],
        "trials": sorted(trials, key=lambda item: (item["scenario_id"], item["trial_alias"])),
        "summaries": summaries,
        "evidence_guarantees": {
            "required_event_loss": 0,
            "required_request_loss": 0,
        },
        "comparison_contract": {
            "minimum_trials_per_arm": 5,
            "minimum_observations_for_p95_p99": 40,
            "repeatability_band": "max(resolution_floor, 3 * baseline_MAD)",
            "unfavorable_relative_threshold": 0.05,
            "resolution_floors": {
                "latency_ms": 5.0,
                "normalized_cpu_percentage_point": 0.1,
                "memory_bytes": 8 * 1024 * 1024,
                "byte_or_count_fraction_of_baseline": 0.01,
            },
        },
        "limitations": [
            "This report defines evidence-validity and disposition rules, not product budgets.",
            "Optional GPU counters may be absent without invalidating core process, UI, and server evidence.",
        ],
    }


def analyze(input_path: Path) -> dict[str, Any]:
    return analyze_many([input_path])


def _resolution_floor(summary: Mapping[str, Any]) -> float:
    unit = summary["unit"]
    baseline = abs(float(summary["distribution"]["median"]))
    if unit == "ms":
        return 5.0
    if unit == "percentage_point":
        return 0.1
    if unit == "bytes" and any(
        token in str(summary["metric"]).lower()
        for token in ("memory", "private", "working_set")
    ):
        return float(8 * 1024 * 1024)
    if unit == "bytes":
        return max(1.0, baseline * 0.01)
    if unit == "bytes_per_second":
        return max(1.0, baseline * 0.01)
    if unit == "count":
        return max(1.0, baseline * 0.01)
    return max(1e-9, baseline * 0.01)


def _validated_report_summaries(
    report: Mapping[str, Any], label: str
) -> dict[tuple[str, str, str], Mapping[str, Any]]:
    if report.get("schema_version") != SCHEMA_VERSION or report.get("status") != "valid":
        raise InvalidData(f"{label} is not a valid schema-version-1 report")
    raw_summaries = report.get("summaries")
    if not isinstance(raw_summaries, list):
        raise InvalidData(f"{label} summaries must be an array")
    summaries: dict[tuple[str, str, str], Mapping[str, Any]] = {}
    for index, item in enumerate(raw_summaries):
        if not isinstance(item, Mapping):
            raise InvalidData(f"{label} summary {index} must be an object")
        scenario_id = _identifier(item.get("scenario_id"), f"{label} summary scenario_id")
        metric = _identifier(item.get("metric"), f"{label} summary metric")
        statistic = _identifier(
            item.get("trial_statistic"), f"{label} summary trial_statistic"
        )
        if statistic not in {"median", "p95", "p99"}:
            raise InvalidData(f"{label} summary has an unsupported trial statistic")
        if item.get("unit") not in {
            "ms",
            "percentage_point",
            "bytes",
            "bytes_per_second",
            "ratio",
            "count",
        }:
            raise InvalidData(f"{label} summary has an unsupported unit")
        if item.get("direction") not in {"lower", "higher", "neutral"}:
            raise InvalidData(f"{label} summary has an unsupported direction")
        fingerprint = item.get("fingerprint_sha256")
        if not isinstance(fingerprint, str) or not HASH_RE.fullmatch(fingerprint):
            raise InvalidData(f"{label} summary has an invalid fingerprint")
        subject = item.get("subject_sha256")
        if subject is not None and (
            not isinstance(subject, str) or not HASH_RE.fullmatch(subject)
        ):
            raise InvalidData(f"{label} summary has an invalid subject fingerprint")
        for count_name in ("eligible_trials", "observation_count"):
            count = item.get(count_name)
            if isinstance(count, bool) or not isinstance(count, int) or count < 0:
                raise InvalidData(f"{label} summary has an invalid {count_name}")
        values = item.get("distribution")
        if not isinstance(values, Mapping):
            raise InvalidData(f"{label} summary distribution must be an object")
        _finite_number(values.get("median"), f"{label} summary median")
        mad = _finite_number(values.get("mad"), f"{label} summary MAD")
        if mad < 0:
            raise InvalidData(f"{label} summary MAD must be nonnegative")
        key = (scenario_id, metric, statistic)
        if key in summaries:
            raise InvalidData(f"{label} has duplicate summary identities")
        summaries[key] = item
    return summaries


def compare_reports(baseline: Mapping[str, Any], current: Mapping[str, Any]) -> dict[str, Any]:
    baseline_summaries = _validated_report_summaries(
        baseline, "--compare baseline report"
    )
    current_summaries = _validated_report_summaries(current, "current report")
    comparisons: list[dict[str, Any]] = []
    for key, after in sorted(current_summaries.items()):
        before = baseline_summaries.get(key)
        if before is None:
            continue
        if before.get("fingerprint_sha256") != after.get("fingerprint_sha256"):
            raise InvalidData(f"comparison fingerprint differs for {'/'.join(key)}")
        before_trials = int(before.get("eligible_trials", 0))
        after_trials = int(after.get("eligible_trials", 0))
        percentile_metric = key[2] in {"p95", "p99"}
        enough_observations = (
            not percentile_metric
            or (
                int(before.get("observation_count", 0)) >= 40
                and int(after.get("observation_count", 0)) >= 40
            )
        )
        eligible = (
            before_trials >= 5
            and after_trials >= 5
            and before_trials == after_trials
            and enough_observations
        )
        before_value = float(before["distribution"]["median"])
        after_value = float(after["distribution"]["median"])
        delta = after_value - before_value
        direction = str(after.get("direction", "neutral"))
        unfavorable = delta if direction == "lower" else -delta if direction == "higher" else 0.0
        relative = math.inf if before_value == 0 and unfavorable > 0 else (
            unfavorable / abs(before_value) if before_value != 0 else 0.0
        )
        repeatability_band = max(
            _resolution_floor(before), 3.0 * float(before["distribution"]["mad"])
        )
        hard_resource_signal = bool(
            str(key[1]).endswith("_sustained_monotonic_growth_flag")
            and before_value <= 0
            and after_value > 0
        )
        disposition = bool(
            eligible
            and direction != "neutral"
            and (
                hard_resource_signal
                or (unfavorable > repeatability_band and relative > 0.05)
            )
        )
        comparisons.append(
            {
                "scenario_id": key[0],
                "metric": key[1],
                "trial_statistic": key[2],
                "direction": direction,
                "eligible": eligible,
                "baseline_trials": before_trials,
                "current_trials": after_trials,
                "baseline_median": before_value,
                "current_median": after_value,
                "baseline_subject_sha256": before.get("subject_sha256"),
                "current_subject_sha256": after.get("subject_sha256"),
                "delta": delta,
                "unfavorable_relative_delta": relative if math.isfinite(relative) else "infinite",
                "repeatability_band": repeatability_band,
                "hard_disposition_signal": hard_resource_signal,
                "disposition_requiring": disposition,
                "reason": (
                    "new sustained monotonic resource growth"
                    if disposition and hard_resource_signal
                    else "unfavorable delta exceeds repeatability band and five percent"
                    if disposition
                    else "insufficient balanced evidence"
                    if not eligible
                    else "within repeatability/disposition contract"
                ),
            }
        )
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "compared",
        "disposition_required": any(item["disposition_requiring"] for item in comparisons),
        "metrics": comparisons,
    }


def _observer_trials(
    report: Mapping[str, Any], label: str, expected_profile: str
) -> dict[tuple[str, str], Mapping[str, Any]]:
    _validated_report_summaries(report, label)
    guarantees = report.get("evidence_guarantees")
    if not isinstance(guarantees, Mapping) or any(
        guarantees.get(name) != 0
        for name in ("required_event_loss", "required_request_loss")
    ):
        raise InvalidData(f"{label} does not prove zero required-event/request loss")
    raw_trials = report.get("trials")
    if not isinstance(raw_trials, list):
        raise InvalidData(f"{label} trials must be an array")
    trials: dict[tuple[str, str], Mapping[str, Any]] = {}
    for index, trial in enumerate(raw_trials):
        if not isinstance(trial, Mapping):
            raise InvalidData(f"{label} trial {index} must be an object")
        scenario_id = _identifier(
            trial.get("scenario_id"), f"{label} trial scenario_id"
        )
        trial_identity = trial.get("trial_identity_sha256")
        if not isinstance(trial_identity, str) or not HASH_RE.fullmatch(trial_identity):
            raise InvalidData(f"{label} trial has an invalid paired-trial identity")
        if trial.get("observer_profile") != expected_profile:
            raise InvalidData(f"{label} is not a {expected_profile}-observer arm")
        for digest_name in (
            "control_fingerprint_sha256",
            "subject_sha256",
            "outcome_sha256",
        ):
            digest = trial.get(digest_name)
            if not isinstance(digest, str) or not HASH_RE.fullmatch(digest):
                raise InvalidData(f"{label} trial has an invalid {digest_name}")
        evidence = trial.get("observer_control_evidence")
        if not isinstance(evidence, Mapping):
            raise InvalidData(f"{label} trial lacks observer-control evidence")
        duration = _nonnegative_number(
            evidence.get("duration_ms"), f"{label} observer-control duration"
        )
        if duration < 60_000:
            raise InvalidData(f"{label} observer-control trial is shorter than 60 seconds")
        if evidence.get("action_latency_scope") != "seek":
            raise InvalidData(f"{label} observer-control latency scope must be seek")
        latencies = evidence.get("action_latency_ms")
        if not isinstance(latencies, list) or not latencies:
            raise InvalidData(f"{label} observer-control trial has no action latency evidence")
        for latency in latencies:
            _nonnegative_number(latency, f"{label} action latency")
        _nonnegative_number(
            evidence.get("max_private_bytes"), f"{label} maximum private memory"
        )
        _nonnegative_number(
            evidence.get("aggregate_cpu_time_ms"), f"{label} aggregate CPU time"
        )
        process_count = _nonnegative_number(
            evidence.get("max_process_count"), f"{label} process count"
        )
        if process_count < 1:
            raise InvalidData(f"{label} observer-control trial has no process tree")
        quantum = _nonnegative_number(
            evidence.get("cpu_accounting_quantum_ms"),
            f"{label} CPU accounting quantum",
        )
        if quantum <= 0:
            raise InvalidData(f"{label} CPU accounting quantum must be positive")
        for evidence_name in (
            "server_request_count",
            "server_completed_count",
            "server_error_count",
            "server_range_count",
            "server_cancelled_count",
            "server_delivered_bytes",
        ):
            _nonnegative_number(
                evidence.get(evidence_name),
                f"{label} {evidence_name.replace('_', ' ')}",
            )
        total_frames = _nonnegative_number(
            evidence.get("scenario_total_frames"),
            f"{label} scenario total frames",
        )
        if total_frames <= 0:
            raise InvalidData(f"{label} scenario total frames must be positive")
        dropped_frames = _nonnegative_number(
            evidence.get("scenario_dropped_frames"),
            f"{label} scenario dropped frames",
        )
        if dropped_frames > total_frames:
            raise InvalidData(f"{label} scenario dropped frames exceed total frames")
        dropped_frame_rate = _nonnegative_number(
            evidence.get("scenario_dropped_frame_rate"),
            f"{label} scenario dropped-frame rate",
        )
        expected_rate = dropped_frames / total_frames
        if not math.isclose(dropped_frame_rate, expected_rate, rel_tol=1e-12, abs_tol=1e-12):
            raise InvalidData(f"{label} scenario dropped-frame rate is inconsistent")
        key = (scenario_id, trial_identity)
        if key in trials:
            raise InvalidData(f"{label} has duplicate paired-trial identities")
        trials[key] = trial
    return trials


def _observer_repeatability_gate(
    minimal_values: Sequence[float],
    full_values: Sequence[float],
    *,
    resolution_floor: float,
) -> dict[str, Any]:
    minimal_median = float(statistics.median(minimal_values))
    full_median = float(statistics.median(full_values))
    minimal_mad = float(
        statistics.median(abs(value - minimal_median) for value in minimal_values)
    )
    repeatability_band = max(float(resolution_floor), 3.0 * minimal_mad)
    increase = full_median - minimal_median
    relative_increase = (
        increase / minimal_median
        if minimal_median > 0
        else math.inf
        if increase > 0
        else 0.0
    )
    exceeds_relative_floor = (
        increase > minimal_median * 0.05 if minimal_median > 0 else increase > 0
    )
    disposition_requiring = increase > repeatability_band and exceeds_relative_floor
    return {
        "minimal_median": minimal_median,
        "full_median": full_median,
        "increase": increase,
        "unfavorable_relative_increase": (
            relative_increase if math.isfinite(relative_increase) else "infinite"
        ),
        "maximum_relative_increase": 0.05,
        "minimal_mad": minimal_mad,
        "resolution_floor": float(resolution_floor),
        "repeatability_band": repeatability_band,
        "passed": not disposition_requiring,
    }


def validate_observer_control(
    minimal: Mapping[str, Any], full: Mapping[str, Any]
) -> dict[str, Any]:
    minimal_trials = _observer_trials(minimal, "minimal observer report", "minimal")
    full_trials = _observer_trials(full, "full observer report", "full")
    if set(minimal_trials) != set(full_trials):
        raise InvalidData("observer-control arms do not contain identical scenario/trial pairs")
    if len(minimal_trials) < 4:
        raise InvalidData("observer-control gate requires at least four matched trial pairs")

    minimal_latencies: list[float] = []
    full_latencies: list[float] = []
    minimal_memory: list[float] = []
    full_memory: list[float] = []
    minimal_cpu: list[float] = []
    full_cpu: list[float] = []
    quantums: list[float] = []
    minimal_process_counts: list[float] = []
    full_process_counts: list[float] = []
    schedule_metric_names = (
        "server_request_count",
        "server_completed_count",
        "server_error_count",
        "server_range_count",
        "server_cancelled_count",
        "server_delivered_bytes",
    )
    minimal_schedule_metrics: dict[str, list[float]] = {
        name: [] for name in schedule_metric_names
    }
    full_schedule_metrics: dict[str, list[float]] = {
        name: [] for name in schedule_metric_names
    }
    minimal_dropped_frame_rates: list[float] = []
    full_dropped_frame_rates: list[float] = []
    frame_totals: list[float] = []
    for key in sorted(minimal_trials):
        before = minimal_trials[key]
        after = full_trials[key]
        if before["control_fingerprint_sha256"] != after["control_fingerprint_sha256"]:
            raise InvalidData(
                f"observer-control fingerprint differs for scenario {key[0]}"
            )
        if before["subject_sha256"] != after["subject_sha256"]:
            raise InvalidData(
                f"observer-control app identity differs for scenario {key[0]}"
            )
        if before["outcome_sha256"] != after["outcome_sha256"]:
            raise InvalidData(
                f"observer-control semantic action/seek/request/error/recovery outcomes differ for scenario {key[0]}"
            )
        before_evidence = before["observer_control_evidence"]
        after_evidence = after["observer_control_evidence"]
        minimal_latencies.extend(float(value) for value in before_evidence["action_latency_ms"])
        full_latencies.extend(float(value) for value in after_evidence["action_latency_ms"])
        minimal_memory.append(float(before_evidence["max_private_bytes"]))
        full_memory.append(float(after_evidence["max_private_bytes"]))
        minimal_cpu.append(float(before_evidence["aggregate_cpu_time_ms"]))
        full_cpu.append(float(after_evidence["aggregate_cpu_time_ms"]))
        quantums.extend(
            (
                float(before_evidence["cpu_accounting_quantum_ms"]),
                float(after_evidence["cpu_accounting_quantum_ms"]),
            )
        )
        minimal_process_counts.append(float(before_evidence["max_process_count"]))
        full_process_counts.append(float(after_evidence["max_process_count"]))
        for name in schedule_metric_names:
            minimal_schedule_metrics[name].append(float(before_evidence[name]))
            full_schedule_metrics[name].append(float(after_evidence[name]))
        minimal_dropped_frame_rates.append(
            float(before_evidence["scenario_dropped_frame_rate"])
        )
        full_dropped_frame_rates.append(
            float(after_evidence["scenario_dropped_frame_rate"])
        )
        frame_totals.extend(
            (
                float(before_evidence["scenario_total_frames"]),
                float(after_evidence["scenario_total_frames"]),
            )
        )

    minimal_latency_median = float(statistics.median(minimal_latencies))
    full_latency_median = float(statistics.median(full_latencies))
    minimal_latency_p95 = percentile_type7(minimal_latencies, 0.95)
    full_latency_p95 = percentile_type7(full_latencies, 0.95)
    minimal_memory_max = max(minimal_memory)
    full_memory_max = max(full_memory)
    minimal_cpu_total = sum(minimal_cpu)
    full_cpu_total = sum(full_cpu)
    cpu_absolute_allowance = max(quantums) * max(
        sum(minimal_process_counts), sum(full_process_counts)
    )
    cpu_allowed_increase = max(minimal_cpu_total * 0.02, cpu_absolute_allowance)
    gates = {
        "median_action_latency": {
            "minimal_ms": minimal_latency_median,
            "full_ms": full_latency_median,
            "maximum_relative_increase": 0.05,
            "passed": full_latency_median <= minimal_latency_median * 1.05,
        },
        "p95_action_latency": {
            "minimal_ms": minimal_latency_p95,
            "full_ms": full_latency_p95,
            "maximum_relative_increase": 0.10,
            "passed": full_latency_p95 <= minimal_latency_p95 * 1.10,
        },
        "maximum_private_memory": {
            "minimal_bytes": minimal_memory_max,
            "full_bytes": full_memory_max,
            "increase_bytes": full_memory_max - minimal_memory_max,
            "maximum_increase_bytes": 16 * 1024 * 1024,
            "passed": full_memory_max - minimal_memory_max <= 16 * 1024 * 1024,
        },
        "aggregate_process_tree_cpu_time": {
            "minimal_ms": minimal_cpu_total,
            "full_ms": full_cpu_total,
            "increase_ms": full_cpu_total - minimal_cpu_total,
            "maximum_increase_ms": cpu_allowed_increase,
            "relative_allowance": 0.02,
            "accounting_quantum_floor_ms": cpu_absolute_allowance,
            "passed": full_cpu_total - minimal_cpu_total <= cpu_allowed_increase,
        },
    }
    for name in schedule_metric_names:
        minimal_values = minimal_schedule_metrics[name]
        full_values = full_schedule_metrics[name]
        gates[name] = _observer_repeatability_gate(
            minimal_values,
            full_values,
            resolution_floor=max(1.0, float(statistics.median(minimal_values)) * 0.01),
        )
    gates["scenario_dropped_frame_rate"] = _observer_repeatability_gate(
        minimal_dropped_frame_rates,
        full_dropped_frame_rates,
        resolution_floor=max(1.0 / total_frames for total_frames in frame_totals),
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "passed" if all(gate["passed"] for gate in gates.values()) else "failed",
        "gate_passed": all(gate["passed"] for gate in gates.values()),
        "matched_pair_count": len(minimal_trials),
        "pairing": "matched_scenario_and_sanitized_trial_identity",
        "outcomes_match": True,
        "required_event_request_loss": 0,
        "gates": gates,
    }


def _load_comparison(path: Path) -> dict[str, Any]:
    if path.is_file():
        return _read_json(path, "comparison report")
    report_path = path / "report.json"
    if report_path.is_file():
        return _read_json(report_path, "comparison report")
    return analyze(path)


def render_markdown(report: Mapping[str, Any]) -> str:
    lines = [
        "# QB-REPLAY-008 replay benchmark report",
        "",
        f"Status: **{report['status']}**",
        "",
        f"Runs: {report['run_count']}  ",
        f"Preserved trials: {report['trial_count']}",
        "",
        "This report applies evidence-validity and disposition rules; it does not define product budgets.",
        "",
        "## Scenario summaries",
        "",
        "| Scenario | Metric | Trial statistic | Trials | Observations | Median | p95 | MAD |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for item in report["summaries"]:
        values = item["distribution"]
        lines.append(
            f"| {item['scenario_id']} | {item['metric']} | {item['trial_statistic']} | "
            f"{item['eligible_trials']} | {item['observation_count']} | "
            f"{values['median']:.6g} | {values['p95']:.6g} | {values['mad']:.6g} |"
        )
    comparison = report.get("comparison")
    if isinstance(comparison, Mapping):
        lines.extend(
            [
                "",
                "## Before/after comparison",
                "",
                f"Disposition required: **{'yes' if comparison['disposition_required'] else 'no'}**",
                "",
                "| Scenario | Metric | Statistic | Eligible | Delta | Band | Disposition |",
                "| --- | --- | --- | --- | ---: | ---: | --- |",
            ]
        )
        for item in comparison["metrics"]:
            lines.append(
                f"| {item['scenario_id']} | {item['metric']} | {item['trial_statistic']} | "
                f"{'yes' if item['eligible'] else 'no'} | {item['delta']:.6g} | "
                f"{item['repeatability_band']:.6g} | "
                f"{'review' if item['disposition_requiring'] else 'no'} |"
            )
    observer_control = report.get("observer_control")
    if isinstance(observer_control, Mapping):
        lines.extend(
            [
                "",
                "## Observer-control gate",
                "",
                f"Gate: **{'passed' if observer_control['gate_passed'] else 'failed'}**  ",
                f"Matched minimal/full pairs: {observer_control['matched_pair_count']}",
                "",
                "| Check | Passed |",
                "| --- | --- |",
            ]
        )
        for name, gate in observer_control["gates"].items():
            lines.append(f"| {name} | {'yes' if gate['passed'] else 'no'} |")
    lines.extend(["", "## Limitations", ""])
    lines.extend(f"- {limitation}" for limitation in report["limitations"])
    return "\n".join(lines) + "\n"


def write_reports(report: Mapping[str, Any], output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    json_text = json.dumps(report, indent=2, sort_keys=True, ensure_ascii=True, allow_nan=False) + "\n"
    (output_dir / "report.json").write_text(json_text, encoding="utf-8", newline="\n")
    (output_dir / "report.md").write_text(render_markdown(report), encoding="utf-8", newline="\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate and deterministically analyze QB-REPLAY-008 run bundles."
    )
    parser.add_argument(
        "--input",
        "--run-root",
        dest="inputs",
        required=True,
        type=Path,
        action="append",
        help="Run-bundle directory or matrix root; repeat to aggregate explicit immutable runs.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="Directory for report.json and report.md (defaults to --input).",
    )
    parser.add_argument(
        "--compare",
        type=Path,
        help="Baseline report.json, report directory, or raw matrix root for before/after rules.",
    )
    parser.add_argument(
        "--observer-control",
        type=Path,
        help=(
            "Minimal-observer report.json, report directory, or raw matrix root; "
            "--input supplies the matching full-observer arm."
        ),
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = build_parser().parse_args(argv)
    try:
        report = analyze_many(arguments.inputs)
        if arguments.compare is not None:
            report["comparison"] = compare_reports(_load_comparison(arguments.compare), report)
        if arguments.observer_control is not None:
            report["observer_control"] = validate_observer_control(
                _load_comparison(arguments.observer_control), report
            )
        if arguments.output_dir is None and len(arguments.inputs) > 1:
            raise InvalidData("--output-dir is required when more than one --input is selected")
        write_reports(report, arguments.output_dir or arguments.inputs[0])
    except InvalidData as error:
        print(f"QB-REPLAY-008 analysis rejected: {error}", file=sys.stderr)
        return 2
    if arguments.observer_control is not None and not report["observer_control"]["gate_passed"]:
        print("QB-REPLAY-008 observer-control gate failed", file=sys.stderr)
        return 3
    print(
        f"QB-REPLAY-008 analysis valid: {report['run_count']} runs, "
        f"{report['trial_count']} preserved trials"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
