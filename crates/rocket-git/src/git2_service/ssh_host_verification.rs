use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use sha2::{Digest, Sha256};
use ssh2::{CheckResult, KnownHostFileKind, Session};
use url::Url;

use crate::{SshHostFailure, SshHostFailureKind};

const DEFAULT_SSH_PORT: u16 = 22;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SshEndpoint {
    pub(super) host: String,
    pub(super) port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownHostClassification {
    Match,
    Mismatch,
    NotFound,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SshHostClassification {
    Match,
    Failure(SshHostFailure),
}

pub(super) fn default_known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

/// Parse the two SSH remote forms supported by Git: an `ssh://` URL or an
/// SCP-like `[user@]host:path` remote.
pub(super) fn parse_ssh_endpoint(remote: &str) -> Result<SshEndpoint, ()> {
    if remote.contains("://") {
        let url = Url::parse(remote).map_err(|_| ())?;
        if url.scheme() != "ssh" {
            return Err(());
        }
        let host = url.host_str().filter(|host| !host.is_empty()).ok_or(())?;
        return Ok(SshEndpoint {
            host: host.to_owned(),
            port: url.port().unwrap_or(DEFAULT_SSH_PORT),
        });
    }

    let (authority, path) = remote.split_once(':').ok_or(())?;
    if authority.is_empty() || path.is_empty() {
        return Err(());
    }
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty() || host.contains(['[', ']']) || host.contains('\0') {
        return Err(());
    }

    Ok(SshEndpoint {
        host: host.to_owned(),
        port: DEFAULT_SSH_PORT,
    })
}

/// Classify a supplied raw SSH host key using only an OpenSSH known_hosts file.
///
/// This function deliberately reports the file's result without deciding
/// whether a connection should be accepted.
pub(super) fn classify_known_host(
    known_hosts_path: &Path,
    host: &str,
    port: u16,
    raw_key: &[u8],
) -> KnownHostClassification {
    if host.contains('\0') {
        return KnownHostClassification::Failure;
    }

    let metadata = match fs::metadata(known_hosts_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return KnownHostClassification::NotFound;
        }
        Err(_) => return KnownHostClassification::Failure,
    };
    if !metadata.is_file() {
        return KnownHostClassification::Failure;
    }

    let contents = match fs::read_to_string(known_hosts_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return KnownHostClassification::NotFound;
        }
        Err(_) => return KnownHostClassification::Failure,
    };
    if !has_valid_known_hosts_shape(&contents) {
        return KnownHostClassification::Failure;
    }

    let session = match Session::new() {
        Ok(session) => session,
        Err(_) => return KnownHostClassification::Failure,
    };
    let mut known_hosts = match session.known_hosts() {
        Ok(known_hosts) => known_hosts,
        Err(_) => return KnownHostClassification::Failure,
    };
    if known_hosts
        .read_file(known_hosts_path, KnownHostFileKind::OpenSSH)
        .is_err()
    {
        return if known_hosts_path.exists() {
            KnownHostClassification::Failure
        } else {
            KnownHostClassification::NotFound
        };
    }

    match known_hosts.check_port(host, port, raw_key) {
        CheckResult::Match => KnownHostClassification::Match,
        CheckResult::Mismatch => KnownHostClassification::Mismatch,
        CheckResult::NotFound => KnownHostClassification::NotFound,
        CheckResult::Failure => KnownHostClassification::Failure,
    }
}

/// Resolve an SSH endpoint and map the offline known_hosts result to a typed
/// failure. A host disagreement is treated as unavailable rather than allowing
/// verification against a different endpoint.
pub(super) fn classify_remote_host(
    remote: &str,
    callback_host: &str,
    known_hosts_path: Option<&Path>,
    algorithm: &str,
    raw_key: &[u8],
    native_sha256_digest: Option<&[u8]>,
) -> SshHostClassification {
    let fingerprint = openssh_sha256_fingerprint(native_sha256_digest, raw_key);
    let endpoint = match parse_ssh_endpoint(remote) {
        Ok(endpoint) => endpoint,
        Err(()) => {
            return unavailable_failure(callback_host, DEFAULT_SSH_PORT, algorithm, fingerprint);
        }
    };

    if !endpoint.host.eq_ignore_ascii_case(callback_host) {
        return unavailable_failure(&endpoint.host, endpoint.port, algorithm, fingerprint);
    }

    let path = match known_hosts_path {
        Some(path) => path,
        None => {
            return unavailable_failure(&endpoint.host, endpoint.port, algorithm, fingerprint);
        }
    };

    let kind = match classify_known_host(path, &endpoint.host, endpoint.port, raw_key) {
        KnownHostClassification::Match => return SshHostClassification::Match,
        KnownHostClassification::Mismatch => SshHostFailureKind::ChangedHost,
        KnownHostClassification::NotFound => SshHostFailureKind::UnknownHost,
        KnownHostClassification::Failure => SshHostFailureKind::VerificationUnavailable,
    };

    SshHostClassification::Failure(SshHostFailure {
        kind,
        host: endpoint.host,
        port: endpoint.port,
        algorithm: algorithm.to_owned(),
        fingerprint,
    })
}

fn has_valid_known_hosts_shape(contents: &str) -> bool {
    contents.lines().all(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return true;
        }

        let fields: Vec<_> = line.split_whitespace().collect();
        let key_index = if fields.first().is_some_and(|field| field.starts_with('@')) {
            3
        } else {
            2
        };
        fields.get(key_index).is_some_and(|key| {
            STANDARD
                .decode(key)
                .or_else(|_| STANDARD_NO_PAD.decode(key))
                .is_ok_and(|key| !key.is_empty())
        })
    })
}

pub(super) fn openssh_sha256_fingerprint(
    native_sha256_digest: Option<&[u8]>,
    raw_key: &[u8],
) -> String {
    let digest = native_sha256_digest
        .filter(|digest| digest.len() == 32)
        .map_or_else(|| Sha256::digest(raw_key).to_vec(), <[u8]>::to_vec);
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
}

fn unavailable_failure(
    host: &str,
    port: u16,
    algorithm: &str,
    fingerprint: String,
) -> SshHostClassification {
    SshHostClassification::Failure(SshHostFailure {
        kind: SshHostFailureKind::VerificationUnavailable,
        host: host.to_owned(),
        port,
        algorithm: algorithm.to_owned(),
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::engine::general_purpose::STANDARD;
    use tempfile::TempDir;

    use super::*;

    const KEY_BASE64: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g";
    const CHANGED_KEY_BASE64: &str =
        "AAAAC3NzaC1lZDI1NTE5AAAAICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj9A";
    const EXPECTED_FINGERPRINT: &str = "SHA256:mKqU+0K8OhKmA8bBQi9Rz0Q5l7/g160hIP+rJYSTNj4";
    const HASHED_HOST: &str = "|1|AAECAwQFBgcICQoLDA0ODxAREhM=|tfTk2zfUwEOJq8/nQE8s/gLfc58=";

    fn raw_key(encoded: &str) -> Vec<u8> {
        STANDARD.decode(encoded).expect("fixture key should decode")
    }

    fn known_hosts_file(line: &str) -> (TempDir, PathBuf) {
        let directory = TempDir::new().expect("temporary directory should be created");
        let path = directory.path().join("known_hosts");
        fs::write(&path, format!("{line}\n")).expect("known_hosts fixture should be written");
        (directory, path)
    }

    #[test]
    fn plain_host_key_matches() {
        let (_directory, path) =
            known_hosts_file(&format!("git.example.com ssh-ed25519 {KEY_BASE64}"));

        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::Match
        );
    }

    #[test]
    fn changed_host_key_is_a_mismatch() {
        let (_directory, path) =
            known_hosts_file(&format!("git.example.com ssh-ed25519 {KEY_BASE64}"));

        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(CHANGED_KEY_BASE64),),
            KnownHostClassification::Mismatch
        );
    }

    #[test]
    fn unknown_host_is_not_found() {
        let (_directory, path) =
            known_hosts_file(&format!("other.example.com ssh-ed25519 {KEY_BASE64}"));

        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::NotFound
        );
    }

    #[test]
    fn bracketed_non_default_port_matches_via_check_port() {
        let (_directory, path) =
            known_hosts_file(&format!("[git.example.com]:2222 ssh-ed25519 {KEY_BASE64}"));

        assert_eq!(
            classify_known_host(&path, "git.example.com", 2222, &raw_key(KEY_BASE64)),
            KnownHostClassification::Match
        );
        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::NotFound
        );
    }

    #[test]
    fn hashed_host_fixture_matches() {
        let (_directory, path) =
            known_hosts_file(&format!("{HASHED_HOST} ssh-ed25519 {KEY_BASE64}"));

        assert_eq!(
            classify_known_host(&path, "hashed.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::Match
        );
    }

    #[test]
    fn missing_known_hosts_is_not_found() {
        let directory = TempDir::new().expect("temporary directory should be created");
        let path = directory.path().join("missing_known_hosts");

        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::NotFound
        );
    }

    #[test]
    fn malformed_known_hosts_is_a_failure() {
        let (_directory, path) = known_hosts_file("this is not a valid known_hosts line");

        assert_eq!(
            classify_known_host(&path, "git.example.com", 22, &raw_key(KEY_BASE64)),
            KnownHostClassification::Failure
        );
    }

    #[test]
    fn unreadable_known_hosts_path_is_a_failure() {
        let directory = TempDir::new().expect("temporary directory should be created");

        assert_eq!(
            classify_known_host(
                directory.path(),
                "git.example.com",
                22,
                &raw_key(KEY_BASE64),
            ),
            KnownHostClassification::Failure
        );
    }

    #[test]
    fn fingerprint_uses_raw_key_fallback_and_omits_padding() {
        assert_eq!(
            openssh_sha256_fingerprint(None, &raw_key(KEY_BASE64)),
            EXPECTED_FINGERPRINT
        );
    }

    #[test]
    fn fingerprint_prefers_valid_native_digest() {
        assert_eq!(
            openssh_sha256_fingerprint(Some(&[7; 32]), b"ignored raw key"),
            format!("SHA256:{}", STANDARD_NO_PAD.encode([7; 32]))
        );
    }

    #[test]
    fn parses_ssh_urls_with_explicit_and_default_ports() {
        assert_eq!(
            parse_ssh_endpoint("ssh://git@git.example.com:2222/team/repo.git"),
            Ok(SshEndpoint {
                host: "git.example.com".to_owned(),
                port: 2222,
            })
        );
        assert_eq!(
            parse_ssh_endpoint("ssh://git@git.example.com/team/repo.git"),
            Ok(SshEndpoint {
                host: "git.example.com".to_owned(),
                port: 22,
            })
        );
    }

    #[test]
    fn parses_scp_like_remote_with_default_port() {
        assert_eq!(
            parse_ssh_endpoint("git@git.example.com:team/repo.git"),
            Ok(SshEndpoint {
                host: "git.example.com".to_owned(),
                port: 22,
            })
        );
    }

    #[test]
    fn rejects_non_ssh_and_malformed_endpoints() {
        assert_eq!(
            parse_ssh_endpoint("https://git.example.com/repo.git"),
            Err(())
        );
        assert_eq!(parse_ssh_endpoint("git.example.com/repo.git"), Err(()));
        assert_eq!(parse_ssh_endpoint("git@:repo.git"), Err(()));
    }

    #[test]
    fn maps_known_host_results_to_typed_failures() {
        let (_directory, path) =
            known_hosts_file(&format!("git.example.com ssh-ed25519 {KEY_BASE64}"));

        let changed = classify_remote_host(
            "git@git.example.com:team/repo.git",
            "git.example.com",
            Some(&path),
            "ssh-ed25519",
            &raw_key(CHANGED_KEY_BASE64),
            None,
        );
        assert!(matches!(
            changed,
            SshHostClassification::Failure(SshHostFailure {
                kind: SshHostFailureKind::ChangedHost,
                port: 22,
                ..
            })
        ));

        let missing_path = path.with_file_name("missing");
        let unknown = classify_remote_host(
            "git@git.example.com:team/repo.git",
            "git.example.com",
            Some(&missing_path),
            "ssh-ed25519",
            &raw_key(KEY_BASE64),
            None,
        );
        assert!(matches!(
            unknown,
            SshHostClassification::Failure(SshHostFailure {
                kind: SshHostFailureKind::UnknownHost,
                ..
            })
        ));
    }

    #[test]
    fn missing_trust_store_maps_to_verification_unavailable() {
        let result = classify_remote_host(
            "git@git.example.com:team/repo.git",
            "git.example.com",
            None,
            "ssh-ed25519",
            &raw_key(KEY_BASE64),
            None,
        );
        assert!(matches!(
            result,
            SshHostClassification::Failure(SshHostFailure {
                kind: SshHostFailureKind::VerificationUnavailable,
                ..
            })
        ));
    }

    #[test]
    fn malformed_known_hosts_maps_to_verification_unavailable() {
        let (_directory, path) = known_hosts_file("malformed known_hosts data");

        let result = classify_remote_host(
            "git@git.example.com:team/repo.git",
            "git.example.com",
            Some(&path),
            "ssh-ed25519",
            &raw_key(KEY_BASE64),
            None,
        );
        assert!(matches!(
            result,
            SshHostClassification::Failure(SshHostFailure {
                kind: SshHostFailureKind::VerificationUnavailable,
                ..
            })
        ));
    }

    #[test]
    fn callback_host_mismatch_fails_closed_as_unavailable() {
        let (_directory, path) =
            known_hosts_file(&format!("git.example.com ssh-ed25519 {KEY_BASE64}"));

        let result = classify_remote_host(
            "ssh://git@git.example.com/team/repo.git",
            "attacker.example.com",
            Some(&path),
            "ssh-ed25519",
            &raw_key(KEY_BASE64),
            None,
        );
        assert!(matches!(
            result,
            SshHostClassification::Failure(SshHostFailure {
                kind: SshHostFailureKind::VerificationUnavailable,
                ref host,
                port: 22,
                ..
            }) if host == "git.example.com"
        ));
    }
}
