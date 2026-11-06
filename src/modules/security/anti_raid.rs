use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use serenity::all::{Context, CreateEmbed};
use tracing::warn;

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
    state: Arc<Mutex<RaidState>>,
}

impl RaidTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn record_join(
        &self,
        ctx: &Context,
        mod_log_channel_id: Option<u64>,
    ) -> bool {
        // Decide under the lock, but never hold it across the HTTP call
        let (count, should_alert) = self.state.lock().await.register(Instant::now());

        if !should_alert {
            return count >= THRESHOLD;
        }

        warn!(target: "security::anti_raid", "Potential raid detected! {} joins in {} seconds", count, WINDOW.as_secs());

        let embed = CreateEmbed::new()
            .title("⚠️ High Join Spike / Potential Raid Detected")
            .color(0xF59E0B) // Amber
            .description(format!(
                "**{} members** joined within the last {} seconds.\nStaff should monitor new joins closely. \
                Further alerts are suppressed for {} seconds.",
                count, WINDOW.as_secs(), ALERT_COOLDOWN.as_secs()
            ))
            .timestamp(chrono::Utc::now());
        crate::util::send_mod_log(&ctx.http, mod_log_channel_id, embed).await;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_on_fifth_join_within_window() {
        let mut state = RaidState::default();
        let t0 = Instant::now();
        for i in 0..4 {
            assert_eq!(state.register(t0 + Duration::from_secs(i)), ((i + 1) as usize, false));
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
