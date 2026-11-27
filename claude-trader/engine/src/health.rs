//! Is buy/sell data reaching the Token Picker? A coin scores 0 without it, whatever the coin does (SPEC 6, 9).
//! Pure logic so the warning is tested without a network.

/// Coins watched with no trade decoded for this long means the data is not arriving.
pub const NO_DATA_AFTER_MS: i64 = 120_000;

pub const NO_DATA_WARNING: &str = "No buy/sell data is arriving, so every coin scores 0.";
pub const FEED_DOWN_WARNING: &str = "The buy/sell data connection is down, so no trades can arrive and every coin scores 0.";

#[derive(Debug, Default)]
pub struct HealthTracker {
    /// When coins started being watched without a break (0 = none are watched right now).
    watching_since: i64,
}

impl HealthTracker {
    /// `watched`: coins we want trades for. `last_trade_ms`: newest decoded trade (0 = none ever).
    /// Returns the plain warning, or an empty string when all is well. Needs 2 minutes of watching with no trade at all.
    pub fn warn(&mut self, now: i64, watched: u32, trades_connected: bool, last_trade_ms: i64) -> String {
        if watched == 0 {
            self.watching_since = 0;
            return String::new();
        }
        if self.watching_since == 0 {
            self.watching_since = now;
        }
        let quiet_since = self.watching_since.max(last_trade_ms);
        if now - quiet_since < NO_DATA_AFTER_MS {
            return String::new();
        }
        if trades_connected { NO_DATA_WARNING.to_string() } else { FEED_DOWN_WARNING.to_string() }
    }
}

/// "3 s ago", "2 min ago", "none yet": the last trade as the health line shows it.
pub fn ago_text(now: i64, last_ms: i64) -> String {
    if last_ms <= 0 {
        return "none yet".into();
    }
    let s = ((now - last_ms) / 1000).max(0);
    if s < 90 { format!("{s} s ago") } else { format!("{} min ago", s / 60) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_000_000_000_000;

    #[test]
    fn no_warning_while_nothing_is_watched() {
        let mut h = HealthTracker::default();
        assert_eq!(h.warn(T, 0, true, 0), "");
        assert_eq!(h.warn(T + 600_000, 0, true, 0), "");
    }

    #[test]
    fn warns_after_two_minutes_of_watching_with_zero_trades() {
        let mut h = HealthTracker::default();
        assert_eq!(h.warn(T, 3, true, 0), "", "just started watching");
        assert_eq!(h.warn(T + 119_000, 3, true, 0), "", "not two minutes yet");
        assert_eq!(h.warn(T + 121_000, 3, true, 0), NO_DATA_WARNING);
        assert!(NO_DATA_WARNING.contains("every coin scores 0"));
    }

    #[test]
    fn a_decoded_trade_clears_the_warning_and_restarts_the_clock() {
        let mut h = HealthTracker::default();
        h.warn(T, 3, true, 0);
        assert_eq!(h.warn(T + 130_000, 3, true, 0), NO_DATA_WARNING);
        assert_eq!(h.warn(T + 131_000, 3, true, T + 130_500), "");
        assert_eq!(h.warn(T + 200_000, 3, true, T + 130_500), "");
        assert_eq!(h.warn(T + 252_000, 3, true, T + 130_500), NO_DATA_WARNING, "two quiet minutes again");
    }

    #[test]
    fn watching_that_stops_resets_the_clock() {
        let mut h = HealthTracker::default();
        h.warn(T, 2, true, 0);
        h.warn(T + 100_000, 0, true, 0);
        assert_eq!(h.warn(T + 200_000, 2, true, 0), "", "a new stretch of watching starts at zero");
    }

    #[test]
    fn a_dropped_connection_gets_its_own_wording() {
        let mut h = HealthTracker::default();
        h.warn(T, 2, false, 0);
        assert_eq!(h.warn(T + 130_000, 2, false, 0), FEED_DOWN_WARNING);
    }

    #[test]
    fn last_trade_text() {
        assert_eq!(ago_text(T, 0), "none yet");
        assert_eq!(ago_text(T, T - 4_000), "4 s ago");
        assert_eq!(ago_text(T, T - 300_000), "5 min ago");
    }
}
