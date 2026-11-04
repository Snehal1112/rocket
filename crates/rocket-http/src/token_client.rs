//! The interface for the HTTP client that sends OAuth2 token requests.

use crate::oauth2::OAuthToken;
use crate::resolved_certificate::ResolvedClientCertificate;
use async_trait::async_trait;
use rocket_shared::error::{DomainError, DomainResult};

/// Builds the client for a token request.
///
/// With a client certificate whose domain matches `token_url`, the client presents it for
/// mutual TLS. The implementation lives in `rocket-infra`, because loading a certificate can read
/// files. A matching certificate that cannot be loaded is an error, so the request is never
/// sent without the certificate the endpoint may require.
#[async_trait]
pub trait TokenClientProvider: Send + Sync {
    fn client_for(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
    ) -> DomainResult<reqwest::Client>;

    /// Posts `form` to `token_url` with `extra_headers` and parses the token response.
    ///
    /// The client comes from `client_for`, so a certificate that cannot be loaded stops the
    /// request before anything is sent.
    async fn post_form(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
        form: &[(String, String)],
        extra_headers: &[(String, String)],
    ) -> DomainResult<OAuthToken> {
        let client = self.client_for(token_url, verify_ssl, certificates)?;
        let mut request = client.post(token_url).form(form);
        for (key, value) in extra_headers {
            request = request.header(key, value);
        }

        let resp = request
            .send()
            .await
            .map_err(|e| DomainError::Internal(format!("Token request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(DomainError::Internal(format!(
                "Token endpoint returned {status}: {body}"
            )));
        }

        resp.json::<OAuthToken>()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to parse token response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;
    use wiremock::matchers::{body_string_contains, header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Hands out a plain client, or fails like a provider with a bad certificate.
    struct PlainProvider {
        fail: bool,
    }

    impl TokenClientProvider for PlainProvider {
        fn client_for(
            &self,
            _token_url: &str,
            _verify_ssl: bool,
            _certificates: &[ResolvedClientCertificate],
        ) -> DomainResult<reqwest::Client> {
            if self.fail {
                Err(DomainError::InvalidInput(
                    "certificate cannot be loaded".into(),
                ))
            } else {
                Ok(reqwest::Client::new())
            }
        }
    }

    fn form() -> Vec<(String, String)> {
        vec![("grant_type".into(), "client_credentials".into())]
    }

    async fn post(
        provider: &PlainProvider,
        server: &MockServer,
        headers: &[(String, String)],
    ) -> DomainResult<OAuthToken> {
        provider
            .post_form(&server.uri(), true, &[], &form(), headers)
            .await
    }

    #[tokio::test]
    async fn post_form_returns_the_parsed_token_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"access_token":"abc","token_type":"Bearer"}"#),
            )
            .mount(&server)
            .await;

        let token = post(&PlainProvider { fail: false }, &server, &[])
            .await
            .expect("token");

        assert_eq!(token.access_token, "abc");
    }

    #[tokio::test]
    async fn post_form_reports_a_non_2xx_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;

        let err = post(&PlainProvider { fail: false }, &server, &[])
            .await
            .expect_err("error");

        assert_eq!(
            err.to_string(),
            DomainError::Internal("Token endpoint returned 400 Bad Request: bad".into())
                .to_string()
        );
    }

    #[tokio::test]
    async fn post_form_reports_an_unparseable_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let err = post(&PlainProvider { fail: false }, &server, &[])
            .await
            .expect_err("error");

        assert!(err.to_string().contains("Failed to parse token response:"));
    }

    #[tokio::test]
    async fn post_form_sends_the_form_and_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Authorization", "Basic xyz"))
            .and(body_string_contains("grant_type=client_credentials"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"access_token":"abc","token_type":"Bearer"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;
        let headers = vec![("Authorization".to_string(), "Basic xyz".to_string())];

        post(&PlainProvider { fail: false }, &server, &headers)
            .await
            .expect("token");
    }

    #[tokio::test]
    async fn post_form_stops_when_client_for_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let err = post(&PlainProvider { fail: true }, &server, &[])
            .await
            .expect_err("error");

        assert!(err.to_string().contains("certificate cannot be loaded"));
    }
}
