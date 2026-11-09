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
    // One trailing separator is allowed for a directory entry. Empty and `.` segments are
    // not, because `a//b` and `a/./b` extract onto the same file as `a/b`.
    let trimmed = name
        .strip_suffix('/')
        .or_else(|| name.strip_suffix('\\'))
        .unwrap_or(name);
    if trimmed.is_empty() {
        return false;
    }
    trimmed.split(['/', '\\']).all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
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

/// The same strict check as `validate`, for a ZIP held in memory (a sync payload).
pub fn validate_bytes(bytes: &[u8]) -> ImportResult<()> {
    validate_archive(io::Cursor::new(bytes))
}

/// Checks the end-of-central-directory record of an archive the reader already accepted. The
/// zip reader keeps entries in a map keyed by name, so an exact duplicate name silently
/// collapses into one entry, and it tolerates trailing bytes and zip64 records. This check
/// fails closed: the record must be the only one in the tail, must end the file exactly (no
/// comment, no trailing byte), must carry equal disk and total counts that equal the reader's
/// count, and no zip64 sentinel or locator may appear. The central directory the reader chose
/// must also be the one this record names: no prepended data, the same start offset, and it
/// must end exactly where the record starts (the reader scans the whole file backwards and
/// skips a decoy record that fails its own checks).
fn check_end_record<R: Read + Seek>(
    reader: &mut R,
    reader_count: usize,
    reader_offset: u64,
    reader_cd_start: u64,
) -> ImportResult<()> {
    const EOCD_LEN: usize = 22;
    const MAX_TAIL: u64 = (u16::MAX as u64) + EOCD_LEN as u64;
    const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
    let bad = || unsafe_zip("the zip end record is not a plain single record");

    let size = reader.seek(io::SeekFrom::End(0))?;
    let tail_len = size.min(MAX_TAIL);
    reader.seek(io::SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    reader.read_exact(&mut tail)?;
    reader.seek(io::SeekFrom::Start(0))?;
    if tail.len() < EOCD_LEN {
        return Err(bad());
    }
    if tail.windows(4).any(|w| w == ZIP64_LOCATOR_SIG) {
        return Err(bad());
    }
    let start = tail.len() - EOCD_LEN;
    if tail[start..start + 4] != EOCD_SIG {
        return Err(bad());
    }
    // The record at the end of the file must be the only signature in the tail.
    if tail.windows(4).filter(|w| *w == EOCD_SIG).count() != 1 {
        return Err(bad());
    }
    let rec = &tail[start..];
    let le16 = |at: usize| u16::from_le_bytes([rec[at], rec[at + 1]]);
    let le32 = |at: usize| u32::from_le_bytes([rec[at], rec[at + 1], rec[at + 2], rec[at + 3]]);
    let disk_entries = le16(8);
    let total_entries = le16(10);
    if le16(4) != 0
        || le16(6) != 0
        || le16(20) != 0
        || disk_entries != total_entries
        || disk_entries == u16::MAX
        || le32(12) == u32::MAX
        || le32(16) == u32::MAX
        || usize::from(total_entries) != reader_count
        || reader_offset != 0
        || u64::from(le32(16)) != reader_cd_start
        || u64::from(le32(16)) + u64::from(le32(12)) != size - EOCD_LEN as u64
    {
        return Err(bad());
    }
    let cd_start = u64::from(le32(16));
    let cd_end = size - EOCD_LEN as u64;
    walk_central_directory(reader, cd_start, cd_end, usize::from(total_entries)).map_err(|_| bad())
}

/// Walks the central directory header by header, as other readers do (they read until the
/// directory size is used up, not until the entry count). Exactly `count` headers must fill
/// `cd_start..cd_end` with nothing left over, so a hidden entry behind a low count is refused.
/// Every read is bounds-checked, and any short or hostile data is an error.
fn walk_central_directory<R: Read + Seek>(
    reader: &mut R,
    cd_start: u64,
    cd_end: u64,
    count: usize,
) -> io::Result<()> {
    const HEADER_LEN: u64 = 46;
    const CDFH_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let mut pos = cd_start;
    for _ in 0..count {
        if pos.checked_add(HEADER_LEN).is_none_or(|next| next > cd_end) {
            return Err(invalid());
        }
        reader.seek(io::SeekFrom::Start(pos))?;
        let mut header = [0u8; HEADER_LEN as usize];
        reader.read_exact(&mut header)?;
        if header[..4] != CDFH_SIG {
            return Err(invalid());
        }
        let var_len: u64 = [28usize, 30, 32]
            .iter()
            .map(|&at| u64::from(u16::from_le_bytes([header[at], header[at + 1]])))
            .sum();
        pos += HEADER_LEN + var_len;
        if pos > cd_end {
            return Err(invalid());
        }
    }
    if pos != cd_end {
        return Err(invalid());
    }
    reader.seek(io::SeekFrom::Start(0))?;
    Ok(())
}

fn validate_archive<R: Read + Seek>(mut reader: R) -> ImportResult<()> {
    let first = zip::ZipArchive::new(&mut reader).map_err(unreadable)?;
    if first.is_empty() {
        return Err(ImportError::EmptyZip);
    }
    let count = first.len();
    let (offset, cd_start) = (first.offset(), first.central_directory_start());
    drop(first);
    check_end_record(&mut reader, count, offset, cd_start)?;
    let mut archive = zip::ZipArchive::new(reader).map_err(unreadable)?;
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

/// Case-insensitive, because detection looks the file up on a case-folding disk too.
fn is_marker(name: &str) -> bool {
    COLLECTION_MARKERS
        .iter()
        .any(|marker| marker.eq_ignore_ascii_case(name))
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
            root_marker |= is_marker(first);
            workspace |= first.eq_ignore_ascii_case("workspace.yml");
            continue;
        }
        tops.insert(first.to_string());
        if parts.len() == 2 && !is_dir {
            nested_marker |= is_marker(parts[1]);
            workspace |= parts[1].eq_ignore_ascii_case("workspace.yml");
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

    /// Builds a ZIP in memory from `(name, bytes)` entries.
    fn zip_in_memory(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer.start_file(*name, opts()).expect("start file");
            writer.write_all(bytes).expect("write file");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn validate_bytes_runs_the_strict_check_in_memory() {
        let good = zip_in_memory(&[("a.yml", b"x")]);
        assert!(validate_bytes(&good).is_ok());

        let unsafe_name = zip_in_memory(&[("../a.yml", b"x")]);
        assert!(is_unsafe(&validate_bytes(&unsafe_name)));

        let twins = zip_in_memory(&[("A.yml", b"x"), ("a.yml", b"y")]);
        assert!(is_unsafe(&validate_bytes(&twins)));

        let mut trailing = good.clone();
        trailing.push(0);
        assert!(
            validate_bytes(&trailing).is_err(),
            "a byte after the end record"
        );
        assert!(validate_bytes(b"not a zip").is_err());
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

    // ---- fix round 1: adversarial cases ----

    /// An in-memory ZIP, built the way `rocket-export`'s Bruno exporter builds one.
    fn mem_zip(names: &[&str]) -> Vec<u8> {
        let mut w = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (i, n) in names.iter().enumerate() {
            w.start_file(*n, SimpleFileOptions::default())
                .expect("start");
            w.write_all(format!("data{i}").as_bytes()).expect("write");
        }
        w.finish().expect("finish").into_inner()
    }

    /// Three entries where the last name is patched into a duplicate of the second.
    fn dup_bytes() -> Vec<u8> {
        let mut b = mem_zip(&[
            "orders/opencollection.yml",
            "orders/get.yml",
            "orders/gex.yml",
        ]);
        for i in 0..b.len() - 7 {
            if &b[i..i + 7] == b"gex.yml" {
                b[i + 2] = b't';
            }
        }
        b
    }

    fn write_bytes(bytes: &[u8]) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("in.zip");
        std::fs::write(&path, bytes).expect("write zip");
        (dir, path)
    }

    fn refused_unsafe(bytes: &[u8]) -> bool {
        let (_dir, path) = write_bytes(bytes);
        is_unsafe(&validate(&files(), &path))
    }

    #[test]
    fn validate_accepts_a_zip_built_like_the_exporter() {
        let bytes = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
        let (_dir, path) = write_bytes(&bytes);
        validate(&files(), &path).expect("exporter style zip passes");
        require_collection_root(&files(), &path).expect("exporter style layout passes");
    }

    #[test]
    fn validate_refuses_a_duplicate_with_a_trailing_byte() {
        let mut b = dup_bytes();
        b.push(0);
        assert!(refused_unsafe(&b));
    }

    #[test]
    fn validate_refuses_a_duplicate_when_the_total_field_lies() {
        let mut b = dup_bytes();
        let e = b.len() - 22;
        b[e + 10] = 2;
        b[e + 11] = 0;
        assert!(refused_unsafe(&b));
    }

    #[test]
    fn validate_refuses_a_duplicate_when_the_disk_count_lies() {
        let mut b = dup_bytes();
        let e = b.len() - 22;
        b[e + 8] = 2;
        b[e + 9] = 0;
        assert!(refused_unsafe(&b));
    }

    #[test]
    fn validate_refuses_a_zip64_sentinel_in_the_eocd() {
        let mut b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
        let e = b.len() - 22;
        b[e + 16..e + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        let (_dir, path) = write_bytes(&b);
        assert!(validate(&files(), &path).is_err());
    }

    #[test]
    fn validate_refuses_a_zip64_locator_signature() {
        // A locator signature right before the EOCD, as a zip64 archive would have.
        let b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
        let e = b.len() - 22;
        let mut out = b[..e].to_vec();
        out.extend_from_slice(&[0x50, 0x4b, 0x06, 0x07]);
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&b[e..]);
        assert!(refused_unsafe(&out));
    }

    #[test]
    fn validate_refuses_a_fake_eocd_inside_the_comment() {
        let mut b = dup_bytes();
        let e = b.len() - 22;
        // The fake record is a copy of the real one, so the zip reader accepts it as well.
        let fake = b[e..e + 22].to_vec();
        // The real record now claims a 22 byte comment that holds the fake record.
        b[e + 20] = 22;
        b.extend_from_slice(&fake);
        assert!(refused_unsafe(&b));
    }

    /// A hand-built end record with equal disk and total counts and no comment.
    fn eocd(count: u16, cd_size: u32, cd_off: u32) -> Vec<u8> {
        let mut e = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0];
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&cd_size.to_le_bytes());
        e.extend_from_slice(&cd_off.to_le_bytes());
        e.extend_from_slice(&[0, 0]);
        e
    }

    #[test]
    fn validate_refuses_a_decoy_eocd_the_reader_skips() {
        // The reader scans the whole file backwards and skips an end record whose central
        // directory offset is bad, so it lands on the older real record far from the end.
        let mut a = dup_bytes();
        a.extend(std::iter::repeat_n(0u8, 70_000));
        a.extend(eocd(2, 0, 0xFFFF_FFF0));
        assert!(refused_unsafe(&a), "case A");

        // The decoy offset looks plausible but no central header sits there.
        let mut b = dup_bytes();
        b.extend(std::iter::repeat_n(0u8, 70_000));
        let off = (b.len() - 100) as u32;
        b.extend(eocd(2, 10, off));
        assert!(refused_unsafe(&b), "case B");
    }

    /// Sets both entry counts of the end record to `count`.
    fn set_counts(b: &mut [u8], count: u16) {
        let e = b.len() - 22;
        b[e + 8..e + 10].copy_from_slice(&count.to_le_bytes());
        b[e + 10..e + 12].copy_from_slice(&count.to_le_bytes());
    }

    #[test]
    fn validate_refuses_a_central_directory_longer_than_the_counts_say() {
        // Case C: three headers, the counts say two, sizes and offsets stay true.
        let mut b = dup_bytes();
        set_counts(&mut b, 2);
        assert!(refused_unsafe(&b));
    }

    #[test]
    fn validate_refuses_a_hidden_traversal_entry_behind_a_low_count() {
        // Case C2: the hidden third entry has a traversal name.
        let mut b = mem_zip(&[
            "orders/opencollection.yml",
            "orders/get.yml",
            "orders/xx/yy/e",
        ]);
        let from = b"orders/xx/yy/e";
        let to = b"orders/../../e";
        for i in 0..b.len() - from.len() {
            if &b[i..i + from.len()] == from {
                b[i..i + to.len()].copy_from_slice(to);
            }
        }
        set_counts(&mut b, 2);
        assert!(refused_unsafe(&b));
    }

    /// Start and size of the central directory of an exporter-style zip.
    fn cd_span(b: &[u8]) -> (usize, usize) {
        let e = b.len() - 22;
        let size = u32::from_le_bytes([b[e + 12], b[e + 13], b[e + 14], b[e + 15]]) as usize;
        let off = u32::from_le_bytes([b[e + 16], b[e + 17], b[e + 18], b[e + 19]]) as usize;
        (off, size)
    }

    #[test]
    fn validate_refuses_a_bad_central_header_signature() {
        let mut b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
        let (off, _) = cd_span(&b);
        b[off + 3] = 0x09;
        let (_dir, path) = write_bytes(&b);
        assert!(validate(&files(), &path).is_err());
    }

    #[test]
    fn validate_refuses_header_lengths_that_run_past_the_directory() {
        for at in [28usize, 30, 32] {
            let mut b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
            let (off, _) = cd_span(&b);
            b[off + at] = 0xF0;
            let (_dir, path) = write_bytes(&b);
            assert!(validate(&files(), &path).is_err(), "field at {at}");
        }
    }

    #[test]
    fn validate_refuses_a_central_directory_size_that_is_too_small_or_too_large() {
        for delta in [-1i64, 1] {
            let mut b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
            let e = b.len() - 22;
            let (_, size) = cd_span(&b);
            let patched = (size as i64 + delta) as u32;
            b[e + 12..e + 16].copy_from_slice(&patched.to_le_bytes());
            let (_dir, path) = write_bytes(&b);
            assert!(validate(&files(), &path).is_err(), "delta {delta}");
        }
    }

    #[test]
    fn validate_refuses_prepended_data() {
        let mut b = vec![0u8; 100];
        b.extend(mem_zip(&["orders/opencollection.yml", "orders/get.yml"]));
        let (_dir, path) = write_bytes(&b);
        assert!(validate(&files(), &path).is_err());
    }

    #[test]
    fn validate_refuses_the_baseline_duplicate_without_a_decoy() {
        assert!(refused_unsafe(&dup_bytes()));
    }

    #[test]
    fn validate_refuses_a_comment_after_the_eocd() {
        let mut b = mem_zip(&["orders/opencollection.yml", "orders/get.yml"]);
        let e = b.len() - 22;
        b[e + 20] = 3;
        b.extend_from_slice(b"abc");
        assert!(refused_unsafe(&b));
    }

    #[test]
    fn validate_refuses_dot_and_empty_segments_and_an_empty_name() {
        for dup in ["orders/./get.yml", "orders//get.yml"] {
            let b = mem_zip(&["orders/opencollection.yml", "orders/get.yml", dup]);
            assert!(refused_unsafe(&b), "{dup:?} must fail");
        }
        let b = mem_zip(&["orders/opencollection.yml", ""]);
        assert!(refused_unsafe(&b), "an empty name must fail");
    }

    #[test]
    fn validate_accepts_a_directory_entry_with_one_trailing_slash() {
        let (_dir, path) = build(|w| {
            w.add_directory("orders/", opts()).expect("dir");
            add_text(w, "orders/opencollection.yml", "info:\n  name: a\n");
        });
        validate(&files(), &path).expect("a directory entry passes");
    }

    #[test]
    fn require_collection_root_refuses_a_workspace_in_any_letter_case() {
        let b = mem_zip(&["orders/opencollection.yml", "orders/Workspace.yml"]);
        let (_dir, path) = write_bytes(&b);
        assert!(is_unsafe(&require_collection_root(&files(), &path)));

        let b = mem_zip(&["WORKSPACE.YML", "OpenCollection.yml"]);
        let (_dir2, path2) = write_bytes(&b);
        assert!(is_unsafe(&require_collection_root(&files(), &path2)));
    }
}
