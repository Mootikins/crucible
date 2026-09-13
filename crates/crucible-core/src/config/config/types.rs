//! Schedule entries from config.

use serde::{Deserialize, Serialize};

use crate::config::serde_helpers::default_true;

/// A declarative schedule entry from `[[schedules]]` in config.
///
/// Each entry runs a Lua snippet at a fixed interval via `cru.schedule`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleEntry {
    /// Human-readable name for logging.
    pub name: String,
    /// Interval string: "1h", "30m", "5s", "1d", or bare seconds.
    pub every: String,
    /// Lua code to execute, optionally prefixed with "lua:".
    pub action: String,
    /// Whether this schedule is active (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Parse a human-readable duration string like "1h", "30m", "5s", "1d".
///
/// Supports suffixes `d` (days), `h` (hours), `m` (minutes), `s` (seconds),
/// or a bare number treated as seconds.
pub fn parse_duration_string(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('d') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 86400));
    }
    if let Some(n) = s.strip_suffix('h') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 3600));
    }
    if let Some(n) = s.strip_suffix('m') {
        return n
            .parse::<u64>()
            .ok()
            .map(|n| std::time::Duration::from_secs(n * 60));
    }
    if let Some(n) = s.strip_suffix('s') {
        return n.parse::<u64>().ok().map(std::time::Duration::from_secs);
    }
    s.parse::<u64>().ok().map(std::time::Duration::from_secs)
}

#[cfg(test)]
mod duration_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parse_seconds_suffix() {
        assert_eq!(parse_duration_string("5s"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_minutes_suffix() {
        assert_eq!(
            parse_duration_string("30m"),
            Some(Duration::from_secs(1800))
        );
    }

    #[test]
    fn parse_hours_suffix() {
        assert_eq!(parse_duration_string("1h"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn parse_days_suffix() {
        assert_eq!(
            parse_duration_string("1d"),
            Some(Duration::from_secs(86400))
        );
    }

    #[test]
    fn parse_bare_number_as_seconds() {
        assert_eq!(parse_duration_string("120"), Some(Duration::from_secs(120)));
    }

    #[test]
    fn parse_with_whitespace() {
        assert_eq!(
            parse_duration_string("  2h  "),
            Some(Duration::from_secs(7200))
        );
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_duration_string("abc"), None);
        assert_eq!(parse_duration_string(""), None);
        assert_eq!(parse_duration_string("5x"), None);
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_duration_string("0s"), Some(Duration::from_secs(0)));
        assert_eq!(parse_duration_string("0"), Some(Duration::from_secs(0)));
    }
}
