//! The interface for the HTTP client that sends OAuth2 token requests.

use crate::resolved_certificate::ResolvedClientCertificate;
use rocket_shared::error::DomainResult;

/// Builds the client for a token request.
///
/// With a client certificate whose domain matches `token_url`, the client presents it for
/// mutual TLS. The implementation lives in `rocket-infra`, because loading a certificate can read
/// files. A matching certificate that cannot be loaded is an error, so the request is never
/// sent without the certificate the endpoint may require.
pub trait TokenClientProvider: Send + Sync {
    fn client_for(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
    ) -> DomainResult<reqwest::Client>;
}
