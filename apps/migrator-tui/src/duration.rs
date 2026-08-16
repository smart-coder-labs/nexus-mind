//! Human-friendly duration formatting for run summaries and logs.

/// Format a millisecond duration the way a person would say it.
///
/// - under 1s: `"<ms>ms"`
/// - under 60s: `"<s.s>s"` (one decimal)
/// - under 60m: `"<m>m <s>s"`
/// - otherwise: `"<h>h <m>m"`
pub fn humanize_duration(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        return format!("{:.1}s", ms as f64 / 1_000.0);
    }
    let total_secs = ms / 1_000;
    if total_secs < 3_600 {
        return format!("{}m {}s", total_secs / 60, total_secs % 60);
    }
    format!("{}h {}m", total_secs / 3_600, (total_secs % 3_600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_a_second_shows_milliseconds() {
        assert_eq!(humanize_duration(42), "42ms");
        assert_eq!(humanize_duration(0), "0ms");
        assert_eq!(humanize_duration(999), "999ms");
    }

    #[test]
    fn under_a_minute_shows_seconds_with_one_decimal() {
        assert_eq!(humanize_duration(1_500), "1.5s");
        assert_eq!(humanize_duration(1_000), "1.0s");
        assert_eq!(humanize_duration(59_999), "60.0s");
    }

    #[test]
    fn under_an_hour_shows_minutes_and_seconds() {
        assert_eq!(humanize_duration(90_000), "1m 30s");
        assert_eq!(humanize_duration(60_000), "1m 0s");
        assert_eq!(humanize_duration(3_599_000), "59m 59s");
    }

    #[test]
    fn an_hour_or_more_shows_hours_and_minutes() {
        assert_eq!(humanize_duration(3_660_000), "1h 1m");
        assert_eq!(humanize_duration(3_600_000), "1h 0m");
        assert_eq!(humanize_duration(7_265_000), "2h 1m");
    }
}
