//! Time limits for one script run.
//!
//! A script gets a budget of busy time: the time the script thread spends
//! running its code. Waiting for a host call or `rok.sleep` is not busy time,
//! so a script can wait on the network without using its budget. A wall-clock
//! ceiling still ends a script that waits forever, such as an endless polling loop.

use std::time::Duration;

/// The limits one script run gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    /// Busy time the script may use.
    pub cpu: Duration,
    /// Wall-clock time the whole run may take, waiting included.
    pub ceiling: Duration,
    /// Longest single `rok.sleep`.
    pub sleep_cap: Duration,
}

impl ScriptLimits {
    /// The limits every real script run uses.
    ///
    /// Five seconds of busy time comfortably exceeds any legitimate script,
    /// which does templating, signing and small JSON work. Waiting does not
    /// count, so network calls get their own timeouts instead.
    pub const DEFAULT: ScriptLimits = ScriptLimits {
        cpu: Duration::from_secs(5),
        ceiling: Duration::from_secs(300),
        sleep_cap: Duration::from_secs(60),
    };
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the watchdog decides after reading the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Keep going, and look again after this long at the latest.
    Continue(Duration),
    /// The script used up its busy time.
    CpuExceeded,
    /// The run reached the wall-clock ceiling.
    CeilingExceeded,
}

impl Verdict {
    /// The error text for a verdict that ends the run.
    pub fn message(self, limits: &ScriptLimits) -> String {
        match self {
            Verdict::CpuExceeded => format!(
                "script execution timed out after {:?} of script time",
                limits.cpu
            ),
            Verdict::CeilingExceeded => format!(
                "script execution timed out: a script may run for at most {:?}",
                limits.ceiling
            ),
            Verdict::Continue(_) => String::new(),
        }
    }
}

/// Decides whether a run may continue, given its busy time and its wall-clock time.
pub fn check(limits: &ScriptLimits, busy: Duration, wall: Duration) -> Verdict {
    if busy >= limits.cpu {
        return Verdict::CpuExceeded;
    }
    if wall >= limits.ceiling {
        return Verdict::CeilingExceeded;
    }
    // Busy time grows no faster than wall time, so neither limit can pass before this.
    let next = (limits.cpu - busy).min(limits.ceiling - wall);
    Verdict::Continue(next.max(Duration::from_millis(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    const LIMITS: ScriptLimits = ScriptLimits {
        cpu: Duration::from_millis(100),
        ceiling: Duration::from_millis(1000),
        sleep_cap: Duration::from_millis(50),
    };

    #[test]
    fn default_limits_match_the_spec() {
        assert_eq!(ScriptLimits::DEFAULT.cpu, Duration::from_secs(5));
        assert_eq!(ScriptLimits::DEFAULT.ceiling, Duration::from_secs(300));
        assert_eq!(ScriptLimits::DEFAULT.sleep_cap, Duration::from_secs(60));
        assert_eq!(ScriptLimits::default(), ScriptLimits::DEFAULT);
    }

    #[test]
    fn check_continues_until_the_nearer_limit() {
        assert_eq!(check(&LIMITS, ms(30), ms(500)), Verdict::Continue(ms(70)));
        assert_eq!(check(&LIMITS, ms(0), ms(950)), Verdict::Continue(ms(50)));
    }

    #[test]
    fn check_stops_at_the_cpu_budget_first() {
        assert_eq!(check(&LIMITS, ms(100), ms(2000)), Verdict::CpuExceeded);
    }

    #[test]
    fn check_stops_at_the_ceiling() {
        assert_eq!(check(&LIMITS, ms(10), ms(1000)), Verdict::CeilingExceeded);
    }

    #[test]
    fn check_never_asks_for_a_zero_wait() {
        let almost = check(&LIMITS, ms(99) + Duration::from_micros(999), ms(0));
        assert_eq!(almost, Verdict::Continue(ms(1)));
    }

    #[test]
    fn stop_messages_say_timed_out() {
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("of script time"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("at most"));
    }
}
