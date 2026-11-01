//! Dedicated generated-window proof of the production service with unavailable Live data.
//! Never reads user configuration, records League, or writes to a user library.

#[cfg(all(windows, feature = "native-failure-injection"))]
fn scenario_restore_time(scenario: &str, seconds: u64) -> anyhow::Result<u64> {
    let (restore_at, minimum) = match scenario {
        "steady" | "close_window" => (7, 10),
        "recreate" => (7, 13),
        "minimize_restore" => (21, 27),
        _ => anyhow::bail!("unsupported lifecycle scenario"),
    };
    anyhow::ensure!(
        (minimum..=600).contains(&seconds),
        "{scenario} requires {minimum}-600 seconds, including fresh frames after restore"
    );
    Ok(restore_at)
}

#[cfg(all(windows, feature = "native-failure-injection"))]
fn finish_with_cleanup<T>(
    result: anyhow::Result<T>,
    cleanup: anyhow::Result<()>,
) -> anyhow::Result<T> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(primary.context(format!("fixture cleanup also failed: {cleanup:#}")))
        }
    }
}

#[cfg(all(windows, feature = "native-failure-injection"))]
fn validate_decoded_frames(scenario: &str, seconds: u64, decoded: &[u8]) -> anyhow::Result<()> {
    let mut chunks = decoded.chunks_exact(16 * 16 * 3);
    let frames: Vec<_> = chunks.by_ref().collect();
    anyhow::ensure!(chunks.remainder().is_empty(), "incomplete decoded frame");
    anyhow::ensure!(
        frames.len() >= seconds.saturating_sub(2).max(4) as usize,
        "recording timeline stopped before process exit"
    );
    if matches!(scenario, "recreate" | "minimize_restore") {
        let red = |frame: &&[u8]| frame[0] > 180 && frame[1] < 70;
        let green = |frame: &&[u8]| frame[1] > 180 && frame[0] < 70;
        anyhow::ensure!(
            frames.iter().take(5).any(red) && frames.iter().rev().take(4).all(green),
            "decoded recording did not resume the restored window's fresh green epoch"
        );
        anyhow::ensure!(
            frames[frames.len() - 4..].windows(2).all(|pair| pair[0]
                .iter()
                .zip(pair[1])
                .any(|(a, b)| a.abs_diff(*b) > 32)),
            "decoded frames did not keep advancing after restore"
        );
    }
    Ok(())
}

#[cfg(all(windows, feature = "native-failure-injection"))]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use anyhow::{Context, bail};
    use chronobreak_recorder_control::windows::{ControlOwner, Namespace, query};
    use chronobreak_recorder_control::{CaptureActivity, ServiceState};
    use chronobreak_replay_time::MediaId;
    use league_replay_recorder::config::Config;
    use league_replay_recorder::service::{EventSink, ServiceCommand, ServiceEvent};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::mpsc;

    let scenario = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "steady".to_owned());
    let seconds: u64 = std::env::args()
        .nth(1)
        .context("duration seconds required")?
        .parse()?;
    let restore_at = scenario_restore_time(&scenario, seconds)?;
    if std::env::var("LEAGUE_REPLAY_PROCESS_NAME").as_deref() != Ok("wgc_fixture.exe")
        || std::env::var("LEAGUE_REPLAY_AUDIO_DEVICE").as_deref() != Ok("silent")
    {
        bail!("fixture requires 10-600 seconds, wgc_fixture.exe process override and silent audio");
    }
    // A successful real Live endpoint would invalidate this unavailable-data fixture.
    let live = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()?;
    if live
        .get("https://127.0.0.1:2999/liveclientdata/gamestats")
        .send()
        .await
        .is_ok()
    {
        bail!("Live API is available; do not run the missing-data fixture during a game");
    }
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("build/qb-data-003/capture-fixtures");
    std::fs::create_dir_all(&parent)?;
    let root = parent.join(MediaId::new_v4().as_str());
    std::fs::create_dir(&root)?;
    std::fs::write(
        root.join(".queueback-context-capture-fixture"),
        b"generated fixture only\n",
    )?;
    let log = std::fs::File::create(root.join("service.log"))?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(Mutex::new(log))
        .init();
    let fixture_exe = std::env::current_exe()?
        .parent()
        .unwrap()
        .join("wgc_fixture.exe");
    let mut fixture = tokio::process::Command::new(fixture_exe)
        .args([
            "--scenario",
            &scenario,
            "--duration-seconds",
            &(seconds + 120).to_string(),
            "--always-on-top",
            "--action-start-signal",
            root.join("recording-started")
                .to_str()
                .context("signal path")?,
            "--action-after-seconds",
            "3",
            "--restore-after-seconds",
            &restore_at.to_string(),
        ])
        .creation_flags(0x0800_0000) // no helper console; the generated capture window stays visible
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut fixture_output =
        BufReader::new(fixture.stdout.take().context("fixture stdout")?).lines();
    let target_ready = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(line) = fixture_output.next_line().await? {
            // The fixture emits this only after drawing its first correctly sized frame.
            if line.starts_with("QUEUEBACK_WGC_TARGET ") && line.ends_with("width=1920 height=1080")
            {
                return Ok::<_, anyhow::Error>(());
            }
        }
        bail!("generated target ended before its first draw");
    })
    .await
    .context("generated target readiness timeout")
    .and_then(|result| result);
    if let Err(error) = target_ready {
        if fixture.try_wait()?.is_none() {
            fixture.kill().await?;
        }
        fixture.wait().await?;
        return Err(error);
    }
    let mut config = Config::default();
    config.storage.output_path = root.to_str().context("fixture path encoding")?.to_owned();
    config.storage.auto_delete_days = 0;
    let (commands, receiver) = mpsc::unbounded_channel();
    let (event_sender, mut events) = mpsc::channel(16);
    let failed = Arc::new(AtomicBool::new(false));
    let event_failure = Arc::clone(&failed);
    let sink: EventSink = Arc::new(move |event| {
        if matches!(event, ServiceEvent::Error { .. }) {
            event_failure.store(true, Ordering::SeqCst);
        }
        if event_sender.try_send(event).is_err() {
            event_failure.store(true, Ordering::SeqCst);
        }
    });
    let namespace = Namespace {
        lease: root.join("recorder-owner.lock"),
        pipe: format!(
            r"\\.\pipe\Chronobreak.CaptureFixture.{}",
            root.file_name().unwrap().to_string_lossy()
        ),
    };
    let owner = ControlOwner::start(&namespace)?.context("fixture owner collision")?;
    let status = owner.status_sender();
    // Only pre-capture readiness is generated. Active capture still uses the real,
    // unavailable Live source, preserving this fixture's missing-data purpose.
    let sink = league_replay_recorder::control::event_sink(Some(status.clone()), sink);
    let service = tokio::spawn(
        league_replay_recorder::service::run_with_generated_readiness_fixture(
            config, receiver, sink,
        ),
    );
    let exercise = async {
        let directory = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(event) = events.recv().await {
                match event {
                    ServiceEvent::Recording { directory } => return Ok(directory),
                    ServiceEvent::Error { .. } | ServiceEvent::ShutdownComplete => bail!("fixture startup failed"),
                    ServiceEvent::Idle | ServiceEvent::SourceUnavailable { .. } | ServiceEvent::Availability(_) => {}
                }
            }
            bail!("fixture service event owner closed");
        }).await.context("fixture startup timeout")??;
        std::fs::write(root.join("recording-started"), b"start")?;
        println!("fixture_recording_started output={}", root.display());
        let observer = query(&namespace, &std::env::current_exe()?).await?;
        if observer.service_state != ServiceState::Available || observer.capture_activity != CaptureActivity::Recording {
            bail!("service readiness did not follow capture startup");
        }
        drop(observer); // Closing an observer must not stop capture.
        let started = Instant::now();
        let mut resources = System::new();
        let pid = Pid::from_u32(std::process::id());
        let mut samples = Vec::new();
        let mut saw_unavailable = false;
        let mut saw_resumed = false;
        loop {
            let memory = tokio::task::block_in_place(|| {
                resources.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false,
                    ProcessRefreshKind::nothing().with_memory());
                resources.process(pid).map(|process| process.memory())
            });
            samples.push(serde_json::json!({"elapsed_ms": started.elapsed().as_millis(), "resident_bytes": memory}));
            if failed.load(Ordering::SeqCst) { bail!("fixture service error"); }
            if started.elapsed() >= Duration::from_secs(seconds) { break; }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => {},
                _ = tokio::signal::ctrl_c() => bail!("fixture interrupted"),
                event = events.recv() => {
                    match event {
                        Some(ServiceEvent::SourceUnavailable { .. }) => saw_unavailable = true,
                        Some(ServiceEvent::Recording { .. }) if saw_unavailable => saw_resumed = true,
                        Some(ServiceEvent::Idle | ServiceEvent::Recording { .. } | ServiceEvent::Availability(_)) => {},
                        _ => bail!("fixture recording ended early"),
                    }
                }
            }
        }
        let reopened = query(&namespace, &std::env::current_exe()?).await?;
        if reopened.capture_activity != CaptureActivity::Recording { bail!("capture did not survive observer close/reopen"); }
        if matches!(scenario.as_str(), "close_window" | "recreate") && !saw_unavailable {
            bail!("window loss never reported waiting for the source");
        }
        if scenario == "recreate" && !saw_resumed {
            bail!("fresh source never reported resumed recording after waiting");
        }
        if scenario != "steady" {
            // Keep publication errors inside the exercise so shutdown/reap still run.
            fixture.kill().await?;
            fixture.wait().await?;
            tokio::time::timeout(Duration::from_secs(45), async {
                while !directory.join("metadata.json").exists() {
                    if failed.load(Ordering::SeqCst) {
                        bail!("process-owned recording failed");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .context("publication after process exit timed out")??;
        }
        Ok::<_, anyhow::Error>((directory, samples))
    }.await;
    let _ = commands.send(ServiceCommand::Shutdown);
    let service_result = service.await;
    let fixture_cleanup = async {
        if fixture.try_wait()?.is_none() {
            fixture.kill().await?;
        }
        fixture.wait().await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let exercise = finish_with_cleanup(
        exercise,
        service_result
            .context("fixture service task panicked")
            .and_then(|result| result),
    );
    let (directory, samples) = finish_with_cleanup(exercise, fixture_cleanup)?;
    if status.borrow().service_state != ServiceState::Stopped {
        bail!("intentional shutdown did not reach Stopped");
    }
    if failed.load(Ordering::SeqCst) {
        bail!("fixture service failed during finalization");
    }
    for file in ["video.mp4", "metadata.json", "game_log.json"] {
        if !directory.join(file).is_file() {
            bail!("fixture canonical publication missing {file}");
        }
    }
    if directory.join("league_match.json").exists() {
        bail!("unavailable-data fixture produced unexpected context");
    }
    // Decode the entire short fixture; sample pixels distinguish a fresh source
    // from a valid MP4 containing only repeated pre-loss images.
    let ffmpeg = league_replay_recorder::encoder::Ffmpeg::resolve().await?;
    let decoded = tokio::process::Command::new(ffmpeg.path())
        .args(["-v", "error", "-threads", "1", "-i"])
        .arg(directory.join("video.mp4"))
        .args([
            "-vf",
            // Convert before scaling so subsampled chroma cannot blend the
            // corner epoch marker with pixels outside its source rectangle.
            "fps=1,format=rgb24,scale=16:16:flags=neighbor",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .creation_flags(0x0800_0000)
        .kill_on_drop(true)
        .output()
        .await?;
    if !decoded.status.success() || !decoded.stderr.is_empty() {
        bail!(
            "fixture full decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
    }
    validate_decoded_frames(&scenario, seconds, &decoded.stdout)?;
    let report = serde_json::json!({"schema_version":1,"duration_seconds":seconds,"scenario":scenario,
        "canonical_publication":true,"match_context_available":false,
        "lcu_delay_exercised":false,"resource_samples":samples});
    std::fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("fixture_pass output={}", root.display());
    Ok(())
}

#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("generated production-service capture fixture requires Windows")
}

#[cfg(all(windows, not(feature = "native-failure-injection")))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("generated readiness fixture requires --features native-failure-injection")
}

#[cfg(all(test, windows, feature = "native-failure-injection"))]
mod tests {
    use super::*;

    #[test]
    fn duration_leaves_time_to_observe_restoration() {
        assert!(scenario_restore_time("minimize_restore", 26).is_err());
        assert_eq!(scenario_restore_time("minimize_restore", 27).unwrap(), 21);
        assert!(scenario_restore_time("recreate", 10).is_err());
        assert_eq!(scenario_restore_time("recreate", 13).unwrap(), 7);
        assert!(scenario_restore_time("close_window", 10).is_ok());
    }

    #[test]
    fn decoded_check_rejects_motion_before_restore_and_frozen_restored_images() {
        for (scenario, seconds, restored_at) in [("recreate", 13, 7), ("minimize_restore", 27, 21)]
        {
            let frames = |restored_at, moving| {
                (0..seconds)
                    .flat_map(|index| {
                        let mut frame = vec![0; 16 * 16 * 3];
                        frame[..3].copy_from_slice(if index >= restored_at {
                            &[20, 240, 20]
                        } else {
                            &[240, 20, 20]
                        });
                        if moving {
                            frame[3] = (index % 2 * 200) as u8;
                        }
                        frame
                    })
                    .collect::<Vec<_>>()
            };
            let valid = frames(restored_at, true);
            assert!(validate_decoded_frames(scenario, seconds, &valid).is_ok());
            assert!(validate_decoded_frames(scenario, seconds, &frames(seconds, true)).is_err());
            assert!(
                validate_decoded_frames(scenario, seconds, &frames(restored_at, false)).is_err()
            );
            assert!(validate_decoded_frames(scenario, seconds, &valid[..16 * 16 * 3 * 4]).is_err());
        }
    }

    #[test]
    fn cleanup_never_erases_the_exercise_error_or_becomes_false_success() {
        let error = finish_with_cleanup::<()>(
            Err(anyhow::anyhow!("publication wait failed")),
            Err(anyhow::anyhow!("service join failed")),
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("publication wait failed"));
        assert!(message.contains("service join failed"));
        assert!(finish_with_cleanup(Ok(()), Err(anyhow::anyhow!("cleanup failed"))).is_err());
    }
}
