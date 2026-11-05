pub mod branch;
pub mod commit;
pub mod conflict;
pub mod credentials;
pub mod diff;
pub mod events;
mod git2_service;
mod home;
pub mod identity;
pub mod remote;
mod remote_verification;
pub mod sanitize;
pub mod service;
pub mod stash;
pub mod status;

pub use branch::*;
pub use commit::*;
pub use conflict::*;
pub use credentials::*;
pub use diff::*;
pub use events::GitEvent;
pub use git2_service::{Git2Service, SshTrustStore, SystemSshTrustStore};
pub use identity::GitIdentity;
pub use remote::*;
pub use remote_verification::{SshHostFailure, SshHostFailureKind, TlsCertificateFailure};
pub use service::GitService;
pub use stash::*;
pub use status::*;

/// Bounds libgit2's network operations: 10 s to connect and 60 s per server read.
///
/// Call this once at startup, before any other thread exists. These options write
/// process-global C state with no synchronization, so they are only safe at that point.
/// Without them, a stalled connection (for example an unreachable remote) blocks the calling
/// thread forever instead of failing. This only bounds the HTTPS transport; libgit2 has no
/// equivalent timeout for its SSH (libssh2) transport.
pub fn set_network_timeouts() {
    // SAFETY: the caller runs this before any other thread starts, so nothing reads or writes
    // the libgit2 global options at the same time.
    unsafe {
        let _ = git2::opts::set_server_connect_timeout_in_milliseconds(10_000);
        let _ = git2::opts::set_server_timeout_in_milliseconds(60_000);
    }
}

#[cfg(test)]
mod network_timeout_tests {
    use super::set_network_timeouts;

    #[test]
    fn set_network_timeouts_can_be_called() {
        set_network_timeouts();
        // SAFETY: the getters only read the libgit2 option values.
        let (connect, server) = unsafe {
            (
                git2::opts::get_server_connect_timeout_in_milliseconds(),
                git2::opts::get_server_timeout_in_milliseconds(),
            )
        };
        assert_eq!(connect.ok(), Some(10_000));
        assert_eq!(server.ok(), Some(60_000));
    }
}
