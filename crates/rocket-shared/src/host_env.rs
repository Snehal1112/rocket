//! The process environment as a port, so services need no global state.

use std::collections::HashMap;
use std::path::PathBuf;

/// Reads the host process: environment variables, the user name and the executable path.
///
/// The real implementation is `ProcessHostEnv` in `rocket-infra`.
pub trait HostEnv: Send + Sync {
    /// Every environment variable. A name or value that is not valid Unicode is skipped.
    fn vars(&self) -> HashMap<String, String>;
    /// One variable. A value that is not valid Unicode counts as not set.
    fn var(&self, name: &str) -> Option<String>;
    /// The signed-in user: `USER`, else `USERNAME`.
    fn user_name(&self) -> Option<String>;
    /// The path of the running executable.
    fn current_exe(&self) -> Option<PathBuf>;
}

/// A host with nothing in it. The default for a service that was not given a real one.
pub struct EmptyHostEnv;

impl HostEnv for EmptyHostEnv {
    fn vars(&self) -> HashMap<String, String> {
        HashMap::new()
    }
    fn var(&self, _name: &str) -> Option<String> {
        None
    }
    fn user_name(&self) -> Option<String> {
        None
    }
    fn current_exe(&self) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn empty_host_env_has_no_vars_user_or_exe() {
        let env: Arc<dyn HostEnv> = Arc::new(EmptyHostEnv);

        assert!(env.vars().is_empty());
        assert_eq!(env.var("PATH"), None);
        assert_eq!(env.user_name(), None);
        assert_eq!(env.current_exe(), None);
    }
}
