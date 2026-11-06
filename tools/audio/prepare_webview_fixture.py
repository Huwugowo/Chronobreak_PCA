"""Build only generated four-channel media in a new isolated proof library."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
from uuid import uuid4


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> None:
    repo = Path(__file__).resolve().parents[2]
    run = Path(sys.argv[1]).resolve()
    expected_parent = repo / "build" / "perf" / "qb-audio-001"
    if run.parent != expected_parent or run.exists():
        raise ValueError("Require a new direct child of build/perf/qb-audio-001")
    run.mkdir(parents=True, exist_ok=False)
    (run / ".chronobreak-audio-proof.json").write_text(
        json.dumps({"root": str(run), "generated_media_only": True}), encoding="utf-8"
    )
    runtime = repo / "build" / "media-runtime" / "windows-x86_64"
    source = run / "four-channel.mp4"
    audio = "|".join(f"0.04*sin(2*PI*{frequency}*t)" for frequency in (697, 941, 1301, 1709))
    command = [
        str(runtime / "bin" / "ffmpeg.exe"), "-hide_banner", "-nostdin", "-v", "error", "-n",
        "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=60:duration=20",
        "-f", "lavfi", "-i", f"aevalsrc={audio}:s=48000:d=20:c=quad",
        "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast",
        "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart", str(source),
    ]
    subprocess.run(command, check=True, timeout=60)
    sentinel = run / ".chronobreak-replay-time"
    sentinel.mkdir()
    corpus_id = "audio-groups"
    spec = {
        "schema_version": 2, "corpus_id": corpus_id, "sentinel_root": str(sentinel),
        "output_root": str(sentinel / corpus_id),
        "fixtures": [{
            "id": "four-channel", "media_id": str(uuid4()), "alias": "four-channel",
            "game_timestamp": "1793400000", "source_video": str(source), "transform": "copy",
            "expected_video_codec": "h264", "expected_audio_codec": "aac",
            "expected_frame_rate": {"numerator": "60", "denominator": "1"},
            "minimum_duration_seconds": 20, "event_interval_seconds": 2, "snapshot_interval_seconds": 5,
        }],
    }
    spec_path = run / "corpus-spec.json"
    spec_path.write_text(json.dumps(spec, indent=2), encoding="utf-8")
    subprocess.run([
        sys.executable, str(repo / "tools/replay_benchmark/build_corpus.py"),
        "--spec", str(spec_path), "--media-runtime-root", str(runtime),
    ], check=True, timeout=90)
    library = run / "library"
    (library / "games").mkdir(parents=True)
    shutil.copytree(sentinel / corpus_id / "four-channel", library / "games" / "1793400000")
    (run / "config.toml").write_text(
        '[recording]\nprofile = "auto"\ncodec = "auto"\n[storage]\n'
        f'output_path = "{library.as_posix()}"\nauto_delete_days = 0\n'
        '[app]\nautostart = false\nhevc_playback_supported = false\n', encoding="utf-8"
    )
    executable = repo / "app/src-tauri/target/release/league-replay-app.exe"
    (run / "fixture.json").write_text(json.dumps({
        "generated_media_only": True,
        "source_sha256": digest(source),
        "app_sha256": digest(executable),
        "app_build_scope": "pre-existing executable; WebView capability only, not current-tree product verification",
        "channel_frequencies_hz": [697, 941, 1301, 1709],
        "group_channels": {"league_fixture": [0, 1], "other_fixture": [2, 3]},
    }, indent=2), encoding="utf-8")
    print(f"WEBVIEW_FIXTURE={run}")


if __name__ == "__main__":
    main()
