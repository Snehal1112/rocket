//! The `ScriptHost` a request's scripts get. It serves `rok.sendRequest` and
//! `rok.runRequest`.
//!
//! It borrows the service, so a call back into the service needs no shared
//! handle and no second script engine.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rocket_http::{HttpRequest, HttpResponse, RequestOptions};
use rocket_scripting::{
    HostError, HostRequest, HostResponse, HostRunOutcome, HostRunRequest, HostScopes, ScriptHost,
};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};

use super::run_request::{
    check_run_chain, find_run_target, normalize_run_path, runtime_changes, RunTarget,
};
use super::{body_mode_from_content_type, ExecuteRequestInput, RequestExecutionService};
use crate::runner_sequence::build_step_input;

/// Serves the host calls of one script run.
pub(crate) struct ExecutionScriptHost<'a> {
    pub(crate) svc: &'a RequestExecutionService,
    /// The request the script belongs to. Nested runs take its collection and environments.
    pub(crate) input: &'a ExecuteRequestInput,
    /// RocketVault values of this run, passed on to nested runs.
    pub(crate) external_secrets: Arc<HashMap<String, String>>,
    /// TLS, redirect, cookie and client-certificate options of the request the
    /// script belongs to. Script requests reuse them.
    pub(crate) options: RequestOptions,
    /// Request paths of this run and the runs that started it, outermost first.
    pub(crate) chain: Vec<String>,
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

    async fn run_request(&self, request: HostRunRequest) -> Result<HostRunOutcome, HostError> {
        let target = normalize_run_path(&request.path);
        check_run_chain(&self.chain, &target).map_err(HostError::Failed)?;
        let invalid = || {
            HostError::Failed(format!(
                "rok.runRequest: invalid request path - {}",
                request.path
            ))
        };
        let collection = self.input.collection.as_deref().ok_or_else(invalid)?;
        let tree = self
            .svc
            .collection_repo
            .get(collection)
            .map_err(|_| invalid())?;
        let item = match find_run_target(&tree, &target) {
            RunTarget::Http(item) => item,
            RunTarget::Skipped => return Ok(HostRunOutcome::default()),
            RunTarget::NotFound => return Err(invalid()),
        };
        if let Some(message) = &item.prepare_error {
            return Err(HostError::Failed(format!("rok.runRequest: {message}")));
        }
        let nested = build_step_input(
            &item,
            collection,
            self.input.environment_name.as_deref(),
            self.input.global_env_name.as_deref(),
            self.input.request_guard_policy.clone(),
        );
        let mut chain = self.chain.clone();
        chain.push(target);
        // The scopes as stored now, so the engine can tell what the nested run changed.
        let before = self.stored_scopes(collection);
        // Boxed, because this future holds another run of the same pipeline.
        let (output, runtime) = Box::pin(self.svc.execute_nested(
            nested,
            &self.external_secrets,
            chain,
            &request.runtime_vars,
        ))
        .await
        .map_err(|e| HostError::Failed(format!("rok.runRequest: {e}")))?;
        // The nested run saved its writes, so the scopes are read back from storage.
        let scopes = self.stored_scopes(collection);
        let (runtime_set, runtime_removed) = runtime_changes(&request.runtime_vars, &runtime);
        Ok(HostRunOutcome {
            response: Some(host_response(&output.response)),
            runtime_set,
            runtime_removed,
            scopes: Some(scopes),
            scopes_before: Some(before),
        })
    }
}

impl ExecutionScriptHost<'_> {
    /// The env, global and collection scopes as they are stored right now.
    fn stored_scopes(&self, collection: &str) -> HostScopes {
        let scopes = self.svc.build_variable_scopes(
            self.input.global_env_name.as_deref(),
            Some(collection),
            self.input.environment_name.as_deref(),
            None,
            &self.external_secrets,
        );
        HostScopes {
            env: scopes.env,
            global_env: scopes.global_env,
            collection: scopes.collection,
            secret_values: scopes.secret_values.into_iter().collect(),
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
