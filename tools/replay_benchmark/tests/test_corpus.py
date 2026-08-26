import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

import jsonschema


TOOLS_ROOT = Path(__file__).resolve().parents[1]
SCHEMA_ROOT = TOOLS_ROOT / "schemas"
SPEC = importlib.util.spec_from_file_location("replay_build_corpus", TOOLS_ROOT / "build_corpus.py")
assert SPEC is not None and SPEC.loader is not None
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)


class ReplayCorpusTests(unittest.TestCase):
    def test_schema_example_is_valid(self) -> None:
        schema = json.loads((SCHEMA_ROOT / "corpus-v1.schema.json").read_text(encoding="utf-8"))
        example = json.loads((SCHEMA_ROOT / "corpus-v1.example.json").read_text(encoding="utf-8"))
        jsonschema.Draft202012Validator(schema).validate(example)

    def test_game_log_is_deterministic_anonymized_and_bounded(self) -> None:
        first = corpus.build_game_log(
            120_000, event_interval_seconds=5, snapshot_interval_seconds=30
        )
        second = corpus.build_game_log(
            120_000, event_interval_seconds=5, snapshot_interval_seconds=30
        )
        self.assertEqual(first, second)
        self.assertEqual(first["game_start_video_offset_ms"], 0)
        self.assertEqual(len(first["snapshots"]), 5)
        self.assertGreaterEqual(len(first["events"]), 23)
        self.assertTrue(all(event["video_time_ms"] < 120_000 for event in first["events"]))
        serialized = json.dumps(first)
        self.assertIn("QB-Blue-1#TEST", serialized)
        self.assertNotIn("summoner_name_here", serialized)

    def test_frame_rate_parser_rejects_invalid_values(self) -> None:
        self.assertEqual(corpus.parse_rate("60/1"), 60.0)
        self.assertAlmostEqual(corpus.parse_rate("60000/1001"), 59.94005994)
        for value in (None, "60", "0/0", "bad/1"):
            with self.subTest(value=value), self.assertRaises(corpus.CorpusError):
                corpus.parse_rate(value)

    def test_repeat_count_is_finite_and_covers_target(self) -> None:
        self.assertEqual(corpus.repeat_count(3600, 1800), 2)
        self.assertEqual(corpus.repeat_count(3601, 1800), 3)
        self.assertEqual(corpus.repeat_count(120, 241.3), 1)
        with self.assertRaises(corpus.CorpusError):
            corpus.repeat_count(3600, 0)

    def test_spec_rejects_existing_output_before_media_work(self) -> None:
        if corpus.os.name != "nt":
            self.skipTest("Windows path safety contract")
        with tempfile.TemporaryDirectory(prefix="queueback-corpus-") as temporary:
            base = Path(temporary).resolve()
            sentinel = base / ".chronobreak-replay-benchmark"
            sentinel.mkdir()
            output = sentinel / "sources" / "corpus-1"
            output.mkdir(parents=True)
            source = base / "source.mp4"
            source.write_bytes(b"fixture")
            specification = {
                "schema_version": 1,
                "corpus_id": "corpus-1",
                "sentinel_root": str(sentinel),
                "output_root": str(output),
                "fixtures": [
                    {
                        "id": "short",
                        "alias": "short",
                        "game_timestamp": "1787600000",
                        "source_video": str(source),
                        "transform": "copy",
                        "backend": "native",
                        "expected_video_codec": "h264",
                        "expected_audio_codec": "aac",
                        "minimum_duration_seconds": 120,
                    }
                ],
            }
            with self.assertRaisesRegex(corpus.CorpusError, "already exists"):
                corpus._validate_spec(specification)

    def test_help_is_available(self) -> None:
        with self.assertRaises(SystemExit) as raised:
            corpus._parser().parse_args(["--help"])
        self.assertEqual(raised.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
