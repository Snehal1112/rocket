//! Where new requests and folders go during an apply. Every name is sanitized, never reserved
//! and never repeats a path (compared case-folded), so the file transaction accepts it.

use crate::error::{ImportError, ImportResult};
use crate::importer::{unique_segment, PORT_RESERVED, REQUEST_RESERVED, SERVICE_RESERVED};
use rocket_collection::{fold_name, request_filename_for, validate_rel_path};
use rocket_shared::error::DomainError;
use std::collections::{HashMap, HashSet};

/// File name of a folder's metadata.
const FOLDER_FILE: &str = "folder.yml";

/// Hands out folders and request paths that are free in the collection.
pub(super) struct PathAlloc {
    /// Case-folded folder path to the folder path as it exists or was created.
    dirs: HashMap<String, String>,
    /// Case-folded folder path to the lowercase file stems taken in it.
    stems: HashMap<String, HashSet<String>>,
    /// Folders created by this apply, outermost first.
    pub(super) new_folders: Vec<String>,
}

impl PathAlloc {
    /// `existing` lists every file of the collection (requests and `folder.yml`), `known`
    /// lists request paths without an extension that must stay taken.
    pub(super) fn new<'a>(
        existing: &HashSet<String>,
        known: impl Iterator<Item = &'a str>,
    ) -> Self {
        let mut alloc = Self {
            dirs: HashMap::new(),
            stems: HashMap::new(),
            new_folders: Vec::new(),
        };
        for path in existing {
            let (dir, file) = split_dir(path);
            alloc.register_dir(dir);
            alloc.take_stem(dir, strip_ext(file));
        }
        for path in known {
            // Only the name is kept taken. A folder with no file on disk is gone and is
            // created again when a new request needs it.
            let (dir, file) = split_dir(path);
            alloc.take_stem(dir, file);
        }
        alloc
    }

    fn register_dir(&mut self, dir: &str) {
        let mut prefix = String::new();
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            self.dirs
                .entry(fold_name(&prefix))
                .or_insert_with(|| prefix.clone());
        }
    }

    fn take_stem(&mut self, dir: &str, stem: &str) {
        self.stems
            .entry(fold_name(dir))
            .or_default()
            .insert(stem.to_lowercase());
    }

    /// The folder path for raw folder names, outermost first. A folder that exists (in any
    /// letter case) is reused; a missing one is created and listed in `new_folders`.
    pub(super) fn folder(&mut self, names: &[String]) -> ImportResult<String> {
        let mut dir = String::new();
        for (depth, name) in names.iter().enumerate() {
            let reserved = if depth == 0 {
                SERVICE_RESERVED
            } else {
                PORT_RESERVED
            };
            let (segment, _) = unique_segment(&mut HashSet::new(), name, reserved);
            let candidate = if dir.is_empty() {
                segment
            } else {
                format!("{dir}/{segment}")
            };
            let key = fold_name(&candidate);
            dir = match self.dirs.get(&key) {
                Some(actual) => actual.clone(),
                None => {
                    self.dirs.insert(key, candidate.clone());
                    self.new_folders.push(candidate.clone());
                    candidate
                }
            };
        }
        Ok(dir)
    }

    /// The checked path of the `folder.yml` of a folder this apply created.
    pub(super) fn folder_file_for(&self, dir: &str) -> ImportResult<String> {
        let rel = format!("{dir}/{FOLDER_FILE}");
        check(&rel)?;
        Ok(rel)
    }

    /// A free request path (without extension) in `dir` for `name`, and the number used to
    /// make it unique (1 when none was needed).
    pub(super) fn request(&mut self, dir: &str, name: &str) -> ImportResult<(String, u32)> {
        let used = self.stems.entry(fold_name(dir)).or_default();
        let (slug, k) = unique_segment(used, name, REQUEST_RESERVED);
        let path = if dir.is_empty() {
            slug
        } else {
            format!("{dir}/{slug}")
        };
        check(&request_filename_for(&path))?;
        Ok((path, k))
    }
}

/// Checks a path with the same rules the file transaction uses.
pub(super) fn check(rel: &str) -> ImportResult<()> {
    validate_rel_path(rel).map_err(ImportError::DomainError)
}

/// The error for a path that could not be made safe.
pub(super) fn unsafe_path(rel: &str) -> ImportError {
    ImportError::DomainError(DomainError::InvalidInput(format!(
        "path \"{rel}\" is not safe to write"
    )))
}

fn split_dir(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((dir, file)) => (dir, file),
        None => ("", path),
    }
}

fn strip_ext(file: &str) -> &str {
    for ext in [".yml", ".yaml", ".json"] {
        if let Some(stem) = file.strip_suffix(ext) {
            return stem;
        }
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc(existing: &[&str]) -> PathAlloc {
        let set: HashSet<String> = existing.iter().map(|s| s.to_string()).collect();
        PathAlloc::new(&set, std::iter::empty())
    }

    #[test]
    fn existing_folder_is_reused_in_any_letter_case() {
        let mut a = alloc(&["pets/list.yml"]);
        assert_eq!(a.folder(&["Pets".to_string()]).expect("dir"), "pets");
        assert!(a.new_folders.is_empty());
    }

    #[test]
    fn taken_stem_gets_a_suffix_and_a_new_one_is_not_repeated() {
        let mut a = alloc(&["Pets/List-pets.yml"]);
        let (first, k) = a.request("Pets", "list pets").expect("path");
        assert_eq!(first, "Pets/list-pets-2");
        assert_eq!(k, 2);
        let (second, _) = a.request("Pets", "LIST PETS").expect("path");
        assert_ne!(first.to_lowercase(), second.to_lowercase());
    }

    #[test]
    fn hostile_names_become_safe_segments() {
        let mut a = alloc(&[]);
        let dir = a.folder(&["../../x".to_string()]).expect("dir");
        assert!(!dir.contains(".."));
        assert!(!dir.contains('/'));
        let (con, _) = a.request(&dir, "con").expect("path");
        assert!(check(&request_filename_for(&con)).is_ok());
        let (empty, _) = a.request(&dir, "").expect("path");
        assert!(empty.ends_with("unnamed"));
        let (colon, _) = a.request(&dir, "a:b").expect("path");
        assert!(!colon.contains(':'));
    }
}
