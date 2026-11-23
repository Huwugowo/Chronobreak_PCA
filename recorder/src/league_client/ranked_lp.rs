//! Match-scoped ranked notification validation and a bounded recovery schedule.
use super::result::Binding;
use chronobreak_league_data::ranked_lp_queue;
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
    pub fn confirm(self, binding: &Binding) -> Option<i32> {
        if Some(self.game_id) != binding.game_id.parse().ok()
            || self.game_id == 0
            || ranked_lp_queue(binding.queue_id) != Some(self.queue_type.as_str())
            || self.change_reason != "GAME_END"
        {
            return None;
        }
        // The captures cover ordinary LP updates. Riot's post-game and leagues
        // plugins also handle these rank and placement transitions.
        matches!(
            self.notify_reason.as_str(),
            "LEAGUE_POINTS_UPDATE"
                | "LEAGUE_PROMOTED"
                | "LEAGUE_DEMOTED"
                | "LEAGUE_SEEDED"
                | "MINISERIES_START"
                | "MINISERIES_LOST"
                | "COMPLETED_PROVISIONALS"
        )
        .then_some(self.league_points_delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CAPTURED_LOSS: &str =
        include_str!("../../tests/fixtures/lcu_endgame/lp_change_notification.json");
    const CAPTURED_WIN: &str =
        include_str!("../../tests/fixtures/spike_gameplay/current-lp-change-notification.json");

    fn binding(game_id: &str) -> Binding {
        Binding {
            media_id: chronobreak_replay_time::MediaId::new_v4(),
            game_id: game_id.into(),
            puuid: "local".into(),
            riot_id: "Player#EUW".into(),
            map_id: 11,
            mode: "CLASSIC".into(),
            queue_id: 420,
        }
    }

    #[test]
    fn accepts_captured_lcu_ranked_loss_and_win_for_the_exact_game() {
        for (payload, game, other_game, delta) in [
            (CAPTURED_LOSS, "8006463758", "7998195590", -20),
            (CAPTURED_WIN, "7998195590", "8006463758", 19),
        ] {
            let confirm = |binding: &Binding| {
                serde_json::from_str::<Notification>(payload)
                    .unwrap()
                    .confirm(binding)
            };
            assert_eq!(confirm(&binding(game)), Some(delta));
            assert_eq!(confirm(&binding(other_game)), None);
            let mut flex = binding(game);
            flex.queue_id = 440;
            assert_eq!(confirm(&flex), None);
        }
    }

    #[test]
    fn validates_ranked_queue_and_postgame_reason_pair_without_recalculating_lp() {
        let base: serde_json::Value = serde_json::from_str(CAPTURED_LOSS).unwrap();
        let solo = binding("8006463758");
        let check = |binding: &Binding, queue, cause, notice, delta| {
            let mut payload = base.clone();
            payload["queueType"] = json!(queue);
            payload["changeReason"] = json!(cause);
            payload["notifyReason"] = json!(notice);
            payload["leaguePointsDelta"] = json!(delta);
            serde_json::from_value::<Notification>(payload)
                .unwrap()
                .confirm(binding)
        };
        let queue = "RANKED_SOLO_5x5";
        for (notice, delta) in [
            ("LEAGUE_PROMOTED", 18),
            ("LEAGUE_DEMOTED", -27),
            ("LEAGUE_SEEDED", 0),
            ("LEAGUE_POINTS_UPDATE", -2),
        ] {
            assert_eq!(check(&solo, queue, "GAME_END", notice, delta), Some(delta));
        }
        let mut flex = solo.clone();
        flex.queue_id = 440;
        assert_eq!(
            check(
                &flex,
                "RANKED_FLEX_SR",
                "GAME_END",
                "LEAGUE_POINTS_UPDATE",
                16
            ),
            Some(16)
        );
        let mut unranked = solo.clone();
        unranked.queue_id = 400;
        assert_eq!(
            check(&unranked, queue, "GAME_END", "LEAGUE_POINTS_UPDATE", 16),
            None
        );

        for (cause, notice) in [
            ("INACTIVITY", "LEAGUE_DEMOTED"),
            ("", "LEAGUE_PROMOTED"),
            ("NONE", "LEAGUE_POINTS_UPDATE"),
            ("GAME_PLAYED", "LEAGUE_POINTS_UPDATE"),
            ("GAME_END", "LEAGUE_REWARD"),
            ("GAME_END", "LEAGUE_DECAY_WARNING"),
            ("GAME_END", "LEAGUE_POINTS_CHANGED"),
            ("GAME_END", "LEAGUE_POINTS_UPDATE_SKIPPED_FULL_PARTY"),
        ] {
            assert_eq!(check(&solo, queue, cause, notice, -20), None);
        }
        for field in [
            "gameId",
            "queueType",
            "changeReason",
            "notifyReason",
            "leaguePointsDelta",
        ] {
            let mut incomplete = base.clone();
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Notification>(incomplete).is_err(),
                "{field}"
            );
        }
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
