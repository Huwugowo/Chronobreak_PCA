#!/usr/bin/env python3
"""Capture raw League Live Client Data API responses into one JSON file."""

import argparse
import json
import ssl
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


URL = "https://127.0.0.1:2999/liveclientdata/allgamedata"


def now():
    return datetime.now(timezone.utc).isoformat()


def fetch(context):
    started = time.perf_counter()
    with urllib.request.urlopen(URL, context=context, timeout=2) as response:
        data = json.load(response)
    return data, round((time.perf_counter() - started) * 1000, 1)


def main():
    parser = argparse.ArgumentParser(
        description="Wait for a League game, poll /allgamedata, and save one JSON file."
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=2.0,
        help="Seconds between polls (default: 2)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output path (default: live-client-capture-<timestamp>.json)",
    )
    args = parser.parse_args()
    if args.interval <= 0:
        parser.error("--interval must be greater than zero")

    output = args.output or Path(
        f"live-client-capture-{datetime.now():%Y%m%d-%H%M%S}.json"
    )
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE

    capture = {
        "source": URL,
        "started_at": now(),
        "poll_interval_seconds": args.interval,
        "samples": [],
    }
    connected = False
    failures = 0
    stop_reason = "interrupted"

    print("Waiting for the League Live Client Data API...")
    print("Start a game, or press Ctrl+C to stop.")

    try:
        while True:
            try:
                data, request_ms = fetch(context)
            except Exception as error:
                if connected:
                    failures += 1
                    print(f"API unavailable ({failures}/5): {type(error).__name__}")
                    if failures >= 5:
                        stop_reason = "api_became_unavailable"
                        break
                time.sleep(args.interval)
                continue

            if not connected:
                connected = True
                print("Game detected. Capturing...")

            failures = 0
            game_time = data.get("gameData", {}).get("gameTime")
            capture["samples"].append(
                {
                    "captured_at": now(),
                    "request_duration_ms": request_ms,
                    "game_time_seconds": game_time,
                    "data": data,
                }
            )
            print(
                f"\rSamples: {len(capture['samples'])}"
                f" | game time: {game_time if game_time is not None else '?'}",
                end="",
                flush=True,
            )
            time.sleep(args.interval)
    except KeyboardInterrupt:
        print()
    finally:
        capture["ended_at"] = now()
        capture["stop_reason"] = stop_reason
        capture["sample_count"] = len(capture["samples"])
        output.write_text(
            json.dumps(capture, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        print(f"Saved {len(capture['samples'])} samples to {output.resolve()}")


if __name__ == "__main__":
    main()
