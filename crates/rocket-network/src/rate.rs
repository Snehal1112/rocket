//! A sliding-window counter for request limits. Pure; the caller passes the time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Allows at most `cap` hits within any `window`.
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    cap: usize,
    window: Duration,
    hits: VecDeque<Instant>,
}

impl SlidingWindow {
    pub fn new(cap: usize, window: Duration) -> SlidingWindow {
        SlidingWindow {
            cap,
            window,
            hits: VecDeque::new(),
        }
    }

    /// Records a hit at `now` and returns true, or returns false when `cap` hits are already
    /// inside the window. A refused hit is not recorded.
    pub fn try_hit(&mut self, now: Instant) -> bool {
        while self
            .hits
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= self.window)
        {
            self.hits.pop_front();
        }
        if self.hits.len() >= self.cap {
            return false;
        }
        self.hits.push_back(now);
        true
    }

    /// Forgets every hit.
    pub fn clear(&mut self) {
        self.hits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn hits_up_to_the_cap_pass_and_the_next_one_is_refused() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(3, secs(60));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0 + secs(1)));
        assert!(
            !window.try_hit(t0 + secs(2)),
            "the fourth hit in the window"
        );
    }

    #[test]
    fn the_window_slides_as_old_hits_age_out() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(2, secs(60));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0 + secs(30)));
        assert!(!window.try_hit(t0 + secs(59)));
        assert!(window.try_hit(t0 + secs(60)), "the first hit has aged out");
        assert!(
            !window.try_hit(t0 + secs(61)),
            "the hits at 30 s and 60 s remain"
        );
        assert!(
            window.try_hit(t0 + secs(90)),
            "the hit at 30 s has aged out"
        );
    }

    #[test]
    fn a_hit_exactly_one_window_old_is_dropped() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        assert!(!window.try_hit(t0 + secs(60) - Duration::from_millis(1)));
        assert!(window.try_hit(t0 + secs(60)));
    }

    #[test]
    fn a_refused_hit_is_not_recorded() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        assert!(!window.try_hit(t0 + secs(30)));
        assert!(window.try_hit(t0 + secs(60)), "only the first hit counted");
    }

    #[test]
    fn a_cap_of_zero_refuses_everything() {
        let mut window = SlidingWindow::new(0, secs(60));
        assert!(!window.try_hit(Instant::now()));
    }

    #[test]
    fn a_clock_that_goes_back_keeps_the_hit_in_the_window() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0 + secs(10)));
        assert!(
            !window.try_hit(t0),
            "an earlier `now` does not panic or free the slot"
        );
    }

    #[test]
    fn clear_forgets_every_hit() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        window.clear();
        assert!(window.try_hit(t0));
    }
}
