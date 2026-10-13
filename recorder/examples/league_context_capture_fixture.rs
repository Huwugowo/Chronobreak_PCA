//! Dedicated generated-window proof of the production service with unavailable Live data.
//! Never reads user configuration, records League, or writes to a user library.

#[cfg(windows)]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use anyhow::{Context, bail};
    use chronobreak_replay_time::MediaId;
    use league_replay_recorder::config::Config;
    use league_replay_recorder::service::{self, EventSink, ServiceCommand, ServiceEvent};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::mpsc;

    let seconds: u64 = std::env::args()
        .nth(1)
        .context("duration seconds required")?
        .parse()?;
    if !(10..=600).contains(&seconds)
        || std::env::var("LEAGUE_REPLAY_PROCESS_NAME").as_deref() != Ok("wgc_fixture.exe")
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
            "steady",
            "--duration-seconds",
            &(seconds + 120).to_string(),
            "--always-on-top",
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
    let service = tokio::spawn(service::run(config, receiver, sink));
    let exercise = async {
        let directory = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(event) = events.recv().await {
                match event {
                    ServiceEvent::Recording { directory } => return Ok(directory),
                    ServiceEvent::Error { .. } | ServiceEvent::ShutdownComplete => bail!("fixture startup failed"),
                    ServiceEvent::Idle => {}
                }
            }
            bail!("fixture service event owner closed");
        }).await.context("fixture startup timeout")??;
        println!("fixture_recording_started output={}", root.display());
        let started = Instant::now();
        let mut resources = System::new();
        let pid = Pid::from_u32(std::process::id());
        let mut samples = Vec::new();
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
                    if !matches!(event, Some(ServiceEvent::Idle | ServiceEvent::Recording { .. })) {
                        bail!("fixture recording ended early");
                    }
                }
            }
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
    fixture_cleanup?;
    service_result??;
    let (directory, samples) = exercise?;
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
    let report = serde_json::json!({"schema_version":1,"duration_seconds":seconds,
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
