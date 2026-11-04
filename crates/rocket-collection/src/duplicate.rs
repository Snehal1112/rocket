//! Naming and copy rules for duplicating a request.

use crate::{generate_uid, Request};

/// How many copy names the service tries before it gives up.
pub const MAX_DUPLICATE_ATTEMPTS: u32 = 1000;

/// The path and display name for one duplicate attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCandidate {
    pub path: String,
    pub name: String,
}

/// n = 1 gives " copy", n >= 2 gives " copy {n}". Keeps the source folder and file stem.
pub fn duplicate_candidate(source_path: &str, source_name: &str, n: u32) -> DuplicateCandidate {
    let (dir, file) = match source_path.rsplit_once('/') {
        Some((dir, file)) => (Some(dir), file),
        None => (None, source_path),
    };
    let stem = [".yml", ".yaml", ".json"]
        .iter()
        .find_map(|ext| file.strip_suffix(ext))
        .unwrap_or(file);
    let suffix = if n == 1 {
        " copy".to_string()
    } else {
        format!(" copy {n}")
    };
    let file_name = format!("{stem}{suffix}.yml");
    let path = match dir {
        Some(dir) => format!("{dir}/{file_name}"),
        None => file_name,
    };
    DuplicateCandidate {
        path,
        name: format!("{source_name}{suffix}"),
    }
}

/// A clone of `source` with a new uid, `name`, `seq = None` and `file_name = None`.
pub fn duplicate_of(source: &Request, name: &str) -> Request {
    let mut copy = source.clone();
    copy.uid = generate_uid();
    copy.name = name.to_string();
    copy.seq = None;
    copy.file_name = None;
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;

    #[test]
    fn first_candidate_is_copy() {
        let c = duplicate_candidate("auth/Login.yml", "Login", 1);
        assert_eq!(c.path, "auth/Login copy.yml");
        assert_eq!(c.name, "Login copy");
    }

    #[test]
    fn later_candidates_are_numbered() {
        let c = duplicate_candidate("auth/Login.yml", "Login", 2);
        assert_eq!(c.path, "auth/Login copy 2.yml");
        assert_eq!(c.name, "Login copy 2");
    }

    #[test]
    fn root_request_has_no_leading_slash() {
        let c = duplicate_candidate("Login.yml", "Login", 1);
        assert_eq!(c.path, "Login copy.yml");
    }

    #[test]
    fn yaml_and_json_sources_get_a_yml_copy() {
        let c = duplicate_candidate("auth/Foo.json", "Foo", 1);
        assert_eq!(c.path, "auth/Foo copy.yml");
        let c = duplicate_candidate("auth/Foo.yaml", "Foo", 1);
        assert_eq!(c.path, "auth/Foo copy.yml");
    }

    #[test]
    fn the_file_stem_is_used_for_the_path_and_the_name_for_the_name() {
        let c = duplicate_candidate("auth/ab.yml", "A/B: C", 1);
        assert_eq!(c.path, "auth/ab copy.yml");
        assert_eq!(c.name, "A/B: C copy");
    }

    #[test]
    fn duplicate_of_resets_identity_and_keeps_the_rest() {
        let mut source = Request::new(
            "Login",
            rocket_shared::types::HttpMethod::Post,
            "https://x.test",
        );
        source.seq = Some(3);
        source.file_name = Some("Login.yml".into());
        let copy = duplicate_of(&source, "Login copy");
        assert_ne!(copy.uid, source.uid);
        assert_eq!(copy.name, "Login copy");
        assert_eq!(copy.seq, None);
        assert_eq!(copy.file_name, None);
        let mut expected = source.clone();
        expected.uid = copy.uid.clone();
        expected.name = "Login copy".into();
        expected.seq = None;
        expected.file_name = None;
        assert_eq!(copy, expected);
    }
}
