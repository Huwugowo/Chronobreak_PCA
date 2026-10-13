"""Validate the frozen QB-REPLAY-010 matrix and compare frontend opening clocks.

Raw validity, resource reports and observer controls use analyze.py. This module
only adds the declared subject roles, complete matrix and process-level opening
statistics. It does not turn latency evidence into full-feature acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import ntpath
from pathlib import Path
import stat
import statistics
from typing import Any

from jsonschema import Draft202012Validator

import analyze


CONTRACT = analyze.OPENING_CONTRACT
ROLES = ("reference", "candidate")
CELLS = ("S", "R", "L", "N", "E")
KINDS = ("cold_open", "warm_open")
SHARED_METRICS = ("library_request_to_games_usable_ms", "intent_to_media_open_ms",
                  "intent_to_loadstart_ms", "intent_to_metadata_ms", "intent_to_canplay_ms",
                  "intent_to_first_frame_ms", "intent_to_details_ms")


def validate_schema(value: Any, name: str) -> None:
    schema = json.loads(Path(__file__).with_name(name).read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    errors = sorted(Draft202012Validator(schema).iter_errors(value), key=lambda error: str(error.path))
    if errors:
        raise analyze.InvalidData(f"{name}: {errors[0].json_path}: {errors[0].message}")


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def owned_path(base: Path, relative: str) -> Path:
    child = Path(relative)
    if child.is_absolute() or ntpath.isabs(relative) or ntpath.splitdrive(relative)[0] or ".." in child.parts:
        raise analyze.InvalidData("campaign artifact path must be relative and contained")
    path = base / child
    resolved = path.resolve()
    if base.resolve() not in resolved.parents:
        raise analyze.InvalidData("campaign artifact escaped its root")
    for part in (path, *path.parents):
        if part.is_symlink() or (part.exists() and getattr(part.stat(), "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT):
            raise analyze.InvalidData("campaign artifacts must be reparse-free")
    return resolved


def checked_file(base: Path, record: dict[str, str]) -> Path:
    path = owned_path(base, record["path"])
    if not path.is_file() or file_digest(path) != record["sha256"]:
        raise analyze.InvalidData("frozen source/validation artifact hash mismatch")
    return path


def process_values(measurement: dict[str, Any], kind: str) -> dict[str, float]:
    cycles = measurement["cycles"][1:] if kind == "warm_open" else measurement["cycles"]
    expected = 5 if kind == "warm_open" else 1
    if len(cycles) != expected:
        raise analyze.InvalidData("missing independent process opening observations")
    names = cycles[0]["metrics"].keys()
    if any(row["metrics"].keys() != names for row in cycles):
        raise analyze.InvalidData("missing process primary metric")
    result = {name: statistics.median([row["metrics"][name] for row in cycles]) for name in names}
    result["library_request_to_games_usable_ms"] = measurement["library_request_to_games_usable_ms"]
    if not set(SHARED_METRICS).issubset(result):
        raise analyze.InvalidData("missing process primary metric")
    return result


def input_fingerprint(manifest: dict[str, Any], scenario: dict[str, Any], config_sha256: str) -> str:
    """Freeze declared inputs before launch, without predicting observed environment.

    The full raw comparison fingerprint additionally binds the measured runtime,
    device, collector accounting quantum and WebView version across both arms.
    """
    # Planning sees the raw scenario; admission sees the analyzer's resolved
    # action count (including its aliases). An unconstrained count stays absent,
    # while every explicit count and the original declaration remain bound.
    scenario = dict(scenario)
    action_count = scenario.get("expected_action_count", scenario.get("action_count", scenario.get("actions")))
    if isinstance(action_count, list):
        action_count = len(action_count)
    if action_count is None:
        scenario.pop("expected_action_count", None)
    else:
        scenario["expected_action_count"] = action_count
    return analyze._digest(analyze._identity_material(manifest, scenario, {"config_sha256": config_sha256}))


def compare_processes(runs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    comparisons = []
    for cell in CELLS:
        for kind in KINDS:
            arms = {role: [run for run in runs if (run["cell"], run["kind"], run["role"]) == (cell, kind, role)] for role in ROLES}
            for arm in arms.values():
                if len(arm) != 5 or {run["round"] for run in arm} != set(range(1, 6)):
                    raise analyze.InvalidData("opening comparison requires five independent processes per arm/cell")
            if len({run["comparison_fingerprint"] for arm in arms.values() for run in arm}) != 1:
                raise analyze.InvalidData("opening non-subject fingerprints differ")
            for metric in SHARED_METRICS:
                try:
                    values = {role: [analyze._nonnegative_number(run["process_metrics"][metric], metric) for run in arms[role]] for role in ROLES}
                except KeyError as error:
                    raise analyze.InvalidData("missing process primary metric") from error
                before, after = (statistics.median(values[role]) for role in ROLES)
                mad = statistics.median([abs(value - before) for value in values["reference"]])
                floor = max(5.0, 3 * mad)
                improved = before - after > before * 0.05 and before - after > floor
                regressed = after - before > before * 0.05 and after - before > floor
                required = cell in {"N", "E"} and metric == "library_request_to_games_usable_ms"
                gate = ("required_improvement_missing" if required and not improved else
                        "disposition_required" if regressed else "improved" if improved else "no_material_regression")
                comparisons.append({"cell": cell, "kind": kind, "metric": metric,
                                    "independent_processes_per_arm": 5, "reference_median": before,
                                    "candidate_median": after, "reference_mad": mad,
                                    "absolute_threshold_ms": floor, "gate": gate})
    return comparisons


def acceptance(campaign_path: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    campaign = json.loads(campaign_path.read_text(encoding="utf-8"))
    validate_schema(campaign, "opening-campaign.schema.json")
    base = campaign_path.parent
    cells = {(cell["cell"], cell["kind"]): cell for cell in campaign["cells"]}
    if set(cells) != {(cell, kind) for cell in CELLS for kind in KINDS} or len(campaign["cells"]) != 10:
        raise analyze.InvalidData("campaign lacks the exact ten opening cells")
    for subject in campaign["subjects"].values():
        checked_file(base, subject["source_patch"])
        receipt = json.loads(checked_file(base, subject["build_receipt"]).read_text(encoding="utf-8"))
        if (receipt.get("status") != "passed"
                or receipt.get("source_revision") != subject["source_revision"]
                or receipt.get("source_patch_sha256") != subject["source_patch"]["sha256"]
                or receipt.get("binary_sha256") != subject["binary_sha256"]):
            raise analyze.InvalidData("frozen app build/source/patch/binary mismatch")
    rows, grouped_paths, seen_paths, seen_ids, seen_slots = [], {}, set(), set(), set()
    subject_fingerprints = {}
    for sequence, declaration in enumerate(campaign["runs"], 1):
        if declaration["sequence"] != sequence:
            raise analyze.InvalidData("campaign run order is not complete and consecutive")
        role, kind, profile = (declaration[name] for name in ("role", "kind", "observer_profile"))
        control = kind == "seek"
        if control != (sequence <= 16):
            raise analyze.InvalidData("observer controls must precede the fixed main matrix")
        slot = (role, declaration["cell"], kind, profile, declaration["round"])
        if slot in seen_slots:
            raise analyze.InvalidData("duplicate campaign matrix slot")
        seen_slots.add(slot)
        path = owned_path(base, declaration["path"])
        if path in seen_paths:
            raise analyze.InvalidData("a process bundle cannot count as independent repetitions")
        seen_paths.add(path)
        if file_digest(path / "manifest.json") != declaration["manifest_sha256"]:
            raise analyze.InvalidData("frozen manifest hash mismatch")
        bundle = analyze.load_bundle(path, f"run-{sequence:03d}")
        if len(bundle.expected) != 1:
            raise analyze.InvalidData("opening acceptance requires one trial per process")
        key, spec = next(iter(bundle.expected.items()))
        if bundle.manifest["run_id"] in seen_ids:
            raise analyze.InvalidData("duplicate process run identity")
        seen_ids.add(bundle.manifest["run_id"])
        subject = campaign["subjects"][role]
        subject_fingerprint = bundle.subject_by_scenario[key.scenario_id]
        if (subject_fingerprint != subject_fingerprints.setdefault(role, subject_fingerprint)
                or bundle.runner_metadata["app_binary_sha256"] != subject["binary_sha256"]):
            raise analyze.InvalidData("frozen subject role/binary mismatch")
        cell = campaign["control"] if control else cells[(declaration["cell"], kind)]
        fingerprint = bundle.identity_by_scenario[key.scenario_id]
        expected_input = cell["input_fingerprints"][profile] if control else cell["input_fingerprint"]
        fixtures = spec.get("fixture_ids", [spec.get("fixture_id")])
        if (key.scenario_id != cell["scenario_id"] or spec.get("kind") != kind
                or fixtures != [cell["fixture_id"]]
                or input_fingerprint(bundle.manifest, spec, bundle.runner_metadata["config_sha256"]) != expected_input
                or bundle.runner_metadata["config_sha256"] != cell["config_sha256"]
                or bundle.manifest["observer_profile"] != profile):
            raise analyze.InvalidData("frozen scenario/fixture/config/runtime identity mismatch")
        fixture = next(f for f in bundle.manifest["fixtures"] if f["id"] == fixtures[0])
        if str(fixture.get("codec", "")).lower() != "h264":
            raise analyze.InvalidData("opening campaign requires H.264 fixtures")
        measurement = analyze.opening_measurements(key.scenario_id, spec, bundle.events, role)
        group = (role, profile) if control else (role, "primary")
        grouped_paths.setdefault(group, []).append(path)
        if control:
            if declaration["cell"] != "R" or declaration["round"] > 4:
                raise analyze.InvalidData("observer control matrix does not match four R pairs")
        else:
            if profile != "full":
                raise analyze.InvalidData("primary opening matrix requires the frozen full profile")
            rows.append({"run_alias": bundle.alias, "role": role, "cell": declaration["cell"],
                         "kind": kind, "round": declaration["round"], "manifest_sha256": declaration["manifest_sha256"],
                         "comparison_fingerprint": fingerprint, "cycles": measurement["cycles"],
                         "process_metrics": process_values(measurement, kind)})
    controls, generic = {}, {}
    for role in ROLES:
        if any(len(grouped_paths.get((role, profile), [])) != 4 for profile in ("minimal", "full")):
            raise analyze.InvalidData("missing observer control processes")
        minimal = analyze.analyze_many(grouped_paths[(role, "minimal")])
        full = analyze.analyze_many(grouped_paths[(role, "full")])
        controls[role] = analyze.validate_observer_control(minimal, full)
        if not controls[role]["gate_passed"]:
            raise analyze.InvalidData(f"{role} observer control failed")
        generic[role] = analyze.analyze_many(grouped_paths[(role, "primary")])
    comparisons = compare_processes(rows)
    report = {"schema_version": 1, "measurement_contract": CONTRACT, "status": "valid",
              "campaign_sha256": file_digest(campaign_path),
              "subjects": {role: {name: subject[name] for name in ("source_revision", "binary_sha256")}
                           | {"source_patch_sha256": subject["source_patch"]["sha256"], "subject_fingerprint": subject_fingerprints[role]}
                           for role, subject in campaign["subjects"].items()},
              "runs": rows, "comparisons": comparisons,
              "latency_gate_passed": all(row["gate"] not in {"required_improvement_missing", "disposition_required"} for row in comparisons),
              "full_feature_acceptance": "requires_resource_and_native_disposition",
              "limitations": ["Cold means fresh process, not cold OS file cache.",
                              "Warm process values are medians of five remounts; initial mounts stay separate.",
                              "Remount intent includes the existing 100 ms scheduling delay.",
                              "Native processing and Job/server counters remain whole-process observations in generic reports.",
                              "Integrated source comparison does not isolate descriptor-only effects."]}
    validate_schema(report, "opening-report.schema.json")
    generic["comparison"] = analyze.compare_reports(generic["reference"], generic["candidate"])
    generic["observer_controls"] = controls
    return report, generic


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        report, generic = acceptance(args.campaign.resolve())
        args.output_dir.mkdir(parents=True, exist_ok=False)
        for name, value in {"opening-report": report, **generic}.items():
            with (args.output_dir / f"{name}.json").open("x", encoding="utf-8") as output:
                json.dump(value, output, indent=2, allow_nan=False)
                output.write("\n")
    except (analyze.InvalidData, OSError, ValueError, KeyError) as error:
        parser.exit(2, f"opening acceptance failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
