//! The `ScriptHost` a request's scripts get. It serves `rok.sendRequest`.
//!
//! It borrows the service, so a call back into the service needs no shared
//! handle and no second script engine.

use std::time::Duration;

use async_trait::async_trait;
use rocket_http::{HttpRequest, HttpResponse, RequestOptions};
use rocket_scripting::{HostError, HostRequest, HostResponse, ScriptHost};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};

use super::{body_mode_from_content_type, RequestExecutionService};

/// Serves the host calls of one script run.
pub(crate) struct ExecutionScriptHost<'a> {
    pub(crate) svc: &'a RequestExecutionService,
    /// TLS, redirect, cookie and client-certificate options of the request the
    /// script belongs to. Script requests reuse them.
    pub(crate) options: RequestOptions,
}

/// Turns an executor response into what a script sees.
pub(crate) fn host_response(response: &HttpResponse) -> HostResponse {
    HostResponse {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response
            .headers
            .iter()
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect(),
        body: response.body.clone(),
        response_time_ms: response.duration_ms,
    }
}

/// Builds the executor request for a `rok.sendRequest` call.
///
/// Variables are not resolved, as in Bruno. The host enforces the script's
/// time limit, so the executor's own limit is one second later as a backstop.
pub(crate) fn http_request_for(
    request: &HostRequest,
    options: &RequestOptions,
) -> Result<HttpRequest, HostError> {
    let method: HttpMethod = request.method.parse().map_err(|_| {
        HostError::Failed(format!("rok.sendRequest: invalid method - {}", request.method))
    })?;
    let mut http = HttpRequest::new(method, request.url.clone());
    http.headers = request
        .headers
        .iter()
        .map(|(key, value)| Header::new(key.clone(), value.clone()))
        .collect();
    if let Some(content) = &request.body {
        let has_content_type = http
            .headers
            .iter()
            .any(|h| h.key.eq_ignore_ascii_case("content-type"));
        let mode = if request.body_is_json {
            BodyMode::Json
        } else if has_content_type {
            body_mode_from_content_type(&http.headers)
        } else {
            BodyMode::Text
        };
        http.body = Some(Body {
            mode,
            content: Some(content.clone()),
            form_data: None,
            file_path: None,
        });
    }
    http.options = options.clone();
    http.options.timeout_ms = request.timeout_ms.saturating_add(1_000);
    Ok(http)
}

#[async_trait]
impl ScriptHost for ExecutionScriptHost<'_> {
    async fn send_request(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        let http = http_request_for(&request, &self.options)?;
        // A RocketVault certificate selected for this URL is fetched first, as for a send.
        let http = self.svc.with_vault_certificates(&http).await;
        let limit = Duration::from_millis(request.timeout_ms);
        match tokio::time::timeout(limit, self.svc.executor.execute(&http)).await {
            Err(_) => Err(HostError::Failed(format!(
                "rok.sendRequest: timed out after {} ms",
                request.timeout_ms
            ))),
            Ok(Err(e)) => Err(HostError::Failed(format!("rok.sendRequest: {e}"))),
            Ok(Ok(response)) => Ok(host_response(&response)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{CertificateSource, ResolvedClientCertificate};

    fn host_request(method: &str, body: Option<&str>, body_is_json: bool) -> HostRequest {
        HostRequest {
            method: method.into(),
            url: "https://side.test/x".into(),
            headers: vec![],
            body: body.map(str::to_string),
            body_is_json,
            timeout_ms: 1500,
        }
    }

    #[test]
    fn http_request_for_copies_the_calling_request_options() {
        let mut options = RequestOptions::default();
        options.verify_ssl = false;
        options.follow_redirects = false;
        options.max_redirects = Some(2);
        options.client_certificates = vec![ResolvedClientCertificate::pem(
            "side.test",
            CertificateSource::File("/certs/c.pem".into()),
            CertificateSource::File("/certs/k.pem".into()),
            None,
        )];
        let http = http_request_for(&host_request("PUT", None, false), &options).expect("valid");
        assert_eq!(http.method, HttpMethod::Put);
        assert!(!http.options.verify_ssl);
        assert!(!http.options.follow_redirects);
        assert_eq!(http.options.max_redirects, Some(2));
        assert_eq!(http.options.client_certificates.len(), 1);
        assert_eq!(http.options.client_certificates[0].domain, "side.test");
        assert_eq!(http.options.timeout_ms, 2_500);
        assert!(http.body.is_none());
    }

    #[test]
    fn http_request_for_picks_the_body_mode() {
        let options = RequestOptions::default();
        let json = http_request_for(&host_request("POST", Some("{}"), true), &options).expect("valid");
        assert_eq!(json.body.expect("body").mode, BodyMode::Json);
        let text = http_request_for(&host_request("POST", Some("hi"), false), &options).expect("valid");
        assert_eq!(text.body.expect("body").mode, BodyMode::Text);
        let mut xml = host_request("POST", Some("<a/>"), false);
        xml.headers = vec![("Content-Type".into(), "application/xml".into())];
        let xml = http_request_for(&xml, &options).expect("valid");
        assert_eq!(xml.body.expect("body").mode, BodyMode::Xml);
    }

    #[test]
    fn http_request_for_rejects_an_invalid_method() {
        let err = http_request_for(&host_request("NOT A METHOD", None, false), &RequestOptions::default())
            .expect_err("invalid");
        assert_eq!(
            err,
            HostError::Failed("rok.sendRequest: invalid method - NOT A METHOD".into())
        );
    }
}
