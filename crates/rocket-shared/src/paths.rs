use std::path::{Path, PathBuf};

/// Expands a leading `~` or `~/` with the given home.
///
/// Other text, including `~user`, is returned unchanged.
pub fn expand_tilde_with(input: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(input);
    };
    if input == "~" {
        return home.to_path_buf();
    }
    match input.strip_prefix("~/") {
        Some("") => home.to_path_buf(),
        Some(rest) => home.join(rest),
        None => PathBuf::from(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Option<&'static Path> {
        Some(Path::new("/home/u"))
    }

    #[test]
    fn tilde_alone_gives_home() {
        assert_eq!(expand_tilde_with("~", home()), PathBuf::from("/home/u"));
    }

    #[test]
    fn tilde_slash_x_joins_home() {
        assert_eq!(expand_tilde_with("~/x", home()), PathBuf::from("/home/u/x"));
        assert_eq!(
            expand_tilde_with("~/a/b", home()),
            PathBuf::from("/home/u/a/b")
        );
    }

    #[test]
    fn tilde_slash_gives_home() {
        assert_eq!(expand_tilde_with("~/", home()), PathBuf::from("/home/u"));
    }

    #[test]
    fn tilde_user_is_unchanged() {
        for s in ["~user", "~user/x"] {
            assert_eq!(expand_tilde_with(s, home()), PathBuf::from(s));
        }
    }

    #[test]
    fn tilde_inside_the_text_is_unchanged() {
        for s in ["a/~/b", "/abs/~", ""] {
            assert_eq!(expand_tilde_with(s, home()), PathBuf::from(s));
        }
    }

    #[test]
    fn plain_paths_are_unchanged() {
        for s in ["/etc/hosts", "rel/path"] {
            assert_eq!(expand_tilde_with(s, home()), PathBuf::from(s));
            assert_eq!(expand_tilde_with(s, None), PathBuf::from(s));
        }
    }

    #[test]
    fn no_home_leaves_every_tilde_form_unchanged() {
        for s in ["~", "~/x", "~/"] {
            assert_eq!(expand_tilde_with(s, None), PathBuf::from(s));
        }
    }
}
