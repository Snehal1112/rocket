//! Sync payloads: the in-memory ZIP of files and (Task 2) the payload machines and their
//! stream. Nothing here touches a file.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use rocket_network::limits::MAX_UNZIPPED_BYTES;
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::sync_checks::Stop;
use super::sync_ports::SyncFile;

fn pack_error() -> DomainError {
    DomainError::Internal("The sync files could not be packed.".to_string())
}

/// A ZIP of `files` in path order, deflated, no folder entries and a fixed time per entry, so
/// the same files always give the same bytes.
pub(super) fn pack_zip(files: &[SyncFile]) -> DomainResult<Vec<u8>> {
    let mut sorted: Vec<&SyncFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for file in sorted {
        writer
            .start_file(file.path.as_str(), options)
            .map_err(|_| pack_error())?;
        writer.write_all(&file.bytes).map_err(|_| pack_error())?;
    }
    let out = writer.finish().map_err(|_| pack_error())?;
    Ok(out.into_inner())
}

/// The entries of a received files ZIP. The strict validator runs first; a refusal of it, or
/// any reader error, is `Stop::Unsafe`. Folder entries are skipped. The validator already caps
/// the unzipped size, and reading takes at most that much again as a second guard.
pub(super) fn unpack_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Stop> {
    rocket_import::transfer_zip::validate_bytes(bytes).map_err(|_| Stop::Unsafe)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| Stop::Unsafe)?;
    let mut left = MAX_UNZIPPED_BYTES;
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| Stop::Unsafe)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut data = Vec::new();
        // One byte more than is left shows an entry that goes past the cap.
        entry
            .take(left + 1)
            .read_to_end(&mut data)
            .map_err(|_| Stop::Unsafe)?;
        left = left.checked_sub(data.len() as u64).ok_or(Stop::Unsafe)?;
        if out.insert(name, data).is_some() {
            return Err(Stop::Unsafe);
        }
    }
    Ok(out)
}

/// Lowercase hex SHA-256 of a payload.
pub(super) fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SyncFile {
        SyncFile {
            path: path.to_string(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn pack_sorts_by_path_and_is_stable() {
        let a = pack_zip(&[file("b.yml", "b"), file("a.yml", "a")]).expect("zip");
        let b = pack_zip(&[file("a.yml", "a"), file("b.yml", "b")]).expect("zip");
        assert_eq!(a, b, "the input order does not matter");
        let names = super::super::transfer_fakes::zip_entry_names(&a);
        assert_eq!(names, vec!["a.yml", "b.yml"]);
    }

    #[test]
    fn unpack_refuses_what_the_strict_validator_refuses() {
        assert_eq!(unpack_zip(b"not a zip"), Err(Stop::Unsafe));
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("../a.yml", SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(b"x").expect("bytes");
        let bad = writer.finish().expect("zip").into_inner();
        assert_eq!(unpack_zip(&bad), Err(Stop::Unsafe));
    }

    #[test]
    fn sha_hex_is_lowercase_sha256() {
        assert_eq!(
            sha_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
