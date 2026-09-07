import copy
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import jsonschema


TOOLS_ROOT = Path(__file__).resolve().parents[1]
SCHEMA_ROOT = TOOLS_ROOT / "schemas"
SPEC = importlib.util.spec_from_file_location(
    "replay_build_corpus", TOOLS_ROOT / "build_corpus.py"
)
assert SPEC is not None and SPEC.loader is not None
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)

MEDIA_ID = "123e4567-e89b-42d3-a456-426614174000"
OTHER_MEDIA_ID = "8a0f5b7e-8bb7-4bd9-85d1-75afcf7ca19e"


def media_summary() -> dict[str, object]:
    return {
        "size_bytes": 1024,
        "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
        "video_codec": "h264",
        "video_profile": "High",
        "width": 1920,
        "height": 1080,
        "video_start_pts": 0,
        "video_duration_ts": 153_600,
        "video_frames": 600,
        "video_packets": 600,
        "has_b_frames": 0,
        "audio_codec": "aac",
        "audio_sample_rate": 48_000,
        "audio_channels": 2,
        "audio_start_pts": 0,
        "audio_duration_ts": 480_000,
        "audio_packets": 470,
        "exact_container_start": {"numerator": "0", "denominator": "1"},
        "exact_container_duration": {"numerator": "10", "denominator": "1"},
        "exact_frame_rate": {"numerator": "60", "denominator": "1"},
        "exact_video_time_base": {"numerator": "1", "denominator": "15360"},
        "exact_audio_time_base": {"numerator": "1", "denominator": "48000"},
    }


def exact_grid() -> dict[str, int]:
    return {
        "frame_count": 600,
        "first_pts": 0,
        "one_past_last_pts": 153_600,
        "duration_ts": 153_600,
        "frame_step_pts": 256,
    }


class ReplayCorpusTests(unittest.TestCase):
    def setUp(self) -> None:
        self.schema = json.loads(
            (SCHEMA_ROOT / "corpus-v2.schema.json").read_text(encoding="utf-8")
        )
        self.example = json.loads(
            (SCHEMA_ROOT / "corpus-v2.example.json").read_text(encoding="utf-8")
        )
        self.validator = jsonschema.Draft202012Validator(self.schema)

    def test_schema_example_is_valid_and_v1_contract_is_removed(self) -> None:
        self.validator.validate(self.example)
        self.assertFalse((SCHEMA_ROOT / "corpus-v1.schema.json").exists())
        self.assertFalse((SCHEMA_ROOT / "corpus-v1.example.json").exists())

    def test_schema_rejects_v1_legacy_fields_and_malformed_identity(self) -> None:
        cases = []

        schema_v1 = copy.deepcopy(self.example)
        schema_v1["schema_version"] = 1
        cases.append(schema_v1)

        legacy = copy.deepcopy(self.example)
        legacy["fixtures"][0]["video_time_ms"] = 0
        cases.append(legacy)

        claimed_recorder_backend = copy.deepcopy(self.example)
        claimed_recorder_backend["fixtures"][0]["backend"] = "native"
        cases.append(claimed_recorder_backend)

        uppercase_identity = copy.deepcopy(self.example)
        uppercase_identity["fixtures"][0]["media_id"] = MEDIA_ID.upper()
        cases.append(uppercase_identity)

        nil_identity = copy.deepcopy(self.example)
        nil_identity["fixtures"][0]["media_id"] = (
            "00000000-0000-0000-0000-000000000000"
        )
        cases.append(nil_identity)

        numeric_target = copy.deepcopy(self.example)
        numeric_target["fixtures"][1]["target_duration_seconds"] = 1001
        cases.append(numeric_target)

        submicrosecond_interval = copy.deepcopy(self.example)
        submicrosecond_interval["fixtures"][0]["event_interval_seconds"] = 1.0000001
        cases.append(submicrosecond_interval)

        for specification in cases:
            with self.subTest(specification=specification):
                self.assertTrue(list(self.validator.iter_errors(specification)))

    def test_read_json_rejects_duplicate_keys_and_nonfinite_numbers(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "spec.json"
            path.write_text('{"schema_version":2,"schema_version":2}', encoding="utf-8")
            with self.assertRaisesRegex(corpus.CorpusError, "duplicate key"):
                corpus._read_json(path, "corpus specification")
            path.write_text('{"schema_version":NaN}', encoding="utf-8")
            with self.assertRaisesRegex(corpus.CorpusError, "non-finite"):
                corpus._read_json(path, "corpus specification")

    def test_exact_rationals_and_frame_boundaries_are_strict(self) -> None:
        self.assertEqual(
            corpus._positive_rational(
                {"numerator": "60000", "denominator": "1001"}, "rate"
            ),
            Fraction(60_000, 1001),
        )
        self.assertEqual(
            corpus._target_frame_count(Fraction(1001, 1), Fraction(60_000, 1001)),
            60_000,
        )
        self.assertEqual(
            corpus.repeat_count(Fraction(3601, 1), Fraction(1800, 1)), 3
        )
        self.assertEqual(
            corpus.repeat_count(Fraction(120, 1), Fraction(2413, 10)), 1
        )
        for value in (
            {"numerator": "2", "denominator": "2"},
            {"numerator": "060", "denominator": "1"},
            {"numerator": "60", "denominator": "0"},
            {"numerator": 60, "denominator": "1"},
        ):
            with self.subTest(value=value), self.assertRaises(corpus.CorpusError):
                corpus._positive_rational(value, "rate")
        for value in (
            {
                "numerator": str(corpus.MAX_I64 + 1),
                "denominator": "1",
            },
            {
                "numerator": "1",
                "denominator": str(corpus.MAX_U64 + 1),
            },
        ):
            with self.subTest(value=value), self.assertRaisesRegex(
                corpus.CorpusError, "schema-v2 rational range"
            ):
                corpus._positive_rational(value, "rate")
        with self.assertRaisesRegex(corpus.CorpusError, "non-nil UUID"):
            corpus._media_id(
                "00000000-0000-0000-0000-000000000000", "media_id"
            )
        with self.assertRaisesRegex(corpus.CorpusError, "frame boundary"):
            corpus._target_frame_count(
                Fraction(corpus.MAX_REPLAY_TICK + 1, 1), Fraction(1, 1)
            )
        with self.assertRaisesRegex(corpus.CorpusError, "exact positive frame boundary"):
            corpus._target_frame_count(Fraction(1, 1), Fraction(60_000, 1001))
        with self.assertRaisesRegex(corpus.CorpusError, "integer microsecond"):
            corpus._seconds_to_game_ticks(1.0000001, "interval", maximum=300)

    def test_frame_scan_proves_each_decoded_presentation_boundary(self) -> None:
        frames = [
            {"pts": str(index * 256), "duration": "256"} for index in range(3)
        ]
        with mock.patch.object(
            corpus, "_run", return_value=json.dumps({"frames": frames})
        ) as run:
            grid = corpus._scan_video_grid(
                Path("ffprobe.exe"),
                Path("fixture.mp4"),
                time_base=Fraction(1, 15_360),
                frame_rate=Fraction(60, 1),
                expected_first_pts=0,
                expected_duration_ts=768,
                expected_frame_count=3,
            )
        self.assertEqual(grid["one_past_last_pts"], 768)
        self.assertEqual(grid["frame_step_pts"], 256)
        self.assertEqual(
            run.call_args.kwargs["max_output_bytes"], corpus.MAX_FRAME_SCAN_BYTES
        )

        frames[1]["pts"] = "257"
        with mock.patch.object(
            corpus, "_run", return_value=json.dumps({"frames": frames})
        ), self.assertRaisesRegex(corpus.CorpusError, "off the exact frame grid"):
            corpus._scan_video_grid(
                Path("ffprobe.exe"),
                Path("fixture.mp4"),
                time_base=Fraction(1, 15_360),
                frame_rate=Fraction(60, 1),
                expected_first_pts=0,
                expected_duration_ts=768,
                expected_frame_count=3,
            )

    def test_transforms_require_proof_and_publish_only_corpus_provenance(self) -> None:
        source_grid_transforms = ("copy", "remux_audio", "repeat")
        for transform in source_grid_transforms:
            fixture = {
                "id": transform,
                "source_video": Path("source.mp4"),
                "transform": transform,
                "expected_frame_rate": Fraction(60, 1),
                "target_duration_seconds": Fraction(10, 1),
            }
            with self.subTest(transform=transform), self.assertRaisesRegex(
                corpus.CorpusError, "exact source-grid evidence"
            ):
                corpus._create_media(
                    Path("ffmpeg.exe"), fixture, Path("output.mp4"), None
                )
            self.assertEqual(
                corpus._producer_backend(transform),
                f"replay-corpus-{transform.replace('_', '-')}",
            )

        incompatible_source = media_summary()
        incompatible_source["video_codec"] = "hevc"
        fixture = {
            "id": "copy",
            "transform": "copy",
            "expected_video_codec": "h264",
            "expected_frame_rate": Fraction(60, 1),
        }
        with mock.patch.object(
            corpus, "_media_summary", return_value=incompatible_source
        ), self.assertRaisesRegex(corpus.CorpusError, "stream-copy transform"):
            corpus._validate_media({}, fixture, source=True)

    def test_media_timeline_uses_only_reconciled_exact_probe_facts(self) -> None:
        fixture = {
            "media_id": MEDIA_ID,
            "transform": "copy",
            "expected_frame_rate": Fraction(60, 1),
        }
        metadata = corpus._build_metadata(
            fixture, media_summary(), exact_grid(), "test-runtime"
        )

        timeline = metadata["media_timeline"]
        self.assertEqual(
            set(metadata),
            {
                "schema_version",
                "media_id",
                "media_timeline",
                "recorded_at",
                "game_mode",
                "local_player_summoner_name",
                "local_player_champion",
                "local_player_team",
                "encoder_used",
                "recording_codec",
                "recording_profile",
                "recording_resolution",
                "capture_backend",
                "capture_adapter_luid",
                "capture_adapter_name",
                "capture_output",
                "encoder_interop",
                "media_runtime_id",
                "capture_support_label",
                "source_frames_surfaced",
                "source_frames_superseded",
                "cfr_duplicates",
                "cfr_discards",
                "pool_recreations",
                "saved",
            },
        )
        self.assertEqual(
            timeline,
            {
                "schema_version": 2,
                "replay_ticks_per_second": "48000000",
                "media_id": MEDIA_ID,
                "video": {
                    "codec": "h264",
                    "profile": "High",
                    "time_base": {"numerator": "1", "denominator": "15360"},
                    "first_pts": "0",
                    "frame_rate": {"numerator": "60", "denominator": "1"},
                    "frame_count": "600",
                    "one_past_last_pts": "153600",
                    "replay_end": "480000000",
                    "exact_cfr": True,
                },
                "audio": {
                    "present": True,
                    "codec": "aac",
                    "sample_rate": 48000,
                    "time_base": {"numerator": "1", "denominator": "48000"},
                    "first_pts": "0",
                    "replay_start": "0",
                    "replay_end": "480000000",
                },
                "container": {
                    "start_seconds": {"numerator": "0", "denominator": "1"},
                    "duration_seconds": {"numerator": "10", "denominator": "1"},
                },
                "producer": {
                    "backend": "replay-corpus-copy",
                    "expected_frame_rate": {
                        "numerator": "60",
                        "denominator": "1",
                    },
                    "expected_frame_count": "600",
                    "media_runtime_id": "test-runtime",
                },
                "capture": None,
            },
        )
        self.assertEqual(metadata["capture_backend"], "replay-corpus-copy")
        self.assertNotIn("native", json.dumps(metadata))
        corpus._assert_bundle_contract(
            metadata,
            corpus.build_game_log(
                MEDIA_ID,
                480_000_000,
                event_interval_game_ticks=5_000_000,
                snapshot_interval_game_ticks=30_000_000,
            ),
        )
        serialized = json.dumps(metadata)
        for legacy_field in corpus.LEGACY_TIME_FIELDS:
            self.assertNotIn(legacy_field, serialized)

        contradictory_grid = exact_grid()
        contradictory_grid["frame_count"] = 599
        with self.assertRaisesRegex(corpus.CorpusError, "contradicts"):
            corpus._build_metadata(
                fixture, media_summary(), contradictory_grid, "test-runtime"
            )

        false_provenance = dict(fixture, producer_backend="native")
        with self.assertRaisesRegex(corpus.CorpusError, "contradicts its transform"):
            corpus._build_metadata(
                false_provenance, media_summary(), exact_grid(), "test-runtime"
            )

    def test_game_log_is_deterministic_anonymized_and_microsecond_bounded(self) -> None:
        replay_end_tick = 120 * corpus.REPLAY_TICKS_PER_SECOND
        arguments = {
            "event_interval_game_ticks": 5_000_000,
            "snapshot_interval_game_ticks": 30_000_000,
        }
        first = corpus.build_game_log(MEDIA_ID, replay_end_tick, **arguments)
        second = corpus.build_game_log(MEDIA_ID, replay_end_tick, **arguments)

        self.assertEqual(first, second)
        self.assertEqual(first["schema_version"], 2)
        self.assertEqual(first["media_id"], MEDIA_ID)
        self.assertEqual(first["calibration"]["replay_tick_at_game_zero"], "0")
        self.assertEqual(first["calibration"]["last_game_tick"], "1000000")
        self.assertEqual(
            set(first),
            {
                "schema_version",
                "media_id",
                "calibration",
                "snapshots",
                "events",
                "snapshot_derived_changes",
            },
        )
        self.assertEqual(
            set(first["calibration"]),
            {
                "status",
                "sample_count",
                "first_game_tick",
                "last_game_tick",
                "replay_tick_at_game_zero",
                "maximum_rtt_game_ticks",
                "maximum_residual_game_ticks",
                "uncertainty_game_ticks",
            },
        )
        self.assertEqual(len(first["snapshots"]), 4)
        self.assertGreaterEqual(len(first["events"]), 23)
        self.assertTrue(
            all(int(event["game_tick"]) < 120_000_000 for event in first["events"])
        )
        serialized = json.dumps(first)
        self.assertIn("QB-Blue-1#TEST", serialized)
        self.assertNotIn("summoner_name_here", serialized)
        for legacy_field in corpus.LEGACY_TIME_FIELDS:
            self.assertNotIn(legacy_field, serialized)

        expected_players = [
            {
                "summoner_name": f"QB-{team}-{index}#TEST",
                "team": "ORDER" if team == "Blue" else "CHAOS",
                "champion": (
                    ["Ahri", "Garen", "LeeSin", "Jinx", "Thresh"]
                    if team == "Blue"
                    else ["Annie", "Darius", "Vi", "Ashe", "Leona"]
                )[index - 1],
                "gold": None,
                "hp": None,
                "hp_max": None,
                "cs": 0,
                "level": 1,
                "items": [],
                "summoner_spells": ["Flash", "Teleport"],
                "keystone_id": 8214,
            }
            for team in ("Blue", "Red")
            for index in range(1, 6)
        ]
        self.assertEqual(
            corpus.build_game_log(
                MEDIA_ID,
                3 * corpus.REPLAY_TICKS_PER_GAME_TICK,
                event_interval_game_ticks=2,
                snapshot_interval_game_ticks=3,
            ),
            {
                "schema_version": 2,
                "media_id": MEDIA_ID,
                "calibration": {
                    "status": "available",
                    "sample_count": 5,
                    "first_game_tick": "0",
                    "last_game_tick": "2",
                    "replay_tick_at_game_zero": "0",
                    "maximum_rtt_game_ticks": "0",
                    "maximum_residual_game_ticks": "0",
                    "uncertainty_game_ticks": "0",
                },
                "snapshots": [{"game_tick": "0", "players": expected_players}],
                "events": [
                    {
                        "type": "ChampionKill",
                        "game_tick": "2",
                        "killer": "QB-Red-2#TEST",
                        "victim": "QB-Blue-1#TEST",
                        "assisters": ["QB-Blue-2#TEST"],
                    }
                ],
                "snapshot_derived_changes": [],
            },
        )

    def test_bundle_identity_mismatch_and_legacy_output_are_rejected(self) -> None:
        fixture = {
            "media_id": MEDIA_ID,
            "transform": "copy",
            "expected_frame_rate": Fraction(60, 1),
        }
        metadata = corpus._build_metadata(
            fixture, media_summary(), exact_grid(), "test-runtime"
        )
        game_log = corpus.build_game_log(
            OTHER_MEDIA_ID,
            480_000_000,
            event_interval_game_ticks=5_000_000,
            snapshot_interval_game_ticks=30_000_000,
        )
        with self.assertRaisesRegex(corpus.CorpusError, "do not match"):
            corpus._assert_bundle_contract(metadata, game_log)

        game_log["media_id"] = MEDIA_ID
        game_log["events"].append({"game_tick": "0", "video_time_ms": 0})
        with self.assertRaisesRegex(corpus.CorpusError, "legacy fields"):
            corpus._assert_bundle_contract(metadata, game_log)

    def test_reparse_sentinel_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="queueback-corpus-reparse-") as temporary:
            sentinel = Path(temporary) / corpus.SENTINEL_NAME
            sentinel.mkdir()
            with mock.patch.object(
                corpus,
                "_has_reparse_attribute",
                side_effect=lambda path: path == sentinel,
            ), self.assertRaisesRegex(corpus.CorpusError, "reparse point"):
                corpus._assert_reparse_free_existing(sentinel, "sentinel_root")

    @unittest.skipUnless(corpus.os.name == "nt", "Windows path safety contract")
    def test_spec_rejects_v1_existing_output_escape_and_missing_sentinel(self) -> None:
        with tempfile.TemporaryDirectory(prefix="queueback-corpus-") as temporary:
            base = Path(temporary).resolve()
            sentinel = base / corpus.SENTINEL_NAME
            sentinel.mkdir()
            source = base / "source.mp4"
            source.write_bytes(b"fixture")

            def specification(output: Path) -> dict[str, object]:
                return {
                    "schema_version": 2,
                    "corpus_id": "corpus-1",
                    "sentinel_root": str(sentinel),
                    "output_root": str(output),
                    "fixtures": [
                        {
                            "id": "short",
                            "media_id": MEDIA_ID,
                            "alias": "short",
                            "game_timestamp": "1787600000",
                            "source_video": str(source),
                            "transform": "copy",
                            "expected_video_codec": "h264",
                            "expected_audio_codec": "aac",
                            "expected_frame_rate": {
                                "numerator": "60",
                                "denominator": "1",
                            },
                            "minimum_duration_seconds": 120,
                        }
                    ],
                }

            wrong_sentinel = base / ".not-the-replay-time-sentinel"
            wrong_sentinel.mkdir()
            wrong = specification(wrong_sentinel / "corpus-1")
            wrong["sentinel_root"] = str(wrong_sentinel)
            with self.assertRaisesRegex(corpus.CorpusError, "must end"):
                corpus._validate_spec(wrong)

            output = sentinel / "sources" / "corpus-1"
            output.mkdir(parents=True)
            with self.assertRaisesRegex(corpus.CorpusError, "already exists"):
                corpus._validate_spec(specification(output))

            v1 = specification(sentinel / "other" / "corpus-1")
            v1["schema_version"] = 1
            with self.assertRaisesRegex(corpus.CorpusError, "integer 2"):
                corpus._validate_spec(v1)

            escaped = specification(base / "corpus-1")
            with self.assertRaisesRegex(corpus.CorpusError, "strictly below"):
                corpus._validate_spec(escaped)

            missing = specification(sentinel / "other" / "corpus-1")
            missing["sentinel_root"] = str(base / "missing" / corpus.SENTINEL_NAME)
            missing["output_root"] = str(
                base / "missing" / corpus.SENTINEL_NAME / "corpus-1"
            )
            with self.assertRaisesRegex(corpus.CorpusError, "missing"):
                corpus._validate_spec(missing)

    def test_preflight_receipt_is_deterministic_and_exact(self) -> None:
        with tempfile.TemporaryDirectory(prefix="queueback-corpus-receipt-") as temporary:
            base = Path(temporary)
            sentinel = base / corpus.SENTINEL_NAME
            sentinel.mkdir()
            source = base / "source.mp4"
            source.write_bytes(b"source")
            fixture = {
                "id": "short",
                "media_id": MEDIA_ID,
                "alias": "short",
                "game_timestamp": "1787600000",
                "source_video": source,
                "transform": "copy",
                "expected_video_codec": "h264",
                "expected_audio_codec": "aac",
                "expected_frame_rate": Fraction(60, 1),
                "minimum_duration_seconds": Fraction(1, 1),
                "target_duration_seconds": None,
                "event_interval_game_ticks": 5_000_000,
                "snapshot_interval_game_ticks": 30_000_000,
            }
            validated = {
                "schema_version": 2,
                "corpus_id": "corpus-1",
                "sentinel_root": sentinel,
                "output_root": sentinel / "sources" / "corpus-1",
                "fixtures": [fixture],
            }
            patches = (
                mock.patch.object(corpus, "_validate_spec", return_value=validated),
                mock.patch.object(
                    corpus,
                    "_resolve_tools",
                    return_value=(
                        Path("ffmpeg.exe"),
                        Path("ffprobe.exe"),
                        "test-runtime",
                    ),
                ),
                mock.patch.object(corpus, "_probe", return_value={}),
                mock.patch.object(
                    corpus, "_validate_media", return_value=media_summary()
                ),
                mock.patch.object(
                    corpus, "_scan_video_grid", return_value=exact_grid()
                ),
            )
            with patches[0], patches[1], patches[2], patches[3], patches[4]:
                first = corpus.build_corpus(
                    {}, Path("runtime"), preflight_only=True
                )
                second = corpus.build_corpus(
                    {}, Path("runtime"), preflight_only=True
                )

            self.assertEqual(first, second)
            evidence = first["fixtures"][0]
            self.assertEqual(
                evidence["source_sha256_before"],
                evidence["source_sha256_after_preflight"],
            )
            self.assertIsInstance(
                evidence["source_media"]["exact_frame_rate"]["numerator"], str
            )
            self.assertEqual(evidence["producer_backend"], "replay-corpus-copy")

    def test_failed_build_preserves_hidden_partial_without_publishing_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="queueback-corpus-publication-") as temporary:
            base = Path(temporary)
            sentinel = base / corpus.SENTINEL_NAME
            sentinel.mkdir()
            output_root = sentinel / "sources" / "corpus-1"
            source = base / "source.mp4"
            source.write_bytes(b"source")
            fixture = {
                "id": "short",
                "media_id": MEDIA_ID,
                "alias": "short",
                "game_timestamp": "1787600000",
                "source_video": source,
                "transform": "copy",
                "expected_video_codec": "h264",
                "expected_audio_codec": "aac",
                "expected_frame_rate": Fraction(60, 1),
                "minimum_duration_seconds": Fraction(1, 1),
                "target_duration_seconds": None,
                "event_interval_game_ticks": 5_000_000,
                "snapshot_interval_game_ticks": 30_000_000,
            }
            validated = {
                "schema_version": 2,
                "corpus_id": "corpus-1",
                "sentinel_root": sentinel,
                "output_root": output_root,
                "fixtures": [fixture],
            }

            def create_media(_ffmpeg, _fixture, destination, _source_grid):
                destination.write_bytes(b"partial-media")
                return 600

            with mock.patch.object(corpus, "_validate_spec", return_value=validated), mock.patch.object(
                corpus,
                "_resolve_tools",
                return_value=(Path("ffmpeg.exe"), Path("ffprobe.exe"), "test-runtime"),
            ), mock.patch.object(corpus, "_probe", return_value={}), mock.patch.object(
                corpus, "_validate_media", return_value=media_summary()
            ), mock.patch.object(corpus, "_scan_video_grid", return_value=exact_grid()), mock.patch.object(
                corpus, "_create_media", side_effect=create_media
            ), mock.patch.object(
                corpus,
                "_build_metadata",
                side_effect=corpus.CorpusError("injected validation failure"),
            ):
                with self.assertRaisesRegex(corpus.CorpusError, "injected"):
                    corpus.build_corpus({}, Path("runtime"), preflight_only=False)

            self.assertTrue(
                (output_root / ".partial" / "short" / "video.partial.mp4").is_file()
            )
            self.assertFalse((output_root / "short").exists())
            self.assertFalse((output_root / "corpus.json").exists())

    def test_help_is_available(self) -> None:
        with self.assertRaises(SystemExit) as raised:
            corpus._parser().parse_args(["--help"])
        self.assertEqual(raised.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
