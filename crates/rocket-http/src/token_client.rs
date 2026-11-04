//! The port for sending OAuth2 token requests.

use crate::oauth2::OAuthToken;
use crate::resolved_certificate::ResolvedClientCertificate;
use async_trait::async_trait;
use rocket_shared::error::DomainResult;

/// Sends the form POST of a token request and parses the token response.
///
/// With a client certificate whose domain matches `token_url`, the implementation presents it for
/// mutual TLS. The implementation lives in `rocket-infra`, because loading a certificate can read
/// files and sending the request needs an HTTP client. A matching certificate that cannot be
/// loaded is an error, so the request is never sent without the certificate the endpoint may
/// require.
#[async_trait]
pub trait TokenClientProvider: Send + Sync {
    /// Posts `form` to `token_url` with `extra_headers` and parses the token response.
    async fn post_form(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
        form: &[(String, String)],
        extra_headers: &[(String, String)],
    ) -> DomainResult<OAuthToken>;
}
