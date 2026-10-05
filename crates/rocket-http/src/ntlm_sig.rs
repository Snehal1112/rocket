//! NTLMv2 authentication messages (MS-NLMP), without signing, sealing or key exchange.
//!
//! Pure functions only. The executor in `rocket-infra` runs the handshake: message 1, the
//! server's challenge (message 2) in a 401, then message 3, all on one connection. NTLMv1 and
//! LM-only servers are not supported.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use md4::{Digest as _, Md4};
use rand::RngCore;

type HmacMd5 = Hmac<md5::Md5>;

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";

const NEGOTIATE_UNICODE: u32 = 0x0000_0001;
const REQUEST_TARGET: u32 = 0x0000_0004;
const NEGOTIATE_NTLM: u32 = 0x0000_0200;
const NEGOTIATE_ALWAYS_SIGN: u32 = 0x0000_8000;
const NEGOTIATE_EXTENDED_SESSIONSECURITY: u32 = 0x0008_0000;
const NEGOTIATE_128: u32 = 0x2000_0000;
const NEGOTIATE_56: u32 = 0x8000_0000;

/// What the client asks for in message 1, and the most it will accept in message 3.
const CLIENT_FLAGS: u32 = NEGOTIATE_UNICODE
    | REQUEST_TARGET
    | NEGOTIATE_NTLM
    | NEGOTIATE_ALWAYS_SIGN
    | NEGOTIATE_EXTENDED_SESSIONSECURITY
    | NEGOTIATE_128
    | NEGOTIATE_56;

/// AV pair id of the server's timestamp.
const AV_TIMESTAMP: u16 = 7;

/// The parts of the server's message 2 that message 3 needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtlmChallenge {
    pub flags: u32,
    pub server_challenge: [u8; 8],
    /// The server's AV pairs, including the end marker, copied into the response.
    pub target_info: Vec<u8>,
    /// The server's `MsvAvTimestamp`, when it sent one.
    pub timestamp: Option<[u8; 8]>,
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// HMAC-MD5 over the concatenation of `parts`. HMAC accepts a key of any length, so building
/// the MAC cannot fail; the zero fallback only exists to keep this function free of panics.
fn hmac_md5(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let Ok(mut mac) = HmacMd5::new_from_slice(key) else {
        return [0u8; 16];
    };
    for part in parts {
        mac.update(part);
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

/// The NT hash: MD4 of the UTF-16LE password.
pub fn nt_hash(password: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&Md4::digest(utf16le(password)));
    out
}

/// NTOWFv2: HMAC-MD5 keyed with the NT hash over the upper-cased user name plus the domain.
pub fn ntowfv2(password: &str, username: &str, domain: &str) -> [u8; 16] {
    let identity = utf16le(&format!("{}{}", username.to_uppercase(), domain));
    hmac_md5(&nt_hash(password), &[&identity])
}

/// Message 1: no domain, no workstation.
pub fn negotiate_message() -> Vec<u8> {
    let mut m = Vec::with_capacity(32);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&1u32.to_le_bytes());
    m.extend_from_slice(&CLIENT_FLAGS.to_le_bytes());
    for _ in 0..2 {
        m.extend_from_slice(&0u16.to_le_bytes());
        m.extend_from_slice(&0u16.to_le_bytes());
        m.extend_from_slice(&32u32.to_le_bytes());
    }
    m
}

fn read_u16(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(msg.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(msg: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(msg.get(at..at + 4)?.try_into().ok()?))
}

/// Reads the server's message 2.
pub fn parse_challenge(msg: &[u8]) -> Result<NtlmChallenge, String> {
    if msg.len() < 32 || &msg[..8] != SIGNATURE {
        return Err("not an NTLM message".into());
    }
    if read_u32(msg, 8) != Some(2) {
        return Err("not an NTLM challenge message".into());
    }
    let flags = read_u32(msg, 20).ok_or("truncated challenge")?;
    let mut server_challenge = [0u8; 8];
    server_challenge.copy_from_slice(msg.get(24..32).ok_or("truncated challenge")?);
    if msg.len() < 48 {
        return Err(
            "the server sent no target information, so only NTLMv1 is possible, \
             which is not supported"
                .into(),
        );
    }
    let len = read_u16(msg, 40).ok_or("truncated challenge")? as usize;
    let offset = read_u32(msg, 44).ok_or("truncated challenge")? as usize;
    let target_info = msg
        .get(
            offset
                ..offset
                    .checked_add(len)
                    .ok_or("malformed target information")?,
        )
        .ok_or("the target information lies outside the message")?
        .to_vec();
    Ok(NtlmChallenge {
        flags,
        server_challenge,
        timestamp: find_timestamp(&target_info),
        target_info,
    })
}

fn find_timestamp(info: &[u8]) -> Option<[u8; 8]> {
    let mut at = 0;
    while let (Some(id), Some(len)) = (read_u16(info, at), read_u16(info, at + 2)) {
        if id == 0 {
            return None;
        }
        let value = info.get(at + 4..at + 4 + len as usize)?;
        if id == AV_TIMESTAMP && len == 8 {
            let mut out = [0u8; 8];
            out.copy_from_slice(value);
            return Some(out);
        }
        at += 4 + len as usize;
    }
    None
}

/// Message 3 with an NTLMv2 response. `client_time` is a Windows FILETIME, used only when the
/// server sent no timestamp. When it did, that timestamp is used and the LM response is zeroed.
pub fn authenticate_message(
    challenge: &NtlmChallenge,
    username: &str,
    password: &str,
    domain: &str,
    workstation: &str,
    client_nonce: [u8; 8],
    client_time: u64,
) -> Vec<u8> {
    let key = ntowfv2(password, username, domain);
    let time = challenge.timestamp.unwrap_or(client_time.to_le_bytes());

    let mut blob = Vec::with_capacity(32 + challenge.target_info.len());
    blob.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
    blob.extend_from_slice(&time);
    blob.extend_from_slice(&client_nonce);
    blob.extend_from_slice(&[0, 0, 0, 0]);
    blob.extend_from_slice(&challenge.target_info);
    blob.extend_from_slice(&[0, 0, 0, 0]);

    let proof = hmac_md5(&key, &[&challenge.server_challenge, &blob]);
    let mut nt_response = proof.to_vec();
    nt_response.extend_from_slice(&blob);

    let lm_response = if challenge.timestamp.is_some() {
        vec![0u8; 24]
    } else {
        let mut lm = hmac_md5(&key, &[&challenge.server_challenge, &client_nonce]).to_vec();
        lm.extend_from_slice(&client_nonce);
        lm
    };

    let domain_bytes = utf16le(domain);
    let user_bytes = utf16le(username);
    let workstation_bytes = utf16le(workstation);
    // Payload order: domain, user, workstation, LM response, NT response, session key (empty).
    let payload: [&[u8]; 6] = [
        &domain_bytes,
        &user_bytes,
        &workstation_bytes,
        &lm_response,
        &nt_response,
        &[],
    ];
    // Header: signature, type, six security buffers, flags. The payload starts right after it.
    let header_len = 64u32;
    let mut offsets = [0u32; 6];
    let mut next = header_len;
    for (i, part) in payload.iter().enumerate() {
        offsets[i] = next;
        next += part.len() as u32;
    }
    let buffer = |m: &mut Vec<u8>, i: usize| {
        let len = payload[i].len() as u16;
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&offsets[i].to_le_bytes());
    };

    let mut m = Vec::with_capacity(next as usize);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&3u32.to_le_bytes());
    // Wire order of the buffers: LM, NT, domain, user, workstation, session key.
    for i in [3, 4, 0, 1, 2, 5] {
        buffer(&mut m, i);
    }
    let flags = (challenge.flags & CLIENT_FLAGS) | NEGOTIATE_UNICODE;
    m.extend_from_slice(&flags.to_le_bytes());
    for part in payload {
        m.extend_from_slice(part);
    }
    m
}

/// `NTLM <base64>` for an `Authorization` header.
pub fn header_value(message: &[u8]) -> String {
    format!("NTLM {}", STANDARD.encode(message))
}

/// The message in an `NTLM <base64>` header value, or `None` for another scheme or a bare
/// `NTLM` with no message.
pub fn parse_header(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    let (scheme, rest) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("ntlm") {
        return None;
    }
    STANDARD.decode(rest.trim()).ok().filter(|m| !m.is_empty())
}

/// The NTLM message among `WWW-Authenticate` values. A value may list several schemes.
pub fn find_token(values: &[&str]) -> Option<Vec<u8>> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .find_map(parse_header)
}

/// Splits `DOMAIN\user` when no domain is given separately. An explicit domain always wins.
pub fn split_account(username: &str, domain: &str) -> (String, String) {
    if domain.is_empty() {
        if let Some((d, u)) = username.split_once('\\') {
            return (u.to_string(), d.to_string());
        }
    }
    (username.to_string(), domain.to_string())
}

pub fn random_nonce() -> [u8; 8] {
    let mut nonce = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// The current time as a Windows FILETIME: 100 ns ticks since 1601-01-01.
pub fn file_time_now() -> u64 {
    let now = chrono::Utc::now();
    let secs = now.timestamp().max(0) as u64 + 11_644_473_600;
    secs * 10_000_000 + u64::from(now.timestamp_subsec_nanos()) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// The target information of the [MS-NLMP] section 4.2.4 examples: NbDomainName "Domain",
    /// NbComputerName "Server", then the end marker.
    fn spec_target_info() -> Vec<u8> {
        let mut v = Vec::new();
        for (id, text) in [(2u16, "Domain"), (1u16, "Server")] {
            let name: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
            v.extend(id.to_le_bytes());
            v.extend((name.len() as u16).to_le_bytes());
            v.extend(name);
        }
        v.extend([0, 0, 0, 0]);
        v
    }

    fn spec_challenge() -> NtlmChallenge {
        NtlmChallenge {
            flags: 0xA088_8215,
            server_challenge: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
            target_info: spec_target_info(),
            timestamp: None,
        }
    }

    /// Reads a security buffer (length, max length, offset) at `at` and returns its bytes.
    fn buffer(msg: &[u8], at: usize) -> Vec<u8> {
        let len = u16::from_le_bytes([msg[at], msg[at + 1]]) as usize;
        let off = u32::from_le_bytes([msg[at + 4], msg[at + 5], msg[at + 6], msg[at + 7]]) as usize;
        msg[off..off + len].to_vec()
    }

    #[test]
    fn nt_hash_matches_the_well_known_value() {
        assert_eq!(
            nt_hash("Password").to_vec(),
            unhex("a4f49c406510bdcab6824ee7c30fd852")
        );
    }

    #[test]
    fn ntowfv2_matches_the_spec_example() {
        // [MS-NLMP] 4.2.4.1.1: User "User", Domain "Domain", Password "Password".
        assert_eq!(
            ntowfv2("Password", "User", "Domain").to_vec(),
            unhex("0c868a403bfd7a93a3001ef22ef02e3f")
        );
    }

    #[test]
    fn the_nt_proof_and_lm_response_match_the_spec_example() {
        // [MS-NLMP] 4.2.4.2: client challenge aa..aa, time 0.
        let msg = authenticate_message(
            &spec_challenge(),
            "User",
            "Password",
            "Domain",
            "COMPUTER",
            [0xaa; 8],
            0,
        );
        let lm = buffer(&msg, 12);
        let nt = buffer(&msg, 20);
        assert_eq!(
            lm,
            unhex("86c35097ac9cec102554764a57cccc19aaaaaaaaaaaaaaaa")
        );
        assert_eq!(nt[..16].to_vec(), unhex("68cd0ab851e51c96aabc927bebef6a1c"));
        // The temp blob follows the proof: version 1.1, zeros, time, client nonce, zeros,
        // the server's target information, zeros.
        let blob = &nt[16..];
        assert_eq!(blob[..8].to_vec(), unhex("0101000000000000"));
        assert_eq!(blob[8..16].to_vec(), vec![0u8; 8]);
        assert_eq!(blob[16..24].to_vec(), vec![0xaa; 8]);
        assert!(blob
            .windows(spec_target_info().len())
            .any(|w| w == spec_target_info().as_slice()));
    }

    #[test]
    fn message_three_has_a_consistent_layout() {
        let msg = authenticate_message(
            &spec_challenge(),
            "User",
            "Password",
            "Domain",
            "COMPUTER",
            [1; 8],
            0,
        );
        assert_eq!(&msg[..8], b"NTLMSSP\0");
        assert_eq!(u32::from_le_bytes([msg[8], msg[9], msg[10], msg[11]]), 3);
        let utf16 =
            |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() };
        assert_eq!(buffer(&msg, 28), utf16("Domain"));
        assert_eq!(buffer(&msg, 36), utf16("User"));
        assert_eq!(buffer(&msg, 44), utf16("COMPUTER"));
        assert!(buffer(&msg, 52).is_empty(), "no session key is negotiated");
        let flags = u32::from_le_bytes([msg[60], msg[61], msg[62], msg[63]]);
        assert_ne!(flags & 0x1, 0, "unicode must be set");
        assert_eq!(flags & 0x4000_0000, 0, "key exchange is never claimed");
    }

    #[test]
    fn a_server_timestamp_is_used_and_the_lm_response_is_zeroed() {
        let mut challenge = spec_challenge();
        challenge.timestamp = Some([9; 8]);
        let msg = authenticate_message(&challenge, "u", "p", "d", "w", [2; 8], 12345);
        assert_eq!(buffer(&msg, 12), vec![0u8; 24]);
        let nt = buffer(&msg, 20);
        assert_eq!(
            nt[16 + 8..16 + 16].to_vec(),
            vec![9u8; 8],
            "the server's time, not ours"
        );
    }

    #[test]
    fn message_one_is_a_bare_negotiate() {
        let msg = negotiate_message();
        assert_eq!(&msg[..8], b"NTLMSSP\0");
        assert_eq!(u32::from_le_bytes([msg[8], msg[9], msg[10], msg[11]]), 1);
        assert_eq!(msg.len(), 32);
    }

    fn challenge_bytes() -> Vec<u8> {
        let info = spec_target_info();
        let mut m = Vec::new();
        m.extend(b"NTLMSSP\0");
        m.extend(2u32.to_le_bytes());
        m.extend([0, 0, 0, 0]);
        m.extend(48u32.to_le_bytes());
        m.extend(0xA088_8215u32.to_le_bytes());
        m.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        m.extend([0u8; 8]);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        m
    }

    #[test]
    fn parses_a_challenge() {
        let c = parse_challenge(&challenge_bytes()).expect("parse");
        assert_eq!(c.flags, 0xA088_8215);
        assert_eq!(c.server_challenge, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(c.target_info, spec_target_info());
        assert_eq!(c.timestamp, None);
    }

    #[test]
    fn finds_the_server_timestamp_pair() {
        let mut info = Vec::new();
        info.extend(7u16.to_le_bytes());
        info.extend(8u16.to_le_bytes());
        info.extend([7u8; 8]);
        info.extend([0, 0, 0, 0]);
        let mut m = challenge_bytes();
        m.truncate(40);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        assert_eq!(parse_challenge(&m).expect("parse").timestamp, Some([7; 8]));
    }

    #[test]
    fn rejects_messages_that_are_not_a_usable_challenge() {
        assert!(parse_challenge(b"nope").is_err());
        let mut wrong_type = challenge_bytes();
        wrong_type[8] = 1;
        assert!(parse_challenge(&wrong_type).is_err());
        let mut short = challenge_bytes();
        short.truncate(30);
        assert!(
            parse_challenge(&short).is_err(),
            "no target information means NTLMv1 only"
        );
        let mut lying = challenge_bytes();
        lying[44] = 0xff; // target info offset far outside the message
        assert!(parse_challenge(&lying).is_err());
    }

    #[test]
    fn header_helpers_round_trip_and_find_a_token_among_schemes() {
        let msg = negotiate_message();
        let header = header_value(&msg);
        assert!(header.starts_with("NTLM "));
        assert_eq!(parse_header(&header), Some(msg.clone()));
        assert_eq!(parse_header("Basic abc"), None);
        assert_eq!(
            parse_header("NTLM"),
            None,
            "a bare scheme carries no message"
        );
        let values = ["Negotiate", header.as_str(), "Basic realm=x"];
        assert_eq!(find_token(&values), Some(msg));
        assert_eq!(find_token(&["Negotiate, NTLM"]), None);
        let combined = format!("Negotiate, {header}");
        assert!(find_token(&[combined.as_str()]).is_some());
    }

    #[test]
    fn splits_domain_and_user_forms() {
        assert_eq!(
            split_account("CORP\\bob", ""),
            ("bob".into(), "CORP".into())
        );
        assert_eq!(split_account("bob", "CORP"), ("bob".into(), "CORP".into()));
        assert_eq!(
            split_account("bob@corp.test", ""),
            ("bob@corp.test".into(), String::new())
        );
        // An explicit domain wins over a prefix on the user name.
        assert_eq!(
            split_account("OTHER\\bob", "CORP"),
            ("OTHER\\bob".into(), "CORP".into())
        );
    }

    #[test]
    fn file_time_is_after_the_unix_epoch_offset() {
        // 1601-01-01 to 1970-01-01 is 11644473600 seconds, in 100 ns ticks.
        assert!(file_time_now() > 11_644_473_600u64 * 10_000_000);
    }
}
