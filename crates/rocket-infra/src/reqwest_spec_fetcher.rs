//! Downloads an OpenAPI spec with reqwest: https only, manual redirects, a size cap, and a
//! token that never leaves the origin it was meant for.

use crate::reqwest_executor::{apply_proxy, read_proxy};
use async_trait::async_trait;
use reqwest::header::{
    HeaderName, ACCEPT, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION,
    RETRY_AFTER, USER_AGENT,
};
use rocket_http::{
    redirect_target_allowed, same_origin, SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest,
    SpecFetcher, MAX_SPEC_REDIRECTS,
};
use std::time::Duration;

/// How long one request may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Fetches specs through the app proxy setting. A client is built per call so a saved proxy
/// change takes effect on the next fetch.
pub struct ReqwestSpecFetcher {
    proxy: Option<rocket_http::SharedProxy>,
    /// When false (tests only), plain `http` and local addresses are allowed.
    strict: bool,
}

impl Default for ReqwestSpecFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestSpecFetcher {
    pub fn new() -> Self {
        Self {
            proxy: None,
            strict: true,
        }
    }

    /// Reads the proxy from `shared`, which the proxy service updates on save.
    pub fn with_proxy(mut self, shared: rocket_http::SharedProxy) -> Self {
        self.proxy = Some(shared);
        self
    }

    /// Lets `wiremock` (plain `http` on 127.0.0.1) be used in tests.
    #[cfg(test)]
    fn allow_local_http(mut self) -> Self {
        self.strict = false;
        self
    }
}

impl ReqwestSpecFetcher {
    /// A client that never follows redirects itself, so every hop is checked here.
    fn client(&self) -> Result<reqwest::Client, SpecFetchFailure> {
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(FETCH_TIMEOUT);
        let proxy = read_proxy(self.proxy.as_ref());
        let builder =
            apply_proxy(builder, &proxy).map_err(|e| SpecFetchFailure::Network(e.to_string()))?;
        builder
            .build()
            .map_err(|e| SpecFetchFailure::Network(e.without_url().to_string()))
    }
}

#[async_trait]
impl SpecFetcher for ReqwestSpecFetcher {
    async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        let first = reqwest::Url::parse(&request.url)
            .map_err(|_| SpecFetchFailure::Network("the link is not a valid URL".to_string()))?;
        if self.strict && first.scheme() != "https" {
            return Err(SpecFetchFailure::NotHttps);
        }
        let client = self.client()?;
        let mut current = first.clone();
        for hop in 0..=MAX_SPEC_REDIRECTS {
            let mut call = client.get(current.clone()).header(USER_AGENT, "RocketAPI");
            if let Some(accept) = &request.accept {
                call = call.header(ACCEPT, accept);
            }
            // The token goes only to the origin it was meant for, never after a redirect away.
            if let Some(token) = &request.bearer_token {
                if same_origin(first.as_str(), current.as_str()) {
                    call = call.bearer_auth(token);
                }
            }
            if let Some(etag) = &request.if_none_match {
                call = call.header(IF_NONE_MATCH, etag);
            }
            if let Some(since) = &request.if_modified_since {
                call = call.header(IF_MODIFIED_SINCE, since);
            }
            let response = call
                .send()
                .await
                .map_err(|e| SpecFetchFailure::Network(e.without_url().to_string()))?;
            let status = response.status();
            if status.is_redirection() && status != reqwest::StatusCode::NOT_MODIFIED {
                let Some(location) = header_text(&response, LOCATION) else {
                    return Err(SpecFetchFailure::Status(status.as_u16()));
                };
                if hop == MAX_SPEC_REDIRECTS {
                    return Err(SpecFetchFailure::TooManyRedirects);
                }
                let next = current.join(&location).map_err(|_| {
                    SpecFetchFailure::Network("the redirect address is not valid".to_string())
                })?;
                if self.strict && !redirect_target_allowed(next.as_str()) {
                    return Err(SpecFetchFailure::BlockedRedirect);
                }
                current = next;
                continue;
            }
            return finish(response, request.max_bytes, current.as_str()).await;
        }
        Err(SpecFetchFailure::TooManyRedirects)
    }
}

fn header_text(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Maps the final response to an outcome, reading at most `max_bytes` of a successful body.
async fn finish(
    mut response: reqwest::Response,
    max_bytes: usize,
    final_url: &str,
) -> Result<SpecFetchOutcome, SpecFetchFailure> {
    let code = response.status().as_u16();
    match code {
        304 => Ok(SpecFetchOutcome::NotModified),
        200..=299 => {
            let etag = header_text(&response, ETAG);
            let last_modified = header_text(&response, LAST_MODIFIED);
            if response
                .content_length()
                .is_some_and(|n| n > max_bytes as u64)
            {
                return Err(SpecFetchFailure::TooLarge);
            }
            let mut body: Vec<u8> = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| SpecFetchFailure::Network(e.without_url().to_string()))?
            {
                if body.len() + chunk.len() > max_bytes {
                    return Err(SpecFetchFailure::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(SpecFetchOutcome::Fetched {
                body,
                etag,
                last_modified,
                final_url: final_url.to_string(),
            })
        }
        401 => Err(SpecFetchFailure::Unauthorized),
        404 => Err(SpecFetchFailure::NotFound),
        429 => Err(rate_limited(&response)),
        403 => {
            let exhausted =
                header_text(&response, HeaderName::from_static("x-ratelimit-remaining"))
                    .is_some_and(|v| v.trim() == "0");
            if exhausted || response.headers().contains_key(RETRY_AFTER) {
                Err(rate_limited(&response))
            } else {
                Err(SpecFetchFailure::Unauthorized)
            }
        }
        other => Err(SpecFetchFailure::Status(other)),
    }
}

/// The wait implied by the `x-ratelimit-reset` header (epoch seconds), held to the same
/// bounds as a `Retry-After` value. `None` when the value is not a number.
fn reset_wait_secs(reset: &str, now_epoch_secs: u64) -> Option<u64> {
    let reset: u64 = reset.trim().parse().ok()?;
    Some(rocket_collection::openapi_schedule::clamp_retry_after(
        reset.saturating_sub(now_epoch_secs),
    ))
}

fn rate_limited(response: &reqwest::Response) -> SpecFetchFailure {
    let retry_after = header_text(response, RETRY_AFTER).and_then(|v| v.trim().parse().ok());
    let from_reset = || {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        reset_wait_secs(
            &header_text(response, HeaderName::from_static("x-ratelimit-reset"))?,
            now,
        )
    };
    SpecFetchFailure::RateLimited {
        retry_after_secs: retry_after.or_else(from_reset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, header_regex, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request(url: String) -> SpecFetchRequest {
        SpecFetchRequest {
            url,
            accept: None,
            bearer_token: None,
            if_none_match: None,
            if_modified_since: None,
            max_bytes: 1024 * 1024,
        }
    }

    fn fetcher() -> ReqwestSpecFetcher {
        ReqwestSpecFetcher::new().allow_local_http()
    }

    #[tokio::test]
    async fn fetches_body_and_validators() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spec"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("openapi: 3.0.3")
                    .insert_header("etag", "\"abc\"")
                    .insert_header("last-modified", "Wed, 21 Oct 2026 07:28:00 GMT"),
            )
            .mount(&server)
            .await;
        let url = format!("{}/spec", server.uri());
        let out = fetcher()
            .fetch(request(url.clone()))
            .await
            .expect("fetched");
        assert_eq!(
            out,
            SpecFetchOutcome::Fetched {
                body: b"openapi: 3.0.3".to_vec(),
                etag: Some("\"abc\"".to_string()),
                last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
                final_url: url,
            }
        );
    }

    #[tokio::test]
    async fn sends_conditional_headers_and_maps_304() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/spec"))
            .and(header("if-none-match", "\"abc\""))
            // `header` splits an expected value at commas, so match the date with a regex.
            .and(header_regex(
                "if-modified-since",
                "^Wed, 21 Oct 2026 07:28:00 GMT$",
            ))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;
        let mut req = request(format!("{}/spec", server.uri()));
        req.if_none_match = Some("\"abc\"".to_string());
        req.if_modified_since = Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string());
        assert_eq!(
            fetcher().fetch(req).await.expect("not modified"),
            SpecFetchOutcome::NotModified
        );
    }

    #[tokio::test]
    async fn maps_error_statuses() {
        let server = MockServer::start().await;
        for (route, template) in [
            ("/nf", ResponseTemplate::new(404)),
            ("/un", ResponseTemplate::new(401)),
            (
                "/rl",
                ResponseTemplate::new(429).insert_header("retry-after", "120"),
            ),
            (
                "/rl403",
                ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "0"),
            ),
            ("/fb", ResponseTemplate::new(403)),
            ("/err", ResponseTemplate::new(500)),
        ] {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(template)
                .mount(&server)
                .await;
        }
        let get = |route: &str| {
            let url = format!("{}{route}", server.uri());
            async move {
                fetcher()
                    .fetch(request(url))
                    .await
                    .expect_err("should fail")
            }
        };
        assert_eq!(get("/nf").await, SpecFetchFailure::NotFound);
        assert_eq!(get("/un").await, SpecFetchFailure::Unauthorized);
        assert_eq!(
            get("/rl").await,
            SpecFetchFailure::RateLimited {
                retry_after_secs: Some(120)
            }
        );
        assert_eq!(
            get("/rl403").await,
            SpecFetchFailure::RateLimited {
                retry_after_secs: None
            }
        );
        assert_eq!(get("/fb").await, SpecFetchFailure::Unauthorized);
        assert_eq!(get("/err").await, SpecFetchFailure::Status(500));
    }

    #[test]
    fn a_reset_time_becomes_a_clamped_wait() {
        assert_eq!(reset_wait_secs("1100", 1000), Some(100));
        // A reset in the past or very near still waits one minute.
        assert_eq!(reset_wait_secs("900", 1000), Some(60));
        assert_eq!(reset_wait_secs("1010", 1000), Some(60));
        // A far reset is held to 24 hours.
        assert_eq!(reset_wait_secs("9999999", 1000), Some(86_400));
        assert_eq!(reset_wait_secs("soon", 1000), None);
    }

    #[tokio::test]
    async fn a_github_primary_limit_uses_the_reset_header() {
        let server = MockServer::start().await;
        let reset = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + 600;
        Mock::given(method("GET"))
            .and(path("/gh"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", reset.to_string().as_str()),
            )
            .mount(&server)
            .await;
        let err = fetcher()
            .fetch(request(format!("{}/gh", server.uri())))
            .await
            .expect_err("limited");
        match err {
            SpecFetchFailure::RateLimited {
                retry_after_secs: Some(secs),
            } => assert!((590..=600).contains(&secs), "{secs}"),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn stops_at_the_size_cap_with_and_without_content_length() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'a'; 2048]))
            .mount(&server)
            .await;
        let mut req = request(format!("{}/big", server.uri()));
        req.max_bytes = 1024;
        assert_eq!(
            fetcher().fetch(req).await.expect_err("too large"),
            SpecFetchFailure::TooLarge
        );

        // A chunked response has no Content-Length, so the cap is enforced while reading.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await;
            for _ in 0..3 {
                let _ = sock.write_all(b"320\r\n").await;
                let _ = sock.write_all(&[b'a'; 800]).await;
                let _ = sock.write_all(b"\r\n").await;
            }
            let _ = sock.write_all(b"0\r\n\r\n").await;
        });
        let mut req = request(format!("http://{addr}/big"));
        req.max_bytes = 1024;
        assert_eq!(
            fetcher().fetch(req).await.expect_err("too large"),
            SpecFetchFailure::TooLarge
        );
    }

    #[tokio::test]
    async fn follows_redirects_and_gives_up_after_five() {
        let server = MockServer::start().await;
        for (from, to) in [
            ("/r1", "/r2"),
            ("/r2", "/r3"),
            ("/r3", "/final"),
            ("/loop", "/loop"),
        ] {
            Mock::given(method("GET"))
                .and(path(from))
                .respond_with(ResponseTemplate::new(302).insert_header("location", to))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_string("done"))
            .mount(&server)
            .await;
        let out = fetcher()
            .fetch(request(format!("{}/r1", server.uri())))
            .await
            .expect("fetched after redirects");
        assert!(matches!(
            out,
            SpecFetchOutcome::Fetched { ref body, ref final_url, .. }
                if body == b"done" && final_url.ends_with("/final")
        ));
        assert_eq!(
            fetcher()
                .fetch(request(format!("{}/loop", server.uri())))
                .await
                .expect_err("loop"),
            SpecFetchFailure::TooManyRedirects
        );
    }

    #[tokio::test]
    async fn token_is_not_sent_to_another_origin() {
        let first = MockServer::start().await;
        let second = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/target", second.uri())),
            )
            .mount(&first)
            .await;
        Mock::given(method("GET"))
            .and(path("/target"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&second)
            .await;
        let mut req = request(format!("{}/start", first.uri()));
        req.bearer_token = Some("s3cret".to_string());
        fetcher().fetch(req).await.expect("fetched");

        let to_first = first.received_requests().await.expect("recorded");
        assert_eq!(
            to_first[0]
                .headers
                .get("authorization")
                .map(|v| v.to_str().unwrap_or_default()),
            Some("Bearer s3cret")
        );
        let to_second = second.received_requests().await.expect("recorded");
        assert_eq!(to_second.len(), 1);
        assert!(to_second[0].headers.get("authorization").is_none());
    }

    #[tokio::test]
    async fn rejects_http_without_the_test_switch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let out = ReqwestSpecFetcher::new()
            .fetch(request(format!("{}/x", server.uri())))
            .await
            .expect_err("http is refused");
        assert_eq!(out, SpecFetchFailure::NotHttps);
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }
}
