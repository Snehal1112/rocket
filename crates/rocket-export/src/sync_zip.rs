//! Reads a Bruno export ZIP back into collection-relative files.

use std::io::{Read, Seek, SeekFrom};

use rocket_shared::error::{DomainError, DomainResult};

/// The most entries a sync ZIP may hold, as for a transfer ZIP.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// The most bytes a sync ZIP may hold unzipped, as for a transfer ZIP.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;

/// Entries of a ZIP with the single root folder removed, sorted by path.
pub fn zip_entries<R: Read + Seek>(reader: R) -> DomainResult<Vec<(String, Vec<u8>)>> {
    let mut reader = reader;
    let declared = declared_entries(&mut reader);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|_| DomainError::InvalidInput("The archive cannot be read.".into()))?;
    // The zip crate keeps one entry per name, so a repeated name shows as a lower count.
    if declared.is_some_and(|count| count != archive.len()) {
        return Err(DomainError::InvalidInput(
            "The archive has the same file twice.".into(),
        ));
    }
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(DomainError::InvalidInput(
            "The archive has too many files.".into(),
        ));
    }
    let mut root: Option<String> = None;
    let mut total: u64 = 0;
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| DomainError::InvalidInput("The archive cannot be read.".into()))?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let rest = split_root(&name, &mut root)?;
        let too_big = || DomainError::InvalidInput("The archive is too big.".into());
        if total.saturating_add(entry.size()) > MAX_UNZIPPED_BYTES {
            return Err(too_big());
        }
        // The declared size is not trusted, so the real bytes are counted as they come.
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_UNZIPPED_BYTES - total + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| DomainError::Io("The archive cannot be read.".into()))?;
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_UNZIPPED_BYTES {
            return Err(too_big());
        }
        out.push((rest, bytes));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    if out.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(DomainError::InvalidInput(
            "The archive has the same file twice.".into(),
        ));
    }
    let paths: std::collections::HashSet<&str> = out.iter().map(|(p, _)| p.as_str()).collect();
    for (path, _) in &out {
        let mut at = 0;
        while let Some(slash) = path[at..].find('/') {
            at += slash;
            if paths.contains(&path[..at]) {
                return Err(DomainError::InvalidInput(
                    "The archive has a file and a folder with the same name.".into(),
                ));
            }
            at += 1;
        }
    }
    Ok(out)
}

/// The entry count that the end-of-central-directory record declares, if it is plain ZIP.
fn declared_entries<R: Read + Seek>(reader: &mut R) -> Option<usize> {
    const EOCD: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const TAIL: u64 = 22 + 65535;
    let end = reader.seek(SeekFrom::End(0)).ok()?;
    let start = end.saturating_sub(TAIL);
    reader.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    reader.by_ref().take(TAIL).read_to_end(&mut tail).ok()?;
    let at = tail.windows(4).rposition(|w| w == EOCD)?;
    let count = u16::from_le_bytes([*tail.get(at + 10)?, *tail.get(at + 11)?]);
    // 0xFFFF means ZIP64, where this record does not hold the count.
    (count != u16::MAX).then_some(usize::from(count))
}

/// Checks one file name and returns the part after the root folder.
fn split_root(name: &str, root: &mut Option<String>) -> DomainResult<String> {
    let unsafe_name = || DomainError::InvalidInput("The archive has an unsafe file name.".into());
    if name.contains('\\') || name.starts_with('/') {
        return Err(unsafe_name());
    }
    let (first, rest) = name.split_once('/').ok_or_else(unsafe_name)?;
    if name
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(unsafe_name());
    }
    match root {
        Some(existing) if existing != first => Err(DomainError::InvalidInput(
            "The archive has more than one root folder.".into(),
        )),
        Some(_) => Ok(rest.to_string()),
        None => {
            *root = Some(first.to_string());
            Ok(rest.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bruno::{bruno_zip, is_synced_path};
    use crate::ports::SourceFile;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn make_zip(entries: &[(&str, &str)]) -> Cursor<Vec<u8>> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default())
                .expect("start");
            w.write_all(body.as_bytes()).expect("write");
        }
        Cursor::new(w.finish().expect("finish").into_inner())
    }

    #[test]
    fn the_single_root_folder_is_stripped_and_the_entries_are_sorted() {
        let zip = make_zip(&[
            ("My API (copy)/users/get.yml", "b: 2"),
            ("My API (copy)/opencollection.yml", "a: 1"),
        ]);
        let got = zip_entries(zip).expect("reads");
        let names: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, vec!["opencollection.yml", "users/get.yml"]);
        assert_eq!(got[0].1, b"a: 1".to_vec());
    }

    #[test]
    fn an_empty_zip_has_no_entries() {
        let got = zip_entries(make_zip(&[])).expect("reads");
        assert!(got.is_empty());
    }

    #[test]
    fn two_root_folders_are_refused() {
        let zip = make_zip(&[("a/x.yml", "1"), ("b/y.yml", "2")]);
        assert!(matches!(
            zip_entries(zip),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn a_file_outside_a_root_folder_is_refused() {
        assert!(matches!(
            zip_entries(make_zip(&[("x.yml", "1")])),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn unsafe_names_are_refused() {
        for name in [
            "r/../x.yml",
            "r/./x.yml",
            "r//x.yml",
            "r\\x.yml",
            "/r/x.yml",
        ] {
            assert!(
                matches!(
                    zip_entries(make_zip(&[(name, "1")])),
                    Err(DomainError::InvalidInput(_))
                ),
                "{name} should be refused"
            );
        }
    }

    #[test]
    fn a_duplicate_path_is_refused() {
        // The writer refuses a duplicate name, so write two names and patch one to match.
        let mut bytes = make_zip(&[("r/x.yml", "1"), ("r/y.yml", "2")]).into_inner();
        let from = b"r/y.yml";
        let to = b"r/x.yml";
        let mut patched = 0;
        let mut at = 0;
        while at + from.len() <= bytes.len() {
            if &bytes[at..at + from.len()] == from {
                bytes[at..at + from.len()].copy_from_slice(to);
                patched += 1;
            }
            at += 1;
        }
        // One local header and one central directory record.
        assert_eq!(patched, 2);
        let err = zip_entries(Cursor::new(bytes)).expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(err.to_string().contains("same file twice"), "{err}");
    }

    #[test]
    fn is_synced_path_mirrors_the_export_filter() {
        let cases = [
            ("opencollection.yml", true),
            ("users/get.yml", true),
            ("users/get.yaml", true),
            ("protos/a.proto", true),
            ("lib/util.js", true),
            ("environments/dev.yml", false),
            ("flows/f.yml", false),
            (".rocket/c.yml", false),
            ("workspace.yml", false),
            (".git/config", false),
            ("node_modules/a.js", false),
            ("notes.txt", false),
        ];
        let files: Vec<SourceFile> = cases
            .iter()
            .map(|(path, _)| SourceFile {
                rel_path: path.to_string(),
                bytes: b"a: 1\n".to_vec(),
            })
            .collect();
        let (zip, _) = bruno_zip(files, "c", false).expect("exports");
        let carried: Vec<String> = zip_entries(Cursor::new(zip))
            .expect("reads")
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        for (path, expected) in cases {
            assert_eq!(is_synced_path(path), expected, "{path}");
            assert_eq!(
                carried.iter().any(|p| p == path),
                expected,
                "{path} against the export"
            );
        }
    }

    #[test]
    fn an_entry_that_inflates_past_its_declared_size_is_refused() {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("r/big.yml", opts).expect("start");
        let chunk = vec![b'a'; 1024 * 1024];
        for _ in 0..(MAX_UNZIPPED_BYTES / 1024 / 1024 + 2) {
            w.write_all(&chunk).expect("write");
        }
        let mut bytes = w.finish().expect("finish").into_inner();
        // Patch the declared uncompressed size to one byte in both headers.
        let local = bytes
            .windows(4)
            .position(|x| x == [0x50, 0x4b, 0x03, 0x04])
            .expect("local");
        let central = bytes
            .windows(4)
            .rposition(|x| x == [0x50, 0x4b, 0x01, 0x02])
            .expect("central");
        bytes[local + 22..local + 26].copy_from_slice(&1u32.to_le_bytes());
        bytes[central + 24..central + 28].copy_from_slice(&1u32.to_le_bytes());
        let err = zip_entries(Cursor::new(bytes)).expect_err("refused");
        assert!(
            err.to_string().contains("too big") || matches!(err, DomainError::Io(_)),
            "{err}"
        );
    }

    #[test]
    fn a_file_and_a_folder_with_the_same_path_are_refused() {
        let zip = make_zip(&[("r/a", "1"), ("r/a/b.yml", "2")]);
        assert!(matches!(
            zip_entries(zip),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn is_synced_path_ignores_case_in_its_rules() {
        for path in [
            "Environments/dev.yml",
            "ENVIRONMENTS/dev.yml",
            "FLOWS/f.yml",
            "Flows/f.yml",
            "Workspace.yml",
            "WORKSPACE.YML",
            "Node_Modules/a.js",
            "a/NODE_MODULES/b.js",
        ] {
            assert!(!is_synced_path(path), "{path} should be refused");
        }
    }

    #[test]
    fn is_synced_path_refuses_odd_segments_and_hidden_names_at_any_depth() {
        for path in [
            "a\\b.yml",
            "a/b\\c.yml",
            "C:/x.yml",
            "a/b:c.yml",
            "a//b.yml",
            "a/./b.yml",
            "./a.yml",
            "",
            "a/.secret/x.yml",
            ".x.yml",
            "a/.x.yml",
            "a/.git/x.yml",
        ] {
            assert!(!is_synced_path(path), "{path:?} should be refused");
        }
        assert!(is_synced_path("a/b/c.yml"));
        assert!(is_synced_path("environments.yml"));
    }

    #[test]
    fn is_synced_path_refuses_parent_segments_and_a_leading_slash() {
        for path in [
            "../x.yml",
            "a/../x.yml",
            "a/b/..",
            "/etc/x.yml",
            "/users/get.yml",
        ] {
            assert!(!is_synced_path(path), "{path} should be refused");
        }
        assert!(is_synced_path("a..b/x.yml"));
    }
}
