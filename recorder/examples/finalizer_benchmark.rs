#[cfg(feature = "replay-time-fixture")]
mod benchmark {
    use std::collections::BTreeSet;
    use std::fs::{self, File, OpenOptions};
    use std::io::{ErrorKind, Read, Write};
    use std::path::{Component, Path, PathBuf};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use anyhow::{Context, Result, bail, ensure};
    use chronobreak_replay_time::{
        FinalizationExpectationsV2, FrameBoundary, MediaId, MediaTimelineV2, ProducerEvidenceV2,
        Rational,
    };
    use league_replay_recorder::{
        FINALIZER_FIXTURE_PROBE_OUTPUT_LIMIT, FINALIZER_FIXTURE_PROBE_TIMEOUT,
        validate_finalizer_fixture,
    };
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};

    const MANIFEST_SCHEMA_VERSION: u32 = 1;
    const RESULT_SCHEMA_VERSION: u32 = 1;
    const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
    const MAX_RESULT_BYTES: usize = 1024 * 1024;
    const MIN_FIXTURE_COUNT: usize = 5;
    const MIN_REPETITIONS: u32 = 5;
    const MAX_REPETITIONS: u32 = 20;
    const P95_LIMIT_MS: f64 = 2_000.0;
    const REPEATABILITY_RELATIVE_THRESHOLD_PERCENT: f64 = 5.0;
    const RUNTIME_MANIFEST_CONTRACT_VERSION: u32 = 3;
    const MAX_RUNTIME_MANIFEST_BYTES: u64 = 1024 * 1024;
    const SAFE_EVIDENCE_ROOT: &str = "build/replay-time/qb-replay-012";
    const SAFE_SOURCE_ROOT: &str = "build/replay-time/qb-replay-012/finalizer-benchmark-sources";
    const SAFE_SENTINEL_ROOT: &str = "build/replay-time/qb-replay-012/finalizer-benchmark-sentinel";
    const SAFE_RUNTIME_ROOT: &str = "build/media-runtime/windows-x86_64";
    const SAFE_FFPROBE_PATH: &str = "build/media-runtime/windows-x86_64/bin/ffprobe.exe";

    #[derive(Debug)]
    struct Arguments {
        manifest: PathBuf,
        result: PathBuf,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Manifest {
        schema_version: u32,
        media_runtime_id: String,
        ffprobe_path: PathBuf,
        source_root: PathBuf,
        sentinel_copy_root: PathBuf,
        repetitions: u32,
        repeatability: RepeatabilityPolicy,
        fixtures: Vec<FixtureManifest>,
    }

    #[derive(Debug, Deserialize)]
    struct RuntimeManifestIdentity {
        contract_version: u32,
        runtime_id: String,
        files: Vec<RuntimeFileIdentity>,
    }

    #[derive(Debug, Deserialize)]
    struct RuntimeFileIdentity {
        path: PathBuf,
        size: u64,
        sha256: String,
    }

    #[derive(Debug)]
    struct ValidatedRuntime {
        ffprobe_path: PathBuf,
        runtime_manifest_path: PathBuf,
        runtime_manifest_sha256: String,
        ffprobe_sha256: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RepeatabilityPolicy {
        resolution_floor_ms: f64,
        unfavorable_relative_threshold_percent: f64,
        baseline: Option<RepeatabilityBaseline>,
        disposition: Option<Disposition>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RepeatabilityBaseline {
        benchmark_id: String,
        aggregate_p95_ms: f64,
        aggregate_mad_ms: f64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Disposition {
        accepted: bool,
        rationale: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FixtureManifest {
        id: String,
        duration_class: DurationClass,
        one_hour: bool,
        source_relative_path: PathBuf,
        source_size_bytes: u64,
        source_sha256: String,
        expected_video_codec: String,
        expected_audio_codec: Option<String>,
        expected_frame_rate: Rational,
        expected_frame_count: FrameBoundary,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum DurationClass {
        Short,
        Medium,
        Long,
    }

    #[derive(Debug, Serialize)]
    struct BenchmarkResult {
        schema_version: u32,
        benchmark_id: String,
        command: String,
        media_runtime_id: String,
        manifest_path: PathBuf,
        manifest_sha256: String,
        runtime_manifest_path: PathBuf,
        runtime_manifest_sha256: String,
        ffprobe_path: PathBuf,
        ffprobe_sha256: String,
        sentinel_copy_root: PathBuf,
        started_unix_ms: u128,
        finished_unix_ms: u128,
        repetitions: u32,
        probe_timeout_ms: u128,
        probe_combined_output_limit_bytes: usize,
        p95_limit_ms: f64,
        fixtures: Vec<FixtureResult>,
        aggregate_p95_ms: f64,
        aggregate_mad_ms: f64,
        repeatability: RepeatabilityEvaluation,
        gates: BenchmarkGates,
    }

    #[derive(Debug, Serialize)]
    struct FixtureResult {
        id: String,
        duration_class: DurationClass,
        one_hour_required: bool,
        source_relative_path: PathBuf,
        source_size_bytes: u64,
        source_sha256_expected: String,
        source_sha256_before: String,
        source_sha256_after: String,
        source_unchanged: bool,
        expected_video_codec: String,
        expected_audio_codec: Option<String>,
        expected_frame_rate: Rational,
        expected_frame_count: FrameBoundary,
        p95_ms: f64,
        mad_ms: f64,
        observations: Vec<Observation>,
    }

    #[derive(Debug, Serialize)]
    struct Observation {
        repetition: u32,
        media_id: MediaId,
        candidate_directory: PathBuf,
        partial_path: PathBuf,
        partial_sha256_before: String,
        partial_sha256_after: String,
        partial_unchanged: bool,
        elapsed_ms: f64,
        probe_stdout_bytes: Option<usize>,
        probe_stderr_bytes: Option<usize>,
        probe_combined_bytes: Option<usize>,
        result: ObservationResult,
        error: Option<String>,
        timeline: Option<MediaTimelineV2>,
        publication_artifacts_absent: bool,
        partial_removed_after_success: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum ObservationResult {
        Passed,
        Failed,
    }

    #[derive(Debug, Serialize)]
    struct RepeatabilityEvaluation {
        policy: RepeatabilityPolicy,
        repeatability_band_ms: Option<f64>,
        unfavorable_delta_ms: Option<f64>,
        unfavorable_relative_percent: Option<f64>,
        outside_band_plus_relative_threshold: Option<bool>,
        disposition_used: bool,
        passed: bool,
    }

    #[derive(Debug, Serialize)]
    struct BenchmarkGates {
        at_least_five_distinct_fixtures: bool,
        short_medium_long_present: bool,
        at_least_five_repetitions_per_fixture: bool,
        one_hour_fixture_validated: bool,
        all_validations_succeeded: bool,
        all_sources_unchanged: bool,
        all_candidate_copies_unchanged: bool,
        no_publication_artifacts_created: bool,
        probe_output_within_production_limit: bool,
        every_fixture_p95_at_most_two_seconds: bool,
        aggregate_p95_at_most_two_seconds: bool,
        repeatability_within_band_or_disposition: bool,
        passed: bool,
    }

    pub async fn run() -> Result<()> {
        let arguments = parse_arguments()?;
        let repository_root_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("recorder manifest directory has no repository parent")?
            .canonicalize()
            .context("could not canonicalize repository root")?;
        let repository_root = repository_root_path.as_path();
        let evidence_root_path = repository_root.join(SAFE_EVIDENCE_ROOT);
        assert_reparse_free_existing_chain(
            &evidence_root_path,
            repository_root,
            "finalizer benchmark evidence root",
        )?;
        let evidence_root = evidence_root_path
            .canonicalize()
            .context("could not canonicalize finalizer benchmark evidence root")?;
        let manifest_candidate =
            resolve_repo_path(repository_root, &arguments.manifest, "manifest")?;
        let manifest_path =
            resolve_existing_evidence_file(&manifest_candidate, &evidence_root, "manifest")?;
        let result_candidate = resolve_repo_path(repository_root, &arguments.result, "result")?;
        let result_path = resolve_new_evidence_file(&result_candidate, &evidence_root, "result")?;

        let manifest_metadata = fs::metadata(&manifest_path)
            .with_context(|| format!("could not inspect manifest {}", manifest_path.display()))?;
        ensure!(
            manifest_metadata.is_file() && manifest_metadata.len() <= MAX_MANIFEST_BYTES,
            "manifest must be a regular file no larger than {MAX_MANIFEST_BYTES} bytes"
        );
        let manifest_bytes = fs::read(&manifest_path)
            .with_context(|| format!("could not read manifest {}", manifest_path.display()))?;
        let manifest_sha256 = sha256_bytes(&manifest_bytes);
        let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
            .context("finalizer benchmark manifest is not the strict schema")?;
        validate_manifest(&manifest)?;

        let runtime = validate_packaged_runtime(
            repository_root,
            &manifest.ffprobe_path,
            &manifest.media_runtime_id,
        )?;
        let ffprobe_path = runtime.ffprobe_path.clone();
        let source_root = resolve_exact_safe_root(
            repository_root,
            &manifest.source_root,
            SAFE_SOURCE_ROOT,
            false,
            "source root",
        )?;
        let sentinel_root = resolve_exact_safe_root(
            repository_root,
            &manifest.sentinel_copy_root,
            SAFE_SENTINEL_ROOT,
            true,
            "sentinel copy root",
        )?;

        let started_unix_ms = unix_milliseconds()?;
        let benchmark_id = format!("finalizer-{}-{}", started_unix_ms, std::process::id());
        let run_root = sentinel_root.join(&benchmark_id);
        fs::create_dir(&run_root).with_context(|| {
            format!(
                "could not create unique benchmark sentinel {}",
                run_root.display()
            )
        })?;

        let mut fixture_results = Vec::with_capacity(manifest.fixtures.len());
        let mut all_elapsed = Vec::with_capacity(
            manifest.fixtures.len() * usize::try_from(manifest.repetitions).unwrap_or(0),
        );
        for fixture in &manifest.fixtures {
            let fixture_result = run_fixture(
                repository_root,
                &source_root,
                &run_root,
                &ffprobe_path,
                &manifest.media_runtime_id,
                manifest.repetitions,
                fixture,
            )
            .await?;
            all_elapsed.extend(
                fixture_result
                    .observations
                    .iter()
                    .map(|observation| observation.elapsed_ms),
            );
            fixture_results.push(fixture_result);
        }

        let aggregate_p95_ms = type7_quantile(&all_elapsed, 0.95)?;
        let aggregate_mad_ms = median_absolute_deviation(&all_elapsed)?;
        let repeatability =
            evaluate_repeatability(manifest.repeatability.clone(), aggregate_p95_ms)?;
        let gates = evaluate_gates(
            &manifest,
            &fixture_results,
            aggregate_p95_ms,
            repeatability.passed,
        );
        let finished_unix_ms = unix_milliseconds()?;
        let command = format!(
            "cargo run --manifest-path recorder/Cargo.toml --example finalizer_benchmark --features replay-time-fixture -- --manifest {} --result {}",
            arguments.manifest.display(),
            arguments.result.display()
        );
        let result = BenchmarkResult {
            schema_version: RESULT_SCHEMA_VERSION,
            benchmark_id,
            command,
            media_runtime_id: manifest.media_runtime_id,
            manifest_path: arguments.manifest,
            manifest_sha256,
            runtime_manifest_path: repository_relative(
                repository_root,
                &runtime.runtime_manifest_path,
            )?,
            runtime_manifest_sha256: runtime.runtime_manifest_sha256,
            ffprobe_path: manifest.ffprobe_path.clone(),
            ffprobe_sha256: runtime.ffprobe_sha256,
            sentinel_copy_root: manifest.sentinel_copy_root,
            started_unix_ms,
            finished_unix_ms,
            repetitions: manifest.repetitions,
            probe_timeout_ms: FINALIZER_FIXTURE_PROBE_TIMEOUT.as_millis(),
            probe_combined_output_limit_bytes: FINALIZER_FIXTURE_PROBE_OUTPUT_LIMIT,
            p95_limit_ms: P95_LIMIT_MS,
            fixtures: fixture_results,
            aggregate_p95_ms,
            aggregate_mad_ms,
            repeatability,
            gates,
        };
        write_create_new_result(&result_path, &result)?;
        ensure!(
            result.gates.passed,
            "finalizer benchmark gates failed; inspect {}",
            result_path.display()
        );
        println!(
            "FINALIZER_BENCHMARK_PASS result={} aggregate_p95_ms={:.3}",
            result_path.display(),
            result.aggregate_p95_ms
        );
        Ok(())
    }

    async fn run_fixture(
        repository_root: &Path,
        source_root: &Path,
        run_root: &Path,
        ffprobe_path: &Path,
        media_runtime_id: &str,
        repetitions: u32,
        fixture: &FixtureManifest,
    ) -> Result<FixtureResult> {
        let source = resolve_fixture_source(source_root, &fixture.source_relative_path)?;
        let source_metadata = fs::metadata(&source)
            .with_context(|| format!("could not inspect fixture source {}", source.display()))?;
        ensure!(source_metadata.is_file(), "fixture source is not a file");
        ensure!(
            source_metadata.len() == fixture.source_size_bytes,
            "fixture {} source size does not match its manifest identity",
            fixture.id
        );
        let source_sha256_before = sha256_file(&source)?;
        ensure!(
            source_sha256_before == fixture.source_sha256,
            "fixture {} source hash does not match its manifest identity",
            fixture.id
        );

        let mut observations = Vec::with_capacity(usize::try_from(repetitions).unwrap_or(0));
        for repetition in 1..=repetitions {
            let candidate_directory = run_root.join(format!("{}-{repetition}", fixture.id));
            fs::create_dir(&candidate_directory).with_context(|| {
                format!(
                    "could not create unique candidate directory {}",
                    candidate_directory.display()
                )
            })?;
            let partial_path = candidate_directory.join("candidate.partial.mp4");
            let copied_bytes = fs::copy(&source, &partial_path).with_context(|| {
                format!(
                    "could not copy fixture {} into private candidate {}",
                    fixture.id,
                    partial_path.display()
                )
            })?;
            ensure!(
                copied_bytes == fixture.source_size_bytes,
                "fixture {} private copy size changed during copy",
                fixture.id
            );
            let partial_sha256_before = sha256_file(&partial_path)?;
            ensure!(
                partial_sha256_before == source_sha256_before,
                "fixture {} private copy hash differs from its source",
                fixture.id
            );

            let media_id = MediaId::new_v4();
            let expectations = FinalizationExpectationsV2 {
                media_id: media_id.clone(),
                expected_video_codec: fixture.expected_video_codec.clone(),
                expected_audio_codec: fixture.expected_audio_codec.clone(),
                producer: ProducerEvidenceV2 {
                    backend: "finalizer-benchmark-generated".to_owned(),
                    expected_frame_rate: fixture.expected_frame_rate,
                    expected_frame_count: fixture.expected_frame_count,
                    media_runtime_id: media_runtime_id.to_owned(),
                },
                capture: None,
                require_zero_video_start: true,
            };
            let started = Instant::now();
            let validation = validate_finalizer_fixture(
                ffprobe_path,
                candidate_directory.clone(),
                partial_path.clone(),
                fixture.expected_frame_rate,
                fixture.expected_frame_count,
                expectations,
            )
            .await;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;

            let partial_sha256_after = sha256_file(&partial_path)?;
            let partial_unchanged = partial_sha256_after == partial_sha256_before;
            let publication_artifacts_absent = publication_artifacts_absent(&candidate_directory);
            let (result, error, timeline, stdout_bytes, stderr_bytes, combined_bytes) =
                match validation {
                    Ok(validated) => {
                        let combined = validated
                            .probe_stdout_bytes
                            .checked_add(validated.probe_stderr_bytes)
                            .context("probe byte accounting overflowed")?;
                        (
                            ObservationResult::Passed,
                            None,
                            Some(validated.timeline),
                            Some(validated.probe_stdout_bytes),
                            Some(validated.probe_stderr_bytes),
                            Some(combined),
                        )
                    }
                    Err(error) => (
                        ObservationResult::Failed,
                        Some(bounded_error(&error)),
                        None,
                        None,
                        None,
                        None,
                    ),
                };

            let partial_removed_after_success = if result == ObservationResult::Passed
                && partial_unchanged
                && publication_artifacts_absent
            {
                fs::remove_file(&partial_path).with_context(|| {
                    format!(
                        "could not remove successful sentinel copy {}",
                        partial_path.display()
                    )
                })?;
                fs::remove_dir(&candidate_directory).with_context(|| {
                    format!(
                        "could not remove empty successful candidate directory {}",
                        candidate_directory.display()
                    )
                })?;
                true
            } else {
                false
            };

            observations.push(Observation {
                repetition,
                media_id,
                candidate_directory: repository_relative(repository_root, &candidate_directory)?,
                partial_path: repository_relative(repository_root, &partial_path)?,
                partial_sha256_before,
                partial_sha256_after,
                partial_unchanged,
                elapsed_ms,
                probe_stdout_bytes: stdout_bytes,
                probe_stderr_bytes: stderr_bytes,
                probe_combined_bytes: combined_bytes,
                result,
                error,
                timeline,
                publication_artifacts_absent,
                partial_removed_after_success,
            });
        }

        let source_sha256_after = sha256_file(&source)?;
        let elapsed = observations
            .iter()
            .map(|observation| observation.elapsed_ms)
            .collect::<Vec<_>>();
        Ok(FixtureResult {
            id: fixture.id.clone(),
            duration_class: fixture.duration_class,
            one_hour_required: fixture.one_hour,
            source_relative_path: fixture.source_relative_path.clone(),
            source_size_bytes: fixture.source_size_bytes,
            source_sha256_expected: fixture.source_sha256.clone(),
            source_sha256_before: source_sha256_before.clone(),
            source_sha256_after: source_sha256_after.clone(),
            source_unchanged: source_sha256_before == source_sha256_after,
            expected_video_codec: fixture.expected_video_codec.clone(),
            expected_audio_codec: fixture.expected_audio_codec.clone(),
            expected_frame_rate: fixture.expected_frame_rate,
            expected_frame_count: fixture.expected_frame_count,
            p95_ms: type7_quantile(&elapsed, 0.95)?,
            mad_ms: median_absolute_deviation(&elapsed)?,
            observations,
        })
    }

    fn parse_arguments() -> Result<Arguments> {
        let mut args = std::env::args().skip(1);
        let mut manifest = None;
        let mut result = None;
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--manifest" => {
                    ensure!(manifest.is_none(), "--manifest may be supplied only once");
                    manifest = Some(PathBuf::from(
                        args.next().context("--manifest requires a path")?,
                    ));
                }
                "--result" => {
                    ensure!(result.is_none(), "--result may be supplied only once");
                    result = Some(PathBuf::from(
                        args.next().context("--result requires a path")?,
                    ));
                }
                other => bail!("unknown argument {other:?}"),
            }
        }
        Ok(Arguments {
            manifest: manifest.context("--manifest is required")?,
            result: result.context("--result is required")?,
        })
    }

    fn validate_manifest(manifest: &Manifest) -> Result<()> {
        ensure!(
            manifest.schema_version == MANIFEST_SCHEMA_VERSION,
            "unsupported finalizer benchmark manifest schema"
        );
        ensure!(
            !manifest.media_runtime_id.trim().is_empty() && manifest.media_runtime_id.len() <= 128,
            "media runtime identity must contain 1..=128 bytes"
        );
        ensure!(
            manifest.source_root == Path::new(SAFE_SOURCE_ROOT),
            "source_root must be the dedicated finalizer benchmark source sentinel"
        );
        ensure!(
            manifest.sentinel_copy_root == Path::new(SAFE_SENTINEL_ROOT),
            "sentinel_copy_root must be the dedicated finalizer benchmark copy sentinel"
        );
        ensure!(
            manifest.ffprobe_path == Path::new(SAFE_FFPROBE_PATH),
            "ffprobe_path must identify the packaged finalizer runtime"
        );
        ensure!(
            manifest.repetitions >= MIN_REPETITIONS && manifest.repetitions <= MAX_REPETITIONS,
            "repetitions must be in {MIN_REPETITIONS}..={MAX_REPETITIONS}"
        );
        ensure!(
            manifest.fixtures.len() >= MIN_FIXTURE_COUNT && manifest.fixtures.len() <= 32,
            "fixture count must be in {MIN_FIXTURE_COUNT}..=32"
        );
        validate_repeatability_policy(&manifest.repeatability)?;

        let mut ids = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let mut classes = BTreeSet::new();
        let mut one_hour_count = 0_usize;
        for fixture in &manifest.fixtures {
            ensure!(
                valid_fixture_id(&fixture.id),
                "fixture id {:?} is not lowercase ASCII slug data",
                fixture.id
            );
            ensure!(ids.insert(fixture.id.clone()), "duplicate fixture id");
            validate_relative_repo_path(&fixture.source_relative_path, "fixture source")?;
            ensure!(
                fixture
                    .source_relative_path
                    .extension()
                    .and_then(|value| value.to_str())
                    == Some("mp4"),
                "fixture {} source must be an MP4",
                fixture.id
            );
            ensure!(
                sources.insert(fixture.source_relative_path.clone()),
                "fixtures must use distinct source files"
            );
            ensure!(
                fixture.source_size_bytes > 0,
                "fixture source must not be empty"
            );
            ensure!(
                valid_sha256(&fixture.source_sha256),
                "fixture {} has a noncanonical SHA-256",
                fixture.id
            );
            ensure!(
                fixture.expected_video_codec == "h264"
                    && fixture.expected_audio_codec.as_deref() == Some("aac"),
                "fixture {} must exercise the production H.264/AAC finalizer contract",
                fixture.id
            );
            ensure!(
                fixture.expected_frame_rate.is_positive(),
                "fixture {} frame rate must be positive",
                fixture.id
            );
            ensure!(
                fixture.expected_frame_count.get() > 0,
                "fixture {} frame count must be positive",
                fixture.id
            );
            classes.insert(match fixture.duration_class {
                DurationClass::Short => 0_u8,
                DurationClass::Medium => 1_u8,
                DurationClass::Long => 2_u8,
            });
            if fixture.one_hour {
                one_hour_count += 1;
                ensure!(
                    fixture.duration_class == DurationClass::Long,
                    "one-hour fixture must be classified as long"
                );
            }
        }
        ensure!(
            classes.len() == 3,
            "short, medium, and long fixtures are required"
        );
        ensure!(
            one_hour_count >= 1,
            "at least one one-hour fixture is required"
        );
        Ok(())
    }

    fn validate_repeatability_policy(policy: &RepeatabilityPolicy) -> Result<()> {
        ensure!(
            policy.resolution_floor_ms.is_finite() && policy.resolution_floor_ms > 0.0,
            "repeatability resolution floor must be finite and positive"
        );
        ensure!(
            policy.unfavorable_relative_threshold_percent
                == REPEATABILITY_RELATIVE_THRESHOLD_PERCENT,
            "repeatability unfavorable threshold must be exactly five percent"
        );
        if let Some(baseline) = &policy.baseline {
            ensure!(
                !baseline.benchmark_id.trim().is_empty()
                    && baseline.aggregate_p95_ms.is_finite()
                    && baseline.aggregate_p95_ms > 0.0
                    && baseline.aggregate_mad_ms.is_finite()
                    && baseline.aggregate_mad_ms >= 0.0,
                "repeatability baseline is invalid"
            );
        }
        if let Some(disposition) = &policy.disposition {
            ensure!(
                disposition.accepted
                    && !disposition.rationale.trim().is_empty()
                    && disposition.rationale.len() <= 2_048,
                "repeatability disposition must be accepted and contain bounded rationale"
            );
        }
        ensure!(
            policy.baseline.is_some() || policy.disposition.is_some(),
            "a repeatability baseline or accepted first-run disposition is required"
        );
        Ok(())
    }

    fn evaluate_repeatability(
        policy: RepeatabilityPolicy,
        measured_p95_ms: f64,
    ) -> Result<RepeatabilityEvaluation> {
        ensure!(measured_p95_ms.is_finite(), "measured p95 is not finite");
        let Some(baseline) = policy.baseline.as_ref() else {
            let passed = policy
                .disposition
                .as_ref()
                .is_some_and(|value| value.accepted);
            return Ok(RepeatabilityEvaluation {
                policy,
                repeatability_band_ms: None,
                unfavorable_delta_ms: None,
                unfavorable_relative_percent: None,
                outside_band_plus_relative_threshold: None,
                disposition_used: passed,
                passed,
            });
        };
        let band = policy
            .resolution_floor_ms
            .max(3.0 * baseline.aggregate_mad_ms);
        let delta = measured_p95_ms - baseline.aggregate_p95_ms;
        let relative_percent = delta / baseline.aggregate_p95_ms * 100.0;
        let outside =
            delta > band && relative_percent > policy.unfavorable_relative_threshold_percent;
        let disposition_used = outside
            && policy
                .disposition
                .as_ref()
                .is_some_and(|value| value.accepted);
        Ok(RepeatabilityEvaluation {
            policy,
            repeatability_band_ms: Some(band),
            unfavorable_delta_ms: Some(delta),
            unfavorable_relative_percent: Some(relative_percent),
            outside_band_plus_relative_threshold: Some(outside),
            disposition_used,
            passed: !outside || disposition_used,
        })
    }

    fn evaluate_gates(
        manifest: &Manifest,
        fixtures: &[FixtureResult],
        aggregate_p95_ms: f64,
        repeatability_passed: bool,
    ) -> BenchmarkGates {
        let ids = fixtures
            .iter()
            .map(|fixture| fixture.id.as_str())
            .collect::<BTreeSet<_>>();
        let short_medium_long_present = [
            DurationClass::Short,
            DurationClass::Medium,
            DurationClass::Long,
        ]
        .into_iter()
        .all(|class| {
            fixtures
                .iter()
                .any(|fixture| fixture.duration_class == class)
        });
        let all_observations = fixtures
            .iter()
            .flat_map(|fixture| fixture.observations.iter())
            .collect::<Vec<_>>();
        let one_hour_fixture_validated = fixtures.iter().any(|fixture| {
            fixture.one_hour_required
                && fixture.observations.iter().all(|observation| {
                    observation.result == ObservationResult::Passed
                        && observation.timeline.as_ref().is_some_and(|timeline| {
                            timeline.container.duration_seconds.to_f64() >= 3_600.0
                        })
                })
        });
        let at_least_five_distinct_fixtures = ids.len() >= MIN_FIXTURE_COUNT;
        let at_least_five_repetitions_per_fixture = fixtures.iter().all(|fixture| {
            fixture.observations.len() >= usize::try_from(MIN_REPETITIONS).unwrap_or(5)
                && fixture.observations.len() == usize::try_from(manifest.repetitions).unwrap_or(0)
        });
        let all_validations_succeeded = all_observations
            .iter()
            .all(|observation| observation.result == ObservationResult::Passed);
        let all_sources_unchanged = fixtures.iter().all(|fixture| fixture.source_unchanged);
        let all_candidate_copies_unchanged = all_observations
            .iter()
            .all(|observation| observation.partial_unchanged);
        let no_publication_artifacts_created = all_observations
            .iter()
            .all(|observation| observation.publication_artifacts_absent);
        let probe_output_within_production_limit = all_observations.iter().all(|observation| {
            observation
                .probe_combined_bytes
                .is_some_and(|bytes| bytes <= FINALIZER_FIXTURE_PROBE_OUTPUT_LIMIT)
        });
        let every_fixture_p95_at_most_two_seconds = fixtures
            .iter()
            .all(|fixture| fixture.p95_ms <= P95_LIMIT_MS);
        let aggregate_p95_at_most_two_seconds = aggregate_p95_ms <= P95_LIMIT_MS;
        let passed = at_least_five_distinct_fixtures
            && short_medium_long_present
            && at_least_five_repetitions_per_fixture
            && one_hour_fixture_validated
            && all_validations_succeeded
            && all_sources_unchanged
            && all_candidate_copies_unchanged
            && no_publication_artifacts_created
            && probe_output_within_production_limit
            && every_fixture_p95_at_most_two_seconds
            && aggregate_p95_at_most_two_seconds
            && repeatability_passed;
        BenchmarkGates {
            at_least_five_distinct_fixtures,
            short_medium_long_present,
            at_least_five_repetitions_per_fixture,
            one_hour_fixture_validated,
            all_validations_succeeded,
            all_sources_unchanged,
            all_candidate_copies_unchanged,
            no_publication_artifacts_created,
            probe_output_within_production_limit,
            every_fixture_p95_at_most_two_seconds,
            aggregate_p95_at_most_two_seconds,
            repeatability_within_band_or_disposition: repeatability_passed,
            passed,
        }
    }

    fn validate_packaged_runtime(
        repository_root: &Path,
        declared_ffprobe: &Path,
        expected_runtime_id: &str,
    ) -> Result<ValidatedRuntime> {
        ensure!(
            declared_ffprobe == Path::new(SAFE_FFPROBE_PATH),
            "ffprobe path is not the packaged finalizer runtime"
        );
        let runtime_root_path = repository_root.join(SAFE_RUNTIME_ROOT);
        assert_reparse_free_existing_chain(
            &runtime_root_path,
            repository_root,
            "packaged runtime root",
        )?;
        let runtime_root = runtime_root_path
            .canonicalize()
            .context("could not canonicalize packaged runtime root")?;
        let runtime_manifest_path = runtime_root.join("runtime-manifest.json");
        assert_reparse_free_existing_chain(
            &runtime_manifest_path,
            &runtime_root,
            "packaged runtime manifest",
        )?;
        let runtime_manifest_metadata =
            fs::metadata(&runtime_manifest_path).with_context(|| {
                format!(
                    "could not inspect packaged runtime manifest {}",
                    runtime_manifest_path.display()
                )
            })?;
        ensure!(
            runtime_manifest_metadata.is_file()
                && runtime_manifest_metadata.len() <= MAX_RUNTIME_MANIFEST_BYTES,
            "packaged runtime manifest is not a bounded regular file"
        );
        let runtime_manifest_bytes = fs::read(&runtime_manifest_path).with_context(|| {
            format!(
                "could not read packaged runtime manifest {}",
                runtime_manifest_path.display()
            )
        })?;
        let runtime_manifest: RuntimeManifestIdentity =
            serde_json::from_slice(&runtime_manifest_bytes)
                .context("packaged runtime manifest identity is malformed")?;
        ensure!(
            runtime_manifest.contract_version == RUNTIME_MANIFEST_CONTRACT_VERSION,
            "packaged runtime manifest contract version is unsupported"
        );
        ensure!(
            runtime_manifest.runtime_id == expected_runtime_id,
            "declared media runtime identity does not match the packaged runtime manifest"
        );

        let ffprobe_relative = Path::new("bin/ffprobe.exe");
        let mut ffprobe_entries = runtime_manifest
            .files
            .iter()
            .filter(|entry| entry.path == ffprobe_relative);
        let ffprobe_entry = ffprobe_entries
            .next()
            .context("packaged runtime manifest lacks ffprobe identity")?;
        ensure!(
            ffprobe_entries.next().is_none(),
            "packaged runtime manifest repeats ffprobe identity"
        );
        ensure!(
            valid_sha256(&ffprobe_entry.sha256) && ffprobe_entry.size > 0,
            "packaged runtime ffprobe identity is invalid"
        );
        let ffprobe_path = runtime_root.join(ffprobe_relative);
        assert_reparse_free_existing_chain(&ffprobe_path, &runtime_root, "packaged ffprobe")?;
        let ffprobe_metadata = fs::metadata(&ffprobe_path).with_context(|| {
            format!(
                "could not inspect packaged ffprobe {}",
                ffprobe_path.display()
            )
        })?;
        ensure!(
            ffprobe_metadata.is_file() && ffprobe_metadata.len() == ffprobe_entry.size,
            "packaged ffprobe size does not match the runtime manifest"
        );
        let ffprobe_sha256 = sha256_file(&ffprobe_path)?;
        ensure!(
            ffprobe_sha256 == ffprobe_entry.sha256,
            "packaged ffprobe hash does not match the runtime manifest"
        );
        Ok(ValidatedRuntime {
            ffprobe_path,
            runtime_manifest_path,
            runtime_manifest_sha256: sha256_bytes(&runtime_manifest_bytes),
            ffprobe_sha256,
        })
    }

    fn resolve_existing_evidence_file(
        path: &Path,
        evidence_root: &Path,
        label: &str,
    ) -> Result<PathBuf> {
        ensure_path_is_within(path, evidence_root, label)?;
        assert_reparse_free_existing_chain(path, evidence_root, label)?;
        let metadata = fs::metadata(path)
            .with_context(|| format!("could not inspect {label} {}", path.display()))?;
        ensure!(metadata.is_file(), "{label} must be a regular file");
        let canonical = path
            .canonicalize()
            .with_context(|| format!("could not canonicalize {label} {}", path.display()))?;
        ensure_path_is_within(&canonical, evidence_root, label)?;
        Ok(canonical)
    }

    fn resolve_new_evidence_file(
        path: &Path,
        evidence_root: &Path,
        label: &str,
    ) -> Result<PathBuf> {
        ensure_path_is_within(path, evidence_root, label)?;
        let parent = path
            .parent()
            .with_context(|| format!("{label} path has no parent"))?;
        assert_reparse_free_existing_chain(parent, evidence_root, label)?;
        let canonical_parent = parent.canonicalize().with_context(|| {
            format!("could not canonicalize {label} parent {}", parent.display())
        })?;
        ensure_path_is_within(&canonical_parent, evidence_root, label)?;
        match fs::symlink_metadata(path) {
            Ok(_) => bail!("refusing to overwrite {label} {}", path.display()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("could not inspect {label} {}", path.display()));
            }
        }
        let file_name = path
            .file_name()
            .with_context(|| format!("{label} path has no file name"))?;
        Ok(canonical_parent.join(file_name))
    }

    fn assert_reparse_free_existing_chain(path: &Path, root: &Path, label: &str) -> Result<()> {
        ensure!(
            path.starts_with(root),
            "{label} is outside its trusted root"
        );
        let mut cursor = path;
        loop {
            let metadata = fs::symlink_metadata(cursor).with_context(|| {
                format!(
                    "could not inspect {label} path component {}",
                    cursor.display()
                )
            })?;
            ensure!(
                !metadata_is_reparse_point(&metadata),
                "{label} traverses a reparse point at {}",
                cursor.display()
            );
            if cursor == root {
                return Ok(());
            }
            cursor = cursor
                .parent()
                .with_context(|| format!("{label} does not descend from its trusted root"))?;
        }
    }

    fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
        if metadata.file_type().is_symlink() {
            return true;
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    fn resolve_exact_safe_root(
        repository_root: &Path,
        declared: &Path,
        expected: &str,
        create: bool,
        label: &str,
    ) -> Result<PathBuf> {
        ensure!(
            declared == Path::new(expected),
            "{label} is not the required sentinel"
        );
        let path = repository_root.join(declared);
        if create {
            match fs::symlink_metadata(&path) {
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    let parent = path
                        .parent()
                        .with_context(|| format!("{label} has no parent"))?;
                    assert_reparse_free_existing_chain(parent, repository_root, label)?;
                    fs::create_dir(&path)
                        .with_context(|| format!("could not create {label} {}", path.display()))?;
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("could not inspect {label} {}", path.display()));
                }
            }
        }
        assert_reparse_free_existing_chain(&path, repository_root, label)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("could not canonicalize {label} {}", path.display()))?;
        let expected_parent = repository_root
            .join(SAFE_EVIDENCE_ROOT)
            .canonicalize()
            .context("could not canonicalize finalizer benchmark evidence root")?;
        ensure_path_is_within(&canonical, &expected_parent, label)?;
        Ok(canonical)
    }

    fn resolve_fixture_source(source_root: &Path, relative: &Path) -> Result<PathBuf> {
        validate_relative_repo_path(relative, "fixture source")?;
        let candidate = source_root.join(relative);
        assert_reparse_free_existing_chain(&candidate, source_root, "fixture source")?;
        let source = candidate.canonicalize().with_context(|| {
            format!(
                "could not canonicalize fixture source {}",
                relative.display()
            )
        })?;
        ensure_path_is_within(&source, source_root, "fixture source")?;
        Ok(source)
    }

    fn resolve_repo_path(repository_root: &Path, relative: &Path, label: &str) -> Result<PathBuf> {
        validate_relative_repo_path(relative, label)?;
        Ok(repository_root.join(relative))
    }

    fn validate_relative_repo_path(path: &Path, label: &str) -> Result<()> {
        ensure!(!path.as_os_str().is_empty(), "{label} path is empty");
        ensure!(
            !path.is_absolute(),
            "{label} path must be repository-relative"
        );
        ensure!(
            path.components()
                .all(|component| matches!(component, Component::Normal(_))),
            "{label} path must contain only normal components"
        );
        Ok(())
    }

    fn ensure_path_is_within(path: &Path, root: &Path, label: &str) -> Result<()> {
        ensure!(
            path.starts_with(root),
            "{label} escapes its dedicated sentinel root"
        );
        Ok(())
    }

    fn repository_relative(repository_root: &Path, path: &Path) -> Result<PathBuf> {
        path.strip_prefix(repository_root)
            .map(Path::to_path_buf)
            .context("benchmark path escaped the repository root")
    }

    fn publication_artifacts_absent(directory: &Path) -> bool {
        ["video.mp4", "metadata.json", "metadata.pending.json"]
            .into_iter()
            .all(|name| !directory.join(name).exists())
    }

    fn valid_fixture_id(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !value.starts_with('-')
            && !value.ends_with('-')
    }

    fn valid_sha256(value: &str) -> bool {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn sha256_file(path: &Path) -> Result<String> {
        let mut file = File::open(path)
            .with_context(|| format!("could not open {} for hashing", path.display()))?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .with_context(|| format!("could not hash {}", path.display()))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    fn type7_quantile(values: &[f64], probability: f64) -> Result<f64> {
        ensure!(
            !values.is_empty(),
            "cannot calculate a quantile of no observations"
        );
        ensure!(
            (0.0..=1.0).contains(&probability),
            "quantile probability is outside zero through one"
        );
        let mut sorted = values.to_vec();
        ensure!(
            sorted.iter().all(|value| value.is_finite()),
            "quantile observation is not finite"
        );
        sorted.sort_by(f64::total_cmp);
        let index = (sorted.len() - 1) as f64 * probability;
        let lower = index.floor() as usize;
        let upper = index.ceil() as usize;
        let fraction = index - lower as f64;
        Ok(sorted[lower] + (sorted[upper] - sorted[lower]) * fraction)
    }

    fn median_absolute_deviation(values: &[f64]) -> Result<f64> {
        let median = type7_quantile(values, 0.5)?;
        let deviations = values
            .iter()
            .map(|value| (value - median).abs())
            .collect::<Vec<_>>();
        type7_quantile(&deviations, 0.5)
    }

    fn bounded_error(error: &anyhow::Error) -> String {
        let rendered = format!("{error:#}");
        rendered.chars().take(4_096).collect()
    }

    fn unix_milliseconds() -> Result<u128> {
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time is before the Unix epoch")?
            .as_millis())
    }

    fn write_create_new_result(path: &Path, result: &BenchmarkResult) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(result)
            .context("could not serialize finalizer benchmark result")?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_RESULT_BYTES,
            "benchmark result exceeds the one-MiB evidence bound"
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("could not create benchmark result {}", path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("could not write benchmark result {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("could not flush benchmark result {}", path.display()))?;
        Ok(())
    }
}

#[cfg(feature = "replay-time-fixture")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    benchmark::run().await
}

#[cfg(not(feature = "replay-time-fixture"))]
fn main() {
    eprintln!("finalizer_benchmark requires --features replay-time-fixture");
}
