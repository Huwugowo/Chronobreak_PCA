//! Match-scoped ranked notification validation and a bounded recovery schedule.
use super::result::Binding;
use chronobreak_league_data::{Outcome, ResultFile, ranked_lp_queue};
use serde::{Deserialize, Serialize};

const FETCH_OFFSETS_MS: [i64; 4] = [0, 5_000, 20_000, 60_000];
const FETCH_WINDOW_MS: i64 = 90_000;
pub(super) const INSTALL_DELAYS_MS: [i64; 5] = [1_000, 3_000, 10_000, 30_000, 90_000];

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub(super) struct Acquisition {
    pub closed_ms: Option<i64>,
    next_slot: usize,
    pub delta: Option<i32>,
}
impl Acquisition {
    pub fn due(self, now: i64) -> bool {
        self.delta.is_none()
            && self.closed_ms.is_some_and(|closed| {
                (0..FETCH_WINDOW_MS).contains(&now.saturating_sub(closed))
                    && FETCH_OFFSETS_MS
                        .get(self.next_slot)
                        .is_some_and(|offset| now.saturating_sub(closed) >= *offset)
            })
    }

    pub fn reserve(&mut self, now: i64) {
        if let Some(closed) = self.closed_ms {
            self.next_slot =
                FETCH_OFFSETS_MS.partition_point(|offset| now.saturating_sub(closed) >= *offset);
        }
    }
}

pub(super) fn same_match(a: &Binding, b: &Binding) -> bool {
    a.game_id == b.game_id && a.queue_id == b.queue_id && a.puuid == b.puuid
}

pub(super) fn completed_ranked_match(binding: &Binding, result: Option<&ResultFile>) -> bool {
    result.is_some_and(|result| {
        result.valid_for(&binding.media_id, &binding.game_id)
            && result.queue_id == Some(u64::from(binding.queue_id))
            && result.outcome != Outcome::Unknown
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Notification {
    game_id: u64,
    queue_type: String,
    league_points_delta: i32,
    change_reason: String,
    notify_reason: String,
}
impl Notification {
    pub fn confirm(self, binding: &Binding, completed: impl FnOnce() -> bool) -> Option<i32> {
        if Some(self.game_id) != binding.game_id.parse().ok()
            || self.game_id == 0
            || ranked_lp_queue(binding.queue_id) != Some(self.queue_type.as_str())
        {
            return None;
        }
        let postgame_notice = matches!(
            self.notify_reason.as_str(),
            "LEAGUE_PROMOTED"
                | "LEAGUE_DEMOTED"
                | "LEAGUE_SEEDED"
                | "MINISERIES_START"
                | "MINISERIES_LOST"
                | "COMPLETED_PROVISIONALS"
        );
        let ordinary_notice = matches!(
            self.notify_reason.as_str(),
            "" | "NONE" | "LP_CHANGED" | "LEAGUE_POINTS_CHANGED" | "GAME" | "GAME_PLAYED"
        );
        let match_cause = matches!(
            self.change_reason.as_str(),
            "GAME" | "GAME_PLAYED" | "MATCH_COMPLETE" | "MATCH_COMPLETED"
        );
        let neutral_cause = matches!(self.change_reason.as_str(), "" | "NONE");
        ((postgame_notice && (match_cause || neutral_cause))
            || (ordinary_notice && (match_cause || (neutral_cause && completed()))))
        .then_some(self.league_points_delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_postgame_notification_identity_and_cause() {
        let binding = Binding {
            media_id: chronobreak_replay_time::MediaId::new_v4(),
            game_id: "7891230123".into(),
            puuid: "local".into(),
            riot_id: "Player#EUW".into(),
            map_id: 11,
            mode: "CLASSIC".into(),
            queue_id: 420,
        };
        let check = |game, queue, cause, reason, delta| {
            serde_json::from_value::<Notification>(json!({
                "gameId":game,"queueType":queue,"changeReason":cause,
                "notifyReason":reason,"leaguePointsDelta":delta,"displayType":"NONE",
                "previousLeaguePoints":90,"leaguePoints":4,
            }))
            .unwrap()
            .confirm(&binding, || false)
        };
        let solo = "RANKED_SOLO_5x5";
        for (cause, reason, delta) in [
            ("GAME_PLAYED", "NONE", 24),
            ("GAME", "LEAGUE_PROMOTED", 18),
            ("", "LEAGUE_DEMOTED", -27),
            ("GAME", "NONE", 0),
            ("GAME_PLAYED", "LEAGUE_POINTS_CHANGED", -2),
        ] {
            assert_eq!(
                check(7891230123_i64, solo, cause, reason, delta),
                Some(delta)
            );
        }
        assert_eq!(check(7891230122, solo, "GAME", "NONE", 25), None);
        assert_eq!(
            check(7891230123, "RANKED_FLEX_SR", "GAME", "NONE", 25),
            None
        );
        assert_eq!(
            check(7891230123, solo, "INACTIVITY", "LEAGUE_DEMOTED", -50),
            None
        );
        assert_eq!(
            check(7891230123, solo, "ADMIN", "LEAGUE_PROMOTED", 25),
            None
        );
        assert_eq!(check(7891230123, solo, "GAME", "LEAGUE_REWARD", 25), None);
        assert_eq!(check(7891230123, solo, "", "NONE", 25), None);
        assert!(
            serde_json::from_value::<Notification>(json!({
                "gameId":7891230123_i64,"queueType":solo,"changeReason":"GAME","notifyReason":"NONE"
            }))
            .is_err()
        );
    }

    #[test]
    fn reserves_at_most_four_requests_and_does_not_catch_up_after_restart() {
        let mut state = Acquisition {
            closed_ms: Some(1000),
            ..Acquisition::default()
        };
        for (due, premature) in [(1000, 999), (6000, 5999), (21000, 20999), (61000, 60999)] {
            assert!(!state.due(premature));
            assert!(state.due(due));
            state.reserve(due);
            assert!(!state.due(due));
        }
        assert!(!state.due(90000));
        let mut state = Acquisition {
            closed_ms: Some(1000),
            ..Acquisition::default()
        };
        assert!(state.due(40000));
        state.reserve(40000);
        let restored: Acquisition =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(!restored.due(40000));
        assert!(restored.due(61000));
        assert!(!restored.due(91000));
        assert!(!Acquisition::default().due(1000));
    }
}
