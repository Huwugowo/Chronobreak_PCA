"""Capture bounded, read-only League API responses for one local test game.

This is a diagnostic tool. Its raw output may contain account and player data and
is written only under the repository's ignored build/ directory.
"""

from __future__ import annotations

import argparse
import base64
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import socket
import ssl
import statistics
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import uuid4


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_LOCKFILE = Path(r"C:\Riot Games\League of Legends\lockfile")
OUTPUT_ROOT = REPOSITORY / "build" / "client-api-captures"
MAX_RESPONSE_BYTES = 2 * 1024 * 1024
MAX_CAPTURE_BYTES = 128 * 1024 * 1024

# Frequencies are for this diagnostic run, not a recorder configuration change.
ENDPOINTS = (
    ("lcu.phase", "/lol-gameflow/v1/gameflow-phase", 2, "always"),
    ("lcu.lobby", "/lol-lobby/v2/lobby", 10, "active"),
    ("lcu.champion_select", "/lol-champ-select/v1/session", 5, "active"),
    ("lcu.session", "/lol-gameflow/v1/session", 5, "active"),
    ("lcu.summoner", "/lol-summoner/v1/current-summoner", 30, "once"),
    ("lcu.end_of_game", "/lol-end-of-game/v1/eog-stats-block", 5, "postgame"),
    (
        "lcu.match_history",
        "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=0",
        15,
        "postgame",
    ),
    ("live.gamestats", "/liveclientdata/gamestats", 1, "always"),
    ("live.eventdata", "/liveclientdata/eventdata", 1, "always"),
    ("live.allgamedata", "/liveclientdata/allgamedata", 10, "always"),
)


def client_connection(lockfile: Path) -> tuple[str, str] | None:
    try:
        parts = lockfile.read_text(encoding="utf-8").strip().split(":")
        if len(parts) != 5 or parts[4].lower() != "https":
            return None
        port = int(parts[2])
        if not 1 <= port <= 65535:
            return None
        auth = base64.b64encode(("riot:" + parts[3]).encode("utf-8")).decode("ascii")
        return f"https://127.0.0.1:{port}", auth
    except (OSError, ValueError):
        return None


def fetch(url: str, auth: str | None, context: ssl.SSLContext) -> tuple[int | None, object | None, int, float]:
    headers = {"Authorization": "Basic " + auth} if auth else {}
    started = time.perf_counter()
    try:
        with urlopen(Request(url, headers=headers), timeout=2, context=context) as response:
            raw = response.read(MAX_RESPONSE_BYTES + 1)
            elapsed_ms = round((time.perf_counter() - started) * 1000, 1)
            if len(raw) > MAX_RESPONSE_BYTES:
                return response.status, {"capture_error": "response_too_large"}, len(raw), elapsed_ms
            try:
                payload = json.loads(raw)
            except json.JSONDecodeError:
                return response.status, {"capture_error": "invalid_json"}, len(raw), elapsed_ms
            return response.status, payload, len(raw), elapsed_ms
    except HTTPError as error:
        return error.code, None, 0, round((time.perf_counter() - started) * 1000, 1)
    except (OSError, URLError, TimeoutError):
        return None, None, 0, round((time.perf_counter() - started) * 1000, 1)


def live_api_listening() -> bool:
    try:
        with socket.create_connection(("127.0.0.1", 2999), timeout=0.2):
            return True
    except OSError:
        return False


def should_poll(mode: str, phase: str | None, seen_game: bool, summoner_saved: bool) -> bool:
    if mode == "always":
        return True
    if mode == "active":
        return phase not in (None, "None")
    if mode == "once":
        return not summoner_saved
    return seen_game and phase in (
        "PreEndOfGame",
        "WaitingForStats",
        "EndOfGame",
        "TerminatedInError",
        "None",
    )


def capture(lockfile: Path, duration_seconds: int) -> Path:
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output_dir = OUTPUT_ROOT / f"{stamp}-{uuid4().hex[:8]}"
    output_dir.mkdir()
    output_path = output_dir / "responses.jsonl"

    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE  # Both APIs use local, self-signed HTTPS.
    started = time.monotonic()
    next_due = defaultdict(float)
    last_digest: dict[str, str] = {}
    statuses: dict[str, int | None] = {}
    counts: dict[str, Counter[str]] = defaultdict(Counter)
    latencies: dict[str, list[float]] = defaultdict(list)
    capture_bytes = 0
    phase: str | None = None
    seen_game = False
    summoner_saved = False
    stop_reason = "duration_elapsed"

    print(f"Capturing read-only League API responses to {output_path}", flush=True)
    print("This ignored local file contains raw account and match data. Press Ctrl+C to finish.", flush=True)
    try:
        with output_path.open("x", encoding="utf-8", newline="\n") as output:
            while time.monotonic() - started < duration_seconds:
                now = time.monotonic()
                connection = client_connection(lockfile)
                live_ready = live_api_listening()
                for name, path, interval, mode in ENDPOINTS:
                    if now < next_due[name] or not should_poll(mode, phase, seen_game, summoner_saved):
                        continue
                    next_due[name] = now + interval
                    if name.startswith("lcu."):
                        if connection is None:
                            status, payload, size, latency_ms = None, None, 0, 0.0
                        else:
                            base, auth = connection
                            status, payload, size, latency_ms = fetch(base + path, auth, context)
                    else:
                        if live_ready:
                            status, payload, size, latency_ms = fetch("https://127.0.0.1:2999" + path, None, context)
                        else:
                            status, payload, size, latency_ms = None, None, 0, 0.0

                    counts[name][str(status) if status is not None else "unavailable"] += 1
                    if status is not None:
                        latencies[name].append(latency_ms)
                    if name == "lcu.phase" and status == 200 and isinstance(payload, str):
                        if payload != phase:
                            phase = payload
                            print(f"Client phase: {phase}", flush=True)
                        if phase == "InProgress":
                            seen_game = True
                    if name == "live.gamestats" and status == 200:
                        seen_game = True

                    # Save full successful responses only when their content changes.
                    # Save status transitions, without a body, for unavailable endpoints.
                    serialized = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
                    digest = hashlib.sha256(serialized.encode("utf-8")).hexdigest() if status == 200 else ""
                    changed = status != statuses.get(name) or (status == 200 and digest != last_digest.get(name))
                    statuses[name] = status
                    if not changed:
                        continue
                    if status == 200:
                        last_digest[name] = digest
                    record = {
                        "at_utc": datetime.now(timezone.utc).isoformat(),
                        "elapsed_ms": round((time.monotonic() - started) * 1000),
                        "endpoint": name,
                        "http_status": status,
                        "response_bytes": size,
                        "latency_ms": latency_ms,
                        "payload": payload if status == 200 else None,
                    }
                    line = json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n"
                    if capture_bytes + len(line.encode("utf-8")) > MAX_CAPTURE_BYTES:
                        stop_reason = "capture_size_limit"
                        break
                    output.write(line)
                    output.flush()
                    capture_bytes += len(line.encode("utf-8"))
                    if name == "lcu.summoner" and status == 200:
                        summoner_saved = True
                if stop_reason == "capture_size_limit":
                    break
                time.sleep(0.2)
    except KeyboardInterrupt:
        stop_reason = "operator_stopped"

    summary = {
        "stop_reason": stop_reason,
        "duration_seconds": round(time.monotonic() - started, 1),
        "capture_bytes": capture_bytes,
        "observed_game": seen_game,
        "last_phase": phase,
        "endpoints": {
            name: {
                "poll_status_counts": dict(status_counts),
                "latency_ms_median": round(statistics.median(latencies[name]), 1) if latencies[name] else None,
            }
            for name, status_counts in counts.items()
        },
    }
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"Capture complete: {output_dir / 'summary.json'}", flush=True)
    return output_dir


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lockfile", type=Path, default=DEFAULT_LOCKFILE)
    parser.add_argument("--duration-seconds", type=int, default=2400)
    args = parser.parse_args()
    if not 1 <= args.duration_seconds <= 7200:
        parser.error("--duration-seconds must be between 1 and 7200")
    capture(args.lockfile, args.duration_seconds)


if __name__ == "__main__":
    main()
