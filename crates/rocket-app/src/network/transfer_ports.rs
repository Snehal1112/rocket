//! Ports of the receive side of a collection transfer: where the ZIP is staged, how it is
//! imported, and which name the new collection gets.

use std::path::Path;

use rocket_import::transfer_zip::{is_hidden_format_char, is_reserved_device_name};
use rocket_shared::error::DomainResult;

pub use rocket_network::staging::{StagedFile, StagedZip, TransferStaging};

/// The collection an import created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCollection {
    pub name: String,
}

/// Imports a staged ZIP as a new collection of the active workspace.
///
/// `import_zip` copies the ZIP to a private temp folder as `<display_name>.zip`, validates
/// that copy, imports the same copy and never overwrites an existing collection. After a
/// failed import it removes only the folders the importer reports it created. It blocks, so
/// callers run it in `spawn_blocking`.
pub trait CollectionImportPort: Send + Sync {
    /// Names of the collections of the active workspace.
    fn existing_names(&self) -> DomainResult<Vec<String>>;
    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection>;
    /// Remembers who sent the collection that was just imported under `imported_name`. The
    /// default does nothing. A failure never changes the outcome of the receive, so the
    /// caller only logs it.
    fn record_origin(&self, _imported_name: &str, _origin: &CollectionOrigin) -> DomainResult<()> {
        Ok(())
    }
}

/// The sender of a received collection, as the receiver knew it at that time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionOrigin {
    pub sender_name: String,
    /// The short device id of the sender, for example `50L2-DPML`.
    pub sender_short_id: String,
}

/// The text `import_zip` returns (as `DomainError::InvalidInput`) for a ZIP that failed the
/// safety checks. The service compares against it to tell "not safe" from "import failed".
pub const UNSAFE_ZIP_MESSAGE: &str =
    "The collection was rejected because it is not safe to import.";

/// The whole display name, suffix included, in bytes. File systems cap a name at 255 bytes.
const MAX_NAME_BYTES: usize = 120;
/// The sender part of the suffix, in bytes.
const MAX_SENDER_BYTES: usize = 40;
const FALLBACK_NAME: &str = "Received collection";
const FALLBACK_SENDER: &str = "a teammate";

/// Cuts `text` to at most `max_bytes` bytes, never inside a character.
fn cap_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Makes text safe to use as a folder name on every platform: the characters the export
/// also replaces, plus the hidden bidi and zero-width format characters, become `_`, the text
/// is cut to `max_bytes` bytes (never inside a character), and spaces and dots at both ends
/// go. A trailing dot or space is dropped by Windows, and a leading dot is refused by
/// `Collection::validate_name`.
fn clean_part(raw: &str, max_bytes: usize) -> String {
    // Whitespace at the ends goes first, so a lone tab or newline counts as blank.
    let replaced: String = raw
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() || is_hidden_format_char(c) => '_',
            c => c,
        })
        .collect();
    cap_bytes(&replaced, max_bytes)
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string()
}

/// Windows cannot create a folder named `CON`, `NUL`, `COM1` and so on, with or without an
/// extension, so `_` goes right after the reserved stem (`CON_`, `nul_.v2`). The stem is at
/// most 4 bytes, so the cap that follows never cuts the `_` away.
fn unreserve(name: String) -> String {
    if !is_reserved_device_name(&name) {
        return name;
    }
    let mapped = match name.split_once('.') {
        Some((stem, rest)) => format!("{stem}_.{rest}"),
        None => format!("{name}_"),
    };
    cap_bytes(&mapped, MAX_NAME_BYTES)
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string()
}

/// The name the receiver gives the new collection: always `<offered> (from <sender>)`, so the
/// source shows in every received collection. The importer still adds `-1`, `-2` when that
/// name is taken. The result is at most `MAX_NAME_BYTES` bytes.
pub fn receiver_display_name(offered: &str, sender: &str) -> String {
    let mut base = clean_part(offered, MAX_NAME_BYTES);
    if base.is_empty() {
        base = FALLBACK_NAME.to_string();
    }
    let mut who = clean_part(sender, MAX_SENDER_BYTES);
    if who.is_empty() {
        who = FALLBACK_SENDER.to_string();
    }
    let suffix = format!(" (from {who})");
    // The suffix is at most 48 bytes, so the offered part keeps at least 72 bytes. Cutting can
    // leave a trailing space or dot, so the part is trimmed again.
    let room = MAX_NAME_BYTES - suffix.len();
    let kept = cap_bytes(&base, room).trim_matches(|c: char| c.is_whitespace() || c == '.');
    unreserve(format!("{kept}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_gets_the_sender() {
        assert_eq!(receiver_display_name("Orders", "Ana"), "Orders (from Ana)");
    }

    #[test]
    fn a_taken_name_is_left_to_the_importer() {
        // The importer adds -1, -2 on its own, so the name does not depend on what exists.
        assert_eq!(receiver_display_name("Orders", "Ana"), "Orders (from Ana)");
        assert_eq!(receiver_display_name("orders", "Ana"), "orders (from Ana)");
    }

    #[test]
    fn the_sender_name_is_trimmed() {
        assert_eq!(
            receiver_display_name("Orders", "  Ana  "),
            "Orders (from Ana)"
        );
    }

    #[test]
    fn an_empty_sender_falls_back_to_a_teammate() {
        for blank in ["", "   ", "\t"] {
            let got = receiver_display_name("Orders", blank);
            assert_eq!(got, "Orders (from a teammate)", "sender was {blank:?}");
        }
    }

    #[test]
    fn receiver_display_name_replaces_characters_unsafe_in_a_folder_name() {
        let got = receiver_display_name("../../etc/passwd", "Ana");
        assert_eq!(got, "_.._etc_passwd (from Ana)");
        assert!(!got.contains('/') && !got.contains('\\'));

        let got = receiver_display_name("a:b*c?d\"e<f>g|h", "Ana");
        assert_eq!(got, "a_b_c_d_e_f_g_h (from Ana)");

        let got = receiver_display_name("tab\there\nnewline\0nul", "Ana");
        assert_eq!(got, "tab_here_newline_nul (from Ana)");
    }

    #[test]
    fn receiver_display_name_replaces_hidden_format_characters() {
        // A right-to-left override or a zero-width space could spoof the name in the sidebar.
        let got = receiver_display_name("a\u{202E}b\u{200B}c\u{FEFF}", "Ana");
        assert_eq!(got, "a_b_c_ (from Ana)");
    }

    #[test]
    fn receiver_display_name_maps_windows_reserved_names() {
        // The suffix makes a bare reserved name safe.
        assert_eq!(receiver_display_name("CON", "Ana"), "CON (from Ana)");
        // Windows also reserves `CON.<anything>`, so a dotted name is mapped.
        assert_eq!(receiver_display_name("nul.v2", "Ana"), "nul_.v2 (from Ana)");
        assert_eq!(receiver_display_name("con.v2", "Ana"), "con_.v2 (from Ana)");
    }

    #[test]
    fn the_sender_name_is_cleaned_the_same_way() {
        assert_eq!(
            receiver_display_name("Orders", "../Ana/"),
            "Orders (from _Ana_)"
        );
    }

    #[test]
    fn receiver_display_name_trims_spaces_and_dots_at_both_ends() {
        assert_eq!(receiver_display_name("Orders.", "Ana"), "Orders (from Ana)");
        assert_eq!(receiver_display_name("Orders ", "Ana"), "Orders (from Ana)");
        assert_eq!(
            receiver_display_name(" . Orders . ", "Ana"),
            "Orders (from Ana)"
        );
        assert_eq!(receiver_display_name(".hidden", "Ana"), "hidden (from Ana)");
        assert_eq!(
            receiver_display_name("v1.2 API", "Ana"),
            "v1.2 API (from Ana)"
        );
    }

    #[test]
    fn receiver_display_name_never_returns_an_empty_name() {
        for odd in ["", "   ", "...", " . ", "\0"] {
            let got = receiver_display_name(odd, "Ana");
            assert!(got.ends_with(" (from Ana)"), "offered was {odd:?}");
            assert!(!got.starts_with('.') && !got.starts_with(' '));
        }
        assert_eq!(
            receiver_display_name("", "Ana"),
            "Received collection (from Ana)"
        );
    }

    #[test]
    fn receiver_display_name_keeps_non_ascii_letters() {
        let got = receiver_display_name("Café – Ünï 日本語", "Zoë");
        assert_eq!(got, "Café – Ünï 日本語 (from Zoë)");
    }

    #[test]
    fn receiver_display_name_cuts_on_a_character_boundary() {
        // 150 two-byte characters = 300 bytes. A byte cut would panic or split a character.
        let long = "é".repeat(150);
        let got = receiver_display_name(&long, "Ana");
        assert!(got.len() <= 120);
        assert!(got.ends_with(" (from Ana)"));
        assert!(got
            .trim_end_matches(" (from Ana)")
            .chars()
            .all(|c| c == 'é'));

        // A three-byte character that does not divide 120 evenly after the suffix.
        let long_sender = "ü".repeat(80);
        let got = receiver_display_name("Orders", &long_sender);
        assert_eq!(got, format!("Orders (from {})", "ü".repeat(20)));
    }

    #[test]
    fn receiver_display_name_caps_cjk_names_by_bytes() {
        // 100 CJK characters are 300 bytes. With the suffix " (from Ana)" (11 bytes) the
        // offered part gets 109 bytes: 36 characters.
        let cjk = "日".repeat(100);
        let got = receiver_display_name(&cjk, "Ana");
        assert_eq!(got, format!("{} (from Ana)", "日".repeat(36)));
        assert_eq!(got.len(), 119);
    }

    #[test]
    fn receiver_display_name_caps_the_whole_name_with_the_suffix_by_bytes() {
        let cjk = "日".repeat(100);
        let cjk_sender = "本".repeat(60);
        for (offered, sender) in [
            ("Orders".to_string(), "Ana".to_string()),
            (cjk.clone(), "Ana".to_string()),
            (cjk.clone(), cjk_sender.clone()),
            ("a".repeat(300), cjk_sender.clone()),
            ("é".repeat(200), "ü".repeat(200)),
        ] {
            let got = receiver_display_name(&offered, &sender);
            assert!(got.len() <= 120, "{} bytes for {offered:.10}", got.len());
            assert!(!got.is_empty() && !got.ends_with(' ') && !got.ends_with('.'));
        }
        // The sender part alone is cut to 40 bytes: 13 characters of 3 bytes.
        let got = receiver_display_name("Orders", &cjk_sender);
        assert_eq!(got, format!("Orders (from {})", "本".repeat(13)));
    }

    #[test]
    fn a_cut_that_ends_in_a_space_is_trimmed() {
        let offered = format!("{} tail", "a".repeat(108));
        let got = receiver_display_name(&offered, "Ana");
        assert_eq!(got, format!("{} (from Ana)", "a".repeat(108)));
    }

    #[test]
    fn the_zip_limits_equal_the_protocol_limits() {
        // `rocket-import` cannot depend on `rocket-network`, so the values are duplicated and
        // this test is what keeps them equal.
        assert_eq!(
            rocket_import::transfer_zip::MAX_ZIP_ENTRIES,
            rocket_network::limits::MAX_ZIP_ENTRIES
        );
        assert_eq!(
            rocket_import::transfer_zip::MAX_UNZIPPED_BYTES,
            rocket_network::limits::MAX_UNZIPPED_BYTES
        );
    }
}
