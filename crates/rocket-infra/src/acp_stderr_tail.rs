//! A bounded in-memory tail of an ACP agent's stderr, used only to explain a
//! failed session start.
//!
//! The tail is never logged and never kept after a successful start. An agent
//! may echo its environment, so every secret value passed to the process is
//! masked before any text leaves this module.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;

/// The most masked text that is ever added to an error message, in bytes.
pub const TAIL_LIMIT: usize = 2048;

/// Raw bytes kept while the session starts. This is larger than
/// `TAIL_LIMIT` so a secret that straddles the old edge of the buffer is
/// still masked whole before the final cut to `TAIL_LIMIT`.
const RAW_LIMIT: usize = TAIL_LIMIT * 4;

/// Values shorter than this are not masked. Masking a one- or two-character
/// value would turn the output into noise, and such values are not real
/// credentials. Longer values (tokens, keys, paths) are masked everywhere.
pub const MIN_MASKED_LEN: usize = 4;

/// How long a failed start waits for the agent's last stderr bytes.
const EOF_GRACE: Duration = Duration::from_millis(200);

/// Shared between the drain task and the start path.
#[derive(Default)]
pub struct StderrTail {
    buf: Mutex<VecDeque<u8>>,
    /// While false, drained bytes are dropped. Set false once the start is over.
    keeping: AtomicBool,
    eof: AtomicBool,
    eof_signal: Notify,
}

impl StderrTail {
    pub fn new() -> Self {
        Self {
            keeping: AtomicBool::new(true),
            ..Self::default()
        }
    }

    /// Adds drained bytes, keeping only the newest `RAW_LIMIT` bytes.
    pub fn push(&self, chunk: &[u8]) {
        if !self.keeping.load(Ordering::Acquire) {
            return;
        }
        let mut buf = self.buf.lock().unwrap_or_else(PoisonError::into_inner);
        buf.extend(chunk.iter().copied());
        let excess = buf.len().saturating_sub(RAW_LIMIT);
        if excess > 0 {
            buf.drain(..excess);
        }
    }

    /// Called by the drain task when the pipe closes.
    pub fn mark_eof(&self) {
        self.eof.store(true, Ordering::Release);
        self.eof_signal.notify_one();
    }

    /// Stops keeping output and frees what was kept. The drain task goes on
    /// reading, so the agent never sees a broken pipe.
    pub fn discard(&self) {
        self.keeping.store(false, Ordering::Release);
        self.buf
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Waits briefly for the pipe to close, so a process that just exited
    /// has its last words in the buffer.
    pub async fn wait_for_eof(&self) {
        if self.eof.load(Ordering::Acquire) {
            return;
        }
        let _ = tokio::time::timeout(EOF_GRACE, self.eof_signal.notified()).await;
    }

    /// Returns the masked, printable tail and frees the buffer.
    pub fn take_masked(&self, secrets: &[&str]) -> String {
        self.keeping.store(false, Ordering::Release);
        let raw: Vec<u8> = {
            let mut buf = self.buf.lock().unwrap_or_else(PoisonError::into_inner);
            buf.drain(..).collect()
        };
        mask_output(&raw, secrets)
    }
}

/// Masks every secret, strips control characters and keeps the last
/// `TAIL_LIMIT` bytes. Masking runs first, on the whole buffer.
pub fn mask_output(raw: &[u8], secrets: &[&str]) -> String {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    let mut secrets: Vec<&str> = secrets
        .iter()
        .copied()
        .filter(|s| s.len() >= MIN_MASKED_LEN)
        .collect();
    // Longest first, so a secret that contains another is masked whole.
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        text = text.replace(secret, "***");
    }
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let text = text.trim();
    let mut start = text.len().saturating_sub(TAIL_LIMIT);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim().to_string()
}

/// Appends the masked tail to `message` as a short "agent output" section.
/// An empty tail leaves the message unchanged.
pub fn with_agent_output(message: String, tail: &str) -> String {
    if tail.is_empty() {
        message
    } else {
        format!("{message}\n\nagent output:\n{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_every_occurrence_of_each_secret() {
        let out = mask_output(
            b"key=sk-abc123 again sk-abc123 proxy=http://p:3128",
            &["sk-abc123", "http://p:3128"],
        );
        assert_eq!(out, "key=*** again *** proxy=***");
    }

    #[test]
    fn short_values_are_not_masked() {
        assert_eq!(mask_output(b"a=1 b=abc", &["1", "abc"]), "a=1 b=abc");
    }

    #[test]
    fn longer_secret_wins_over_one_it_contains() {
        assert_eq!(
            mask_output(b"x supersecret-long y", &["secret", "supersecret-long"]),
            "x *** y"
        );
    }

    #[test]
    fn control_characters_are_stripped() {
        let out = mask_output(b"a\x1b[31mred\x00\x07\r\nnext\ttab", &[]);
        assert_eq!(out, "a[31mred\nnext\ttab");
    }

    #[test]
    fn output_is_capped_at_the_limit_keeping_the_end() {
        let mut raw = vec![b'a'; 5000];
        raw.extend_from_slice(b"THE-END");
        let out = mask_output(&raw, &[]);
        assert!(out.len() <= TAIL_LIMIT);
        assert!(out.ends_with("THE-END"));
    }

    #[test]
    fn cap_respects_utf8_boundaries() {
        let raw = "\u{e9}".repeat(3000);
        let out = mask_output(raw.as_bytes(), &[]);
        assert!(out.len() <= TAIL_LIMIT);
        assert!(out.chars().all(|c| c == '\u{e9}'));
    }

    #[test]
    fn a_secret_cut_by_the_old_buffer_edge_is_still_masked_whole() {
        let tail = StderrTail::new();
        tail.push(&vec![b'x'; 5000]);
        tail.push(b"token=sk-abc123 end");
        let out = tail.take_masked(&["sk-abc123"]);
        assert!(!out.contains("sk-abc"));
        assert!(out.ends_with("token=*** end"));
    }

    #[test]
    fn buffer_is_bounded() {
        let tail = StderrTail::new();
        for _ in 0..100 {
            tail.push(&[b'z'; 1000]);
        }
        let held = tail.buf.lock().expect("lock").len();
        assert_eq!(held, RAW_LIMIT);
    }

    #[test]
    fn discard_frees_and_ignores_later_output() {
        let tail = StderrTail::new();
        tail.push(b"secret stuff");
        tail.discard();
        tail.push(b"more");
        assert!(tail.buf.lock().expect("lock").is_empty());
        assert_eq!(tail.take_masked(&[]), "");
    }

    #[test]
    fn empty_tail_leaves_the_message_unchanged() {
        assert_eq!(with_agent_output("failed".to_string(), ""), "failed");
        assert!(with_agent_output("failed".to_string(), "boom").contains("agent output:\nboom"));
    }
}
