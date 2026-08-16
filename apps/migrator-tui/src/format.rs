//! Small display-formatting helpers shared across screens.

/// Formats a millisecond duration the way a human would say it, picking the
/// coarsest unit that still reads precisely: milliseconds under a second,
/// one-decimal seconds under a minute, minutes+seconds under an hour, and
/// hours+minutes beyond that.
pub fn humanize_duration(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        return format!("{:.1}s", ms as f64 / 1_000.0);
    }
    let total_secs = ms / 1_000;
    if total_secs < 3_600 {
        let m = total_secs / 60;
        let s = total_secs % 60;
        return format!("{m}m {s}s");
    }
    let h = total_secs / 3_600;
    let m = (total_secs % 3_600) / 60;
    format!("{h}h {m}m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_a_second_shows_whole_milliseconds() {
        assert_eq!(humanize_duration(42), "42ms");
        assert_eq!(humanize_duration(0), "0ms");
        assert_eq!(humanize_duration(999), "999ms");
    }

    #[test]
    fn under_a_minute_shows_one_decimal_seconds() {
        assert_eq!(humanize_duration(1_500), "1.5s");
        assert_eq!(humanize_duration(1_000), "1.0s");
        assert_eq!(humanize_duration(59_999), "60.0s");
    }

    #[test]
    fn under_an_hour_shows_minutes_and_seconds() {
        assert_eq!(humanize_duration(60_000), "1m 0s");
        assert_eq!(humanize_duration(90_000), "1m 30s");
        assert_eq!(humanize_duration(3_599_000), "59m 59s");
    }

    #[test]
    fn an_hour_or_more_shows_hours_and_minutes() {
        assert_eq!(humanize_duration(3_600_000), "1h 0m");
        assert_eq!(humanize_duration(3_660_000), "1h 1m");
        assert_eq!(humanize_duration(7_384_000), "2h 3m");
    }
}
