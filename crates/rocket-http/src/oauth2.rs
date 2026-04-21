use serde::{Deserialize, Serialize};

use rocket_shared::error::{DomainError, DomainResult};

/// Configuration for an OAuth 2.0 token request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub grant_type: String,
    pub client_id: String,
    pub client_secret: String,
    pub token_url: String,
    pub scope: Option<String>,
    /// Required for the password grant type.
    pub username: Option<String>,
    /// Required for the password grant type.
    pub password: Option<String>,
    /// Required for the authorization_code grant type.
    pub code: Option<String>,
    /// Required for the authorization_code grant type.
    pub redirect_uri: Option<String>,
    /// PKCE code verifier for the authorization_code grant type.
    pub code_verifier: Option<String>,
}

/// Represents a token response from an OAuth 2.0 provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    /// Raw ID token JWT string, if returned by the OIDC provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

impl OAuthToken {
    /// Returns true if the token has expired based on when it was acquired.
    pub fn is_expired(&self, acquired_at_secs: u64, now_secs: u64) -> bool {
        match self.expires_in {
            Some(exp) => now_secs >= acquired_at_secs + exp,
            None => false,
        }
    }
}

/// A user-defined parameter sent at a specific phase of the OAuth2 flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalParam {
    pub key: String,
    pub value: String,
    /// Where to send: "queryparams" or "body".
    pub send_in: String,
    pub enabled: bool,
}

/// Appends enabled query-type additional params to a URL string.
pub fn apply_params_to_url(url: &str, params: &[AdditionalParam]) -> String {
    let query_params: Vec<&AdditionalParam> = params
        .iter()
        .filter(|p| p.enabled && p.send_in == "queryparams")
        .collect();
    if query_params.is_empty() {
        return url.to_string();
    }
    let mut result = url.to_string();
    let separator = if result.contains('?') { '&' } else { '?' };
    for (i, p) in query_params.iter().enumerate() {
        if i == 0 {
            result.push(separator);
        } else {
            result.push('&');
        }
        result.push_str(&urlencoding::encode(&p.key));
        result.push('=');
        result.push_str(&urlencoding::encode(&p.value));
    }
    result
}

/// Appends enabled body-type additional params to a form data vec.
pub fn apply_params_to_body(form: &mut Vec<(String, String)>, params: &[AdditionalParam]) {
    for p in params.iter().filter(|p| p.enabled && p.send_in == "body") {
        form.push((p.key.clone(), p.value.clone()));
    }
}

/// Acquire an OAuth 2.0 token by posting to the token endpoint.
pub async fn acquire_token(
    config: &OAuthConfig,
    client: &reqwest::Client,
) -> DomainResult<OAuthToken> {
    let mut params = vec![
        ("grant_type", config.grant_type.as_str()),
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
    ];

    if let Some(scope) = &config.scope {
        params.push(("scope", scope.as_str()));
    }

    match config.grant_type.as_str() {
        "client_credentials" => {} // No extra params needed.
        "password" => {
            if let (Some(u), Some(p)) = (&config.username, &config.password) {
                params.push(("username", u.as_str()));
                params.push(("password", p.as_str()));
            }
        }
        "authorization_code" => {
            if let Some(code) = &config.code {
                params.push(("code", code.as_str()));
            }
            if let Some(uri) = &config.redirect_uri {
                params.push(("redirect_uri", uri.as_str()));
            }
            if let Some(verifier) = &config.code_verifier {
                params.push(("code_verifier", verifier.as_str()));
            }
        }
        other => {
            return Err(DomainError::InvalidInput(format!(
                "Unsupported grant type: {other}"
            )));
        }
    }

    let resp = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| DomainError::Internal(format!("OAuth token request failed: {e}")))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(DomainError::Internal(format!("OAuth token error: {body}")));
    }

    resp.json::<OAuthToken>()
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to parse OAuth token: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(expires_in: Option<u64>) -> OAuthToken {
        OAuthToken {
            access_token: "access_tok".into(),
            token_type: "Bearer".into(),
            expires_in,
            refresh_token: None,
            scope: None,
            id_token: None,
        }
    }

    #[test]
    fn test_token_not_expired() {
        let token = make_token(Some(3600));
        assert!(!token.is_expired(0, 100));
    }

    #[test]
    fn test_token_expired() {
        let token = make_token(Some(3600));
        assert!(token.is_expired(0, 3700));
    }

    #[test]
    fn test_token_no_expiry() {
        let token = make_token(None);
        assert!(!token.is_expired(0, 999_999));
    }

    #[test]
    fn test_oauth_token_deserialization() {
        let json = r#"{
            "access_token": "ya29.abc123",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "1//refresh",
            "scope": "openid email"
        }"#;
        let token: OAuthToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.access_token, "ya29.abc123");
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, Some(3600));
        assert_eq!(token.refresh_token.as_deref(), Some("1//refresh"));
        assert_eq!(token.scope.as_deref(), Some("openid email"));
    }

    #[test]
    fn apply_params_to_url_adds_query_params() {
        let params = vec![
            AdditionalParam {
                key: "nonce".into(),
                value: "abc123".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
            AdditionalParam {
                key: "audience".into(),
                value: "api/v1".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
        ];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert!(result.starts_with("https://auth.example.com/authorize?"));
        assert!(result.contains("nonce=abc123"));
        assert!(result.contains("audience=api%2Fv1"));
    }

    #[test]
    fn apply_params_to_url_appends_to_existing_query() {
        let params = vec![AdditionalParam {
            key: "nonce".into(),
            value: "xyz".into(),
            send_in: "queryparams".into(),
            enabled: true,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize?foo=bar", &params);
        assert!(result.contains("foo=bar&nonce=xyz"));
    }

    #[test]
    fn apply_params_to_url_skips_disabled() {
        let params = vec![AdditionalParam {
            key: "nonce".into(),
            value: "abc".into(),
            send_in: "queryparams".into(),
            enabled: false,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert_eq!(result, "https://auth.example.com/authorize");
    }

    #[test]
    fn apply_params_to_url_skips_body_type() {
        let params = vec![AdditionalParam {
            key: "secret".into(),
            value: "val".into(),
            send_in: "body".into(),
            enabled: true,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert_eq!(result, "https://auth.example.com/authorize");
    }

    #[test]
    fn apply_params_to_body_adds_body_params() {
        let params = vec![
            AdditionalParam {
                key: "audience".into(),
                value: "api/v1".into(),
                send_in: "body".into(),
                enabled: true,
            },
            AdditionalParam {
                key: "nonce".into(),
                value: "abc".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
        ];
        let mut form = vec![("grant_type".to_string(), "client_credentials".to_string())];
        apply_params_to_body(&mut form, &params);
        assert_eq!(form.len(), 2);
        assert_eq!(form[1], ("audience".to_string(), "api/v1".to_string()));
    }

    #[test]
    fn oauth_token_deserialization_with_id_token() {
        let json = r#"{
            "access_token": "ya29.abc",
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature"
        }"#;
        let token: OAuthToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.access_token, "ya29.abc");
        assert!(token.id_token.is_some());
        assert!(token.id_token.unwrap().starts_with("eyJ"));
    }

    #[test]
    fn oauth_token_deserialization_without_id_token() {
        let json = r#"{
            "access_token": "ya29.abc",
            "token_type": "Bearer"
        }"#;
        let token: OAuthToken = serde_json::from_str(json).unwrap();
        assert!(token.id_token.is_none());
    }
}
