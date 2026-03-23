use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{redirect, Client, Method};

use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{ApiKeyLocation, Auth, Body, BodyMode, Header};

pub struct ReqwestExecutor;

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReqwestExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpExecutor for ReqwestExecutor {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        let client = build_client(request)?;
        let method = map_method(&request.method);
        let start = Instant::now();

        // Merge enabled query params into the URL.
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|e| DomainError::InvalidInput(format!("Invalid URL: {e}")))?;
        {
            let mut pairs = url.query_pairs_mut();
            for p in &request.query_params {
                if p.enabled {
                    pairs.append_pair(&p.key, &p.value);
                }
            }
        }

        let mut builder = client.request(method, url);

        // Add enabled headers.
        for header in request.headers.iter().filter(|h| h.enabled) {
            builder = builder.header(&header.key, &header.value);
        }

        // Apply authentication.
        builder = apply_auth(builder, &request.auth);

        // Apply request body.
        builder = apply_body(builder, &request.body)?;

        // Per-request timeout overrides the client-level default.
        builder = builder.timeout(Duration::from_millis(request.options.timeout_ms));

        let response = builder
            .send()
            .await
            .map_err(|e| DomainError::Http(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();

        let headers: Vec<Header> = response
            .headers()
            .iter()
            .map(|(k, v)| Header::new(k.as_str(), v.to_str().unwrap_or("")))
            .collect();

        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| DomainError::Http(e.to_string()))?;

        let size_bytes = body_bytes.len();
        let body = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body,
            duration_ms,
            size_bytes,
        })
    }
}

fn build_client(request: &HttpRequest) -> DomainResult<Client> {
    let redirect_policy = if request.options.follow_redirects {
        redirect::Policy::limited(10)
    } else {
        redirect::Policy::none()
    };

    Client::builder()
        .redirect(redirect_policy)
        .danger_accept_invalid_certs(!request.options.verify_ssl)
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
}

fn map_method(method: &rocket_shared::types::HttpMethod) -> Method {
    use rocket_shared::types::HttpMethod::*;
    match method {
        Get => Method::GET,
        Post => Method::POST,
        Put => Method::PUT,
        Patch => Method::PATCH,
        Delete => Method::DELETE,
        Options => Method::OPTIONS,
        Head => Method::HEAD,
    }
}

fn apply_auth(mut builder: reqwest::RequestBuilder, auth: &Auth) -> reqwest::RequestBuilder {
    match auth {
        Auth::None => {}
        Auth::Basic { username, password } => {
            builder = builder.basic_auth(username, Some(password));
        }
        Auth::Bearer { token } => {
            builder = builder.bearer_auth(token);
        }
        Auth::ApiKey { key, value, add_to } => match add_to {
            ApiKeyLocation::Header => {
                builder = builder.header(key.as_str(), value.as_str());
            }
            ApiKeyLocation::Query => {
                builder = builder.query(&[(key.as_str(), value.as_str())]);
            }
        },
        Auth::OAuth2 {
            access_token: Some(token),
            ..
        } => {
            builder = builder.bearer_auth(token);
        }
        Auth::OAuth2 { .. } => {
            // No access token available yet; skip auth header.
        }
        Auth::AwsSigV4 { .. } => {
            // AWS SigV4 signing is not yet implemented.
        }
    }
    builder
}

fn apply_body(
    mut builder: reqwest::RequestBuilder,
    body: &Option<Body>,
) -> DomainResult<reqwest::RequestBuilder> {
    let Some(body) = body else {
        return Ok(builder);
    };

    match &body.mode {
        BodyMode::None => {}
        BodyMode::Json => {
            let content = body.content.as_deref().unwrap_or("");
            builder = builder
                .header("Content-Type", "application/json")
                .body(content.to_string());
        }
        BodyMode::Xml => {
            let content = body.content.as_deref().unwrap_or("");
            builder = builder
                .header("Content-Type", "text/xml")
                .body(content.to_string());
        }
        BodyMode::Text => {
            let content = body.content.as_deref().unwrap_or("");
            builder = builder
                .header("Content-Type", "text/plain")
                .body(content.to_string());
        }
        BodyMode::Binary => {
            let content = body.content.as_deref().unwrap_or("");
            builder = builder.body(content.to_string());
        }
        BodyMode::FormData => {
            if let Some(entries) = &body.form_data {
                let params: Vec<(&str, &str)> = entries
                    .iter()
                    .filter(|e| e.enabled)
                    .map(|e| (e.key.as_str(), e.value.as_str()))
                    .collect();
                builder = builder.form(&params);
            }
        }
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::HttpRequest;
    use rocket_shared::types::HttpMethod;

    #[test]
    fn maps_all_http_methods() {
        assert_eq!(map_method(&HttpMethod::Get), Method::GET);
        assert_eq!(map_method(&HttpMethod::Post), Method::POST);
        assert_eq!(map_method(&HttpMethod::Put), Method::PUT);
        assert_eq!(map_method(&HttpMethod::Patch), Method::PATCH);
        assert_eq!(map_method(&HttpMethod::Delete), Method::DELETE);
        assert_eq!(map_method(&HttpMethod::Options), Method::OPTIONS);
        assert_eq!(map_method(&HttpMethod::Head), Method::HEAD);
    }

    #[test]
    fn build_client_respects_ssl_option() {
        let mut req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        req.options.verify_ssl = false;
        // Should not error when building a client that accepts invalid certs.
        assert!(build_client(&req).is_ok());
    }

    #[test]
    fn apply_body_none_leaves_builder_unchanged() {
        let req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let client = Client::new();
        let builder = client.get("https://example.com");
        let result = apply_body(builder, &req.body);
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn execute_real_get_request() {
        let executor = ReqwestExecutor::new();
        let req = HttpRequest::new(HttpMethod::Get, "https://httpbin.org/get");
        let response = executor.execute(&req).await.unwrap();
        assert!(response.is_success());
        assert_eq!(response.status, 200);
    }
}
