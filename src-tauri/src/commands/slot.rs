//! A slot that holds the handle of one running background task, so a replace or the app exit can
//! stop it.

use std::sync::Mutex;

/// A running background loop that can be told to stop.
pub(crate) trait Stoppable {
    fn stop(&self);
}

struct SlotState<H> {
    handle: Option<H>,
    stopped_for_good: bool,
}

/// Holds one handle. Generic over `Stoppable` so tests can use a counting stand-in.
pub(crate) struct Slot<H: Stoppable>(Mutex<SlotState<H>>);

impl<H: Stoppable> Default for Slot<H> {
    fn default() -> Self {
        Self(Mutex::new(SlotState {
            handle: None,
            stopped_for_good: false,
        }))
    }
}

impl<H: Stoppable> Slot<H> {
    /// Keeps the new handle and stops the old one. After `stop_for_good`, the new handle is
    /// stopped at once instead.
    pub(crate) fn replace(&self, handle: H) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped_for_good {
            handle.stop();
            return;
        }
        if let Some(old) = state.handle.take() {
            old.stop();
        }
        state.handle = Some(handle);
    }

    /// Stops and drops the kept handle. Calling it again does nothing, and a later `replace`
    /// still keeps its handle.
    pub(crate) fn stop(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = state.handle.take() {
            old.stop();
        }
    }

    /// Like `stop`, and also remembers it, so a handle that arrives later (a start that raced the
    /// exit) is stopped at once. Used on the app exit paths only.
    pub(crate) fn stop_for_good(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped_for_good = true;
        if let Some(old) = state.handle.take() {
            old.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    struct Counting(Arc<AtomicUsize>);

    impl Stoppable for Counting {
        fn stop(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[test]
    fn replacing_the_handle_stops_the_old_one() {
        let (first, second) = (counter(), counter());
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(first.clone()));
        slot.replace(Counting(second.clone()));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stopping_twice_stops_the_handle_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(count.clone()));
        slot.stop();
        slot.stop();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_plain_stop_does_not_latch() {
        let (first, second) = (counter(), counter());
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(first.clone()));
        slot.stop();
        slot.replace(Counting(second.clone()));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(
            second.load(Ordering::SeqCst),
            0,
            "a workspace switch keeps the new handle"
        );
        slot.stop();
        assert_eq!(second.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stopping_for_good_stops_the_kept_handle_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(count.clone()));
        slot.stop_for_good();
        slot.stop_for_good();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_handle_that_arrives_after_stopping_for_good_is_stopped_at_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.stop_for_good();
        slot.replace(Counting(count.clone()));
        assert_eq!(count.load(Ordering::SeqCst), 1, "exit raced the start");
    }
}
