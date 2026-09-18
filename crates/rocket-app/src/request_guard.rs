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
///
/// Accepts a host with or without the `[...]` brackets `url::Url::host_str()`
/// puts around an IPv6 literal — stripping them here, once, means every
/// caller (including this module's own tests, which pass bare addresses) is
/// checking the address `std::net`'s range methods actually see, rather than
/// silently failing to parse a bracketed form and falling through to "not
/// blocked".
pub fn is_blocked_host(host: &str, also_block_private: bool) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    match unbracketed.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => is_blocked_ipv4(v4, also_block_private),
        Ok(IpAddr::V6(v6)) => is_blocked_ipv6(v6, also_block_private),
        Err(_) => false,
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr, also_block_private: bool) -> bool {
    if ip.is_loopback() || ip.is_link_local() {
        return true;
    }
    also_block_private && ip.is_private()
}

fn is_blocked_ipv6(ip: Ipv6Addr, also_block_private: bool) -> bool {
    // An IPv4-mapped IPv6 address (::ffff:a.b.c.d) carries an IPv4 host's
    // real reachability — including the metadata endpoint's
    // ::ffff:169.254.169.254 form — so it must be judged by the same rules
    // as the unmapped address, not just its own IPv6 loopback bit.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4, also_block_private);
    }
    if ip.is_loopback() || ip.is_unicast_link_local() {
        return true;
    }
    also_block_private && ip.is_unique_local()
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
    fn blocks_bracketed_ipv6_loopback() {
        // url::Url::host_str() returns an IPv6 literal wrapped in brackets —
        // this is the exact form check_request_guard passes in, not the bare
        // form the test above uses.
        assert!(is_blocked_host("[::1]", false));
    }

    #[test]
    fn blocks_ipv4_mapped_metadata_endpoint() {
        // ::ffff:169.254.169.254 in both its expanded-hex and bracketed forms
        // must be judged as the IPv4 address it maps to, not waved through as
        // an "ordinary" IPv6 address.
        assert!(is_blocked_host("::ffff:169.254.169.254", false));
        assert!(is_blocked_host("[::ffff:a9fe:a9fe]", false));
    }

    #[test]
    fn blocks_ipv6_unicast_link_local() {
        assert!(is_blocked_host("[fe80::1]", false));
    }

    #[test]
    fn blocks_ipv6_unique_local_only_with_private_flag() {
        assert!(!is_blocked_host("[fc00::1]", false));
        assert!(is_blocked_host("[fc00::1]", true));
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
