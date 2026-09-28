"""Fixed 100-launch replay-opening campaign planner, driver and verifier.

The existing matrix/runner/analyzer remain authorities for each launch. This
module additionally proves the global order and never retries an attempted slot.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import subprocess
import sys
import time
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path

import analyze
import matrix

SEED = 20260923
CEILING = 100
COOLDOWN = 5
CELLS = tuple("SRLNE")
MODES = ("cold", "warm")
ARMS = ("reference", "candidate")
COMMITS = {"reference": "3b14df20aa7ab4e567fd0372c1abc72c07bbe18e",
           "candidate": "a840c645e5080ec13a7cb23c540eabc388676a83"}
BUILD_COMMAND = ["npm.cmd", "run", "desktop:build:benchmark", "--prefix", "app"]
ARTIFACTS = tuple(dict.fromkeys((*analyze.ARTIFACTS, "runner-metadata.json", "runner-result.json",
                               "post-hashes.json", "terminal.app.json")))
TOOLS = ("opening_campaign.py", "opening_comparison.py", "opening_environment.ps1", "matrix.py", "analyze.py", "run.ps1")
Error = matrix.MatrixError


def read(path):
    return matrix._read_json(Path(path), "campaign artifact")[0]


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def inside(root, path, exists=True):
    path = Path(path)
    if not path.is_absolute():
        raise Error("campaign paths must be absolute")
    matrix._assert_reparse_free(path, "campaign path")
    resolved = path.resolve(strict=exists)
    if resolved == root or not resolved.is_relative_to(root):
        raise Error("campaign path escapes sentinel")
    return resolved


def write_new(path, value):
    raw = (json.dumps(value, indent=2, allow_nan=False) + "\n").encode("utf-8")
    with Path(path).open("xb") as output:
        output.write(raw)
        output.flush()
        os.fsync(output.fileno())


def order():
    rng, rows, pair = random.Random(SEED), [], 0
    for round_number in range(1, 6):
        cells = [(cell, mode) for cell in CELLS for mode in MODES]
        rng.shuffle(cells)
        for cell, mode in cells:
            pair += 1
            arms = ARMS if pair % 2 else ARMS[::-1]
            for arm in arms:
                rows.append({"sequence": len(rows) + 1, "pair": pair, "round": round_number,
                             "cell": cell, "mode": mode, "arm": arm})
    return rows


def file_identity(root, path):
    path = inside(root, path)
    if not path.is_file():
        raise Error("frozen artifact is not a regular file")
    return {"path": str(path), "sha256": digest(path)}


def compare_identity(manifest, metadata):
    scenario = manifest["scenarios"][0]
    return analyze._digest(analyze._identity_material(manifest, scenario, metadata))


def compile_plan(spec_path):
    spec_path = Path(spec_path).resolve(strict=True)
    spec = read(spec_path)
    if spec.get("schema_version") != 1:
        raise Error("campaign spec schema version must be 1")
    root = Path(spec["sentinel_root"])
    matrix._assert_reparse_free(root, "campaign sentinel")
    root = root.resolve(strict=True)
    if root.name != matrix.SENTINEL_NAME:
        raise Error("campaign sentinel name is invalid")
    inside(root, spec_path)
    if spec.get("conditions", {}).get("display") != "visible_always_on_top" or spec.get("conditions", {}).get("operator_input") != "none":
        raise Error("declare visible display and no operator input")
    if not spec["conditions"].get("background_description"):
        raise Error("declare background conditions before collection")
    runner_environment = spec.get("runner_environment")
    if not isinstance(runner_environment, dict) or not runner_environment.get("environment"):
        raise Error("freeze the normalized runner environment before collection")
    environment_receipt = file_identity(root, spec["environment_receipt"])
    environment = read(environment_receipt["path"])
    if (environment.get("schema_version") != 1 or environment.get("runner_environment") != runner_environment
            or not analyze.HASH_RE.fullmatch(str(environment.get("installed_webview_sha256", "")))):
        raise Error("frozen environment receipt mismatch")
    subjects = {}
    for arm in ARMS:
        subject = spec["subjects"][arm]
        binary = inside(root, subject["path"])
        if subject.get("source_commit") != COMMITS[arm]:
            raise Error("subject source commit differs from approved reference/candidate")
        archive = file_identity(root, subject["source_archive"])
        receipt = file_identity(root, subject["build_receipt"])
        built = read(receipt["path"])
        if (built.get("source_commit") != COMMITS[arm] or built.get("exit_code") != 0
                or built.get("command") != BUILD_COMMAND
                or built.get("source_archive_sha256") != archive["sha256"]
                or built.get("binary_sha256") != digest(binary)):
            raise Error("subject lacks matching successful optimized build provenance")
        runtime_root = inside(root, binary.parent / "resources" / "media-runtime")
        runtime_files = [file_identity(root, path) for path in sorted(runtime_root.rglob("*")) if path.is_file()]
        if not runtime_files:
            raise Error("packaged runtime is missing")
        subjects[arm] = {**file_identity(root, binary), "runtime_files": runtime_files,
                         "source_commit": COMMITS[arm], "source_archive": archive, "build_receipt": receipt}
    if subjects["reference"]["sha256"] == subjects["candidate"]["sha256"]:
        raise Error("reference and candidate binaries must differ")
    if subjects["reference"]["source_archive"]["sha256"] == subjects["candidate"]["source_archive"]["sha256"]:
        raise Error("reference and candidate source archives must differ")
    ref_runtime = {str(Path(row["path"]).relative_to(Path(subjects["reference"]["path"]).parent)): row["sha256"]
                   for row in subjects["reference"]["runtime_files"]}
    cur_runtime = {str(Path(row["path"]).relative_to(Path(subjects["candidate"]["path"]).parent)): row["sha256"]
                   for row in subjects["candidate"]["runtime_files"]}
    if ref_runtime != cur_runtime:
        raise Error("subjects do not carry identical packaged runtimes")
    tool_root = inside(root, spec["tool_root"])
    tools = [file_identity(root, tool_root / name) for name in TOOLS]
    if not (tool_root / "schemas").is_dir():
        raise Error("frozen tool schemas missing")
    tools.extend(file_identity(root, path) for path in sorted((tool_root / "schemas").glob("*.json")))
    selected, plans = {}, []
    for declared in spec["matrices"]:
        key = (declared["cell"], declared["mode"], declared["arm"])
        if key in selected or key[0] not in CELLS or key[1] not in MODES or key[2] not in ARMS:
            raise Error("duplicate or unknown matrix membership")
        path = inside(root, declared["path"])
        matrix.verify_matrix(path)
        local = read(path)
        if (len(local["launches"]) != 5 or local["seed"] != SEED or local["cooldown_seconds"] != COOLDOWN
                or local.get("isolate_launch_roots") is not True):
            raise Error("matrix must contain five fixed trials with the campaign seed/cooldown")
        selected[key] = (path, local)
        plans.append({**dict(zip(("cell", "mode", "arm"), key)), **file_identity(root, path)})
    if set(selected) != {(c, m, a) for c in CELLS for m in MODES for a in ARMS}:
        raise Error("campaign requires all twenty matrices")
    entries, unique = [], {name: set() for name in ("run_id", "result_root", "manifest_path", "app_data_root", "scratch_root")}
    for item in order():
        path, local = selected[(item["cell"], item["mode"], item["arm"])]
        launch = local["launches"][item["round"] - 1]
        manifest_path = inside(root, launch["manifest_path"])
        manifest = read(manifest_path)
        scenario = manifest["scenarios"][0]
        expected_id = f"qb010-library-v2-replay-opening-{item['cell'].lower()}-{item['mode']}"
        if (scenario["id"] != expected_id or scenario["kind"] != item["mode"] + "_open"
                or scenario.get("warmup_seconds") != 5 or manifest["observer_profile"] != "minimal"
                or (item["mode"] == "warm" and scenario.get("iterations") != 6)
                or (item["mode"] == "cold" and scenario.get("iterations", 1) != 1)
                or manifest["ddragon"]["mode"] != "offline" or manifest.get("timeout_seconds") != 180):
            raise Error("scenario differs from fixed opening contract")
        binary = subjects[item["arm"]]
        if inside(root, manifest["app_binary"]) != Path(binary["path"]):
            raise Error("manifest subject differs from frozen arm")
        config_path = inside(root, manifest["config_path"])
        config_sha = digest(config_path)
        metadata = {**runner_environment, "config_sha256": config_sha, "app_binary_sha256": binary["sha256"]}
        for name in ("result_root", "app_data_root", "scratch_root"):
            inside(root, manifest[name], exists=False)
        if not Path(manifest["result_root"]).is_relative_to(root / "results"):
            raise Error("result must be below campaign results directory")
        for name in unique:
            value = launch[name] if name in launch else manifest[name]
            folded = str(value).casefold()
            if folded in unique[name]:
                raise Error(f"reused campaign {name}")
            unique[name].add(folded)
        entries.append({**item, "matrix_path": str(path), "matrix_sha256": digest(path),
                        "matrix_sequence": launch["sequence"], "manifest_path": str(manifest_path),
                        "manifest_sha256": launch["manifest_sha256"], "run_id": launch["run_id"],
                        "trial_id": scenario["trial_id"], "scenario_id": scenario["id"],
                        "subject_sha256": binary["sha256"], "result_root": launch["result_root"],
                        "config_sha256": config_sha, "comparison_identity_sha256": compare_identity(manifest, metadata)})
    for left, right in zip(entries[::2], entries[1::2]):
        if left["comparison_identity_sha256"] != right["comparison_identity_sha256"] or left["config_sha256"] != right["config_sha256"]:
            raise Error("adjacent reference/candidate pair has unmatched comparison identity")
        if read(left["manifest_path"])["fixtures"] != read(right["manifest_path"])["fixtures"]:
            raise Error("paired full corpus identities differ")
    return {"schema_version": 1, "document_type": "queueback-replay-opening-campaign",
            "sentinel_root": str(root), "seed": SEED, "launch_ceiling": CEILING,
            "cooldown_seconds": COOLDOWN, "timeout_seconds": 180,
            "specification": file_identity(root, spec_path), "conditions": spec["conditions"],
            "runner_environment": runner_environment, "environment_receipt": environment_receipt, "subjects": subjects,
            "tools": tools, "matrices": plans, "launches": entries}


def create_plan(spec_path):
    plan = compile_plan(spec_path)
    root = Path(plan["sentinel_root"])
    for name in ("receipts", "preflight", "live"):
        if (root / name).exists():
            raise Error("campaign execution destination already exists")
    if any(Path(entry["result_root"]).exists() for entry in plan["launches"]):
        raise Error("campaign result already exists")
    path = root / "campaign-plan.json"
    if path.exists() or (root / "campaign-plan.sha256.json").exists():
        raise Error("campaign plan already exists")
    roots = []
    for entry in plan["launches"]:
        manifest = read(entry["manifest_path"])
        for field in ("app_data_root", "scratch_root"):
            target = inside(root, manifest[field], exists=False)
            if target.exists():
                raise Error("isolated launch root already exists")
            roots.append(target)
        template = read(read(entry["matrix_path"])["template"]["path"])
        cache = inside(root, template["ddragon"]["cache_root"])
        if any(cache.iterdir()):
            raise Error("opening campaign requires an empty offline Data Dragon cache")
    for target in roots:
        target.mkdir()
    for entry in plan["launches"]:
        inside(root, read(entry["manifest_path"])["ddragon"]["cache_root"], exists=False).mkdir()
    write_new(path, plan)
    write_new(root / "campaign-plan.sha256.json", {"sha256": digest(path)})
    return path


def _validate_result(entry, plan):
    manifest = read(entry["manifest_path"])
    result = Path(entry["result_root"])
    started, completed = matrix._verify_result(entry, manifest)
    matrix._verify_matrix_final_integrity(entry, manifest)
    bundle = analyze.load_bundle(result, f"run-{entry['sequence']:03d}")
    # This also checks order-neutral cycle completeness before allowing advance.
    import opening_comparison
    opening_comparison.extract(bundle, entry["cell"], entry["mode"], entry["arm"], entry["sequence"])
    metadata = read(result / "runner-metadata.json")
    if (metadata["app_binary_sha256"] != entry["subject_sha256"]
            or metadata["manifest_sha256"] != entry["manifest_sha256"]
            or metadata["config_sha256"] != entry["config_sha256"]
            or compare_identity(manifest, metadata) != entry["comparison_identity_sha256"]):
        raise Error("result does not match frozen subject/manifest/environment")
    return {"runner_started_utc": started.isoformat(), "runner_completed_utc": completed.isoformat(),
            "artifacts": {name: digest(result / name) for name in ARTIFACTS},
            "bundle_status": "valid"}


def _receipt_identity(entry, campaign_sha):
    return {"schema_version": 1, "campaign_sha256": campaign_sha, "sequence": entry["sequence"],
            "entry_sha256": analyze._digest(entry)}


def verify_campaign(path, *, require_results=False):
    path = Path(path).resolve(strict=True)
    plan = read(path)
    root = Path(plan["sentinel_root"])
    if path != root / "campaign-plan.json":
        raise Error("campaign plan must be at the sentinel root")
    expected = compile_plan(Path(plan["specification"]["path"]))
    if plan != expected:
        raise Error("campaign plan or frozen identity changed")
    campaign_sha = digest(path)
    seal = inside(root, root / "campaign-plan.sha256.json")
    if read(seal) != {"sha256": campaign_sha}:
        raise Error("campaign plan bytes changed")
    for tool in plan["tools"]:
        name = Path(tool["path"]).name
        if name in TOOLS and digest(Path(__file__).parent / name) != tool["sha256"]:
            raise Error("executing tool differs from frozen campaign tool")
    receipts = root / "receipts"
    if receipts.exists():
        inside(root, receipts)
    expected_names, completed_count, gap, previous = set(), 0, False, None
    planned_results = {Path(entry["result_root"]).resolve() for entry in plan["launches"]}
    # Only runner-created run directories belong below the campaign results root.
    results_root = root / "results"
    if results_root.exists():
        inside(root, results_root)
        for child in results_root.rglob("*"):
            inside(root, child)
            if child.is_dir() and child not in planned_results and not any(p.is_relative_to(child) for p in planned_results):
                # Subdirectories of an already planned result are runner artifacts.
                if not any(child.is_relative_to(p) for p in planned_results):
                    raise Error("unplanned result root")
    for entry in plan["launches"]:
        number = entry["sequence"]
        start_path = receipts / f"{number:03d}.started.json"
        complete_path = receipts / f"{number:03d}.completed.json"
        exists = (start_path.exists(), complete_path.exists(), Path(entry["result_root"]).exists())
        if exists == (False, False, False):
            gap = True
            continue
        if gap or exists != (True, True, True):
            raise Error("partial, restarted, unreceipted or noncontiguous campaign slot")
        expected_names.update((start_path.name, complete_path.name))
        inside(root, start_path)
        inside(root, complete_path)
        start, complete = read(start_path), read(complete_path)
        identity = _receipt_identity(entry, campaign_sha)
        if {k: start.get(k) for k in identity} != identity or {k: complete.get(k) for k in identity} != identity:
            raise Error("receipt identity mismatch")
        if complete.get("started_receipt_sha256") != digest(start_path):
            raise Error("started receipt changed")
        actual = _validate_result(entry, plan)
        if complete.get("validation") != actual:
            raise Error("completed result evidence changed")
        receipt_start = matrix._utc_timestamp(start.get("started_utc"), "receipt start")
        receipt_end = matrix._utc_timestamp(complete.get("completed_utc"), "receipt completion")
        runner_start = matrix._utc_timestamp(actual["runner_started_utc"], "runner start")
        runner_end = matrix._utc_timestamp(actual["runner_completed_utc"], "runner completion")
        if not receipt_start <= runner_start <= runner_end <= receipt_end:
            raise Error("receipt/runner time containment failed")
        if previous is not None and receipt_start < previous + timedelta(seconds=COOLDOWN):
            raise Error("global start order or cooldown violated")
        previous = receipt_end
        completed_count += 1
    if receipts.exists() and {p.name for p in receipts.iterdir()} != expected_names:
        raise Error("extra or duplicate launch receipt")
    live = root / "live"
    if live.exists():
        inside(root, live)
        expected_logs = {f"{i:03d}.log" for i in range(1, completed_count + 1)}
        actual_logs = {p.name for p in live.iterdir()}
        if actual_logs - {"complete.json"} != expected_logs:
            raise Error("missing or unplanned live launch log")
        if "complete.json" in actual_logs:
            if completed_count != CEILING or read(live / "complete.json") != {
                    "schema_version": 1, "campaign_sha256": campaign_sha, "completed_count": CEILING}:
                raise Error("campaign completion receipt mismatch")
    elif completed_count:
        raise Error("missing launch log directory")
    if require_results:
        if completed_count != CEILING:
            raise Error("campaign lacks all 100 valid slots")
        for local in plan["matrices"]:
            matrix.verify_matrix(Path(local["path"]), require_results=True)
    return {"plan": plan, "campaign_sha256": campaign_sha, "completed_count": completed_count,
            "previous_completed_utc": previous.isoformat() if previous else None}


@contextmanager
def campaign_lock(root):
    # OS ownership releases on process death. The file remains; no stale-lock
    # deletion or unrelated process termination is needed to resume.
    path = root / "campaign.lock"
    inside(root, path, exists=False)
    with path.open("a+b") as lock:
        if lock.tell() == 0:
            lock.write(b"\0")
            lock.flush()
        lock.seek(0)
        if os.name == "nt":
            import msvcrt
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            yield
        finally:
            lock.seek(0)
            if os.name == "nt":
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock, fcntl.LOCK_UN)


def _now():
    return datetime.now(timezone.utc)


def _invoke(entry, plan, log_path, *, preflight=False):
    tools_root = Path(plan["tools"][0]["path"]).parent
    command = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
               str(tools_root / "run.ps1"), "-Manifest", entry["manifest_path"],
               "-AppBinary", plan["subjects"][entry["arm"]]["path"], "-TimeoutSeconds", "180"]
    if preflight:
        command.append("-PreflightOnly")
    with Path(log_path).open("xb") as log:
        # run.ps1 owns finite app/Job termination and collector finalization.
        # Do not kill the wrapper and strand its owned process tree.
        return subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False).returncode


def _verify_environment(plan):
    tools_root = Path(plan["tools"][0]["path"]).parent
    command = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
               str(tools_root / "opening_environment.ps1"), "-Runner", str(tools_root / "run.ps1"),
               "-MediaRuntimeId", plan["runner_environment"]["media_runtime_id"]]
    result = subprocess.run(command, capture_output=True, encoding="utf-8", timeout=45, check=True)
    actual = json.loads(result.stdout)
    expected = read(plan["environment_receipt"]["path"])
    if actual != expected:
        raise Error("prelaunch machine/display/CPU/runtime environment drifted")


def preflight(path):
    checked = verify_campaign(path)
    root, plan = Path(checked["plan"]["sentinel_root"]), checked["plan"]
    with campaign_lock(root):
        checked = verify_campaign(path)
        if checked["completed_count"]:
            raise Error("preflight is only for an unstarted campaign")
        _verify_environment(plan)
        destination = root / "preflight"
        destination.mkdir()
        for entry in plan["launches"]:
            code = _invoke(entry, plan, destination / f"{entry['sequence']:03d}.log", preflight=True)
            if code:
                raise Error(f"preflight failed at {entry['sequence']}; preserved log")
        write_new(destination / "complete.json", {
            "schema_version": 1, "campaign_sha256": checked["campaign_sha256"], "count": CEILING,
            "logs": {f"{i:03d}.log": digest(destination / f"{i:03d}.log") for i in range(1, CEILING + 1)}})


def run_campaign(path, *, resume=False, invoke=None, now=None, sleep=None):
    invoke, now, sleep = invoke or _invoke, now or _now, sleep or time.sleep
    checked = verify_campaign(path)
    root = Path(checked["plan"]["sentinel_root"])
    with campaign_lock(root):
        checked = verify_campaign(path)
        plan, campaign_sha = checked["plan"], checked["campaign_sha256"]
        pre = read(root / "preflight" / "complete.json")
        logs = {f"{i:03d}.log": digest(root / "preflight" / f"{i:03d}.log") for i in range(1, CEILING + 1)}
        if pre != {"schema_version": 1, "campaign_sha256": campaign_sha, "count": CEILING, "logs": logs}:
            raise Error("complete preflight identity missing or changed")
        if (root / "live").exists() and not resume:
            raise Error("campaign already started; explicit resume is required")
        if checked["completed_count"] == CEILING:
            raise Error("campaign is already complete")
        if not (root / "live").exists():
            (root / "live").mkdir()
            (root / "receipts").mkdir()
        previous = checked["previous_completed_utc"]
        for entry in plan["launches"][checked["completed_count"]:]:
            if previous:
                remaining = (matrix._utc_timestamp(previous, "previous completion")
                             + timedelta(seconds=COOLDOWN) - now()).total_seconds()
                if remaining > 0:
                    sleep(remaining)
            _verify_environment(plan)
            # Recheck frozen inputs and completed prefix immediately before admission.
            current = verify_campaign(path)
            if current["campaign_sha256"] != campaign_sha or current["completed_count"] != entry["sequence"] - 1:
                raise Error("campaign prefix changed during execution")
            started = root / "receipts" / f"{entry['sequence']:03d}.started.json"
            identity = _receipt_identity(entry, campaign_sha)
            write_new(started, {**identity, "started_utc": now().isoformat()})
            code = invoke(entry, plan, root / "live" / f"{entry['sequence']:03d}.log")
            if code:
                raise Error(f"launch {entry['sequence']} failed; slot preserved and campaign stopped")
            validation = _validate_result(entry, plan)
            previous = now().isoformat()
            write_new(root / "receipts" / f"{entry['sequence']:03d}.completed.json", {
                **identity, "started_receipt_sha256": digest(started),
                "completed_utc": previous, "validation": validation})
        final = verify_campaign(path, require_results=True)
        write_new(root / "live" / "complete.json", {
            "schema_version": 1, "campaign_sha256": campaign_sha, "completed_count": final["completed_count"]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--spec", type=Path, required=True)
    for name in ("verify", "preflight", "run"):
        p = sub.add_parser(name)
        p.add_argument("--campaign", type=Path, required=True)
        if name == "verify":
            p.add_argument("--require-results", action="store_true")
        if name == "run":
            p.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "plan":
            print(create_plan(args.spec))
        elif args.command == "preflight":
            preflight(args.campaign)
            print("all 100 preflights passed")
        elif args.command == "run":
            run_campaign(args.campaign, resume=args.resume)
            print("all 100 launches completed and globally verified")
        else:
            checked = verify_campaign(args.campaign, require_results=args.require_results)
            print(json.dumps({"campaign_sha256": checked["campaign_sha256"],
                              "planned": CEILING, "completed": checked["completed_count"]}))
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(2, f"opening campaign failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
