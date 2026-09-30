//! Client certificate selection for mutual TLS.
//!
//! Pure matching only: the executor in `rocket-infra` reads the files of the certificate
//! chosen here.

use reqwest::Url;
use rocket_shared::certificate::ClientCertificate;

/// Returns the `domain` a certificate is configured for.
pub fn certificate_domain(cert: &ClientCertificate) -> &str {
    match cert {
        ClientCertificate::Pem { domain, .. } | ClientCertificate::Pkcs12 { domain, .. } => domain,
    }
}

/// Picks the first certificate whose domain matches `url`.
///
/// A domain is a host, optionally with a scheme, a port and a path, for example
/// `api.example.com`, `https://api.example.com:8443` or `*.example.com`. `*` matches any run
/// of characters. A domain with a port only matches that port. An empty domain never matches,
/// so a half-filled entry cannot send a certificate to every host.
pub fn find_certificate<'a>(
    certs: &'a [ClientCertificate],
    url: &str,
) -> Option<&'a ClientCertificate> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let port = parsed.port_or_known_default();
    certs
        .iter()
        .find(|c| domain_matches(certificate_domain(c), &host, port))
}

fn domain_matches(domain: &str, host: &str, port: Option<u16>) -> bool {
    let domain = domain.trim().to_ascii_lowercase();
    let domain = domain
        .split_once("://")
        .map_or(domain.as_str(), |(_, rest)| rest);
    let domain = domain.split('/').next().unwrap_or("");
    if domain.is_empty() {
        return false;
    }
    let (pattern, wanted_port) = match domain.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h, p.parse::<u16>().ok())
        }
        _ => (domain, None),
    };
    if wanted_port.is_some() && wanted_port != port {
        return false;
    }
    wildcard_match(pattern, host)
}

/// Matches `text` against a pattern where `*` stands for any run of characters.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(pos) => rest = &rest[pos + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkcs12(domain: &str) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.into(),
            pkcs12_file_path: format!("/certs/{domain}.p12"),
            passphrase: None,
        }
    }

    fn found(certs: &[ClientCertificate], url: &str) -> Option<String> {
        find_certificate(certs, url).map(|c| certificate_domain(c).to_string())
    }

    #[test]
    fn exact_host_matches_ignoring_case() {
        let certs = [pkcs12("API.Example.com")];
        assert_eq!(
            found(&certs, "https://api.example.com/x").as_deref(),
            Some("API.Example.com")
        );
        assert_eq!(found(&certs, "https://other.example.com/x"), None);
    }

    #[test]
    fn wildcard_matches_subdomains_but_not_the_bare_domain() {
        let certs = [pkcs12("*.example.com")];
        assert!(found(&certs, "https://api.example.com").is_some());
        assert!(found(&certs, "https://a.b.example.com").is_some());
        assert!(found(&certs, "https://example.com").is_none());
        assert!(found(&certs, "https://example.com.evil.org").is_none());
    }

    #[test]
    fn scheme_and_path_in_the_domain_are_ignored() {
        let certs = [pkcs12("https://api.example.com/v1")];
        assert!(found(&certs, "https://api.example.com/other").is_some());
    }

    #[test]
    fn a_port_in_the_domain_must_match_the_url_port() {
        let certs = [pkcs12("api.example.com:8443")];
        assert!(found(&certs, "https://api.example.com:8443/x").is_some());
        assert!(found(&certs, "https://api.example.com/x").is_none());
        let default_port = [pkcs12("api.example.com:443")];
        assert!(found(&default_port, "https://api.example.com/x").is_some());
    }

    #[test]
    fn a_domain_without_a_port_matches_any_port() {
        let certs = [pkcs12("localhost")];
        assert!(found(&certs, "https://localhost:9443/x").is_some());
    }

    #[test]
    fn first_matching_certificate_wins() {
        let certs = [pkcs12("*.example.com"), pkcs12("api.example.com")];
        assert_eq!(
            found(&certs, "https://api.example.com").as_deref(),
            Some("*.example.com")
        );
    }

    #[test]
    fn an_empty_domain_never_matches() {
        assert!(found(&[pkcs12("")], "https://api.example.com").is_none());
        assert!(found(&[pkcs12("  ")], "https://api.example.com").is_none());
    }

    #[test]
    fn an_unparseable_url_matches_nothing() {
        assert!(found(&[pkcs12("*")], "not a url").is_none());
        assert!(found(&[], "https://api.example.com").is_none());
    }

    #[test]
    fn wildcard_in_the_middle_and_at_both_ends() {
        let certs = [pkcs12("api-*-eu.example.com")];
        assert!(found(&certs, "https://api-prod-eu.example.com").is_some());
        assert!(found(&certs, "https://api-prod-us.example.com").is_none());
        assert!(found(&[pkcs12("*")], "https://anything.test").is_some());
    }
}
