//! Where a gRPC URL connects to. The transport and the request guard both read a URL
//! through this one parser, so the guard checks the host the transport really dials.

use rocket_shared::error::{DomainError, DomainResult};

/// Where to connect and whether to use TLS.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    /// `http://authority` or `https://authority`, which is what tonic expects.
    pub uri: String,
    pub tls: bool,
}

/// `grpcs://` and `https://` use TLS. `grpc://`, `http://` and a bare `host:port`
/// do not. Anything after the authority is ignored, since the method is chosen separately.
pub fn parse_target(url: &str) -> DomainResult<Target> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidInput("the gRPC URL is empty".into()));
    }
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("grpc".to_string(), trimmed),
    };
    let tls = match scheme.as_str() {
        "grpcs" | "https" => true,
        "grpc" | "http" => false,
        other => {
            return Err(DomainError::InvalidInput(format!(
                "unsupported URL scheme '{other}'; use grpc://, grpcs://, http:// or https://"
            )))
        }
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.chars().any(char::is_whitespace) {
        return Err(DomainError::InvalidInput(format!(
            "'{trimmed}' has no valid host"
        )));
    }
    Ok(Target {
        uri: format!("{}://{authority}", if tls { "https" } else { "http" }),
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_decide_tls() {
        let cases = [
            ("localhost:50051", "http://localhost:50051", false),
            ("grpc://localhost:50051", "http://localhost:50051", false),
            ("http://localhost:50051", "http://localhost:50051", false),
            ("grpcs://api.example.com", "https://api.example.com", true),
            (
                "HTTPS://api.example.com:8443/x/y",
                "https://api.example.com:8443",
                true,
            ),
            ("  grpc://h:1/  ", "http://h:1", false),
        ];
        for (input, uri, tls) in cases {
            let t = parse_target(input).unwrap_or_else(|e| panic!("{input}: {e}"));
            assert_eq!((t.uri.as_str(), t.tls), (uri, tls), "{input}");
        }
    }

    #[test]
    fn bad_urls_are_invalid_input() {
        for input in ["", "   ", "ftp://host:1", "grpc://", "grpc:// host:1"] {
            assert!(
                matches!(parse_target(input), Err(DomainError::InvalidInput(_))),
                "{input:?}"
            );
        }
    }
}
