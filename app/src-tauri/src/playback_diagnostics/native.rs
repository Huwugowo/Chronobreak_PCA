//! COM objects remain on the WebView thread. Only bounded JSON and parsed state cross it.

use std::cell::RefCell;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use tauri::Emitter;
use tokio::sync::{mpsc, oneshot};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
    Microsoft::Web::WebView2::Win32::*,
};
use windows::{
    Win32::System::Com::CoTaskMemFree,
    core::{HSTRING, PWSTR},
};

use super::*;

struct Event {
    kind: &'static str,
    json: String,
}

#[derive(Clone)]
struct Queue {
    sender: mpsc::Sender<Event>,
    bytes: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

impl Queue {
    fn push(&self, kind: &'static str, json: String) {
        let length = json.len();
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |bytes| {
                bytes
                    .checked_add(length)
                    .filter(|total| *total <= MAX_PENDING_BYTES)
            })
            .is_err()
        {
            self.dropped.store(true, Ordering::Release);
            return;
        }
        if self.sender.try_send(Event { kind, json }).is_err() {
            self.bytes.fetch_sub(length, Ordering::AcqRel);
            self.dropped.store(true, Ordering::Release);
        }
    }
}

struct Owner {
    id: String,
    webview: ICoreWebView2,
    receivers: Vec<(ICoreWebView2DevToolsProtocolEventReceiver, i64)>,
    state: Arc<Mutex<Monitor>>,
    stopped: Arc<AtomicBool>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        for (receiver, token) in self.receivers.drain(..) {
            // SAFETY: Owner is stored/dropped only in the WebView thread's TLS.
            if unsafe { receiver.remove_DevToolsProtocolEventReceived(token) }.is_err() {
                self.state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .invalidate("native subscription removal failed");
            }
        }
        let completion =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
        // SAFETY: CoreWebView2 and callback are used on their owning WebView thread.
        let _ = unsafe {
            self.webview.CallDevToolsProtocolMethod(
                &HSTRING::from("Media.disable"),
                &HSTRING::from("{}"),
                &completion,
            )
        };
    }
}

thread_local! {
    static OWNER: RefCell<Option<Owner>> = const { RefCell::new(None) };
}

// The caller supplies a COM-owned NUL-terminated allocation; always free it, and
// scan at most 4096 UTF-16 units before copying. Oversize events invalidate proof.
unsafe fn take_bounded(raw: PWSTR) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    let mut length = 0;
    // SAFETY: ParameterObjectAsJson returned a live NUL-terminated allocation;
    // reads stop at NUL or our bound, and the allocation is freed exactly once.
    unsafe {
        while length < 4096 && *raw.0.add(length) != 0 {
            length += 1;
        }
        let value = if length == 4096 {
            None
        } else {
            String::from_utf16(std::slice::from_raw_parts(raw.0, length)).ok()
        };
        CoTaskMemFree(Some(raw.0.cast()));
        value
    }
}

fn call(
    webview: &ICoreWebView2,
    method: &'static str,
    params: &str,
    queue: Queue,
) -> windows::core::Result<()> {
    let completion =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, value| {
            if result.is_ok() && value.len() <= MAX_PENDING_BYTES {
                queue.push(method, value);
            } else {
                queue.dropped.store(true, Ordering::Release);
            }
            Ok(())
        }));
    // SAFETY: This helper is invoked only inside a with_webview closure.
    unsafe {
        webview.CallDevToolsProtocolMethod(
            &HSTRING::from(method),
            &HSTRING::from(params),
            &completion,
        )
    }
}

fn marker(window: &tauri::WebviewWindow, owner: String, id: String, node: u64, queue: Queue) {
    let _ = window.with_webview(move |view| {
        let current = OWNER.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|value| value.id == owner)
        });
        if !current {
            return;
        }
        // SAFETY: with_webview provides access on the owning COM thread.
        let Ok(webview) = (unsafe { view.controller().CoreWebView2() }) else {
            return;
        };
        let completion =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, value| {
                if result.is_ok() && value.len() <= MAX_PENDING_BYTES {
                    let parsed = serde_json::from_str::<Value>(&value).ok();
                    let marked = parsed
                        .as_ref()
                        .and_then(|value| value["node"]["attributes"].as_array())
                        .is_some_and(|attributes| {
                            attributes.chunks_exact(2).any(|pair| {
                                pair[0] == "data-qb-primary-playback" && pair[1] == "true"
                            })
                        });
                    queue.push(
                        "DOM.marker",
                        serde_json::json!({"playerId":id, "marked":marked}).to_string(),
                    );
                } else {
                    queue.dropped.store(true, Ordering::Release);
                }
                Ok(())
            }));
        // SAFETY: CoreWebView2 and handler are live on the WebView thread; fixed method only.
        let _ = unsafe {
            webview.CallDevToolsProtocolMethod(
                &HSTRING::from("DOM.describeNode"),
                &HSTRING::from(serde_json::json!({"backendNodeId":node}).to_string()),
                &completion,
            )
        };
    });
}

async fn observe(
    window: tauri::WebviewWindow,
    state: Arc<Mutex<Monitor>>,
    stopped: Arc<AtomicBool>,
    queue: Queue,
    mut receiver: mpsc::Receiver<Event>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let mut last = None;
    loop {
        let event = tokio::select! {
            event = receiver.recv() => event,
            _ = interval.tick() => None,
        };
        if stopped.load(Ordering::Acquire) {
            return;
        }
        let (snapshot, markers) = {
            let mut monitor = state.lock().unwrap_or_else(|error| error.into_inner());
            if queue.dropped.swap(false, Ordering::AcqRel) {
                monitor.invalidate("diagnostic events dropped or exceeded budget");
            }
            if let Some(event) = event {
                queue.bytes.fetch_sub(event.json.len(), Ordering::AcqRel);
                if event.kind == "DOM.marker" {
                    if let Ok(value) = serde_json::from_str::<Value>(&event.json)
                        && let Some(candidate) = value["playerId"]
                            .as_str()
                            .and_then(|id| monitor.candidates.get_mut(id))
                    {
                        candidate.marker = value["marked"].as_bool();
                    }
                } else {
                    monitor.ingest(event.kind, &event.json);
                }
            }
            monitor.reconcile();
            let markers: Vec<_> = monitor
                .candidates
                .iter_mut()
                .filter_map(|(id, candidate)| {
                    if candidate.marker.is_none() {
                        candidate.node.map(|node| {
                            candidate.marker = Some(false);
                            (id.clone(), node)
                        })
                    } else {
                        None
                    }
                })
                .collect();
            (monitor.snapshot.clone(), markers)
        };
        for (id, node) in markers {
            marker(&window, snapshot.owner.clone(), id, node, queue.clone());
        }
        if last.as_ref() != Some(&snapshot) {
            if window.emit("playback-decoder", &snapshot).is_err() {
                return;
            }
            last = Some(snapshot);
        }
    }
}

pub(super) async fn open(
    window: tauri::WebviewWindow,
    id: String,
) -> Result<DecoderSnapshot, String> {
    let (reply, response) = oneshot::channel();
    let worker_window = window.clone();
    let close_id = id.clone();
    window
        .with_webview(move |view| {
            let setup = || -> windows::core::Result<()> {
                OWNER.with(|slot| {
                    slot.borrow_mut().take();
                });
                // SAFETY: Tauri invokes this closure on the owning WebView thread.
                let webview = unsafe { view.controller().CoreWebView2()? };
                let state = Arc::new(Mutex::new(Monitor::new(id.clone())));
                let stopped = Arc::new(AtomicBool::new(false));
                let (sender, receiver) = mpsc::channel(16);
                let queue = Queue {
                    sender,
                    bytes: Arc::new(AtomicUsize::new(0)),
                    dropped: Arc::new(AtomicBool::new(false)),
                };
                let mut owner = Owner {
                    id,
                    webview: webview.clone(),
                    receivers: Vec::new(),
                    state: Arc::clone(&state),
                    stopped: Arc::clone(&stopped),
                };
                for kind in [
                    "Media.playerCreated",
                    "Media.playersCreated",
                    "Media.playerEventsAdded",
                    "Media.playerPropertiesChanged",
                    "Media.playerErrorsRaised",
                ] {
                    let event_queue = queue.clone();
                    let handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(
                        move |_, args| {
                            let Some(args) = args else { return Ok(()) };
                            let mut raw = PWSTR::null();
                            // SAFETY: The COM event callback owns valid args on the WebView thread.
                            unsafe {
                                args.ParameterObjectAsJson(&mut raw)?;
                            }
                            // SAFETY: raw is the COM-owned NUL-terminated allocation returned above.
                            if let Some(json) = unsafe { take_bounded(raw) } {
                                event_queue.push(kind, json);
                            } else {
                                event_queue.dropped.store(true, Ordering::Release);
                            }
                            Ok(())
                        },
                    ));
                    // SAFETY: Registration and later token removal use the same WebView thread.
                    let receiver =
                        unsafe { webview.GetDevToolsProtocolEventReceiver(&HSTRING::from(kind))? };
                    let mut token = 0;
                    // SAFETY: receiver/handler are live COM interfaces on their owning thread.
                    unsafe {
                        receiver.add_DevToolsProtocolEventReceived(&handler, &mut token)?;
                    }
                    owner.receivers.push((receiver, token));
                }
                call(&webview, "Browser.getVersion", "{}", queue.clone())?;
                tauri::async_runtime::spawn(observe(
                    worker_window,
                    Arc::clone(&state),
                    stopped,
                    queue,
                    receiver,
                ));
                OWNER.with(|slot| *slot.borrow_mut() = Some(owner));
                let completion = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                    move |result, _| {
                        let mut monitor = state.lock().unwrap_or_else(|error| error.into_inner());
                        monitor.snapshot.enabled = result.is_ok();
                        if result.is_err() {
                            monitor.snapshot.status = DecoderPath::Unsupported;
                            monitor.snapshot.reason = "Media protocol is unavailable".into();
                        }
                        let _ = reply.send(Ok(monitor.snapshot.clone()));
                        Ok(())
                    },
                ));
                // SAFETY: Fixed Media.enable call on the WebView thread, after subscription.
                unsafe {
                    webview.CallDevToolsProtocolMethod(
                        &HSTRING::from("Media.enable"),
                        &HSTRING::from("{}"),
                        &completion,
                    )?;
                }
                Ok(())
            };
            if let Err(error) = setup() {
                eprintln!("playback diagnostics unavailable: {error}");
                OWNER.with(|slot| {
                    slot.borrow_mut().take();
                });
            }
        })
        .map_err(|error| error.to_string())?;
    match tokio::time::timeout(ACQUISITION, response).await {
        Ok(Ok(result)) => result,
        _ => {
            close(window, close_id)?;
            Err("decoder diagnostic subscription timed out or failed".into())
        }
    }
}

pub(super) async fn bind(
    window: tauri::WebviewWindow,
    id: String,
    binding: Binding,
) -> Result<DecoderSnapshot, String> {
    let (reply, response) = oneshot::channel();
    window
        .with_webview(move |_| {
            let result = OWNER.with(|slot| {
                let slot = slot.borrow();
                let owner = slot
                    .as_ref()
                    .filter(|owner| owner.id == id)
                    .ok_or("diagnostic owner superseded")?;
                let mut monitor = owner
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                monitor.bind(binding)?;
                Ok(monitor.snapshot.clone())
            });
            let _ = reply.send(result);
        })
        .map_err(|error| error.to_string())?;
    tokio::time::timeout(ACQUISITION, response)
        .await
        .map_err(|_| "diagnostic binding timed out".to_owned())?
        .map_err(|_| "diagnostic binding cancelled".to_owned())?
}

pub(super) fn close(window: tauri::WebviewWindow, id: String) -> Result<(), String> {
    window
        .with_webview(move |_| {
            OWNER.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.as_ref().is_some_and(|owner| owner.id == id) {
                    slot.take();
                }
            });
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_diagnostics_are_bounded_and_overflow_is_explicit() {
        let (sender, mut receiver) = mpsc::channel(16);
        let queue = Queue {
            sender,
            bytes: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(AtomicBool::new(false)),
        };
        for _ in 0..20 {
            queue.push("Media.playerEventsAdded", "x".repeat(4096));
        }
        assert!(queue.bytes.load(Ordering::Acquire) <= MAX_PENDING_BYTES);
        assert!(queue.dropped.load(Ordering::Acquire));
        let mut received = 0;
        while let Ok(event) = receiver.try_recv() {
            queue.bytes.fetch_sub(event.json.len(), Ordering::AcqRel);
            received += 1;
        }
        assert_eq!(received, 16);
        assert_eq!(queue.bytes.load(Ordering::Acquire), 0);
    }
}
