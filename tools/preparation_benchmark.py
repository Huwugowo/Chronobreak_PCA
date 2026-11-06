"""Measure the production headless preparer with a new disposable cache.

The source library is read only. No cache is copied or pre-populated. Completion
requires every published recording and every overlay source, not just cards.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time


def source_hashes(library):
    return {
        str(path.relative_to(library)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted((library / "games").glob("*/*"))
        if path.is_file() and path.suffix in {".json", ".jsonl"}
    }


def projection_hashes(database):
    with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as db:
        # The legacy records table duplicated the source journal. No query or
        # replay reader consumes it, and new preparations leave it empty.
        tables = [row[0] for row in db.execute(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        ) if row[0] not in {"cache_identity", "records"}]
        result = {}
        for table in tables:
            # Column declaration order can differ between equivalent cache
            # schemas created during development. Compare named facts instead.
            columns = sorted(row[1] for row in db.execute('PRAGMA table_info("' + table + '")'))
            if table == "recordings" and "history_stamp" in columns:
                # The 83-second reference predates the separate card-readiness
                # stamp. Fully prepared rows must match their validated source
                # stamp; all the other recording facts compare byte for byte.
                assert db.execute("SELECT count(*) FROM recordings WHERE history_stamp IS NOT source_stamp").fetchone()[0] == 0
                columns.remove("history_stamp")
            selection = ",".join('"' + column + '"' for column in columns)
            rows = sorted(db.execute('SELECT ' + selection + ' FROM "' + table + '"').fetchall(), key=repr)
            result[table] = {"rows": len(rows), "sha256": hashlib.sha256(
                json.dumps(rows, separators=(",", ":"), ensure_ascii=False,
                           default=lambda value: {"blob": value.hex()}).encode()
            ).hexdigest()}
        return result


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--recordings", type=int, default=8)
    parser.add_argument("--samples", type=int, default=2472)
    parser.add_argument("--budget", type=float, default=10)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--below-normal", action="store_true",
                        help="use the recorder-owned helper's background priority on Windows")
    args = parser.parse_args()
    library = args.library.resolve()
    artifact = args.artifact.resolve()
    artifact.mkdir(parents=True, exist_ok=False)
    before = source_hashes(library)
    env = os.environ.copy()
    env["LEAGUE_REPLAY_APP_DATA_DIR"] = str(artifact / "app-data")
    env["CHRONOBREAK_DISABLE_RECORDER_SUPERVISION"] = "1"
    observations = []
    complete = False
    database = None
    started = time.perf_counter()
    with (artifact / "preparation.stderr.log").open("w", encoding="utf-8") as stderr:
        process = subprocess.Popen(
            [str(args.exe.resolve()), "--prepare-library", str(library)],
            env=env, stdout=subprocess.DEVNULL, stderr=stderr,
            creationflags=(subprocess.CREATE_NO_WINDOW
                           | (subprocess.BELOW_NORMAL_PRIORITY_CLASS if args.below_normal else 0))
                          if os.name == "nt" else 0,
        )
        try:
            while time.perf_counter() - started < args.timeout:
                assert process.poll() is None, "preparer exited unexpectedly"
                candidates = list((artifact / "app-data" / "league-index").glob("*.sqlite3"))
                if candidates:
                    database = candidates[0]
                    try:
                        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as db:
                            recording = db.execute(
                                "SELECT count(*), coalesce(sum(facts_current),0), "
                                "coalesce(sum(telemetry_status='ready'),0) FROM recordings"
                            ).fetchone()
                            overlay = db.execute(
                                "SELECT count(*), coalesce(sum(complete),0) FROM prepared_overlay_sources"
                            ).fetchone()
                            samples = db.execute("SELECT count(*) FROM prepared_overlay_samples").fetchone()[0]
                        elapsed = time.perf_counter() - started
                        observations.append({"seconds": elapsed, "recordings": recording,
                                             "overlays": overlay, "samples": samples})
                        complete = (recording == (args.recordings,) * 3
                                    and overlay == (args.recordings,) * 2 and samples == args.samples)
                        if complete:
                            break
                    except sqlite3.Error:
                        pass
                time.sleep(0.025)
        finally:
            process.terminate()
            process.wait(timeout=10)
    elapsed = observations[-1]["seconds"] if observations else time.perf_counter() - started
    after = source_hashes(library)
    hashes = projection_hashes(database) if complete else None
    reference = projection_hashes(args.reference.resolve()) if args.reference and complete else hashes
    summary = {"complete": complete, "seconds": elapsed, "budget_seconds": args.budget,
               "within_budget": complete and elapsed <= args.budget,
               "source_hashes_unchanged": before == after,
               "projection_matches_reference": hashes == reference,
               "first_all_cards_seconds": next((row["seconds"] for row in observations
                                                  if row["recordings"][1] == args.recordings), None)}
    summary.update({"exe": str(args.exe.resolve()),
                    "exe_sha256": hashlib.sha256(args.exe.read_bytes()).hexdigest(),
                    "library": str(library), "below_normal_priority": args.below_normal,
                    "card_readiness_regressions": sum(
                        after["recordings"][1] < before["recordings"][1]
                        for before, after in zip(observations, observations[1:]))})
    for name, value in [("summary.json", summary), ("samples.json", observations),
                        ("source-hashes.json", before), ("projection-hashes.json", hashes)]:
        (artifact / name).write_text(json.dumps(value, indent=2), encoding="utf-8")
    print(json.dumps(summary))
    assert complete, "full preparation did not complete"
    assert before == after, "source data changed"
    assert hashes == reference, "projection differs from reference"
    assert elapsed <= args.budget, "cold preparation exceeded budget"


if __name__ == "__main__":
    main()
