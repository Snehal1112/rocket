//! Pure host-blocking decision logic for the opt-in request-mutation host
//! guard (docs/superpowers/specs/2026-09-16-request-mutation-host-guard-spec.md).
//! No I/O — consumed by `RequestExecutionService::check_request_guard`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Returns true if `host` should be blocked under the given policy.
///
/// Checks, at minimum: 127.0.0.0/8, ::1, 169.254.0.0/16 (including the
/// 169.254.169.254 cloud metadata endpoint), and the literal string
/// "localhost". When `also_block_private` is true, additionally checks
/// 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
pub fn is_blocked_host(host: &str, also_block_private: bool) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => is_blocked_ipv4(v4, also_block_private),
        Ok(IpAddr::V6(v6)) => is_blocked_ipv6(v6),
        Err(_) => false,
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr, also_block_private: bool) -> bool {
    if ip.is_loopback() || ip.is_link_local() {
        return true;
    }
    also_block_private && ip.is_private()
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ipv4_loopback() {
        assert!(is_blocked_host("127.0.0.1", false));
    }

    #[test]
    fn blocks_ipv6_loopback() {
        assert!(is_blocked_host("::1", false));
    }

    #[test]
    fn blocks_link_local_metadata_endpoint() {
        // 169.254.169.254 — the standard cloud metadata endpoint. Highest-value
        // entry in the blocklist per the spec.
        assert!(is_blocked_host("169.254.169.254", false));
    }

    #[test]
    fn blocks_other_link_local_addresses() {
        assert!(is_blocked_host("169.254.1.1", false));
    }

    #[test]
    fn blocks_localhost_string_case_insensitively() {
        assert!(is_blocked_host("localhost", false));
        assert!(is_blocked_host("LOCALHOST", false));
    }

    #[test]
    fn private_ranges_not_blocked_without_flag() {
        assert!(!is_blocked_host("10.0.0.5", false));
        assert!(!is_blocked_host("172.16.5.5", false));
        assert!(!is_blocked_host("192.168.1.1", false));
    }

    #[test]
    fn private_ranges_blocked_with_flag() {
        assert!(is_blocked_host("10.0.0.5", true));
        assert!(is_blocked_host("172.16.5.5", true));
        assert!(is_blocked_host("192.168.1.1", true));
    }

    #[test]
    fn loopback_and_link_local_blocked_even_without_private_flag() {
        // also_block_private_ranges only affects RFC1918 — loopback/link-local
        // are always blocked once the guard itself is on.
        assert!(is_blocked_host("127.0.0.1", true));
        assert!(is_blocked_host("169.254.169.254", true));
    }

    #[test]
    fn public_ip_not_blocked() {
        assert!(!is_blocked_host("8.8.8.8", true));
    }

    #[test]
    fn ordinary_hostname_not_blocked() {
        assert!(!is_blocked_host("api.example.com", true));
        assert!(!is_blocked_host("internal-service.corp", true));
    }
}
