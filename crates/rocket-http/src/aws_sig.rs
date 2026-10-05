use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// AWS credentials used for Signature Version 4 signing.
pub struct AwsCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub service: String,
    pub session_token: Option<String>,
}

/// Prints the access key id, region and service. The secret key and the token are redacted.
impl std::fmt::Debug for AwsCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsCredentials")
            .field("access_key", &self.access_key)
            .field("secret_key", &"<redacted>")
            .field("region", &self.region)
            .field("service", &self.service)
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Headers produced by signing a request with AWS Signature v4.
pub struct SignedHeaders {
    pub authorization: String,
    pub x_amz_date: String,
    pub x_amz_security_token: Option<String>,
    pub x_amz_content_sha256: String,
}

/// Payload hash for a body that cannot be hashed up front, such as a streamed multipart body.
/// AWS accepts it over TLS for services that support unsigned payloads, such as S3.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Sign an HTTP request using AWS Signature Version 4.
///
/// Returns the headers that must be added to the outgoing request.
/// The `timestamp` must be in ISO 8601 basic format: `"20130524T000000Z"`.
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    credentials: &AwsCredentials,
    timestamp: &str,
) -> Result<SignedHeaders, String> {
    sign_request_with_payload_hash(
        method,
        url,
        headers,
        &hex_sha256(body),
        credentials,
        timestamp,
    )
}

/// Sign an HTTP request using AWS Signature Version 4, with the payload hash given by the
/// caller: the hex SHA-256 of the body, or `UNSIGNED_PAYLOAD`.
///
/// Returns the headers that must be added to the outgoing request.
/// The `timestamp` must be in ISO 8601 basic format: `"20130524T000000Z"`.
pub fn sign_request_with_payload_hash(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    payload_hash: &str,
    credentials: &AwsCredentials,
    timestamp: &str,
) -> Result<SignedHeaders, String> {
    let parsed_url = reqwest::Url::parse(url).map_err(|e| format!("Failed to parse URL: {e}"))?;

    // Date portion is the first 8 characters of the timestamp.
    let date_stamp = timestamp
        .get(..8)
        .ok_or_else(|| format!("Invalid signing timestamp: {timestamp}"))?;

    // Build canonical headers and signed-headers list.
    // We must include any caller-supplied headers plus x-amz-date (and
    // x-amz-security-token when present). All header names are lowercased
    // and sorted alphabetically.
    let mut canonical_headers_map: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();

    canonical_headers_map.push(("x-amz-date".to_string(), timestamp.to_string()));

    if let Some(token) = &credentials.session_token {
        canonical_headers_map.push(("x-amz-security-token".to_string(), token.clone()));
    }

    // S3 requires the content hash header to be signed. The executor always sends it.
    if credentials.service == "s3" {
        canonical_headers_map.push(("x-amz-content-sha256".to_string(), payload_hash.to_string()));
    }

    // Sort by header name for canonical ordering.
    canonical_headers_map.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = canonical_headers_map
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect();

    let signed_headers: String = canonical_headers_map
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<&str>>()
        .join(";");

    let canonical_uri = canonical_path(parsed_url.path(), &credentials.service);
    let canonical_querystring = canonical_query(&parsed_url);

    // Step 1: Canonical request.
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_querystring}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    // Step 2: String to sign.
    let scope = format!(
        "{date_stamp}/{}/{}/aws4_request",
        credentials.region, credentials.service
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex_sha256(canonical_request.as_bytes())
    );

    // Step 3: Signing key via HMAC chain.
    let k_date = hmac_sha256(
        format!("AWS4{}", credentials.secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, credentials.region.as_bytes());
    let k_service = hmac_sha256(&k_region, credentials.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    // Step 4: Signature.
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    // Step 5: Authorization header.
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key
    );

    Ok(SignedHeaders {
        authorization,
        x_amz_date: timestamp.to_string(),
        x_amz_security_token: credentials.session_token.clone(),
        x_amz_content_sha256: payload_hash.to_string(),
    })
}

/// Canonical query string: every key and value percent-encoded, sorted by encoded key.
fn canonical_query(url: &reqwest::Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            (
                urlencoding::encode(&k).into_owned(),
                urlencoding::encode(&v).into_owned(),
            )
        })
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Canonical URI for the signature. The URL path is already percent-encoded once. Every service
/// except S3 signs the path encoded a second time; S3 signs it as it is.
fn canonical_path(path: &str, service: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    if service == "s3" {
        return path.to_string();
    }
    path.split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Compute HMAC-SHA256 and return the raw bytes.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Compute hex-encoded SHA-256 digest.
pub fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_request_produces_valid_authorization() {
        let creds = AwsCredentials {
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
        };
        let result = sign_request(
            "GET",
            "https://examplebucket.s3.amazonaws.com/test.txt",
            &[(
                "host".to_string(),
                "examplebucket.s3.amazonaws.com".to_string(),
            )],
            b"",
            &creds,
            "20130524T000000Z",
        )
        .unwrap();

        assert!(result.authorization.starts_with("AWS4-HMAC-SHA256"));
        assert!(result.authorization.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result
            .authorization
            .contains("20130524/us-east-1/s3/aws4_request"));
        assert_eq!(result.x_amz_date, "20130524T000000Z");
        // SHA-256 of empty string.
        assert_eq!(
            result.x_amz_content_sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sign_with_session_token() {
        let creds = AwsCredentials {
            access_key: "AKID".into(),
            secret_key: "SECRET".into(),
            region: "us-west-2".into(),
            service: "execute-api".into(),
            session_token: Some("TOKEN123".into()),
        };
        let result = sign_request(
            "GET",
            "https://api.example.com/",
            &[],
            b"",
            &creds,
            "20240101T000000Z",
        )
        .unwrap();
        assert_eq!(result.x_amz_security_token, Some("TOKEN123".into()));
    }

    /// AWS Signature V4 requires query parameters in the canonical request to
    /// be sorted alphabetically by key.  The URL has `b=2&a=1` (wrong order)
    /// and sign_request must normalise it to `a=1&b=2`.  We verify this by
    /// checking that the resulting Authorization header is deterministic and
    /// identical when the same params are given in either order.
    #[test]
    fn test_sign_request_query_params_are_sorted_canonically() {
        let creds = AwsCredentials {
            access_key: "AKID".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
        };
        let headers = &[("host".to_string(), "example.com".to_string())];
        // Params in reverse-alphabetical order — must produce same sig as sorted.
        let reversed = sign_request(
            "GET",
            "https://example.com/path?b=2&a=1",
            headers,
            b"",
            &creds,
            "20240101T000000Z",
        )
        .unwrap();
        // Params already in alphabetical order.
        let sorted = sign_request(
            "GET",
            "https://example.com/path?a=1&b=2",
            headers,
            b"",
            &creds,
            "20240101T000000Z",
        )
        .unwrap();
        assert_eq!(
            reversed.authorization, sorted.authorization,
            "query params in any order must produce identical Authorization — \
             canonical form requires alphabetical key sort"
        );
    }

    #[test]
    fn test_sign_request_with_body() {
        let creds = AwsCredentials {
            access_key: "AKID".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
        };
        let body = b"hello world";
        let result = sign_request(
            "PUT",
            "https://example.com/upload",
            &[("host".to_string(), "example.com".to_string())],
            body,
            &creds,
            "20240101T000000Z",
        )
        .unwrap();
        // Body hash should not be the empty-string hash.
        assert_ne!(
            result.x_amz_content_sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn matches_the_aws_get_vanilla_test_vector() {
        let creds = AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "service".into(),
            session_token: None,
        };
        let signed = sign_request(
            "GET",
            "https://example.amazonaws.com/",
            &[("host".to_string(), "example.amazonaws.com".to_string())],
            b"",
            &creds,
            "20150830T123600Z",
        )
        .expect("sign");
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    /// The "get-vanilla-query-order-key-case" vector of the AWS SigV4 test suite.
    #[test]
    fn matches_the_aws_query_order_test_vector() {
        let creds = AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "service".into(),
            session_token: None,
        };
        let signed = sign_request(
            "GET",
            "https://example.amazonaws.com/?Param2=value2&Param1=value1",
            &[("host".to_string(), "example.amazonaws.com".to_string())],
            b"",
            &creds,
            "20150830T123600Z",
        )
        .expect("sign");
        assert!(
            signed.authorization.ends_with(
                "Signature=b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
            ),
            "{}",
            signed.authorization
        );
    }

    fn s3_creds() -> AwsCredentials {
        AwsCredentials {
            access_key: "AKID".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
        }
    }

    #[test]
    fn debug_output_redacts_the_secret_key_and_token() {
        let mut creds = s3_creds();
        creds.session_token = Some("TOKEN123".into());
        let text = format!("{creds:?}");
        assert!(text.contains("AKID"), "{text}");
        assert!(
            !text.contains("SECRET") && !text.contains("TOKEN123"),
            "{text}"
        );
    }

    #[test]
    fn the_payload_hash_entry_point_matches_hashing_the_body() {
        let body = b"{\"a\":1}";
        let headers = [("host".to_string(), "example.com".to_string())];
        let by_body = sign_request(
            "POST",
            "https://example.com/x",
            &headers,
            body,
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        let by_hash = sign_request_with_payload_hash(
            "POST",
            "https://example.com/x",
            &headers,
            &hex_sha256(body),
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert_eq!(by_body.authorization, by_hash.authorization);
        assert_eq!(by_hash.x_amz_content_sha256, hex_sha256(body));
    }

    #[test]
    fn an_explicit_unsigned_payload_marker_is_used_verbatim() {
        let signed = sign_request_with_payload_hash(
            "POST",
            "https://example.com/x",
            &[("host".to_string(), "example.com".to_string())],
            UNSIGNED_PAYLOAD,
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert_eq!(signed.x_amz_content_sha256, "UNSIGNED-PAYLOAD");
    }

    #[test]
    fn query_values_are_percent_encoded_in_the_canonical_request() {
        // The URL parser decodes `+` and raw spaces to a space and `%2F` to `/`. The canonical
        // query must encode every key and value again, then sort by the encoded key.
        let url = reqwest::Url::parse("https://example.com/p?b=a b&a=1+2&c=%2F").expect("url");
        assert_eq!(canonical_query(&url), "a=1%202&b=a%20b&c=%2F");
    }

    #[test]
    fn non_s3_paths_are_double_encoded_and_s3_paths_are_not() {
        let headers = [("host".to_string(), "example.com".to_string())];
        let mut api = s3_creds();
        api.service = "execute-api".into();
        let url = "https://example.com/a%20b";
        let api_sig =
            sign_request("GET", url, &headers, b"", &api, "20240101T000000Z").expect("sign");
        let s3_sig =
            sign_request("GET", url, &headers, b"", &s3_creds(), "20240101T000000Z").expect("sign");
        // Same URL, different canonical path rule, so different signatures.
        assert_ne!(api_sig.authorization, s3_sig.authorization);
        assert_eq!(canonical_path("/a%20b", "execute-api"), "/a%2520b");
        assert_eq!(canonical_path("/a%20b", "s3"), "/a%20b");
        assert_eq!(canonical_path("", "s3"), "/");
    }

    #[test]
    fn s3_requests_sign_the_content_hash_header() {
        let signed = sign_request(
            "GET",
            "https://example.com/k",
            &[("host".to_string(), "example.com".to_string())],
            b"",
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert!(
            signed
                .authorization
                .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"),
            "{}",
            signed.authorization
        );
    }

    /// A body, a host with a port, an encoded path and a query that needs re-encoding, all at
    /// once. The expected signature was computed by an independent Python implementation of
    /// SigV4 (hmac and hashlib), not by this code.
    #[test]
    fn signs_body_port_path_and_query_like_an_independent_implementation() {
        let mut creds = s3_creds();
        creds.service = "execute-api".into();
        let body = b"{\"id\":1}";
        let signed = sign_request(
            "POST",
            "https://localhost:8080/a%20b?b=a b&a=1+2&c=%2F",
            &[("host".to_string(), "localhost:8080".to_string())],
            body,
            &creds,
            "20240101T000000Z",
        )
        .expect("sign");
        assert_eq!(
            signed.x_amz_content_sha256,
            "037c9214eef74cc3887f3a4f085b4e17d76280dafd273b0ee160c09c4ba1cfd4"
        );
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKID/20240101/us-east-1/execute-api/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=eb09819a7e1508d6ac41b84b3ff8db12742630ebaf8e3dea0b8833be315ff568"
        );
    }
}
