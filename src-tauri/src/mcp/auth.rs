//! Bearer-token authentication for the per-session MCP HTTP server.
//!
//! Chosen over a hand-rolled `tower::Layer`: `axum::middleware::from_fn_with_state`
//! is the documented, idiomatic way to run a stateful async check in front of
//! an axum router without writing a `Service`/`Layer` pair by hand, and every
//! request here needs exactly one thing checked (the `Authorization` header
//! against this session's token) — a full custom `Layer` would just
//! re-implement `from_fn_with_state`'s plumbing for no extra benefit.
//!
//! The token comparison uses `subtle::ConstantTimeEq` rather than `==`, so a
//! byte-by-byte mismatch does not return early and leak timing information
//! about how many leading bytes of a guessed token were correct. `subtle` is
//! already present in this workspace's dependency graph (pulled in
//! transitively by `sha2`/`hmac`), so this adds no new supply-chain surface —
//! and hand-rolling constant-time comparison is exactly the kind of code a
//! well-audited, purpose-built crate should be preferred over, since it is
//! easy to defeat by accident (an early `return false`, or the compiler
//! optimizing a naive XOR-and-check loop into a short-circuiting comparison).

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

/// Rejects any request whose `Authorization: Bearer <token>` header does not
/// match `expected_token` in constant time. A missing header, wrong scheme,
/// and a mismatched token are all treated identically (401), so a caller
/// cannot distinguish "no token supplied" from "wrong token" through timing
/// or response shape.
pub async fn require_bearer_token(
    State(expected_token): State<String>,
    req: Request,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let authorized = match provided {
        Some(token) => tokens_match(token.as_bytes(), expected_token.as_bytes()),
        None => false,
    };

    if authorized {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "invalid or missing bearer token").into_response()
    }
}

/// Constant-time byte comparison. The length check runs first — this
/// compares *lengths*, not *contents*, and the token's length (a fixed-size
/// UUID string) is not a secret, so this does not reintroduce the timing
/// side channel the constant-time comparison exists to close.
fn tokens_match(provided: &[u8], expected: &[u8]) -> bool {
    provided.len() == expected.len() && bool::from(provided.ct_eq(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_router(token: &str) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                token.to_string(),
                require_bearer_token,
            ))
    }

    #[tokio::test]
    async fn correct_bearer_token_is_allowed() {
        let request = Request::builder()
            .uri("/ping")
            .header(header::AUTHORIZATION, "Bearer secret-token")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_token_is_rejected() {
        let request = Request::builder()
            .uri("/ping")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn incorrect_token_is_rejected() {
        let request = Request::builder()
            .uri("/ping")
            .header(header::AUTHORIZATION, "Bearer wrong-token")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_expected_token_still_rejects_a_request_with_no_header() {
        // Defends against a future refactor that treats an empty expected
        // token as "auth disabled" -- it must not.
        let request = Request::builder()
            .uri("/ping")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("").oneshot(request).await.expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
