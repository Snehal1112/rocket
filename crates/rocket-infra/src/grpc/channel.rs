use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rocket_grpc::GrpcCall;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::grpc::GrpcMetadataPair;
use tonic::metadata::{Ascii, Binary, KeyAndValueRef, MetadataKey, MetadataMap, MetadataValue};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Where to connect and whether to use TLS.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Target {
    /// `http://authority` or `https://authority`, which is what tonic expects.
    pub uri: String,
    pub tls: bool,
}

/// `grpcs://` and `https://` use TLS. `grpc://`, `http://` and a bare `host:port`
/// do not. Anything after the authority is ignored, since the method is chosen separately.
pub(crate) fn parse_target(url: &str) -> DomainResult<Target> {
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

/// Opens a channel. The connect step has its own 10 second limit. The deadline of
/// the call itself is applied by the executor.
pub(crate) async fn connect(call: &GrpcCall) -> DomainResult<Channel> {
    let target = parse_target(&call.url)?;
    let mut endpoint = Endpoint::from_shared(target.uri.clone())
        .map_err(|e| DomainError::InvalidInput(format!("invalid gRPC URL '{}': {e}", call.url)))?
        .connect_timeout(CONNECT_TIMEOUT);
    if target.tls {
        let mut tls = ClientTlsConfig::new().with_native_roots();
        if let Some(pem) = &call.tls_ca_pem {
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| DomainError::InvalidInput(format!("invalid TLS setup: {e}")))?;
    }
    endpoint.connect().await.map_err(|e| {
        DomainError::Http(format!(
            "could not connect to {}: {}",
            target.uri,
            describe_error(&e)
        ))
    })
}

/// Joins an error and its sources, so "transport error" comes with the real cause.
pub(crate) fn describe_error(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts: Vec<String> = vec![error.to_string()];
    let mut source = error.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if parts.last() != Some(&text) {
            parts.push(text);
        }
        source = inner.source();
    }
    parts.join(": ")
}

/// Names the client sets itself. A user value for one of them would break the call.
fn is_reserved(name: &str) -> bool {
    name.starts_with("grpc-") || matches!(name, "content-type" | "te" | "user-agent")
}

/// Copies `pairs` into request metadata. Names are lower-cased. A `-bin` name takes
/// base64 text. Entries with an empty name are skipped.
pub(crate) fn apply_metadata(
    map: &mut MetadataMap,
    pairs: &[GrpcMetadataPair],
) -> DomainResult<()> {
    for pair in pairs {
        let name = pair.name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if is_reserved(&name) {
            return Err(DomainError::InvalidInput(format!(
                "metadata '{name}' is set by the gRPC client and cannot be overridden"
            )));
        }
        if name.ends_with("-bin") {
            let bytes = STANDARD.decode(pair.value.trim()).map_err(|e| {
                DomainError::InvalidInput(format!("metadata '{name}' must be base64: {e}"))
            })?;
            let key = MetadataKey::<Binary>::from_bytes(name.as_bytes()).map_err(|e| {
                DomainError::InvalidInput(format!("invalid metadata name '{name}': {e}"))
            })?;
            map.append_bin(key, MetadataValue::from_bytes(&bytes));
        } else {
            let key = MetadataKey::<Ascii>::from_bytes(name.as_bytes()).map_err(|e| {
                DomainError::InvalidInput(format!("invalid metadata name '{name}': {e}"))
            })?;
            // gRPC allows printable ASCII only. tonic would also pass bytes above 0x7e.
            let printable = pair.value.bytes().all(|b| (0x20..=0x7e).contains(&b));
            let value = MetadataValue::<Ascii>::try_from(pair.value.as_str())
                .ok()
                .filter(|_| printable)
                .ok_or_else(|| {
                    DomainError::InvalidInput(format!(
                        "metadata '{name}' has a value that is not printable ASCII"
                    ))
                })?;
            map.append(key, value);
        }
    }
    Ok(())
}

/// Reads response metadata. Binary values are shown as base64.
pub(crate) fn pairs_from(map: &MetadataMap) -> Vec<GrpcMetadataPair> {
    map.iter()
        .map(|entry| match entry {
            KeyAndValueRef::Ascii(key, value) => {
                GrpcMetadataPair::new(key.as_str(), value.to_str().unwrap_or("<not printable>"))
            }
            KeyAndValueRef::Binary(key, value) => GrpcMetadataPair::new(
                key.as_str(),
                value
                    .to_bytes()
                    .map(|b| STANDARD.encode(b))
                    .unwrap_or_else(|_| "<invalid base64>".into()),
            ),
        })
        .collect()
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

    #[test]
    fn metadata_names_are_lowercased_and_empty_names_skipped() {
        let mut map = MetadataMap::new();
        apply_metadata(
            &mut map,
            &[
                GrpcMetadataPair::new("X-Trace-Id", "abc"),
                GrpcMetadataPair::new("  ", "ignored"),
            ],
        )
        .expect("apply");
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("x-trace-id").and_then(|v| v.to_str().ok()),
            Some("abc")
        );
    }

    #[test]
    fn reserved_names_are_rejected() {
        for name in ["content-type", "TE", "grpc-timeout", "user-agent"] {
            let mut map = MetadataMap::new();
            let err =
                apply_metadata(&mut map, &[GrpcMetadataPair::new(name, "x")]).expect_err(name);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{name}");
        }
    }

    #[test]
    fn binary_metadata_takes_base64_and_reads_back_as_base64() {
        let mut map = MetadataMap::new();
        apply_metadata(&mut map, &[GrpcMetadataPair::new("trace-bin", "aGk=")]).expect("apply");
        let pairs = pairs_from(&map);
        assert_eq!(pairs, vec![GrpcMetadataPair::new("trace-bin", "aGk=")]);
        let err = apply_metadata(&mut map, &[GrpcMetadataPair::new("x-bin", "!!")])
            .expect_err("bad base64");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_non_ascii_value_is_rejected_by_name() {
        let mut map = MetadataMap::new();
        let err = apply_metadata(&mut map, &[GrpcMetadataPair::new("x-name", "caf\u{e9}")])
            .expect_err("non ascii");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("x-name")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn connecting_to_a_closed_port_names_the_address() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let call = GrpcCall {
            url: format!("127.0.0.1:{port}"),
            full_method: "a.B/C".into(),
            metadata: vec![],
            timeout: None,
            tls_ca_pem: None,
        };
        let err = connect(&call).await.expect_err("closed port");
        assert!(
            matches!(&err, DomainError::Http(m) if m.contains(&format!("127.0.0.1:{port}"))),
            "{err:?}"
        );
    }
}
