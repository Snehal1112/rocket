//! Which side opens the peer WebSocket.

use std::time::Duration;

use crate::device::DeviceId;
use crate::limits::DIAL_GRACE;

/// How the local node learned about the remote one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialHint {
    Mdns,
    ManualAddress,
}

/// Decides whether the local node should dial now.
///
/// A manual address always dials, because only this side knows the address.
/// For mDNS the lower device id dials. The higher id dials too once `waited`
/// reaches `DIAL_GRACE`, which covers a lower side that cannot dial out.
pub fn should_dial(local: &DeviceId, remote: &DeviceId, hint: DialHint, waited: Duration) -> bool {
    match hint {
        DialHint::ManualAddress => true,
        DialHint::Mdns => local < remote || waited >= DIAL_GRACE,
    }
}

/// When two connections exist for one pair, keep the one dialled by this id.
pub fn keep_dialed_by(local: &DeviceId, remote: &DeviceId) -> DeviceId {
    *std::cmp::min(local, remote)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn low() -> DeviceId {
        DeviceId::from_bytes([1; 32])
    }

    fn high() -> DeviceId {
        DeviceId::from_bytes([2; 32])
    }

    #[test]
    fn the_lower_id_dials_an_mdns_peer_at_once() {
        assert!(should_dial(&low(), &high(), DialHint::Mdns, Duration::ZERO));
    }

    #[test]
    fn the_higher_id_waits_for_the_grace_period() {
        assert!(!should_dial(
            &high(),
            &low(),
            DialHint::Mdns,
            Duration::ZERO
        ));
        assert!(!should_dial(
            &high(),
            &low(),
            DialHint::Mdns,
            DIAL_GRACE - Duration::from_millis(1)
        ));
        assert!(should_dial(&high(), &low(), DialHint::Mdns, DIAL_GRACE));
        assert!(should_dial(
            &high(),
            &low(),
            DialHint::Mdns,
            DIAL_GRACE + Duration::from_secs(5)
        ));
    }

    #[test]
    fn a_manual_address_always_dials() {
        assert!(should_dial(
            &high(),
            &low(),
            DialHint::ManualAddress,
            Duration::ZERO
        ));
        assert!(should_dial(
            &low(),
            &high(),
            DialHint::ManualAddress,
            Duration::ZERO
        ));
    }

    #[test]
    fn the_same_id_on_both_sides_waits_for_the_grace_period() {
        assert!(!should_dial(&low(), &low(), DialHint::Mdns, Duration::ZERO));
    }

    #[test]
    fn keep_dialed_by_picks_the_lower_id_from_either_side() {
        assert_eq!(keep_dialed_by(&low(), &high()), low());
        assert_eq!(keep_dialed_by(&high(), &low()), low());
        assert_eq!(keep_dialed_by(&low(), &low()), low());
    }
}
