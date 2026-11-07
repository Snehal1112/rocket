//! Strict check of a ZIP that came from another node, run before the importer sees it.
//!
//! The importer's own extractor skips unsafe entry names silently and allows 20000 entries
//! and 512 MiB, so it stays as a second line of defence and is not the check.

use std::collections::{BTreeSet, HashSet};
use std::io::{self, Read, Seek, Write};
use std::path::Path;

use crate::error::{ImportError, ImportResult};
use crate::files::ImportFiles;

/// The most entries a pushed ZIP may hold. Equals `rocket_network::limits::MAX_ZIP_ENTRIES`.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// The most bytes a pushed ZIP may expand to. Equals `rocket_network::limits::MAX_UNZIPPED_BYTES`.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
const COLLECTION_MARKERS: [&str; 2] = ["opencollection.yml", "bruno.json"];

fn unsafe_zip(why: &str) -> ImportError {
    ImportError::UnsafeTransferZip(why.to_string())
}

fn unreadable(e: impl std::fmt::Display) -> ImportError {
    ImportError::ZipExtractionFailed(e.to_string())
}

/// A writer that only counts the bytes it is given.
struct CountingSink;

impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const RESERVED_DEVICE_NAMES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, any case, with or without an
/// extension. Windows cannot create a file or folder with such a name.
pub fn is_reserved_device_name(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && (b'1'..=b'9').contains(&bytes[3])
}

/// The bidi and zero-width format characters that can spoof a name (spec section 6).
pub fn is_hidden_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// True for a name that stays inside the extraction folder and can be created on every
/// platform: not absolute, no `:` (drive letters and NTFS streams), no control or hidden
/// format character, no `..` segment, no segment ending in a dot or a space and no Windows
/// reserved device name (with either kind of separator).
fn is_plain_name(name: &str) -> bool {
    if name.starts_with('/') || name.starts_with('\\') || name.contains(':') {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || is_hidden_format_char(c))
    {
        return false;
    }
    name.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .all(|part| {
            part != ".."
                && !part.ends_with('.')
                && !part.ends_with(' ')
                && !is_reserved_device_name(part)
        })
}

/// The key two entry names collide on, on a case-folding disk.
fn folded(name: &str) -> String {
    name.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn is_symlink(unix_mode: Option<u32>) -> bool {
    unix_mode.is_some_and(|mode| mode & S_IFMT == S_IFLNK)
}

/// Checks a staged ZIP before the importer sees it. It refuses a name that `is_plain_name`
/// refuses, two entries whose names match case-folded, a symlink entry, more than
/// `MAX_ZIP_ENTRIES` entries and more than `MAX_UNZIPPED_BYTES` unzipped. The size is counted
/// by decompressing every entry into a counting sink, so a lying declared size cannot pass.
///
/// The file is opened through the `ImportFiles` port, as the crate rule requires. A missing
/// or unreadable file is an I/O error from `open_read`. The texts are fixed sentences and
/// never carry an entry name.
pub fn validate(files: &dyn ImportFiles, path: &Path) -> ImportResult<()> {
    let file = files.open_read(path)?;
    validate_archive(file)
}

/// Reads the entry count that the end-of-central-directory record declares. The zip reader
/// keeps entries in a map keyed by name, so an exact duplicate name silently collapses into
/// one entry. Comparing this count with the reader's count exposes such a duplicate.
/// Returns `None` when no consistent record is found.
fn declared_entry_count<R: Read + Seek>(reader: &mut R) -> io::Result<Option<u16>> {
    const EOCD_LEN: usize = 22;
    const MAX_TAIL: u64 = (u16::MAX as u64) + EOCD_LEN as u64;
    let size = reader.seek(io::SeekFrom::End(0))?;
    let tail_len = size.min(MAX_TAIL);
    reader.seek(io::SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    reader.read_exact(&mut tail)?;
    reader.seek(io::SeekFrom::Start(0))?;
    if tail.len() < EOCD_LEN {
        return Ok(None);
    }
    for start in (0..=tail.len() - EOCD_LEN).rev() {
        if tail[start..start + 4] != [0x50, 0x4b, 0x05, 0x06] {
            continue;
        }
        let comment_len = u16::from_le_bytes([tail[start + 20], tail[start + 21]]) as usize;
        if start + EOCD_LEN + comment_len == tail.len() {
            return Ok(Some(u16::from_le_bytes([
                tail[start + 10],
                tail[start + 11],
            ])));
        }
    }
    Ok(None)
}

fn validate_archive<R: Read + Seek>(mut reader: R) -> ImportResult<()> {
    let declared = declared_entry_count(&mut reader)?;
    let mut archive = zip::ZipArchive::new(reader).map_err(unreadable)?;
    if archive.is_empty() {
        return Err(ImportError::EmptyZip);
    }
    if declared.is_some_and(|count| usize::from(count) != archive.len()) {
        return Err(unsafe_zip("the zip holds two entries with the same name"));
    }
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(unsafe_zip("the zip has too many entries"));
    }

    let mut total: u64 = 0;
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(unreadable)?;
        if !is_plain_name(entry.name()) || entry.enclosed_name().is_none() {
            return Err(unsafe_zip("an entry has an unsafe path"));
        }
        if !seen.insert(folded(entry.name())) {
            return Err(unsafe_zip("the zip holds two entries with the same name"));
        }
        if is_symlink(entry.unix_mode()) {
            return Err(unsafe_zip("the zip holds a symbolic link"));
        }
        if entry.is_dir() {
            continue;
        }
        // Read one byte past what is left, so the limit is exact and a bomb stops early.
        let allowed = MAX_UNZIPPED_BYTES - total;
        let copied =
            io::copy(&mut (&mut entry).take(allowed + 1), &mut CountingSink).map_err(unreadable)?;
        total += copied;
        if total > MAX_UNZIPPED_BYTES {
            return Err(unsafe_zip("the zip expands to too many bytes"));
        }
    }
    Ok(())
}

/// Requires that the ZIP holds exactly one collection: an `opencollection.yml` or
/// `bruno.json` at the root, or one wrapper folder that holds it and everything else. A
/// Bruno workspace (`workspace.yml` at the root or in the wrapper) is refused, because the
/// importer would create several collections from it.
pub fn require_collection_root(files: &dyn ImportFiles, path: &Path) -> ImportResult<()> {
    let file = files.open_read(path)?;
    let archive = zip::ZipArchive::new(file).map_err(unreadable)?;
    check_layout(archive.file_names())
}

fn check_layout<'a>(names: impl Iterator<Item = &'a str>) -> ImportResult<()> {
    let mut tops: BTreeSet<String> = BTreeSet::new();
    let mut loose_root_files = false;
    let mut root_marker = false;
    let mut nested_marker = false;
    let mut workspace = false;

    for name in names {
        let normal = name.replace('\\', "/");
        let is_dir = normal.ends_with('/');
        let parts: Vec<&str> = normal
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let Some(first) = parts.first().copied() else {
            continue;
        };
        if parts.len() == 1 && !is_dir {
            loose_root_files = true;
            root_marker |= COLLECTION_MARKERS.contains(&first);
            workspace |= first == "workspace.yml";
            continue;
        }
        tops.insert(first.to_string());
        if parts.len() == 2 && !is_dir {
            nested_marker |= COLLECTION_MARKERS.contains(&parts[1]);
            workspace |= parts[1] == "workspace.yml";
        }
    }

    if workspace {
        return Err(unsafe_zip("the zip holds a workspace, not one collection"));
    }
    if root_marker || (nested_marker && tops.len() == 1 && !loose_root_files) {
        return Ok(());
    }
    Err(unsafe_zip("the zip does not hold exactly one collection"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::error::ImportError;
    use crate::importer::test_files::StdFiles;

    const MIB: u64 = 1024 * 1024;

    /// The crate-local filesystem port for tests.
    fn files() -> StdFiles {
        StdFiles
    }

    fn opts() -> SimpleFileOptions {
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
    }

    /// Builds a ZIP on disk. The returned `TempDir` must stay alive while the path is used.
    fn build(fill: impl FnOnce(&mut ZipWriter<std::fs::File>)) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("in.zip");
        let file = std::fs::File::create(&path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        fill(&mut writer);
        writer.finish().expect("finish zip");
        (dir, path)
    }

    fn add_text(w: &mut ZipWriter<std::fs::File>, name: &str, text: &str) {
        w.start_file(name, opts()).expect("start file");
        w.write_all(text.as_bytes()).expect("write file");
    }

    /// Writes `total` zero bytes into one entry. Zeros deflate to almost nothing, so the
    /// test stays fast.
    fn add_zeros(w: &mut ZipWriter<std::fs::File>, name: &str, total: u64) {
        w.start_file(name, opts()).expect("start file");
        let chunk = vec![0u8; MIB as usize];
        let mut left = total;
        while left > 0 {
            let n = left.min(MIB) as usize;
            w.write_all(&chunk[..n]).expect("write zeros");
            left -= n as u64;
        }
    }

    fn is_unsafe(result: &ImportResult<()>) -> bool {
        matches!(result, Err(ImportError::UnsafeTransferZip(_)))
    }

    fn good_collection(w: &mut ZipWriter<std::fs::File>) {
        add_text(
            w,
            "orders/opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: orders\n",
        );
        add_text(w, "orders/get.yml", "info:\n  name: get\n");
    }

    // ---- validate ----

    #[test]
    fn validate_accepts_a_good_collection_zip() {
        let (_dir, path) = build(good_collection);
        validate(&files(), &path).expect("a good zip passes");
    }

    #[test]
    fn validate_accepts_a_zip_with_one_root_file() {
        let (_dir, path) = build(|w| add_text(w, "opencollection.yml", "info:\n  name: a\n"));
        validate(&files(), &path).expect("a single root file is fine for the safety check");
    }

    #[test]
    fn validate_accepts_non_ascii_entry_names() {
        let (_dir, path) = build(|w| {
            add_text(w, "Café – Ünï API/opencollection.yml", "info:\n  name: x\n");
            add_text(w, "Café – Ünï API/日本語.yml", "info:\n  name: y\n");
        });
        validate(&files(), &path).expect("non-ascii names pass");
    }

    #[test]
    fn validate_refuses_a_zip_slip_name() {
        let (_dir, path) = build(|w| add_text(w, "../evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_dot_dot_segment_in_the_middle() {
        // `enclosed_name()` allows `a/../b`, because the depth never goes below zero.
        let (_dir, path) = build(|w| add_text(w, "a/../b.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_backslash_traversal() {
        let (_dir, path) = build(|w| add_text(w, "..\\evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_an_absolute_path() {
        let (_dir, path) = build(|w| add_text(w, "/etc/passwd", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_windows_drive_path() {
        let (_dir, path) = build(|w| add_text(w, "C:/evil.txt", "x"));
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_a_symlink_entry() {
        let (_dir, path) = build(|w| {
            good_collection(w);
            w.add_symlink("orders/link", "/etc/passwd", opts())
                .expect("add symlink");
        });
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_accepts_exactly_the_entry_limit_and_refuses_one_more() {
        let (_a, at_limit) = build(|w| {
            for i in 0..MAX_ZIP_ENTRIES {
                add_text(w, &format!("c/f{i}.txt"), "");
            }
        });
        validate(&files(), &at_limit).expect("5000 entries pass");

        let (_b, over) = build(|w| {
            for i in 0..=MAX_ZIP_ENTRIES {
                add_text(w, &format!("c/f{i}.txt"), "");
            }
        });
        assert!(
            is_unsafe(&validate(&files(), &over)),
            "5001 entries must fail"
        );
    }

    #[test]
    fn validate_accepts_exactly_the_unzipped_limit_and_refuses_one_more_byte() {
        let (_a, at_limit) = build(|w| add_zeros(w, "c/big.bin", MAX_UNZIPPED_BYTES));
        validate(&files(), &at_limit).expect("exactly 50 MiB passes");

        let (_b, over) = build(|w| add_zeros(w, "c/big.bin", MAX_UNZIPPED_BYTES + 1));
        assert!(
            is_unsafe(&validate(&files(), &over)),
            "50 MiB + 1 byte must fail"
        );
    }

    #[test]
    fn validate_counts_all_entries_together() {
        let (_dir, path) = build(|w| {
            for i in 0..3 {
                add_zeros(w, &format!("c/part{i}.bin"), 20 * MIB);
            }
        });
        assert!(
            is_unsafe(&validate(&files(), &path)),
            "3 x 20 MiB is over the limit"
        );
    }

    /// Patches the declared uncompressed size of the only entry, in the local header and in
    /// the central directory.
    fn lie_about_size(path: &Path, declared: u32) {
        let mut bytes = std::fs::read(path).expect("read zip");
        let central = bytes
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .expect("central directory header");
        bytes[central + 24..central + 28].copy_from_slice(&declared.to_le_bytes());
        bytes[22..26].copy_from_slice(&declared.to_le_bytes());
        std::fs::write(path, bytes).expect("write zip");
    }

    #[test]
    fn validate_refuses_a_zip_whose_declared_size_lies() {
        // 60 MiB of data that claims to be 1 KiB. Either the counter sees 60 MiB or the
        // zip reader fails its size or CRC check. Both must end in an error.
        let (_dir, path) = build(|w| add_zeros(w, "c/big.bin", 60 * MIB));
        lie_about_size(&path, 1024);
        assert!(validate(&files(), &path).is_err());
    }

    #[test]
    fn validate_refuses_a_file_that_is_not_a_zip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("not.zip");
        std::fs::write(&path, b"this is not a zip file").expect("write");
        assert!(matches!(
            validate(&files(), &path),
            Err(ImportError::ZipExtractionFailed(_))
        ));
    }

    #[test]
    fn validate_refuses_an_empty_zip() {
        let (_dir, path) = build(|_| {});
        assert!(matches!(
            validate(&files(), &path),
            Err(ImportError::EmptyZip)
        ));
    }

    #[test]
    fn validate_reports_a_missing_file_as_an_io_error() {
        let dir = TempDir::new().expect("tempdir");
        let result = validate(&files(), &dir.path().join("missing.zip"));
        assert!(matches!(result, Err(ImportError::IoError(_))));
    }

    #[test]
    fn the_refusal_text_does_not_echo_the_entry_name() {
        let (_dir, path) = build(|w| add_text(w, "../secret-name-9f2.txt", "x"));
        let text = validate(&files(), &path).expect_err("refused").to_string();
        assert!(!text.contains("secret-name-9f2"), "text was: {text}");
    }

    #[test]
    fn validate_refuses_a_colon_anywhere_in_an_entry_name() {
        // A colon names an NTFS alternate data stream on Windows.
        let (_dir, path) = build(|w| {
            good_collection(w);
            add_text(w, "orders/get.yml:hidden", "x");
        });
        assert!(is_unsafe(&validate(&files(), &path)));
    }

    #[test]
    fn validate_refuses_windows_reserved_names_in_any_segment() {
        for name in [
            "orders/con.yml",
            "orders/NUL",
            "Aux/get.yml",
            "orders/com1.txt",
            "orders/LPT9.yml",
        ] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name} must fail");
        }
        // Longer names that only start with a reserved word are fine.
        let (_dir, path) = build(|w| {
            add_text(w, "orders/CONSOLE.yml", "x");
            add_text(w, "orders/com10.yml", "x");
        });
        validate(&files(), &path).expect("CONSOLE and COM10 are not reserved");
    }

    #[test]
    fn validate_refuses_a_trailing_dot_or_space_in_a_segment() {
        for name in [
            "orders./get.yml",
            "orders/get.yml.",
            "orders /get.yml",
            "orders/get.yml ",
        ] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name:?} must fail");
        }
    }

    #[test]
    fn validate_refuses_control_and_hidden_format_characters() {
        for name in [
            "orders/a\u{7}b.yml",
            "orders/\u{202E}lmy.exe",
            "orders/a\u{200B}b.yml",
            "orders/\u{FEFF}x.yml",
        ] {
            let (_dir, path) = build(|w| add_text(w, name, "x"));
            assert!(is_unsafe(&validate(&files(), &path)), "{name:?} must fail");
        }
    }

    #[test]
    fn validate_refuses_duplicate_entry_names_compared_case_folded() {
        // `ZipWriter` refuses an exact duplicate, so write `gex.yml` and patch the name bytes
        // (local and central headers) to `get.yml`. The CRC covers the data, not the name.
        let (_a, exact) = build(|w| {
            add_text(w, "orders/get.yml", "a");
            add_text(w, "orders/gex.yml", "b");
        });
        let bytes = std::fs::read(&exact).expect("read zip");
        let patched = String::from_utf8_lossy(&bytes).contains("gex.yml");
        assert!(patched, "the fixture name is in the file");
        let mut out = bytes.clone();
        for i in 0..out.len().saturating_sub(7) {
            if &out[i..i + 7] == b"gex.yml" {
                out[i + 2] = b't';
            }
        }
        std::fs::write(&exact, out).expect("write zip");
        // The zip reader itself may refuse the duplicate (`ZipExtractionFailed`); either refusal is fine.
        assert!(
            validate(&files(), &exact).is_err(),
            "an exact duplicate must fail"
        );

        let (_b, folded) = build(|w| {
            add_text(w, "orders/Get.yml", "a");
            add_text(w, "Orders/get.YML", "b");
        });
        assert!(
            is_unsafe(&validate(&files(), &folded)),
            "a case-folded duplicate must fail"
        );
    }

    #[test]
    fn the_shared_name_helpers_match_the_spec_lists() {
        for name in [
            "CON",
            "con.txt",
            "Prn",
            "aux.yml",
            "NUL",
            "com1",
            "COM9.json",
            "lpt1",
            "LPT9",
        ] {
            assert!(is_reserved_device_name(name), "{name}");
        }
        for name in ["CONSOLE", "com10", "lpt0", "nul_", "orders"] {
            assert!(!is_reserved_device_name(name), "{name}");
        }
        for c in [
            '\u{00AD}', '\u{061C}', '\u{180E}', '\u{200B}', '\u{200F}', '\u{202A}', '\u{202E}',
            '\u{2060}', '\u{2064}', '\u{2066}', '\u{2069}', '\u{FEFF}',
        ] {
            assert!(is_hidden_format_char(c), "{:04X}", c as u32);
        }
        for c in ['a', 'é', '日', ' ', '\u{2065}'] {
            assert!(!is_hidden_format_char(c), "{:04X}", c as u32);
        }
    }

    // ---- require_collection_root ----

    #[test]
    fn require_collection_root_accepts_a_wrapped_modern_collection() {
        let (_dir, path) = build(good_collection);
        require_collection_root(&files(), &path).expect("one wrapped collection");
    }

    #[test]
    fn require_collection_root_accepts_a_wrapped_legacy_collection() {
        let (_dir, path) = build(|w| {
            add_text(w, "Old API/bruno.json", "{\"name\":\"Old API\"}");
            add_text(w, "Old API/get.bru", "meta {\n  name: get\n}\n");
        });
        require_collection_root(&files(), &path).expect("one wrapped legacy collection");
    }

    #[test]
    fn require_collection_root_accepts_a_flat_root_marker() {
        let (_dir, path) = build(|w| add_text(w, "opencollection.yml", "info:\n  name: a\n"));
        require_collection_root(&files(), &path)
            .expect("a single root marker file is a collection");
    }

    #[test]
    fn require_collection_root_accepts_non_ascii_wrapper_names() {
        let (_dir, path) = build(|w| {
            add_text(w, "Café – Ünï API/opencollection.yml", "info:\n  name: x\n");
        });
        require_collection_root(&files(), &path).expect("non-ascii wrapper");
    }

    #[test]
    fn require_collection_root_refuses_a_single_file_without_a_marker() {
        let (_dir, path) = build(|w| add_text(w, "notes.txt", "hello"));
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_a_workspace() {
        let (_dir, path) = build(|w| {
            add_text(w, "workspace.yml", "opencollection: \"1.0.0\"\n");
            add_text(w, "opencollection.yml", "info:\n  name: a\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));

        let (_dir2, wrapped) = build(|w| {
            add_text(w, "ws/workspace.yml", "opencollection: \"1.0.0\"\n");
            add_text(w, "ws/opencollection.yml", "info:\n  name: a\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &wrapped)));
    }

    #[test]
    fn require_collection_root_refuses_two_top_level_folders() {
        let (_dir, path) = build(|w| {
            add_text(w, "one/opencollection.yml", "info:\n  name: a\n");
            add_text(w, "two/opencollection.yml", "info:\n  name: b\n");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_loose_files_next_to_the_wrapper() {
        let (_dir, path) = build(|w| {
            good_collection(w);
            add_text(w, "readme.txt", "hello");
        });
        assert!(is_unsafe(&require_collection_root(&files(), &path)));
    }

    #[test]
    fn require_collection_root_refuses_a_file_that_is_not_a_zip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("not.zip");
        std::fs::write(&path, b"nope").expect("write");
        assert!(require_collection_root(&files(), &path).is_err());
    }
}
