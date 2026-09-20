use std::path::PathBuf;

use super::ssh_host_verification;

/// Read-only source for the OpenSSH `known_hosts` file used to classify SSH
/// certificate failures.
///
/// This is intentionally read-only and offline: it never decides whether a
/// connection is accepted (libgit2/libssh2's own verification remains
/// authoritative, see `certificate_failure_policy`) and never writes a trust
/// decision back to disk, so there is no trust-on-first-use path here.
pub trait SshTrustStore: Send + Sync {
    fn known_hosts_path(&self) -> Option<PathBuf>;
}

#[derive(Debug, Default)]
pub struct SystemSshTrustStore;

impl SshTrustStore for SystemSshTrustStore {
    fn known_hosts_path(&self) -> Option<PathBuf> {
        ssh_host_verification::default_known_hosts_path()
    }
}

#[derive(Debug)]
pub(super) struct FixedPathSshTrustStore {
    pub(super) path: PathBuf,
}

impl SshTrustStore for FixedPathSshTrustStore {
    fn known_hosts_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }
}
