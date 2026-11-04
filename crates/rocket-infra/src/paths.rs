//! The home directory of the user and `~` expansion, read from the real process.

use std::path::PathBuf;

/// Returns the home directory, only when it is an absolute path.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().filter(|h| h.is_absolute())
}

/// Expands a leading `~` or `~/` with the home directory of the user.
pub fn expand_tilde(input: &str) -> PathBuf {
    rocket_shared::paths::expand_tilde_with(input, home_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_is_absolute_when_present() {
        if let Some(home) = home_dir() {
            assert!(home.is_absolute());
        }
    }

    #[test]
    fn expand_tilde_leaves_plain_paths_alone() {
        assert_eq!(expand_tilde("/etc/hosts"), PathBuf::from("/etc/hosts"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }

    #[test]
    fn expand_tilde_slash_joins_the_home() {
        if let Some(home) = home_dir() {
            assert_eq!(expand_tilde("~/x"), home.join("x"));
        }
    }
}
