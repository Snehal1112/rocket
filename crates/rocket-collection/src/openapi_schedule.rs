//! Pure rules for when the next OpenAPI spec check is due. No clock, no I/O.

use chrono::{DateTime, Duration, Utc};

use crate::openapi_source::SyncInterval;

const BACKOFF_BASE_SECS: u64 = 15 * 60;
const BACKOFF_CAP_SECS: u64 = 6 * 3600;
const RETRY_AFTER_MIN_SECS: u64 = 60;
const RETRY_AFTER_MAX_SECS: u64 = 24 * 3600;
const JITTER_CAP_SECS: u64 = 300;

/// What the rule needs to know about one collection.
pub struct SchedulePlan<'a> {
    pub interval: SyncInterval,
    pub source_id: &'a str,
    pub last_checked_at: Option<&'a str>,
    pub last_failed: bool,
    pub consecutive_failures: u32,
    pub not_before: Option<&'a str>,
    /// The current time, used to tell a last check in the future from a real one.
    pub now: DateTime<Utc>,
}

fn parse(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// A stable offset from the source id: up to 10% of the interval, at most 5 minutes.
pub fn jitter_secs(source_id: &str, interval: SyncInterval) -> u64 {
    let Some(secs) = interval.as_secs() else {
        return 0;
    };
    let cap = (secs / 10).min(JITTER_CAP_SECS);
    if cap == 0 {
        return 0;
    }
    // FNV-1a, so the offset is the same on every run and platform.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash % cap
}

/// Keeps a `Retry-After` value within one minute and 24 hours.
pub fn clamp_retry_after(secs: u64) -> u64 {
    secs.clamp(RETRY_AFTER_MIN_SECS, RETRY_AFTER_MAX_SECS)
}

/// The delay after a failed check. The ladder starts at 15 minutes and doubles for each
/// further failure, up to 6 hours and never above the interval. A `Retry-After` can only raise it.
pub fn backoff_secs(failures: u32, interval: SyncInterval, retry_after_secs: Option<u64>) -> u64 {
    let steps = failures.saturating_sub(1).min(16);
    let mut cap = BACKOFF_CAP_SECS;
    if let Some(secs) = interval.as_secs() {
        cap = cap.min(secs);
    }
    let ladder = BACKOFF_BASE_SECS.saturating_mul(1u64 << steps).min(cap);
    match retry_after_secs {
        Some(secs) => ladder.max(clamp_retry_after(secs)),
        None => ladder,
    }
}

fn add_secs(time: DateTime<Utc>, secs: u64) -> DateTime<Utc> {
    // An overflow means "far away", never "now".
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    Duration::try_seconds(secs)
        .and_then(|d| time.checked_add_signed(d))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// When the next check is due. `None` when the interval is off. A missing, unreadable or
/// future last check is due at once.
pub fn next_check_at(plan: &SchedulePlan) -> Option<DateTime<Utc>> {
    let interval_secs = plan.interval.as_secs()?;
    let last = plan
        .last_checked_at
        .and_then(parse)
        .filter(|last| *last <= plan.now);
    let due = match last {
        None => plan.now,
        Some(last) if plan.last_failed => {
            let failures = plan.consecutive_failures.max(1);
            add_secs(last, backoff_secs(failures, plan.interval, None))
        }
        Some(last) => {
            let delay = interval_secs + jitter_secs(plan.source_id, plan.interval);
            add_secs(last, delay)
        }
    };
    match plan.not_before.and_then(parse) {
        Some(floor) if floor > due => Some(floor),
        _ => Some(due),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        parse("2026-10-26T12:00:00Z").expect("date")
    }

    fn ago(secs: i64) -> String {
        (now() - Duration::seconds(secs)).to_rfc3339()
    }

    fn plan<'a>(interval: SyncInterval, last: Option<&'a str>) -> SchedulePlan<'a> {
        SchedulePlan {
            interval,
            source_id: "src-1",
            last_checked_at: last,
            last_failed: false,
            consecutive_failures: 0,
            not_before: None,
            now: now(),
        }
    }

    #[test]
    fn off_is_never_due() {
        assert_eq!(next_check_at(&plan(SyncInterval::Off, None)), None);
        let old = ago(900 * 86_400);
        assert_eq!(next_check_at(&plan(SyncInterval::Off, Some(&old))), None);
    }

    #[test]
    fn no_last_check_is_due_now() {
        assert_eq!(next_check_at(&plan(SyncInterval::Daily, None)), Some(now()));
    }

    #[test]
    fn unreadable_or_future_last_check_is_due_now() {
        let future = (now() + Duration::hours(2)).to_rfc3339();
        for last in ["not a date", future.as_str()] {
            let mut p = plan(SyncInterval::Daily, Some(last));
            assert_eq!(next_check_at(&p), Some(now()), "{last}");
            p.last_failed = true;
            assert_eq!(next_check_at(&p), Some(now()), "{last}");
        }
    }

    #[test]
    fn healthy_check_waits_interval_plus_jitter() {
        let last = ago(10);
        let p = plan(SyncInterval::Daily, Some(&last));
        let jitter = i64::try_from(jitter_secs("src-1", SyncInterval::Daily)).expect("fits");
        let expected = now() - Duration::seconds(10) + Duration::seconds(86_400 + jitter);
        assert_eq!(next_check_at(&p), Some(expected));
    }

    #[test]
    fn jitter_is_stable_and_below_the_cap() {
        for interval in [
            SyncInterval::Hourly,
            SyncInterval::Daily,
            SyncInterval::Monthly,
        ] {
            for id in ["a", "b", "src-1", "0b9f-1"] {
                let j = jitter_secs(id, interval);
                assert_eq!(j, jitter_secs(id, interval));
                assert!(j < 300, "{j}");
            }
        }
        assert_eq!(jitter_secs("a", SyncInterval::Off), 0);
        let values: std::collections::HashSet<u64> = (0..20)
            .map(|i| jitter_secs(&format!("id-{i}"), SyncInterval::Daily))
            .collect();
        assert!(values.len() > 10);
    }

    #[test]
    fn short_intervals_wait_their_length_plus_a_small_jitter() {
        for (interval, secs) in [
            (SyncInterval::Min5, 300i64),
            (SyncInterval::Min15, 900),
            (SyncInterval::Min30, 1800),
        ] {
            for id in ["a", "b", "src-1", "0b9f-1", "zz"] {
                let last = ago(0);
                let mut p = plan(interval, Some(&last));
                p.source_id = id;
                let wait = (next_check_at(&p).expect("due") - now()).num_seconds();
                assert!(wait >= secs, "{interval:?} {wait}");
                assert!(wait * 10 <= secs * 11, "{interval:?} {id} {wait}");
            }
        }
    }

    #[test]
    fn failure_follows_the_backoff_ladder() {
        let d = SyncInterval::Monthly;
        let ladder: Vec<u64> = (1..=7).map(|n| backoff_secs(n, d, None)).collect();
        assert_eq!(ladder, vec![900, 1800, 3600, 7200, 14_400, 21_600, 21_600]);
        assert_eq!(backoff_secs(1, SyncInterval::Hourly, None), 900);
        assert_eq!(backoff_secs(3, SyncInterval::Hourly, None), 3600);
        assert_eq!(backoff_secs(9, SyncInterval::Hourly, None), 3600);
        assert_eq!(backoff_secs(u32::MAX, d, None), 21_600);
        let last = ago(0);
        let mut p = plan(SyncInterval::Daily, Some(&last));
        p.last_failed = true;
        p.consecutive_failures = 2;
        assert_eq!(next_check_at(&p), Some(now() + Duration::seconds(1800)));
    }

    #[test]
    fn retry_after_raises_the_delay_within_its_bounds() {
        let d = SyncInterval::Monthly;
        assert_eq!(clamp_retry_after(5), 60);
        assert_eq!(clamp_retry_after(10 * 86_400), 86_400);
        assert_eq!(backoff_secs(1, d, Some(5)), 900);
        assert_eq!(backoff_secs(1, d, Some(1000)), 1000);
        assert_eq!(backoff_secs(1, d, Some(10 * 86_400)), 86_400);
        assert_eq!(backoff_secs(5, d, Some(60)), 14_400);
    }

    #[test]
    fn not_before_is_a_floor() {
        let last = ago(0);
        let floor = (now() + Duration::hours(5)).to_rfc3339();
        let mut p = plan(SyncInterval::Daily, Some(&last));
        p.last_failed = true;
        p.consecutive_failures = 1;
        p.not_before = Some(&floor);
        assert_eq!(next_check_at(&p), Some(now() + Duration::hours(5)));
        let early = (now() + Duration::minutes(1)).to_rfc3339();
        p.not_before = Some(&early);
        assert_eq!(next_check_at(&p), Some(now() + Duration::seconds(900)));
        // The floor also holds for a missing or future last check.
        p.not_before = Some(&floor);
        p.last_checked_at = None;
        assert_eq!(next_check_at(&p), Some(now() + Duration::hours(5)));
        let future = (now() + Duration::hours(2)).to_rfc3339();
        p.last_checked_at = Some(&future);
        assert_eq!(next_check_at(&p), Some(now() + Duration::hours(5)));
        p.not_before = Some(&early);
        assert_eq!(next_check_at(&p), Some(now() + Duration::minutes(1)));
        p.not_before = Some("not a date");
        assert_eq!(next_check_at(&p), Some(now()));
    }

    #[test]
    fn an_overflowing_delay_is_far_away_not_now() {
        assert_eq!(add_secs(now(), u64::MAX), DateTime::<Utc>::MAX_UTC);
    }
}
