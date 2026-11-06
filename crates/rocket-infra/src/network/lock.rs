//! One place for the "a panic elsewhere must not stop the network" lock rule.

use std::sync::{Mutex, MutexGuard};

/// Locks `mutex`. A lock poisoned by a panic in another thread is used as it is, because the
/// data behind these mutexes stays valid after any single update.
pub(crate) fn lock_ignore_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn it_locks_a_healthy_mutex() {
        let mutex = Mutex::new(5);
        *lock_ignore_poison(&mutex) += 1;
        assert_eq!(*lock_ignore_poison(&mutex), 6);
    }

    #[test]
    fn it_still_locks_after_another_thread_panicked_holding_the_lock() {
        let mutex = Arc::new(Mutex::new(1));
        let poisoner = mutex.clone();
        let result = std::thread::spawn(move || {
            let _guard = poisoner.lock().expect("lock before the panic");
            panic!("poison the mutex on purpose");
        })
        .join();
        assert!(result.is_err());
        assert!(mutex.is_poisoned());
        assert_eq!(*lock_ignore_poison(&mutex), 1);
    }
}
