//! The home directory and `~` expansion for the libgit2 adapter. `rocket-git` is a mixed crate
//! (D-01), so it may read the process environment itself.

use std::path::PathBuf;

/// Returns the home directory, only when it is an absolute path.
pub(crate) fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().filter(|h| h.is_absolute())
}

/// Expands a leading `~` or `~/` with the home directory of the user.
pub(crate) fn expand_tilde(input: &str) -> PathBuf {
    rocket_shared::paths::expand_tilde_with(input, home_dir().as_deref())
}
