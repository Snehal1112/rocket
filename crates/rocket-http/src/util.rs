//! Small helpers shared by the signers in this crate.

use rand::{distributions::Alphanumeric, Rng};

/// Returns `n` random ASCII letters and digits.
pub(crate) fn rand_alnum(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// Percent-encodes everything except unreserved characters.
pub(crate) fn percent_encode(input: &str) -> String {
    urlencoding::encode(input).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: &[&str] = &[
        "", "%", "a b", "~", "+", "*", "a/b", "é", "a%20b", "\u{1F680}",
    ];

    #[test]
    fn rand_alnum_has_the_requested_length() {
        for n in [1, 16, 32, 64] {
            assert_eq!(rand_alnum(n).len(), n);
        }
    }

    #[test]
    fn rand_alnum_zero_is_empty() {
        assert_eq!(rand_alnum(0), "");
    }

    #[test]
    fn rand_alnum_is_only_ascii_letters_and_digits() {
        assert!(rand_alnum(256).chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn rand_alnum_two_calls_differ() {
        assert_ne!(rand_alnum(32), rand_alnum(32));
    }

    #[test]
    fn percent_encode_matches_urlencoding_for_awkward_inputs() {
        for input in AWKWARD {
            assert_eq!(
                percent_encode(input),
                urlencoding::encode(input).into_owned(),
                "input: {input:?}"
            );
        }
    }
}
