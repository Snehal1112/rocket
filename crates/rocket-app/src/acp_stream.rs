//! Coalesces and masks the agent's live text before it reaches the webview.
//!
//! The agent sends its reply and its reasoning as many small chunks. Each
//! chunk used to become one event. `ChunkStream` joins them, so at most one
//! event per kind leaves per `CHUNK_FLUSH_WINDOW`, or sooner when
//! `MAX_COALESCED_CHUNK_BYTES` is reached. It also masks known secret values
//! on a rolling buffer, so a secret split across two chunks is never shown.
//!
//! Everything here is pure. Time is passed in, so tests need no sleeps.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::redaction::{redact_secrets, redaction_forms, url_secret_forms, MIN_REDACTION_LEN};

/// How long text may wait before it is published as one event.
pub(crate) const CHUNK_FLUSH_WINDOW: Duration = Duration::from_millis(50);

/// Pending text of this size is published at once, without waiting for the
/// window. It also bounds the memory one pending chunk can use.
pub(crate) const MAX_COALESCED_CHUNK_BYTES: usize = 16 * 1024;

/// The two kinds of live agent text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChunkKind {
    /// The reply, published as `AcpSessionChunk`.
    Text,
    /// The reasoning, published as `AcpThoughtChunk`.
    Thought,
}

/// Every form of the given secret values that the live stream masks.
///
/// These are the same forms as `mask_secret_text` uses for stored
/// conversations: the whole value, its trimmed form, each line of a
/// multi-line value, and the percent-encoded forms of each.
pub(crate) fn stream_secret_forms(secret_values: &HashSet<String>) -> HashSet<String> {
    let mut base: HashSet<String> = HashSet::new();
    for value in secret_values {
        base.extend(redaction_forms(value));
    }
    url_secret_forms(&base)
        .into_iter()
        .filter(|form| form.len() >= MIN_REDACTION_LEN)
        .collect()
}

/// Masks a stream of text that arrives in pieces.
///
/// It holds back the last `longest form - 1` bytes, because they may be the
/// start of a secret whose rest has not arrived yet. A secret that is
/// already complete is never cut: the cut moves in front of it.
#[derive(Debug)]
pub(crate) struct StreamMasker {
    forms: HashSet<String>,
    /// Bytes held back at the end of the text, at most.
    hold: usize,
    tail: String,
}

impl StreamMasker {
    pub(crate) fn new(forms: HashSet<String>) -> Self {
        let hold = forms
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(0)
            .saturating_sub(1);
        Self {
            forms,
            hold,
            tail: String::new(),
        }
    }

    /// Adds text and returns the masked text that is safe to show now.
    pub(crate) fn push(&mut self, text: &str) -> String {
        self.tail.push_str(text);
        if self.forms.is_empty() {
            return std::mem::take(&mut self.tail);
        }
        let cut = self.safe_cut();
        let rest = self.tail.split_off(cut);
        let ready = std::mem::replace(&mut self.tail, rest);
        redact_secrets(&ready, &self.forms)
    }

    /// Returns the held-back text, masked. Call it when the stream ends.
    pub(crate) fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.tail);
        redact_secrets(&rest, &self.forms)
    }

    /// The end of the text that can be shown now. It leaves `hold` bytes and
    /// never falls inside a complete secret.
    fn safe_cut(&self) -> usize {
        let text = &self.tail;
        let mut cut = text.len().saturating_sub(self.hold);
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        // Moving the cut can put it inside another secret, so repeat.
        loop {
            let mut moved = false;
            for form in &self.forms {
                for start in occurrences(text, form) {
                    if start < cut && start + form.len() > cut {
                        cut = start;
                        moved = true;
                    }
                }
            }
            if !moved {
                return cut;
            }
        }
    }
}

/// Start offsets of every occurrence of `needle`, overlapping ones too.
fn occurrences<'a>(haystack: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        let found = haystack.get(from..)?.find(needle)? + from;
        // Step one character, so the next search starts on a boundary.
        let step = haystack[found..].chars().next().map_or(1, char::len_utf8);
        from = found + step;
        Some(found)
    })
}

/// Joins live agent text into fewer events and masks it.
///
/// Only one kind is pending at a time. A chunk of the other kind, or any
/// other update, first publishes everything pending, held-back text
/// included, so the order between kinds and updates is kept.
#[derive(Debug)]
pub(crate) struct ChunkStream {
    masker: StreamMasker,
    /// The kind of the pending text and of the masker's held-back text.
    kind: Option<ChunkKind>,
    pending: String,
    /// When the pending text started to wait.
    opened_at: Option<Instant>,
}

impl ChunkStream {
    pub(crate) fn new(forms: HashSet<String>) -> Self {
        Self {
            masker: StreamMasker::new(forms),
            kind: None,
            pending: String::new(),
            opened_at: None,
        }
    }

    /// Adds one chunk. Returns the events to publish now, in order.
    pub(crate) fn push(
        &mut self,
        kind: ChunkKind,
        text: &str,
        now: Instant,
    ) -> Vec<(ChunkKind, String)> {
        let mut out = Vec::new();
        if self.kind.is_some_and(|open| open != kind) {
            out.extend(self.finish());
        }
        self.kind = Some(kind);
        if self.opened_at.is_none() {
            self.opened_at = Some(now);
        }
        self.pending.push_str(text);
        let window_over = self.deadline().is_some_and(|deadline| now >= deadline);
        if self.pending.len() >= MAX_COALESCED_CHUNK_BYTES || window_over {
            out.extend(self.flush_pending());
        }
        out
    }

    /// When the pending text must be published, if any waits.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.opened_at.map(|opened| opened + CHUNK_FLUSH_WINDOW)
    }

    /// Publishes the pending text because its window ended. The masker
    /// keeps its held-back text for the next chunk.
    pub(crate) fn flush_due(&mut self) -> Vec<(ChunkKind, String)> {
        self.flush_pending().into_iter().collect()
    }

    /// Publishes everything, held-back text included. Call it before an
    /// update of another kind and before the turn's end.
    pub(crate) fn finish(&mut self) -> Vec<(ChunkKind, String)> {
        let Some(kind) = self.kind.take() else {
            return Vec::new();
        };
        self.opened_at = None;
        let pending = std::mem::take(&mut self.pending);
        let mut text = self.masker.push(&pending);
        text.push_str(&self.masker.finish());
        if text.is_empty() {
            Vec::new()
        } else {
            vec![(kind, text)]
        }
    }

    fn flush_pending(&mut self) -> Option<(ChunkKind, String)> {
        self.opened_at = None;
        let kind = self.kind?;
        let pending = std::mem::take(&mut self.pending);
        let text = self.masker.push(&pending);
        (!text.is_empty()).then_some((kind, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::REDACTED;

    fn forms(values: &[&str]) -> HashSet<String> {
        stream_secret_forms(&values.iter().map(|v| v.to_string()).collect())
    }

    /// Feeds every piece through a masker and returns all it showed.
    fn mask_pieces(values: &[&str], pieces: &[&str]) -> (Vec<String>, String) {
        let mut masker = StreamMasker::new(forms(values));
        let mut shown = Vec::new();
        for piece in pieces {
            shown.push(masker.push(piece));
        }
        let end = masker.finish();
        let mut all: String = shown.concat();
        all.push_str(&end);
        shown.push(end);
        (shown, all)
    }

    #[test]
    fn masker_hides_a_secret_split_across_chunks() {
        let (shown, all) = mask_pieces(&["sk-live-123456"], &["key is sk-li", "ve-12", "3456 ok"]);
        assert_eq!(all, format!("key is {REDACTED} ok"));
        // No single piece shows any part of the secret beyond what a reader
        // could not tell from the prefix alone.
        for piece in &shown {
            assert!(!piece.contains("live"), "{shown:?}");
            assert!(!piece.contains("3456"), "{shown:?}");
        }
    }

    #[test]
    fn masker_flushes_the_held_tail_at_the_end() {
        let mut masker = StreamMasker::new(forms(&["sk-live-123456"]));
        let shown = masker.push("plain text ending in sk-li");
        assert!(!shown.contains("sk-li"));
        assert_eq!(
            format!("{shown}{}", masker.finish()),
            "plain text ending in sk-li"
        );
    }

    #[test]
    fn masker_leaves_text_without_secrets_complete() {
        let text = ["héllo ", "wörld ", "✓ done"];
        let (_, all) = mask_pieces(&["sk-live-123456"], &text);
        assert_eq!(all, text.concat());
    }

    #[test]
    fn masker_without_secrets_holds_nothing() {
        let mut masker = StreamMasker::new(HashSet::new());
        assert_eq!(masker.push("abc"), "abc");
        assert_eq!(masker.finish(), "");
    }

    #[test]
    fn masker_masks_a_secret_at_the_very_end() {
        let (_, all) = mask_pieces(&["sk-live-123456"], &["value: sk-live-", "123456"]);
        assert_eq!(all, format!("value: {REDACTED}"));
    }

    #[test]
    fn masker_masks_two_secrets() {
        let (_, all) = mask_pieces(
            &["sk-live-123456", "password-xyz"],
            &["a sk-live-1", "23456 b passw", "ord-xyz c"],
        );
        assert_eq!(all, format!("a {REDACTED} b {REDACTED} c"));
    }

    #[test]
    fn masker_masks_a_secret_longer_than_the_coalescing_window() {
        let secret = "s".repeat(40) + &"é".repeat(150) + "-end";
        let text = format!("start {secret} stop");
        // Seven bytes at a time, cut on character boundaries.
        let mut pieces = Vec::new();
        let mut rest = text.as_str();
        while !rest.is_empty() {
            let mut cut = rest.len().min(7);
            while !rest.is_char_boundary(cut) {
                cut += 1;
            }
            pieces.push(&rest[..cut]);
            rest = &rest[cut..];
        }
        let (shown, all) = mask_pieces(&[secret.as_str()], &pieces);
        assert_eq!(all, format!("start {REDACTED} stop"));
        for piece in &shown {
            assert!(!piece.contains("sss"), "a piece showed part of the secret");
        }
    }

    #[test]
    fn masker_masks_the_percent_encoded_form() {
        let (_, all) = mask_pieces(&["a b/c&d=e"], &["q=a%20b%2", "Fc%26d%3De end"]);
        assert_eq!(all, format!("q={REDACTED} end"));
    }

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn stream_joins_chunks_within_the_window() {
        let start = t0();
        let mut stream = ChunkStream::new(HashSet::new());
        assert!(stream.push(ChunkKind::Text, "Hello, ", start).is_empty());
        assert!(stream
            .push(ChunkKind::Text, "world!", start + Duration::from_millis(10))
            .is_empty());
        assert_eq!(stream.deadline(), Some(start + CHUNK_FLUSH_WINDOW));
        assert_eq!(
            stream.flush_due(),
            vec![(ChunkKind::Text, "Hello, world!".to_string())]
        );
        assert_eq!(stream.deadline(), None);
        assert!(stream.finish().is_empty());
    }

    #[test]
    fn stream_publishes_once_the_window_is_over_even_without_a_gap() {
        let start = t0();
        let mut stream = ChunkStream::new(HashSet::new());
        assert!(stream.push(ChunkKind::Text, "a", start).is_empty());
        let out = stream.push(ChunkKind::Text, "b", start + CHUNK_FLUSH_WINDOW);
        assert_eq!(out, vec![(ChunkKind::Text, "ab".to_string())]);
    }

    #[test]
    fn stream_publishes_at_the_size_threshold() {
        let start = t0();
        let mut stream = ChunkStream::new(HashSet::new());
        let big = "x".repeat(MAX_COALESCED_CHUNK_BYTES);
        let out = stream.push(ChunkKind::Thought, &big, start);
        assert_eq!(out, vec![(ChunkKind::Thought, big)]);
    }

    #[test]
    fn stream_keeps_order_between_kinds() {
        let start = t0();
        let mut stream = ChunkStream::new(forms(&["sk-live-123456"]));
        let mut events = Vec::new();
        events.extend(stream.push(ChunkKind::Thought, "think ", start));
        events.extend(stream.push(ChunkKind::Thought, "more", start));
        events.extend(stream.push(ChunkKind::Text, "answer ", start));
        events.extend(stream.push(ChunkKind::Text, "done", start));
        events.extend(stream.finish());
        assert_eq!(
            events,
            vec![
                (ChunkKind::Thought, "think more".to_string()),
                (ChunkKind::Text, "answer done".to_string()),
            ]
        );
    }

    #[test]
    fn stream_text_joins_back_to_the_original_with_multibyte_chunks() {
        let start = t0();
        let pieces = ["Grüß", "e, 世", "界 ", "🚀", " ok", "é", "\u{301}"];
        let mut stream = ChunkStream::new(forms(&["unrelated-secret"]));
        let mut out = Vec::new();
        for (i, piece) in pieces.iter().enumerate() {
            let now = start + Duration::from_millis(30 * i as u64);
            out.extend(stream.push(ChunkKind::Text, piece, now));
        }
        out.extend(stream.finish());
        let joined: String = out.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, pieces.concat());
        assert!(out.len() > 1, "the window should have split the stream");
    }

    #[test]
    fn stream_never_publishes_a_split_secret_and_flushes_the_tail_at_the_end() {
        let start = t0();
        let mut stream = ChunkStream::new(forms(&["sk-live-123456"]));
        let mut out = Vec::new();
        for (i, piece) in ["token sk-li", "ve-1", "23456 and sk-l"].iter().enumerate() {
            let now = start + CHUNK_FLUSH_WINDOW * (i as u32 + 1);
            out.extend(stream.push(ChunkKind::Text, piece, now));
            out.extend(stream.flush_due());
        }
        for (_, text) in &out {
            assert!(!text.contains("live") && !text.contains("3456"), "{out:?}");
        }
        out.extend(stream.finish());
        let joined: String = out.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, format!("token {REDACTED} and sk-l"));
    }
}
