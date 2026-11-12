//! Deterministic contact policy; UTC elapsed time and local wall time are distinct.
use chrono::{DateTime, Duration, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactWait {
    Paused,
    Busy,
    QuietHours,
    DailyLimit,
    MinimumInterval,
    ClockRollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ContactReceipt {
    Delivered { message_id: Uuid },
    Deferred { reason: ContactWait },
    Cancelled { reason: String },
}

pub(crate) struct ContactPolicy {
    pub timezone: Tz,
    pub quiet_start: NaiveTime,
    pub quiet_end: NaiveTime,
    pub daily_limit: i64,
    pub minimum_interval: Duration,
}
impl ContactPolicy {
    pub(crate) fn wait(
        &self,
        now: DateTime<Utc>,
        used: i64,
        last: Option<DateTime<Utc>>,
    ) -> Option<ContactWait> {
        if last.is_some_and(|last| last > now) {
            return Some(ContactWait::ClockRollback);
        }
        let time = now.with_timezone(&self.timezone).time();
        let quiet = if self.quiet_start < self.quiet_end {
            time >= self.quiet_start && time < self.quiet_end
        } else if self.quiet_start > self.quiet_end {
            time >= self.quiet_start || time < self.quiet_end
        } else {
            false
        };
        if quiet {
            Some(ContactWait::QuietHours)
        } else if used >= self.daily_limit {
            Some(ContactWait::DailyLimit)
        } else if last.is_some_and(|last| now - last < self.minimum_interval) {
            Some(ContactWait::MinimumInterval)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    fn policy() -> ContactPolicy {
        ContactPolicy {
            timezone: chrono_tz::America::New_York,
            quiet_start: "22:00".parse().unwrap(),
            quiet_end: "08:00".parse().unwrap(),
            daily_limit: 2,
            minimum_interval: Duration::hours(4),
        }
    }
    #[test]
    fn local_quiet_boundaries_and_disabled_or_daytime_ranges() {
        let mut p = policy();
        for (utc, quiet) in [
            ("2026-11-11T02:59:59Z", false),
            ("2026-11-11T03:00:00Z", true),
            ("2026-11-11T12:59:59Z", true),
            ("2026-11-11T13:00:00Z", false),
            ("2026-03-08T06:30:00Z", true),
            ("2026-03-08T07:30:00Z", true),
            ("2026-11-01T05:30:00Z", true),
            ("2026-11-01T06:30:00Z", true),
        ] {
            assert_eq!(
                p.wait(time(utc), 0, None),
                quiet.then_some(ContactWait::QuietHours)
            );
        }
        p.quiet_end = p.quiet_start;
        assert_eq!(p.wait(time("2026-11-01T06:30:00Z"), 0, None), None);
        p.quiet_start = "12:00".parse().unwrap();
        p.quiet_end = "13:00".parse().unwrap();
        assert_eq!(
            p.wait(time("2026-11-11T17:00:00Z"), 0, None),
            Some(ContactWait::QuietHours)
        );
        assert_eq!(p.wait(time("2026-11-11T18:00:00Z"), 0, None), None);
    }
    #[test]
    fn elapsed_interval_survives_dst_and_clock_rollback() {
        let mut p = policy();
        p.quiet_end = p.quiet_start;
        for (before, after) in [
            ("2026-03-08T06:00:00Z", "2026-03-08T10:00:00Z"),
            ("2026-11-01T04:00:00Z", "2026-11-01T08:00:00Z"),
        ] {
            assert_eq!(
                p.wait(time(after) - Duration::seconds(1), 1, Some(time(before))),
                Some(ContactWait::MinimumInterval)
            );
            assert_eq!(p.wait(time(after), 1, Some(time(before))), None);
            assert_eq!(
                p.wait(time(after), 2, Some(time(before))),
                Some(ContactWait::DailyLimit)
            );
            assert_eq!(
                p.wait(time(before) - Duration::seconds(1), 0, Some(time(before))),
                Some(ContactWait::ClockRollback)
            );
        }
    }
}
