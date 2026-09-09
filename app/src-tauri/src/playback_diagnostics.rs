//! Bounded per-player WebView decoder observations. No arbitrary CDP access is exposed.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_PLAYERS: usize = 8;
const MAX_PROPERTIES: usize = 64;
const MAX_TEXT: usize = 512;
const MAX_PENDING_BYTES: usize = 64 * 1024;
const ACQUISITION: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub session_token: String,
    pub media_id: String,
    pub generation: u64,
    pub url: String,
    pub codec: String,
    pub profile: Option<String>,
}

impl Binding {
    fn validate(&self) -> Result<(), String> {
        let url = reqwest::Url::parse(&self.url).map_err(|error| error.to_string())?;
        let uuid = |value: &str| {
            value.len() == 36
                && value.bytes().enumerate().all(|(index, byte)| {
                    if [8, 13, 18, 23].contains(&index) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
                    }
                })
        };
        if !uuid(&self.session_token)
            || !uuid(&self.media_id)
            || url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || self.url.len() > MAX_TEXT
            || self.codec.len() > 32
            || self.profile.as_ref().is_some_and(|value| value.len() > 64)
            || url.query_pairs().collect::<Vec<_>>()
                != [(
                    "qb_playback_session".into(),
                    self.session_token.as_str().into(),
                )]
        {
            return Err("invalid primary playback diagnostic binding".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DecoderPath {
    Unknown,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DecoderSnapshot {
    pub owner: String,
    pub session_token: Option<String>,
    pub media_id: Option<String>,
    pub generation: Option<u64>,
    pub enabled: bool,
    pub status: DecoderPath,
    pub reason: String,
    pub expected_codec: Option<String>,
    pub expected_profile: Option<String>,
    pub expected_pixel_format: Option<String>,
    pub decoder_name: Option<String>,
    pub platform_decoder: Option<bool>,
    pub runtime: Option<String>,
    pub protocol: Option<String>,
    pub source: &'static str,
    pub associated: bool,
    pub transitions: VecDeque<String>,
}

impl DecoderSnapshot {
    fn new(owner: String) -> Self {
        Self {
            owner,
            session_token: None,
            media_id: None,
            generation: None,
            enabled: false,
            status: DecoderPath::Unknown,
            reason: "awaiting per-player decoder evidence".into(),
            expected_codec: None,
            expected_profile: None,
            expected_pixel_format: None,
            decoder_name: None,
            platform_decoder: None,
            runtime: None,
            protocol: None,
            source: "WebView2 CDP Media",
            associated: false,
            transitions: VecDeque::new(),
        }
    }
}

#[derive(Default)]
struct Candidate {
    url: Option<String>,
    node: Option<u64>,
    marker: Option<bool>,
    // CDP PlayerProperty identifies only a player, not a load. Never attach these
    // observations to a Binding's generation, even after a matching kLoad.
    player_properties: BTreeMap<String, String>,
}

struct Monitor {
    binding: Option<Binding>,
    candidates: BTreeMap<String, Candidate>,
    snapshot: DecoderSnapshot,
    acquired_at: Instant,
    invalidated: bool,
}

impl Monitor {
    fn new(owner: String) -> Self {
        Self {
            binding: None,
            candidates: BTreeMap::new(),
            snapshot: DecoderSnapshot::new(owner),
            acquired_at: Instant::now(),
            invalidated: false,
        }
    }

    fn bind(&mut self, binding: Binding) -> Result<(), String> {
        binding.validate()?;
        if let Some(current) = &self.binding {
            if binding.generation < current.generation || *current == binding {
                return Ok(());
            }
            if binding.generation == current.generation {
                return Err(
                    "conflicting playback diagnostic binding for current generation".into(),
                );
            }
        }
        self.snapshot.session_token = Some(binding.session_token.clone());
        self.snapshot.media_id = Some(binding.media_id.clone());
        self.snapshot.generation = Some(binding.generation);
        self.snapshot.expected_codec = Some(binding.codec.clone());
        self.snapshot.expected_profile = binding.profile.clone();
        self.snapshot.expected_pixel_format = (binding.codec == "h264"
            && binding.profile.as_deref() == Some("High"))
        .then(|| "8-bit 4:2:0 (canonical expectation)".into());
        self.snapshot.transitions.clear();
        self.binding = Some(binding);
        self.acquired_at = Instant::now();
        self.invalidated = false;
        self.set_path(
            DecoderPath::Unknown,
            "new media generation",
            None,
            None,
            false,
        );
        self.reconcile();
        Ok(())
    }

    fn set_path(
        &mut self,
        status: DecoderPath,
        reason: &str,
        decoder: Option<String>,
        platform: Option<bool>,
        associated: bool,
    ) {
        if self.snapshot.status != status || self.snapshot.decoder_name != decoder {
            if self.snapshot.transitions.len() == 16 {
                self.snapshot.transitions.pop_front();
            }
            self.snapshot.transitions.push_back(format!(
                "{status:?}: {}",
                decoder.as_deref().unwrap_or("unobserved")
            ));
        }
        self.snapshot.status = status;
        self.snapshot.reason = reason.chars().take(MAX_TEXT).collect();
        self.snapshot.decoder_name = decoder;
        self.snapshot.platform_decoder = platform;
        self.snapshot.associated = associated;
    }

    fn invalidate(&mut self, reason: &str) {
        self.invalidated = true;
        self.set_path(DecoderPath::Unknown, reason, None, None, false);
    }

    fn candidate(&mut self, id: &str) -> Option<&mut Candidate> {
        if id.len() > 128
            || (!self.candidates.contains_key(id) && self.candidates.len() >= MAX_PLAYERS)
        {
            self.invalidate("candidate player budget exceeded");
            return None;
        }
        Some(self.candidates.entry(id.to_owned()).or_default())
    }

    fn ingest(&mut self, kind: &str, text: &str) {
        if text.len() > MAX_PENDING_BYTES {
            self.invalidate("diagnostic event budget exceeded");
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            self.invalidate("malformed diagnostic event");
            return;
        };
        if kind == "Browser.getVersion" {
            self.snapshot.runtime = bounded(value.get("product"));
            self.snapshot.protocol = bounded(value.get("protocolVersion"));
            return;
        }
        if kind == "Media.playerCreated" {
            let player = &value["player"];
            if let Some(id) = player["playerId"].as_str()
                && let Some(candidate) = self.candidate(id)
            {
                candidate.node = player["domNodeId"].as_u64();
            }
        } else if kind == "Media.playersCreated" {
            if let Some(players) = value["players"].as_array() {
                for id in players.iter().filter_map(Value::as_str) {
                    self.candidate(id);
                }
            }
        } else if let Some(id) = value["playerId"].as_str() {
            match kind {
                "Media.playerPropertiesChanged" => {
                    if let Some(properties) = value["properties"].as_array() {
                        for property in properties {
                            let Some(name) = property["name"].as_str() else {
                                continue;
                            };
                            let value = bounded(property.get("value"));
                            let Some(candidate) = self.candidate(id) else {
                                return;
                            };
                            if name.len() > 128
                                || (!candidate.player_properties.contains_key(name)
                                    && candidate.player_properties.len() >= MAX_PROPERTIES)
                            {
                                self.invalidate("player property budget exceeded");
                                return;
                            }
                            if let Some(value) = value {
                                candidate.player_properties.insert(name.into(), value);
                            } else {
                                candidate.player_properties.remove(name);
                            }
                        }
                    }
                }
                "Media.playerEventsAdded" => {
                    if let Some(events) = value["events"].as_array() {
                        for event in events {
                            let Some(raw) = event["value"].as_str() else {
                                continue;
                            };
                            let Ok(event) = serde_json::from_str::<Value>(raw) else {
                                continue;
                            };
                            if event["event"].as_str() == Some("kLoad") {
                                let url = bounded(event.get("url"));
                                if let Some(candidate) = self.candidate(id) {
                                    // URL establishes player association only. Properties
                                    // may arrive out of order across this load boundary.
                                    candidate.url = url;
                                }
                            }
                            if event["event"].as_str() == Some("kWebMediaPlayerDestroyed") {
                                self.candidates.remove(id);
                            }
                        }
                    }
                }
                "Media.playerErrorsRaised" if self.matches(id) => {
                    self.invalidate("associated player reported a media error");
                }
                _ => {}
            }
        }
        self.reconcile();
    }

    fn matches(&self, id: &str) -> bool {
        let Some(binding) = &self.binding else {
            return false;
        };
        let Some(url) = self
            .candidates
            .get(id)
            .and_then(|player| player.url.as_deref())
        else {
            return false;
        };
        // Full URL equality verifies origin, path and the exact unique session query.
        reqwest::Url::parse(url).ok() == reqwest::Url::parse(&binding.url).ok()
    }

    fn reconcile(&mut self) {
        if self.invalidated || self.binding.is_none() {
            return;
        }
        let matched: Vec<_> = self
            .candidates
            .keys()
            .filter(|id| self.matches(id))
            .collect();
        if matched.len() != 1 {
            let reason = if matched.len() > 1 {
                "ambiguous current-player association"
            } else if self.acquired_at.elapsed() >= ACQUISITION {
                "decoder acquisition timed out after 5000 ms"
            } else {
                "awaiting associated player load URL"
            };
            self.set_path(DecoderPath::Unknown, reason, None, None, false);
            return;
        }
        let candidate = &self.candidates[matched[0]];
        if candidate.node.is_some() && candidate.marker != Some(true) {
            self.set_path(
                DecoderPath::Unknown,
                "awaiting primary DOM marker cross-check",
                None,
                None,
                false,
            );
            return;
        }
        // Neither PlayerProperty nor playerCreated carries a load identity;
        // playerCreated can also replay discovery of an already active player.
        // Matching a separate kLoad URL (or a DOM marker) cannot promote these
        // player-scoped values to current-load evidence. In particular, g1's
        // decoder property delivered after kLoad(g2) must never confirm g2.
        // A positive path requires a source that actually owns load provenance.
        self.set_path(
            DecoderPath::Unknown,
            "player associated; CDP decoder properties cannot prove current-load provenance",
            None,
            None,
            true,
        );
    }
}

fn bounded(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    (text.len() <= MAX_TEXT).then(|| text.to_owned())
}

#[cfg(windows)]
mod native;

#[tauri::command]
pub async fn open_playback_diagnostics(
    window: tauri::WebviewWindow,
    owner: String,
) -> Result<DecoderSnapshot, String> {
    if owner.len() != 36 {
        return Err("invalid diagnostic owner".into());
    }
    #[cfg(windows)]
    {
        native::open(window, owner).await
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        let mut snapshot = DecoderSnapshot::new(owner);
        snapshot.status = DecoderPath::Unsupported;
        snapshot.reason = "WebView2 diagnostics require Windows".into();
        Ok(snapshot)
    }
}

#[tauri::command]
pub async fn bind_playback_diagnostics(
    window: tauri::WebviewWindow,
    owner: String,
    binding: Binding,
) -> Result<DecoderSnapshot, String> {
    binding.validate()?;
    #[cfg(windows)]
    {
        native::bind(window, owner, binding).await
    }
    #[cfg(not(windows))]
    {
        let _ = binding;
        open_playback_diagnostics(window, owner).await
    }
}

#[tauri::command]
pub fn close_playback_diagnostics(
    window: tauri::WebviewWindow,
    owner: String,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::close(window, owner)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, owner);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn binding() -> Binding {
        let token = "11111111-2222-4333-8444-555555555555";
        Binding {
            session_token: token.into(),
            media_id: token.into(),
            generation: 1,
            url: format!("http://127.0.0.1:123/games/1/video.mp4?qb_playback_session={token}"),
            codec: "h264".into(),
            profile: Some("High".into()),
        }
    }
    fn load(monitor: &mut Monitor, id: &str, url: &str) {
        monitor.ingest("Media.playerEventsAdded", &json!({"playerId": id, "events": [{"value": json!({"event":"kLoad", "url":url}).to_string()}]}).to_string());
    }
    fn decoder(monitor: &mut Monitor, name: &str, platform: bool) {
        monitor.ingest("Media.playerPropertiesChanged", &json!({"playerId":"p", "properties":[{"name":"kVideoDecoderName", "value":name}, {"name":"kIsPlatformVideoDecoder", "value":platform.to_string()}]}).to_string());
    }
    fn assert_unproven_current_decoder(monitor: &Monitor) {
        assert!(monitor.snapshot.associated);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        assert_eq!(monitor.snapshot.decoder_name, None);
        assert_eq!(monitor.snapshot.platform_decoder, None);
        assert!(monitor.snapshot.reason.contains("current-load provenance"));
    }
    #[test]
    fn binding_is_monotonic_idempotent_and_rejects_same_generation_conflicts() {
        let mut monitor = Monitor::new("owner".into());
        let mut current = binding();
        current.generation = 2;
        current.session_token = current.session_token.replace("11111111", "99999999");
        current.url = current.url.replace("11111111", "99999999");
        monitor.bind(current.clone()).unwrap();
        let snapshot = monitor.snapshot.clone();
        let acquired_at = monitor.acquired_at;

        monitor.bind(binding()).unwrap();
        monitor.bind(current.clone()).unwrap();
        let mut conflict = binding();
        conflict.generation = 2;
        assert!(monitor.bind(conflict).is_err());
        let mut conflict = current.clone();
        conflict.profile = Some("Main".into());
        assert!(monitor.bind(conflict).is_err());
        assert_eq!(monitor.binding, Some(current.clone()));
        assert_eq!(monitor.snapshot, snapshot);
        assert_eq!(monitor.acquired_at, acquired_at);

        monitor.ingest(
            "Media.playerCreated",
            r#"{"player":{"playerId":"p","domNodeId":7}}"#,
        );
        load(&mut monitor, "p", &current.url);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        monitor.candidates.get_mut("p").unwrap().marker = Some(true);
        monitor.reconcile();
        assert_eq!(monitor.snapshot.generation, Some(2));
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        assert!(monitor.snapshot.associated);

        current.generation = 3;
        current.session_token = current.session_token.replace("99999999", "aaaaaaaa");
        current.url = current.url.replace("99999999", "aaaaaaaa");
        monitor.bind(current.clone()).unwrap();
        assert_eq!(monitor.binding, Some(current));
        assert_eq!(monitor.snapshot.generation, Some(3));
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        assert!(!monitor.snapshot.associated);
    }
    #[test]
    fn player_properties_do_not_prove_a_load_even_with_exact_association() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        load(&mut monitor, "p", &binding().url);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        for (name, platform) in [
            ("D3D11VideoDecoder", true),
            ("FFmpegVideoDecoder", false),
            ("MojoVideoDecoder", true),
        ] {
            decoder(&mut monitor, name, platform);
            assert_unproven_current_decoder(&monitor);
        }
        assert!(!monitor.snapshot.transitions.iter().any(|entry| {
            entry.contains("HardwareConfirmed") || entry.contains("SoftwareFallback")
        }));
    }
    #[test]
    fn old_token_wrong_origin_ambiguous_player_and_overflow_invalidate_evidence() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        load(&mut monitor, "p", &binding().url.replace(":123/", ":456/"));
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        load(&mut monitor, "p", &binding().url);
        load(&mut monitor, "other", &binding().url);
        assert!(!monitor.snapshot.associated);
        for index in 0..10 {
            load(&mut monitor, &index.to_string(), "http://other/");
        }
        assert!(monitor.invalidated);
        assert!(monitor.candidates.len() <= MAX_PLAYERS);
    }
    #[test]
    fn recovery_rotates_evidence_and_requires_current_marker_when_present() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        load(&mut monitor, "p", &binding().url);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        monitor.ingest(
            "Media.playerCreated",
            r#"{"player":{"playerId":"p","domNodeId":7}}"#,
        );
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        monitor.candidates.get_mut("p").unwrap().marker = Some(true);
        monitor.reconcile();
        assert_unproven_current_decoder(&monitor);
        let mut next = binding();
        next.session_token = next.session_token.replace("11111111", "99999999");
        next.url = next.url.replace("11111111", "99999999");
        next.generation += 1;
        monitor.bind(next).unwrap();
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
    }
    #[test]
    fn reordered_old_decoder_delivery_cannot_establish_a_new_generation() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        load(&mut monitor, "p", &binding().url);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_unproven_current_decoder(&monitor);

        let mut next = binding();
        next.generation = 2;
        next.session_token = next.session_token.replace("11111111", "99999999");
        next.url = next.url.replace("11111111", "99999999");
        monitor.bind(next.clone()).unwrap();
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        load(&mut monitor, "p", &next.url);
        assert_eq!(monitor.snapshot.generation, Some(2));
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        assert_eq!(monitor.snapshot.decoder_name, None);
        assert_eq!(monitor.snapshot.platform_decoder, None);
        // Chromium produced decoder(g1) BEFORE kLoad(g2), but delivers the
        // property AFTER kLoad(g2). There is no load ID on this property.
        monitor.ingest(
            "Media.playerPropertiesChanged",
            r#"{"playerId":"p","properties":[{"name":"kVideoDecoderName","value":"D3D11VideoDecoder"}]}"#,
        );
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        assert_eq!(monitor.snapshot.platform_decoder, None);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_unproven_current_decoder(&monitor);

        // Another property notification cannot fix the missing provenance, nor
        // can a repeated identical-URL load or late active-player discovery.
        load(&mut monitor, "p", &next.url);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        decoder(&mut monitor, "FFmpegVideoDecoder", false);
        assert_unproven_current_decoder(&monitor);
        monitor.ingest("Media.playersCreated", r#"{"players":["p"]}"#);
        assert_unproven_current_decoder(&monitor);
        monitor.ingest(
            "Media.playerCreated",
            r#"{"player":{"playerId":"p","domNodeId":7}}"#,
        );
        monitor.candidates.get_mut("p").unwrap().marker = Some(true);
        monitor.reconcile();
        assert_unproven_current_decoder(&monitor);
    }
    #[test]
    fn load_before_binding_cannot_relabel_player_properties_as_generation_evidence() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        load(&mut monitor, "p", &binding().url);
        decoder(&mut monitor, "D3D11VideoDecoder", true);

        let mut next = binding();
        next.generation = 2;
        next.session_token = next.session_token.replace("11111111", "99999999");
        next.url = next.url.replace("11111111", "99999999");
        load(&mut monitor, "p", &next.url);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        decoder(&mut monitor, "FFmpegVideoDecoder", false);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
        monitor.bind(next).unwrap();
        assert_eq!(monitor.snapshot.generation, Some(2));
        assert_unproven_current_decoder(&monitor);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_unproven_current_decoder(&monitor);
    }
    #[test]
    fn acquisition_deadline_and_dropped_events_are_not_success() {
        let mut monitor = Monitor::new("owner".into());
        monitor.bind(binding()).unwrap();
        monitor.acquired_at = Instant::now() - ACQUISITION;
        monitor.reconcile();
        assert!(monitor.snapshot.reason.contains("timed out"));
        load(&mut monitor, "p", &binding().url);
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        monitor.invalidate("diagnostic events dropped");
        decoder(&mut monitor, "D3D11VideoDecoder", true);
        assert_eq!(monitor.snapshot.status, DecoderPath::Unknown);
    }
}
