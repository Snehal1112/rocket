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

/// Headers produced by signing a request with AWS Signature v4.
pub struct SignedHeaders {
    pub authorization: String,
    pub x_amz_date: String,
    pub x_amz_security_token: Option<String>,
    pub x_amz_content_sha256: String,
}

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
    let parsed_url =
        reqwest::Url::parse(url).map_err(|e| format!("Failed to parse URL: {e}"))?;

    // Date portion is the first 8 characters of the timestamp.
    let date_stamp = &timestamp[..8];

    // Payload hash (hex-encoded SHA-256 of the body).
    let payload_hash = hex_sha256(body);

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

    // Canonical URI (percent-encoded path).
    let canonical_uri = if parsed_url.path().is_empty() {
        "/".to_string()
    } else {
        parsed_url.path().to_string()
    };

    // Canonical query string (sorted parameters).
    let mut query_pairs: Vec<(String, String)> = parsed_url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    query_pairs.sort();
    let canonical_querystring: String = query_pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join("&");

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
        x_amz_content_sha256: payload_hash,
    })
}

/// Compute HMAC-SHA256 and return the raw bytes.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Compute hex-encoded SHA-256 digest.
fn hex_sha256(data: &[u8]) -> String {
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
}
