//! The home directory of the user and `~` expansion, read from the real process.

use std::path::PathBuf;

/// Returns the home directory, only when it is an absolute path.
pub fn home_dir() -> Option<PathBuf> {
    home_dir_from(dirs::home_dir())
}

/// Keeps a home candidate only when it is an absolute path.
fn home_dir_from(candidate: Option<PathBuf>) -> Option<PathBuf> {
    candidate.filter(|h| h.is_absolute())
}

/// Expands a leading `~` or `~/` with the home directory of the user.
pub fn expand_tilde(input: &str) -> PathBuf {
    rocket_shared::paths::expand_tilde_with(input, home_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::paths::expand_tilde_with;

    #[test]
    fn home_dir_is_absolute_when_present() {
        // This only checks the real process, so it may see no home.
        if let Some(home) = home_dir() {
            assert!(home.is_absolute());
        }
    }

    #[test]
    fn home_dir_from_drops_a_relative_candidate() {
        assert_eq!(home_dir_from(Some(PathBuf::from("rel/home"))), None);
        assert_eq!(home_dir_from(None), None);
        let abs = std::env::temp_dir();
        assert_eq!(home_dir_from(Some(abs.clone())), Some(abs));
    }

    #[test]
    fn expand_tilde_leaves_plain_paths_alone() {
        assert_eq!(expand_tilde("/etc/hosts"), PathBuf::from("/etc/hosts"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }

    #[test]
    fn expand_tilde_uses_the_given_home() {
        let home = tempfile::TempDir::new().expect("home");
        assert_eq!(
            expand_tilde_with("~/x", Some(home.path())),
            home.path().join("x")
        );
        assert_eq!(expand_tilde_with("~", Some(home.path())), home.path());
    }

    #[test]
    fn expand_tilde_without_a_home_keeps_the_path() {
        assert_eq!(expand_tilde_with("~/x", None), PathBuf::from("~/x"));
        assert_eq!(expand_tilde_with("~", None), PathBuf::from("~"));
    }
}
