//! Paths the file watcher never reports.

use std::path::{Component, Path};

/// Directory or file names that are never collection content.
const IGNORED_COMPONENTS: &[&str] = &[".git", "node_modules", ".DS_Store"];

/// True when a change at `rel` (relative to the watched directory) is noise.
///
/// Ignored: any path with a `.git`, `node_modules` or `.DS_Store`
/// component, editor swap and backup files (`*.swp`, `*.swo`, `*.swx`, `*~`,
/// `.#*`, `#*#`), vim write probes (`4913`, `5036`, ...), `*.tmp` files and
/// the temp files of `atomic_write` (`<name>.tmp.<pid>_<hex>`).
pub(crate) fn is_ignored(rel: &Path) -> bool {
    let mut last = None;
    for component in rel.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if IGNORED_COMPONENTS.contains(&name.as_ref()) {
                return true;
            }
            last = Some(name);
        }
    }
    match last {
        Some(name) => is_temp_file_name(&name),
        None => false,
    }
}

/// True for editor swap, backup and probe files and atomic-write temp files.
fn is_temp_file_name(name: &str) -> bool {
    name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".swx")
        || name.ends_with('~')
        || name.starts_with(".#")
        || (name.len() > 1 && name.starts_with('#') && name.ends_with('#'))
        || name.ends_with(".tmp")
        || is_vim_probe(name)
        || is_atomic_write_temp(name)
}

/// Vim tests a directory by writing `4913`, then `5036` and so on (steps of 123).
fn is_vim_probe(name: &str) -> bool {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    match name.parse::<u64>() {
        Ok(n) => n >= 4913 && (n - 4913) % 123 == 0,
        Err(_) => false,
    }
}

/// Matches the temp names of `atomic_write`: `<stem>.tmp.<pid>_<hex>`.
fn is_atomic_write_temp(name: &str) -> bool {
    let Some(index) = name.rfind(".tmp.") else {
        return false;
    };
    let suffix = &name[index + ".tmp.".len()..];
    let Some((pid, hex)) = suffix.split_once('_') else {
        return false;
    };
    !pid.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && !hex.is_empty()
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_table() {
        let ignored = [
            ".git",
            "api/.git/index",
            "api/.git/objects/ab/cdef",
            "api/node_modules/left-pad/index.js",
            "api/.DS_Store",
            ".DS_Store",
            "api/get-users.yml.swp",
            "api/.get-users.yml.swp",
            "api/.get-users.yml.swo",
            "api/.get-users.yml.swx",
            "api/get-users.yml~",
            "api/.#get-users.yml",
            "api/#get-users.yml#",
            "api/4913",
            "api/5036",
            "api/scratch.tmp",
            "api/get-users.yml.tmp.12345_0a1b2c3d",
            "api/folder.yml.tmp.1_ff",
            "api/_order.tmp.99_00000001",
        ];
        for path in ignored {
            assert!(is_ignored(Path::new(path)), "{path} should be ignored");
        }

        // Every file kind the collection, environment, flow, contract and
        // script repositories read must still be reported.
        let kept = [
            "api",
            "api/opencollection.yml",
            "api/collection.yml",
            "api/folder.yml",
            "api/users/folder.yml",
            "api/_order.yml",
            "api/_order.json",
            "api/get-users.yml",
            "api/get-users.yaml",
            "api/legacy.json",
            "api/.uid",
            "api/users/.uid",
            "api/utils.js",
            "api/lib/helper.js",
            "api/environments/prod.yml",
            "api/flows/login-flow.yml",
            "api/.rocket/contracts/get-users.yml",
            "api/.env",
            "api/.env.local",
            "api/prod.env",
            "api/greeter.proto",
            "api/.gitignore",
            "api/README.md",
            "api/get-users.bru",
            "api/2024/get.yml",
            "api/4914",
            "api/my.tmp.request.yml",
            "api/target/get.yml",
            "api/targets/x.yml",
            "api/my-target.yml",
        ];
        for path in kept {
            assert!(!is_ignored(Path::new(path)), "{path} should be kept");
        }
    }

    #[test]
    fn empty_path_is_not_ignored() {
        assert!(!is_ignored(Path::new("")));
    }
}
