//! Time limits for one script run.
//!
//! A script gets a budget of busy time: the time the script thread spends
//! running its code. Waiting for a host call or `rok.sleep` is not busy time,
//! so a script can wait on the network without using its budget. A wall-clock
//! ceiling still ends a script that waits forever, such as an endless polling loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

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

/// Busy time of one run, shared between the script thread and the watchdog.
pub struct BudgetClock {
    time: Mutex<BusyTime>,
    aborted: AtomicBool,
    wake: Notify,
}

#[derive(Default)]
struct BusyTime {
    total: Duration,
    since: Option<Instant>,
}

impl BudgetClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            time: Mutex::new(BusyTime::default()),
            aborted: AtomicBool::new(false),
            wake: Notify::new(),
        })
    }

    fn time(&self) -> MutexGuard<'_, BusyTime> {
        // The guarded data is two plain values that a panic cannot leave
        // half-written, so a poisoned lock is still safe to use.
        self.time.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Marks the start of a stretch of busy time.
    pub fn enter_busy(&self) {
        let mut time = self.time();
        if time.since.is_none() {
            time.since = Some(Instant::now());
        }
    }

    /// Marks the end of a stretch of busy time.
    pub fn leave_busy(&self) {
        let mut time = self.time();
        if let Some(since) = time.since.take() {
            time.total += since.elapsed();
        }
    }

    /// Busy time so far, counting a stretch that is still running.
    pub fn busy(&self) -> Duration {
        let time = self.time();
        time.total + time.since.map(|since| since.elapsed()).unwrap_or_default()
    }

    /// Asks the run to stop. Safe to call more than once.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
        // A stored permit wakes a waiter that starts waiting later.
        self.wake.notify_one();
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Resolves once `abort` was called.
    pub async fn aborted(&self) {
        if self.is_aborted() {
            return;
        }
        self.wake.notified().await;
    }
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
    fn busy_time_counts_only_marked_stretches() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        clock.leave_busy();
        std::thread::sleep(ms(40));
        let busy = clock.busy();
        assert!(busy >= ms(20) && busy < ms(40), "{busy:?}");
    }

    #[test]
    fn busy_time_includes_a_stretch_that_is_still_running() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        assert!(clock.busy() >= ms(20));
    }

    #[test]
    fn a_second_enter_does_not_restart_the_stretch() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        clock.enter_busy();
        clock.leave_busy();
        assert!(clock.busy() >= ms(20));
    }

    #[tokio::test]
    async fn aborted_resolves_when_abort_came_first() {
        let clock = BudgetClock::new();
        clock.abort();
        assert!(clock.is_aborted());
        tokio::time::timeout(ms(100), clock.aborted())
            .await
            .expect("aborted() resolves");
    }

    #[tokio::test]
    async fn aborted_wakes_a_waiting_task() {
        let clock = BudgetClock::new();
        let waiter = {
            let clock = Arc::clone(&clock);
            tokio::spawn(async move { clock.aborted().await })
        };
        tokio::time::sleep(ms(10)).await;
        clock.abort();
        tokio::time::timeout(ms(200), waiter)
            .await
            .expect("woken in time")
            .expect("task joined");
    }

    #[test]
    fn stop_messages_say_timed_out() {
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("of script time"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("at most"));
    }
}
