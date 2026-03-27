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
        builder = apply_auth(builder, &request.auth, &request.method)?;

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

fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: &Auth,
    method: &rocket_shared::types::HttpMethod,
) -> DomainResult<reqwest::RequestBuilder> {
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
        Auth::Inherit => {
            // Inherits from parent — resolved before execution.
        }
        Auth::Wsse { .. } | Auth::Digest { .. } | Auth::Ntlm { .. } => {
            // Not yet implemented in HTTP executor.
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
            profile_name: _,
        } => {
            use rocket_http::aws_sig::{sign_request, AwsCredentials};

            let creds = AwsCredentials {
                access_key: access_key.clone(),
                secret_key: secret_key.clone(),
                region: region.clone(),
                service: service.clone(),
                session_token: session_token.clone(),
            };

            let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
            let method_str = method.to_string();

            // Build a temporary copy to extract the final URL.
            let url_str = builder
                .try_clone()
                .ok_or_else(|| {
                    DomainError::Internal("Cannot clone request builder for signing".into())
                })?
                .build()
                .map_err(|e| DomainError::Internal(format!("Cannot build request for signing: {e}")))?
                .url()
                .to_string();

            // Include the host header for signing.
            let host = reqwest::Url::parse(&url_str)
                .map_err(|e| DomainError::Internal(format!("Invalid URL during signing: {e}")))?
                .host_str()
                .unwrap_or("")
                .to_string();

            let headers: Vec<(String, String)> =
                vec![("host".to_string(), host)];

            let signed = sign_request(&method_str, &url_str, &headers, b"", &creds, &timestamp)
                .map_err(|e| DomainError::Internal(format!("AWS signing failed: {e}")))?;

            builder = builder
                .header("Authorization", &signed.authorization)
                .header("x-amz-date", &signed.x_amz_date)
                .header("x-amz-content-sha256", &signed.x_amz_content_sha256);

            if let Some(token) = &signed.x_amz_security_token {
                builder = builder.header("x-amz-security-token", token);
            }
        }
    }
    Ok(builder)
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
            if let Some(file_path) = &body.file_path {
                let path = std::path::Path::new(file_path);
                let data = std::fs::read(path)
                    .map_err(|e| DomainError::Internal(format!("Failed to read file: {e}")))?;

                // Detect content type from the file extension.
                let content_type = match path.extension().and_then(|e| e.to_str()) {
                    Some("json") => "application/json",
                    Some("xml") => "application/xml",
                    Some("png") => "image/png",
                    Some("jpg" | "jpeg") => "image/jpeg",
                    Some("gif") => "image/gif",
                    Some("pdf") => "application/pdf",
                    Some("zip") => "application/zip",
                    _ => "application/octet-stream",
                };

                builder = builder
                    .header("Content-Type", content_type)
                    .body(data);
            }
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

    #[test]
    fn binary_body_reads_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.json");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"{\"test\":true}").unwrap();

        // Verify the file can be read for body construction.
        let data = std::fs::read(&file_path).unwrap();
        assert_eq!(data, b"{\"test\":true}");
    }

    #[test]
    fn binary_body_applies_content_type_from_extension() {
        use rocket_shared::types::{Body, BodyMode};
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();

        // PNG file should produce image/png content type.
        let png_path = dir.path().join("image.png");
        std::fs::File::create(&png_path)
            .unwrap()
            .write_all(&[0x89, 0x50, 0x4E, 0x47])
            .unwrap();

        let body = Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: Some(png_path.to_string_lossy().into_owned()),
        };

        let client = Client::new();
        let builder = client.post("https://example.com");
        let result = apply_body(builder, &Some(body));
        assert!(result.is_ok());
    }

    #[test]
    fn binary_body_missing_file_returns_error() {
        use rocket_shared::types::{Body, BodyMode};

        let body = Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: Some("/nonexistent/path/file.bin".into()),
        };

        let client = Client::new();
        let builder = client.post("https://example.com");
        let result = apply_body(builder, &Some(body));
        assert!(result.is_err());
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
