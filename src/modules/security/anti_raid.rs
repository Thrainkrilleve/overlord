use serenity::all::CreateEmbed;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const WINDOW: Duration = Duration::from_secs(10);
const THRESHOLD: usize = 5; // >= 5 joins within 10s triggers raid mode warning
const ALERT_COOLDOWN: Duration = Duration::from_secs(60); // one alert per minute during a sustained raid

#[derive(Default)]
struct RaidState {
    joins: Vec<Instant>,
    last_alert: Option<Instant>,
}

impl RaidState {
    /// Record a join at `now`. Returns the joins inside the window and whether to post an alert.
    fn register(&mut self, now: Instant) -> (usize, bool) {
        self.joins.retain(|&time| now.duration_since(time) < WINDOW);
        self.joins.push(now);

        let count = self.joins.len();
        let cooled_down = self
            .last_alert
            .is_none_or(|last| now.duration_since(last) >= ALERT_COOLDOWN);
        let should_alert = count >= THRESHOLD && cooled_down;
        if should_alert {
            self.last_alert = Some(now);
        }
        (count, should_alert)
    }
}

#[derive(Clone, Default)]
pub struct RaidTracker {
    guilds: Arc<Mutex<HashMap<u64, RaidState>>>,
}

impl RaidTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a join in one guild. Returns `(joins_in_window, should_alert)`.
    /// The window is not shared with any other guild.
    pub async fn record_join(&self, guild_id: u64) -> (usize, bool) {
        let mut guilds = self.guilds.lock().await;
        guilds.entry(guild_id).or_default().register(Instant::now())
    }
}

pub fn raid_alert_embed(count: usize) -> CreateEmbed {
    CreateEmbed::new()
        .title("⚠️ High Join Spike / Potential Raid Detected")
        .color(0xF59E0B) // Amber
        .description(format!(
            "**{} members** joined within the last {} seconds.\nStaff should monitor new joins closely. \
            Further alerts are suppressed for {} seconds.",
            count,
            WINDOW.as_secs(),
            ALERT_COOLDOWN.as_secs()
        ))
        .timestamp(chrono::Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_on_fifth_join_within_window() {
        let mut state = RaidState::default();
        let t0 = Instant::now();
        for i in 0..4 {
            assert_eq!(
                state.register(t0 + Duration::from_secs(i)),
                ((i + 1) as usize, false)
            );
        }
        assert_eq!(state.register(t0 + Duration::from_secs(4)), (5, true));
    }

    #[test]
    fn sustained_raid_alerts_once_per_cooldown() {
        let mut state = RaidState::default();
        let t0 = Instant::now();
        let mut alerts = 0;
        // 100 joins, one every 200ms, over 20 seconds
        for i in 0..100 {
            if state.register(t0 + Duration::from_millis(i * 200)).1 {
                alerts += 1;
            }
        }
        assert_eq!(alerts, 1);
        // After the cooldown, a still-active raid alerts again
        let later = t0 + ALERT_COOLDOWN + Duration::from_secs(1);
        for i in 0..5 {
            state.register(later + Duration::from_millis(i * 100));
        }
        assert!(state.last_alert.unwrap() >= later);
    }

    #[tokio::test]
    async fn guilds_do_not_share_a_join_window() {
        let tracker = RaidTracker::new();
        for _ in 0..4 {
            let (_, alert) = tracker.record_join(1).await;
            assert!(!alert);
        }
        for _ in 0..4 {
            let (_, alert) = tracker.record_join(2).await;
            assert!(!alert);
        }

        let (count, alert) = tracker.record_join(1).await;
        assert_eq!(count, 5);
        assert!(alert);

        // Guild 2 is still under the threshold, and guild 1's alert does not cool it down.
        let (count, alert) = tracker.record_join(2).await;
        assert_eq!(count, 5);
        assert!(alert);

        let (count, _) = tracker.record_join(3).await;
        assert_eq!(count, 1);
    }

    #[test]
    fn slow_joins_never_alert() {
        let mut state = RaidState::default();
        let t0 = Instant::now();
        for i in 0..50 {
            // one join every 3 seconds: at most 4 inside any 10s window
            assert!(!state.register(t0 + Duration::from_secs(i * 3)).1);
        }
    }
}
