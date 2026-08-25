use anyhow::Result;
use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::str::FromStr;

pub fn next_occurrence(
    kind: &str,
    expression: Option<&str>,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    match kind {
        "manual" => Ok(None),
        "interval" => {
            let minutes: i64 = expression
                .ok_or_else(|| anyhow::anyhow!("schedule_expression_required"))?
                .parse()?;
            if minutes < 15 {
                anyhow::bail!("interval_too_short");
            }
            Ok(Some(after + Duration::minutes(minutes)))
        }
        "daily" => {
            let tz = Tz::from_str(timezone).map_err(|_| anyhow::anyhow!("invalid_timezone"))?;
            let time = NaiveTime::parse_from_str(
                expression.ok_or_else(|| anyhow::anyhow!("schedule_expression_required"))?,
                "%H:%M",
            )
            .map_err(|_| anyhow::anyhow!("invalid_daily_time"))?;
            let local_after = after.with_timezone(&tz);
            let mut date = local_after.date_naive();
            if local_after.time() >= time {
                date += Duration::days(1);
            }
            for _ in 0..370 {
                let candidate = date.and_time(time);
                match tz.from_local_datetime(&candidate) {
                    chrono::LocalResult::Single(value) => {
                        return Ok(Some(value.with_timezone(&Utc)))
                    }
                    chrono::LocalResult::Ambiguous(first, second) => {
                        return Ok(Some(first.min(second).with_timezone(&Utc)));
                    }
                    chrono::LocalResult::None => {
                        // Spring-forward gap: run at the next valid local minute.
                        for minute in 1..=180 {
                            let shifted = candidate + Duration::minutes(minute);
                            if let chrono::LocalResult::Single(value) =
                                tz.from_local_datetime(&shifted)
                            {
                                return Ok(Some(value.with_timezone(&Utc)));
                            }
                        }
                    }
                }
                date += Duration::days(1);
            }
            anyhow::bail!("schedule_unresolvable")
        }
        _ => anyhow::bail!("invalid_schedule_kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bogota_daily_six_is_eleven_utc() {
        let after = DateTime::parse_from_rfc3339("2026-08-18T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_occurrence("daily", Some("06:00"), "America/Bogota", after)
            .unwrap()
            .unwrap();
        assert_eq!(next.to_rfc3339(), "2026-08-18T11:00:00+00:00");
    }

    #[test]
    fn interval_rejects_less_than_fifteen_minutes() {
        assert!(next_occurrence("interval", Some("5"), "UTC", Utc::now()).is_err());
    }

    #[test]
    fn spring_forward_gap_uses_next_valid_local_minute() {
        let after = DateTime::parse_from_rfc3339("2026-03-08T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_occurrence("daily", Some("02:30"), "America/New_York", after)
            .unwrap()
            .unwrap();
        assert_eq!(next.to_rfc3339(), "2026-03-08T07:00:00+00:00");
    }

    #[test]
    fn fall_back_ambiguity_chooses_first_instant() {
        let after = DateTime::parse_from_rfc3339("2026-11-01T04:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = next_occurrence("daily", Some("01:30"), "America/New_York", after)
            .unwrap()
            .unwrap();
        assert_eq!(next.to_rfc3339(), "2026-11-01T05:30:00+00:00");
    }
}
