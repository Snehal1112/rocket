use rocket_shared::error::{DomainError, DomainResult};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// Result of a successful OAuth2 callback.
pub struct CallbackResult {
    pub code: String,
    pub port: u16,
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><body style="font-family:system-ui;text-align:center;padding:60px">
<h2>Authorization Successful</h2>
<p>You can close this tab and return to Rocket.</p>
</body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html><body style="font-family:system-ui;text-align:center;padding:60px;color:#c00">
<h2>Authorization Failed</h2>
<p>Something went wrong. Please try again in Rocket.</p>
</body></html>"#;

/// Simple percent-decoding for ASCII values.
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

/// Parses query parameters from a raw URL path string.
fn parse_query(path: &str) -> HashMap<String, String> {
    path.split('?')
        .nth(1)
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((percent_decode(k), percent_decode(v)))
        })
        .collect()
}

/// Sends an HTTP response with the given HTML body and closes the connection.
async fn send_response(stream: &mut (impl AsyncWriteExt + Unpin), html: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

/// Waits for a single OAuth2 callback on a pre-bound TCP listener.
///
/// The listener should be bound to `127.0.0.1:0` (or a specific port).
/// The function waits up to `timeout_secs` for one incoming request,
/// verifies the `state` parameter, and returns the authorization `code`.
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout_secs: u64,
) -> DomainResult<CallbackResult> {
    let port = listener
        .local_addr()
        .map_err(|e| DomainError::Internal(format!("No local address: {e}")))?
        .port();

    let expected = expected_state.to_string();

    let result = timeout(Duration::from_secs(timeout_secs), async {
        // Loop to handle multiple connections. Browsers may send preflight
        // connections, favicon requests, or other requests before/after the
        // actual callback with the authorization code.
        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|e| DomainError::Internal(format!("Accept failed: {e}")))?;

            let mut buf = vec![0u8; 8192];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| DomainError::Internal(format!("Read failed: {e}")))?;

            if n == 0 {
                // Empty connection (preconnect). Skip and wait for next.
                continue;
            }

            let request = String::from_utf8_lossy(&buf[..n]);

            // Extract the path from the HTTP request line.
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("");

            let params = parse_query(path);

            // Skip requests that don't have a code or error (e.g., /favicon.ico).
            if !params.contains_key("code") && !params.contains_key("error") {
                let response = "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
                continue;
            }

            // Check for error from auth server.
            if let Some(error) = params.get("error") {
                let desc = params.get("error_description").unwrap_or(error);
                send_response(&mut stream, ERROR_HTML).await;
                return Err(DomainError::Internal(format!("Authorization denied: {desc}")));
            }

            // Verify state to prevent CSRF.
            let state = params.get("state").map(|s| s.as_str()).unwrap_or("");
            if state != expected {
                send_response(&mut stream, ERROR_HTML).await;
                return Err(DomainError::Internal(
                    "State mismatch — possible CSRF attack.".into(),
                ));
            }

            // Extract authorization code.
            let code = params
                .get("code")
                .ok_or_else(|| DomainError::Internal("No authorization code in callback.".into()))?
                .clone();

            send_response(&mut stream, SUCCESS_HTML).await;

            return Ok(CallbackResult { code, port });
        }
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(DomainError::Internal(
            "Authorization timed out. Please try again.".into(),
        )),
    }
}
