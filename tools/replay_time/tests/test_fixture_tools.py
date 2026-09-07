from __future__ import annotations

import argparse
from array import array
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import wave


TOOLS_ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "replay_time_fixture_tools", TOOLS_ROOT / "fixture_tools.py"
)
assert SPEC is not None and SPEC.loader is not None
fixture_tools = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture_tools)

WIDTH = 320
HEIGHT = 240
FRAME_BYTES = WIDTH * HEIGHT
GENERATION = 0x1234
DURATION_SECONDS = 6
MARKER_SAMPLES = (48_000, 144_000, 240_000)
LIVE_TIMESTAMPS = (
    100,
    330,
    610,
    850,
    980,
    1_005,
    1_100,
    1_320,
    1_610,
    1_850,
    2_100,
    2_330,
    2_610,
    2_850,
    2_980,
    3_005,
    3_100,
    3_320,
    3_610,
    3_850,
    4_100,
    4_330,
    4_610,
    4_850,
    4_980,
    5_005,
    5_100,
    5_320,
    5_610,
    5_850,
)


def marker_payload(
    generation: int,
    state: int,
    timestamp_ms: int,
    *,
    corrupt_checksum: bool = False,
) -> int:
    gray = timestamp_ms ^ (timestamp_ms >> 1)
    without_checksum = (
        (fixture_tools.VISUAL_MAGIC << 80)
        | (generation << 64)
        | (state << 56)
        | (gray << 32)
        | (gray << 8)
    )
    checksum = fixture_tools.NATIVE_VISUAL_CHECKSUM_SEED
    remaining = without_checksum >> 8
    for _ in range(11):
        checksum ^= remaining & 0xFF
        remaining >>= 8
    if corrupt_checksum:
        checksum ^= 1
    return without_checksum | checksum


def marker_frame(payload: int, timestamp_ms: int | None) -> bytes:
    background = 32 if timestamp_ms is None else 48
    if timestamp_ms is not None:
        for center, luma in zip((1_000, 3_000, 5_000), (112, 152, 192)):
            if abs(timestamp_ms - center) <= 25:
                background = luma
                break
    frame = bytearray([background]) * FRAME_BYTES
    for bit_index in range(fixture_tools.NATIVE_VISUAL_BITS):
        bit = (payload >> (fixture_tools.NATIVE_VISUAL_BITS - 1 - bit_index)) & 1
        column = bit_index % fixture_tools.NATIVE_VISUAL_COLUMNS
        row = bit_index // fixture_tools.NATIVE_VISUAL_COLUMNS
        left = 16 + column * 16
        top = 16 + row * 16
        value = 235 if bit else 16
        for y_value in range(top, top + 16):
            start = y_value * WIDTH + left
            frame[start : start + 16] = bytes([value]) * 16
    return bytes(frame)


class NativeRecorderMarkerVerifierTests(unittest.TestCase):
    def make_inputs(
        self,
        root: Path,
        *,
        replacement_payload: tuple[int, int] | None = None,
        audio_offsets: tuple[int, int, int] = (0, 0, 0),
        pts_values: list[int] | None = None,
    ) -> argparse.Namespace:
        source_media = root / "source.mp4"
        source_media.write_bytes(b"native recorder marker fixture")
        video = root / "marker.gray"
        pre_epoch_timestamp = fixture_tools.NATIVE_VISUAL_TIMESTAMP_MASK
        payloads = [
            marker_payload(
                GENERATION,
                fixture_tools.NATIVE_VISUAL_PRE_EPOCH_STATE,
                pre_epoch_timestamp,
            )
        ] + [
            marker_payload(
                GENERATION,
                fixture_tools.NATIVE_VISUAL_LIVE_STATE,
                timestamp,
            )
            for timestamp in LIVE_TIMESTAMPS
        ]
        if replacement_payload is not None:
            index, payload = replacement_payload
            payloads[index] = payload
        with video.open("wb") as output:
            output.write(marker_frame(payloads[0], None))
            for payload, timestamp in zip(payloads[1:], LIVE_TIMESTAMPS):
                output.write(marker_frame(payload, timestamp))

        audio = root / "marker.wav"
        samples = array("h", [0]) * (DURATION_SECONDS * fixture_tools.SAMPLE_RATE)
        for marker_sample, offset in zip(MARKER_SAMPLES, audio_offsets):
            samples[marker_sample + offset] = fixture_tools.IMPULSE_AMPLITUDE
        with wave.open(str(audio), "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(fixture_tools.SAMPLE_RATE)
            output.writeframes(samples.tobytes())

        if pts_values is None:
            pts_values = [50, *(timestamp + 50 for timestamp in LIVE_TIMESTAMPS)]
        probe = root / "probe.json"
        probe.write_text(
            json.dumps(
                {
                    "frames": [
                        {
                            "media_type": "video",
                            "best_effort_timestamp": str(pts),
                        }
                        for pts in pts_values
                    ],
                    "streams": [{"codec_type": "video", "time_base": "1/1000"}],
                }
            ),
            encoding="utf-8",
        )
        return argparse.Namespace(
            source_media=str(source_media),
            video=str(video),
            audio=str(audio),
            probe=str(probe),
            duration_seconds=DURATION_SECONDS,
            expected_generation=GENERATION,
            manifest=str(root / "manifest.json"),
            result=str(root / "result.json"),
        )

    def test_uses_actual_nonzero_nonuniform_video_pts_and_accepts_reserved_pre_epoch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = self.make_inputs(root)

            result = fixture_tools.command_verify_native_recorder_media(arguments)

            self.assertTrue(result["passed"])
            self.assertFalse(result["synthetic"])
            self.assertEqual(result["command"], "verify-native-recorder-media")
            self.assertEqual(result["pre_epoch_frame_count"], 1)
            self.assertEqual(result["invalid_post_epoch_payload_frames"], 0)
            self.assertEqual(result["first_video_pts"], "50")
            self.assertEqual(result["markers_in_interval"], 3)
            self.assertEqual(
                [measurement["visual_sample"] for measurement in result["measurements"]],
                list(MARKER_SAMPLES),
            )
            self.assertEqual(
                [measurement["av_disagreement_samples"] for measurement in result["measurements"]],
                [0, 0, 0],
            )
            manifest = json.loads(Path(arguments.manifest).read_text(encoding="utf-8"))
            self.assertEqual(manifest["generation"], GENERATION)
            self.assertEqual(
                manifest["validation_contract"]["visual_time_authority"],
                "decoded video presentation PTS",
            )
            self.assertFalse(result["capture_latency_identifiable"])
            self.assertFalse(result["drift_gate_applied"])
            self.assertEqual(
                result["strict_drift_authority"],
                "native-post-capture-mux-replay-time",
            )

    def test_accepts_pipeline_delayed_pre_epoch_frame_as_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary),
                pts_values=[80, *(timestamp + 50 for timestamp in LIVE_TIMESTAMPS)],
            )

            result = fixture_tools.command_verify_native_recorder_media(arguments)

            self.assertTrue(result["passed"])
            self.assertEqual(result["pre_epoch_frame_count"], 1)
            self.assertEqual(result["video_epoch_time_seconds"]["numerator"], "1")
            self.assertEqual(result["video_epoch_time_seconds"]["denominator"], "20")

    def test_rejects_any_corrupt_or_wrong_generation_post_epoch_word(self) -> None:
        bad_payloads = (
            marker_payload(
                GENERATION + 1,
                fixture_tools.NATIVE_VISUAL_LIVE_STATE,
                LIVE_TIMESTAMPS[8],
            ),
            marker_payload(
                GENERATION,
                fixture_tools.NATIVE_VISUAL_LIVE_STATE,
                LIVE_TIMESTAMPS[8],
                corrupt_checksum=True,
            ),
            marker_payload(GENERATION, 0x7E, LIVE_TIMESTAMPS[8]),
        )
        for case, payload in enumerate(bad_payloads):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                arguments = self.make_inputs(root, replacement_payload=(9, payload))

                with self.assertRaisesRegex(
                    fixture_tools.FixtureError, "strict allowance is zero"
                ):
                    fixture_tools.command_verify_native_recorder_media(arguments)
                self.assertFalse(Path(arguments.result).exists())

    def test_rejects_pre_epoch_state_after_live_transition(self) -> None:
        pre_epoch = marker_payload(
            GENERATION,
            fixture_tools.NATIVE_VISUAL_PRE_EPOCH_STATE,
            fixture_tools.NATIVE_VISUAL_TIMESTAMP_MASK,
        )
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary), replacement_payload=(9, pre_epoch)
            )
            with self.assertRaisesRegex(fixture_tools.FixtureError, "returned to pre-epoch"):
                fixture_tools.command_verify_native_recorder_media(arguments)

    def test_rejects_frame_pts_count_mismatch_and_non_monotonic_pts(self) -> None:
        cases = (
            [50, *LIVE_TIMESTAMPS[:-1]],
            [50, *LIVE_TIMESTAMPS[:8], LIVE_TIMESTAMPS[7], *LIVE_TIMESTAMPS[9:]],
        )
        for case, pts_values in enumerate(cases):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                arguments = self.make_inputs(Path(temporary), pts_values=pts_values)
                with self.assertRaises(fixture_tools.FixtureError):
                    fixture_tools.command_verify_native_recorder_media(arguments)

    def test_rejects_fifty_millisecond_violation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary), audio_offsets=(2_401, 0, 0)
            )
            with self.assertRaisesRegex(
                fixture_tools.FixtureError, "no marker impulse|exceeds 50 ms"
            ):
                fixture_tools.command_verify_native_recorder_media(arguments)

    def test_reports_late_drift_without_treating_wgc_as_drift_authority(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary), audio_offsets=(0, 0, 241)
            )

            result = fixture_tools.command_verify_native_recorder_media(arguments)

            self.assertTrue(result["passed"])
            self.assertFalse(result["drift_gate_applied"])
            self.assertEqual(result["maximum_observed_drift_samples"], 241)
            self.assertEqual(
                [
                    measurement["drift_from_early_samples"]
                    for measurement in result["measurements"]
                ],
                [0, 0, -241],
            )



class SyntheticMediaDriftVerifierTests(unittest.TestCase):
    FRAME_COUNT = 120
    SAMPLES_PER_FRAME = fixture_tools.SAMPLE_RATE // 60

    def make_inputs(
        self,
        root: Path,
        disagreements: tuple[int, int, int],
    ) -> argparse.Namespace:
        video = root / "decoded.yuv"
        audio = root / "decoded.wav"
        manifest = root / "manifest.json"
        fixture_tools.command_generate(
            argparse.Namespace(
                video=str(video),
                audio=str(audio),
                manifest=str(manifest),
                width=160,
                height=160,
                frames=self.FRAME_COUNT,
                rate="60/1",
                result=None,
            )
        )
        manifest_value = json.loads(manifest.read_text(encoding="utf-8"))
        samples = array("h", [0]) * (self.FRAME_COUNT * self.SAMPLES_PER_FRAME)
        for marker, disagreement in zip(
            manifest_value["markers"], disagreements
        ):
            video_sample = int(marker["sample_index"])
            audio_sample = video_sample - disagreement
            samples[audio_sample] = fixture_tools.IMPULSE_AMPLITUDE
        with wave.open(str(audio), "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(fixture_tools.SAMPLE_RATE)
            output.writeframes(samples.tobytes())
        return argparse.Namespace(
            video=str(video),
            audio=str(audio),
            manifest=str(manifest),
            expected_start_frame=0,
            expected_frame_count=self.FRAME_COUNT,
            result=str(root / "result.json"),
        )

    def test_signed_drift_is_invariant_to_constant_offsets(self) -> None:
        for disagreement in (-960, 960):
            with self.subTest(disagreement=disagreement), tempfile.TemporaryDirectory() as temporary:
                result = fixture_tools.command_verify_media(
                    self.make_inputs(
                        Path(temporary),
                        (disagreement, disagreement, disagreement),
                    )
                )

                self.assertEqual(
                    [
                        measurement["drift_from_early_samples"]
                        for measurement in result["measurements"]
                    ],
                    [0, 0, 0],
                )

    def test_rejects_sign_crossing_and_decreasing_magnitude_drift(self) -> None:
        cases = (
            (960, 480, -960),
            (960, 480, 480),
        )
        for disagreements in cases:
            with self.subTest(disagreements=disagreements), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaisesRegex(
                    fixture_tools.FixtureError, "5 ms drift"
                ):
                    fixture_tools.command_verify_media(
                        self.make_inputs(Path(temporary), disagreements)
                    )

class NativeMuxMarkerVerifierTests(unittest.TestCase):
    FRAME_COUNT = 120
    FRAME_RATE = 60
    SAMPLES_PER_FRAME = fixture_tools.SAMPLE_RATE // FRAME_RATE

    def make_inputs(
        self,
        root: Path,
        *,
        video_offsets: tuple[int, ...] | None = None,
        marker_disagreements: tuple[int, ...] | None = None,
        swap_frame_ids: tuple[int, int] | None = None,
        input_timestamp_mode: str = "cfr",
    ) -> argparse.Namespace:
        video = root / "decoded.yuv"
        audio = root / "decoded.wav"
        manifest = root / "manifest.json"
        fixture_tools.command_generate(
            argparse.Namespace(
                video=str(video),
                audio=str(audio),
                manifest=str(manifest),
                width=160,
                height=160,
                frames=self.FRAME_COUNT,
                rate="60/1",
                result=None,
            )
        )
        if video_offsets is None:
            video_offsets = (0,) * self.FRAME_COUNT
        if marker_disagreements is not None:
            manifest_value = json.loads(manifest.read_text(encoding="utf-8"))
            markers = manifest_value["markers"]
            self.assertEqual(len(marker_disagreements), len(markers))
            with wave.open(str(audio), "rb") as reader:
                parameters = reader.getparams()
                samples = array("h")
                samples.frombytes(reader.readframes(reader.getnframes()))
            impulses: list[tuple[int, list[int]]] = []
            for marker, disagreement in zip(markers, marker_disagreements):
                source = int(marker["sample_index"])
                target = source - disagreement
                end = source + fixture_tools.IMPULSE_LENGTH_SAMPLES
                target_end = target + fixture_tools.IMPULSE_LENGTH_SAMPLES
                self.assertGreaterEqual(target, 0)
                self.assertLessEqual(target_end, len(samples))
                impulses.append((target, samples[source:end].tolist()))
                samples[source:end] = array(
                    "h", [0] * fixture_tools.IMPULSE_LENGTH_SAMPLES
                )
            for target, impulse in impulses:
                samples[target : target + len(impulse)] = array("h", impulse)
            with wave.open(str(audio), "wb") as writer:
                writer.setparams(parameters)
                writer.writeframes(samples.tobytes())
        self.assertEqual(len(video_offsets), self.FRAME_COUNT)

        if swap_frame_ids is not None:
            first, second = swap_frame_ids
            manifest_value = json.loads(manifest.read_text(encoding="utf-8"))
            frame_bytes = int(manifest_value["frame_bytes"])
            payload = bytearray(video.read_bytes())
            first_frame = bytes(
                payload[first * frame_bytes : (first + 1) * frame_bytes]
            )
            second_frame = bytes(
                payload[second * frame_bytes : (second + 1) * frame_bytes]
            )
            payload[first * frame_bytes : (first + 1) * frame_bytes] = second_frame
            payload[second * frame_bytes : (second + 1) * frame_bytes] = first_frame
            video.write_bytes(payload)

        pts_values = [
            frame * self.SAMPLES_PER_FRAME + video_offsets[frame]
            for frame in range(self.FRAME_COUNT)
        ]
        probe = root / "probe.json"
        probe.write_text(
            json.dumps(
                {
                    "frames": [
                        {
                            "media_type": "video",
                            "best_effort_timestamp": str(pts),
                        }
                        for pts in pts_values
                    ],
                    "streams": [{"codec_type": "video", "time_base": "1/48000"}],
                }
            ),
            encoding="utf-8",
        )
        source_media = root / "source.mp4"
        source_media.write_bytes(b"post-capture native mux fixture")
        return argparse.Namespace(
            source_media=str(source_media),
            video=str(video),
            audio=str(audio),
            probe=str(probe),
            manifest=str(manifest),
            input_timestamp_mode=input_timestamp_mode,
            result=str(root / "result.json"),
        )

    def test_signed_drift_is_invariant_to_constant_av_offset(self) -> None:
        for offset in (-960, 960):
            with self.subTest(offset=offset), tempfile.TemporaryDirectory() as temporary:
                arguments = self.make_inputs(
                    Path(temporary),
                    marker_disagreements=(offset,) * 3,
                )

                result = fixture_tools.command_verify_native_mux_media(arguments)

                self.assertTrue(result["passed"])
                self.assertFalse(result["capture_path_in_scope"])
                self.assertEqual(result["maximum_observed_drift_samples"], 0)
                self.assertTrue(result["exact_video_grid"]["passed"])
                self.assertEqual(result["exact_video_grid"]["first_pts"], "0")
                self.assertEqual(result["exact_video_grid"]["step_pts"], "800")
                self.assertEqual(
                    [
                        measurement["av_disagreement_samples"]
                        for measurement in result["measurements"]
                    ],
                    [offset, offset, offset],
                )
                self.assertEqual(
                    [
                        measurement["drift_from_early_samples"]
                        for measurement in result["measurements"]
                    ],
                    [0, 0, 0],
                )

    def test_rejects_known_six_millisecond_video_drift(self) -> None:
        disagreements = (0, 144, 288)
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary),
                marker_disagreements=disagreements,
            )

            with self.assertRaisesRegex(fixture_tools.FixtureError, "5 ms drift"):
                fixture_tools.command_verify_native_mux_media(arguments)

    def test_rejects_signed_drift_even_when_magnitude_does_not_grow(self) -> None:
        disagreements = (960, 480, -960)
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary),
                marker_disagreements=disagreements,
            )

            with self.assertRaisesRegex(fixture_tools.FixtureError, "5 ms drift"):
                fixture_tools.command_verify_native_mux_media(arguments)

    def test_rejects_lineage_swap_before_drift_can_cancel(self) -> None:
        disagreements = (0, 144, 288)
        with tempfile.TemporaryDirectory() as temporary:
            arguments = self.make_inputs(
                Path(temporary),
                marker_disagreements=disagreements,
                swap_frame_ids=(6, 7),
            )

            with self.assertRaisesRegex(fixture_tools.FixtureError, "frame identity"):
                fixture_tools.command_verify_native_mux_media(arguments)

    def test_rejects_nonzero_or_irregular_video_grid_before_result(self) -> None:
        cases = (
            (1,) * self.FRAME_COUNT,
            tuple(
                frame * self.SAMPLES_PER_FRAME + (1 if frame == 7 else 0)
                - frame * self.SAMPLES_PER_FRAME
                for frame in range(self.FRAME_COUNT)
            ),
        )
        for video_offsets in cases:
            with self.subTest(video_offsets=video_offsets), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                arguments = self.make_inputs(root, video_offsets=video_offsets)
                with self.assertRaisesRegex(
                    fixture_tools.FixtureError, "exact zero-based 60 Hz"
                ):
                    fixture_tools.command_verify_native_mux_media(arguments)
                self.assertFalse((root / "result.json").exists())


if __name__ == "__main__":
    unittest.main()
