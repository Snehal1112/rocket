use base64::Engine;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};

/// Decoded JWT claims for display purposes.
/// Extracted from both the JWT header and payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JwtClaims {
    /// sub claim
    pub subject: Option<String>,
    /// iss claim
    pub issuer: Option<String>,
    /// aud claim (stringified — array joined with space)
    pub audience: Option<String>,
    /// exp claim (unix timestamp)
    pub expiry: Option<u64>,
    /// iat claim (unix timestamp)
    pub issued_at: Option<u64>,
    /// scope or scp claim
    pub scope: Option<String>,
    /// typ from JWT header
    pub token_type: Option<String>,
    /// alg from JWT header
    pub algorithm: Option<String>,
    /// Full JSON payload as a pretty-printed string
    pub raw_payload: String,
}

/// Decodes a JWT token WITHOUT signature verification.
/// Used for display purposes only (showing token metadata in the UI).
pub fn decode_jwt(token: &str) -> Result<JwtClaims, DomainError> {
    // Split the JWT into its 3 parts.
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(DomainError::InvalidInput(
            "Invalid JWT: expected 3 dot-separated parts.".into(),
        ));
    }

    // Decode header.
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT header encoding: {e}")))?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT header JSON: {e}")))?;

    // Decode payload.
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT payload encoding: {e}")))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT payload JSON: {e}")))?;

    // Extract audience — can be string or array of strings.
    let audience = match payload.get("aud") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(arr)) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    };

    // Extract scope — check both "scope" (standard) and "scp" (Azure AD).
    let scope = payload
        .get("scope")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("scp").and_then(|v| v.as_str()))
        .map(String::from);

    let raw_payload =
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());

    Ok(JwtClaims {
        subject: payload.get("sub").and_then(|v| v.as_str()).map(String::from),
        issuer: payload.get("iss").and_then(|v| v.as_str()).map(String::from),
        audience,
        expiry: payload.get("exp").and_then(|v| v.as_u64()),
        issued_at: payload.get("iat").and_then(|v| v.as_u64()),
        scope,
        token_type: header.get("typ").and_then(|v| v.as_str()).map(String::from),
        algorithm: header.get("alg").and_then(|v| v.as_str()).map(String::from),
        raw_payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    /// Helper: builds a JWT string from header and payload JSON (no real signature).
    fn fake_jwt(header_json: &str, payload_json: &str) -> String {
        let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{h}.{p}.fake_signature")
    }

    #[test]
    fn decodes_standard_oidc_claims() {
        let token = fake_jwt(
            r#"{"alg":"RS256","typ":"JWT"}"#,
            r#"{"sub":"user123","iss":"https://auth.example.com","aud":"my-client","exp":1700000000,"iat":1699996400,"scope":"openid email profile"}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.subject.as_deref(), Some("user123"));
        assert_eq!(claims.issuer.as_deref(), Some("https://auth.example.com"));
        assert_eq!(claims.audience.as_deref(), Some("my-client"));
        assert_eq!(claims.expiry, Some(1700000000));
        assert_eq!(claims.issued_at, Some(1699996400));
        assert_eq!(claims.scope.as_deref(), Some("openid email profile"));
        assert_eq!(claims.algorithm.as_deref(), Some("RS256"));
        assert_eq!(claims.token_type.as_deref(), Some("JWT"));
    }

    #[test]
    fn decodes_azure_scp_claim() {
        let token = fake_jwt(
            r#"{"alg":"RS256"}"#,
            r#"{"sub":"user","scp":"User.Read Mail.Read"}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.scope.as_deref(), Some("User.Read Mail.Read"));
    }

    #[test]
    fn decodes_array_audience() {
        let token = fake_jwt(
            r#"{"alg":"RS256"}"#,
            r#"{"aud":["client-1","client-2"]}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.audience.as_deref(), Some("client-1 client-2"));
    }

    #[test]
    fn handles_minimal_token() {
        let token = fake_jwt(r#"{"alg":"none"}"#, r#"{}"#);
        let claims = decode_jwt(&token).unwrap();
        assert!(claims.subject.is_none());
        assert!(claims.issuer.is_none());
        assert!(claims.expiry.is_none());
        assert_eq!(claims.algorithm.as_deref(), Some("none"));
    }

    #[test]
    fn rejects_malformed_token() {
        let result = decode_jwt("not.a.valid.jwt");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_jwt_string() {
        let result = decode_jwt("just-a-random-string");
        assert!(result.is_err());
    }
}
