//! The real process environment, for `rocket-app` services that take a `HostEnv`.

use std::collections::HashMap;
use std::path::PathBuf;

use rocket_shared::host_env::HostEnv;

/// The host environment of the running process.
pub struct ProcessHostEnv;

impl HostEnv for ProcessHostEnv {
    fn vars(&self) -> HashMap<String, String> {
        // `vars_os` does not panic on a value that is not valid Unicode, unlike `vars`.
        std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect()
    }

    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn user_name(&self) -> Option<String> {
        self.var("USER").or_else(|| self.var("USERNAME"))
    }

    fn current_exe(&self) -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        crate::paths::home_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::host_env::HostEnv;

    #[test]
    fn var_reads_the_real_environment() {
        std::env::set_var("ROCKET_HOST_ENV_TEST_VAR", "from-os");

        assert_eq!(
            ProcessHostEnv.var("ROCKET_HOST_ENV_TEST_VAR").as_deref(),
            Some("from-os")
        );
        assert_eq!(ProcessHostEnv.var("ROCKET_HOST_ENV_TEST_UNSET"), None);
    }

    #[test]
    fn vars_contains_a_set_variable() {
        std::env::set_var("ROCKET_HOST_ENV_TEST_VARS", "v");

        assert_eq!(
            ProcessHostEnv
                .vars()
                .get("ROCKET_HOST_ENV_TEST_VARS")
                .map(String::as_str),
            Some("v")
        );
    }

    #[test]
    fn user_name_is_none_or_non_empty() {
        if let Some(name) = ProcessHostEnv.user_name() {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn current_exe_is_some_and_absolute() {
        let exe = ProcessHostEnv.current_exe().expect("exe");

        assert!(exe.is_absolute());
    }
}
