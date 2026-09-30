//! Builds the masked record of a request a Flow step sent.

use crate::execution_service::sensitive_auth_label;
use crate::redaction::{is_sensitive_header, redact_secrets, redact_url_secrets, REDACTED};
use rocket_http::{HttpRequest, HttpResponse};
use rocket_shared::events::{FlowDebugHeader, FlowDebugRequest, FlowDebugResponse};
use rocket_shared::types::{Auth, Body, BodyMode, Header};
use std::collections::HashSet;

/// Builds a debug record with every secret and credential masked.
pub(crate) fn build_debug_request(
    sent: &HttpRequest,
    response: Option<&HttpResponse>,
    error: Option<&str>,
    secret_values: &HashSet<String>,
) -> FlowDebugRequest {
    let mut headers = mask_headers(&sent.headers, secret_values);
    if let Some(line) = auth_line(&sent.auth) {
        headers.push(line);
    }
    FlowDebugRequest {
        method: sent.method.to_string(),
        url: redact_url_secrets(&full_url(sent), secret_values),
        headers,
        body: sent.body.as_ref().and_then(|b| body_text(b, secret_values)),
        response: response.map(|r| FlowDebugResponse {
            status: r.status,
            status_text: r.status_text.clone(),
            duration_ms: r.duration_ms,
            size_bytes: r.size_bytes as u64,
            headers: mask_headers(&r.headers, secret_values),
            body: redact_secrets(&r.body, secret_values),
            truncated: false,
        }),
        error: error.map(|e| redact_url_secrets(e, secret_values)),
    }
}

/// The URL with enabled query params appended, as the executor sends it.
fn full_url(sent: &HttpRequest) -> String {
    let enabled: Vec<_> = sent.query_params.iter().filter(|p| p.enabled).collect();
    match url::Url::parse(&sent.url) {
        Ok(mut url) => {
            // An empty append would leave a trailing '?'.
            if !enabled.is_empty() {
                let mut pairs = url.query_pairs_mut();
                for p in enabled {
                    pairs.append_pair(&p.key, &p.value);
                }
            }
            url.to_string()
        }
        Err(_) => sent.url.clone(),
    }
}

fn mask_headers(headers: &[Header], secret_values: &HashSet<String>) -> Vec<FlowDebugHeader> {
    headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| FlowDebugHeader {
            key: h.key.clone(),
            value: if is_sensitive_header(&h.key) {
                REDACTED.to_string()
            } else {
                redact_secrets(&h.value, secret_values)
            },
        })
        .collect()
}

/// One masked line describing the auth in use. The value is never shown.
fn auth_line(auth: &Auth) -> Option<FlowDebugHeader> {
    let (key, value) = match auth {
        Auth::None | Auth::Inherit => return None,
        Auth::Basic { .. } => ("Authorization".to_string(), format!("Basic {REDACTED}")),
        Auth::Bearer { .. } => ("Authorization".to_string(), format!("Bearer {REDACTED}")),
        Auth::ApiKey { key, placement, .. } => match placement.as_str() {
            "header" => (key.clone(), REDACTED.to_string()),
            "query" => ("Auth".to_string(), format!("API key in query \"{key}\"")),
            // The executor sends nothing for an unknown placement.
            _ => return None,
        },
        other => (
            "Auth".to_string(),
            sensitive_auth_label(other).unwrap_or("unknown").to_string(),
        ),
    };
    Some(FlowDebugHeader { key, value })
}

fn body_text(body: &Body, secret_values: &HashSet<String>) -> Option<String> {
    match body.mode {
        BodyMode::FormUrlEncoded | BodyMode::FormData => {
            let lines: Vec<String> = body
                .form_data
                .as_ref()?
                .iter()
                .filter(|e| e.enabled)
                .map(|e| format!("{}={}", e.key, redact_secrets(&e.value, secret_values)))
                .collect();
            if lines.is_empty() {
                None
            } else {
                Some(lines.join("\n"))
            }
        }
        _ => body
            .content
            .as_ref()
            .map(|c| redact_secrets(c, secret_values)),
    }
}
// Used by the step recording in Task 2.
/// The largest response body an exchange record keeps, in bytes.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const EXCHANGE_BODY_LIMIT: usize = 262_144;

/// Cuts the response body to `EXCHANGE_BODY_LIMIT` bytes at a UTF-8
/// boundary and marks the record as truncated.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn cap_exchange(mut record: FlowDebugRequest) -> FlowDebugRequest {
    if let Some(response) = record.response.as_mut() {
        if response.body.len() > EXCHANGE_BODY_LIMIT {
            let mut cut = EXCHANGE_BODY_LIMIT;
            while !response.body.is_char_boundary(cut) {
                cut -= 1;
            }
            response.body.truncate(cut);
            response.truncated = true;
        }
    }
    record
}

/// The record of an accepted callback. The call itself is the response,
/// so a reader sees what arrived; the request side holds its method and
/// path.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn callback_exchange(
    call: &crate::callback_listener::ReceivedCall,
    duration_ms: u64,
    secret_values: &HashSet<String>,
) -> FlowDebugRequest {
    let query: Vec<String> = call.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let url = if query.is_empty() {
        call.path.clone()
    } else {
        format!("{}?{}", call.path, query.join("&"))
    };
    let headers = call
        .headers
        .iter()
        .map(|(key, value)| FlowDebugHeader {
            key: key.clone(),
            value: if is_sensitive_header(key) {
                REDACTED.to_string()
            } else {
                redact_secrets(value, secret_values)
            },
        })
        .collect();
    cap_exchange(FlowDebugRequest {
        method: call.method.clone(),
        url: redact_url_secrets(&url, secret_values),
        headers: Vec::new(),
        body: None,
        response: Some(FlowDebugResponse {
            status: 200,
            status_text: call.method.clone(),
            duration_ms,
            size_bytes: call.body.len() as u64,
            headers,
            body: redact_secrets(&call.body, secret_values),
            truncated: false,
        }),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::{HttpMethod, QueryParam};

    fn secrets(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn header(key: &str, value: &str, enabled: bool) -> Header {
        Header {
            key: key.into(),
            value: value.into(),
            enabled,
            description: None,
        }
    }

    fn request(auth: Auth) -> HttpRequest {
        let mut req = HttpRequest::new(
            HttpMethod::Post,
            "https://api.example.com/login?t=sekret-token",
        );
        req.query_params = vec![
            QueryParam {
                key: "page".into(),
                value: "2".into(),
                enabled: true,
                description: None,
            },
            QueryParam {
                key: "off".into(),
                value: "x".into(),
                enabled: false,
                description: None,
            },
        ];
        req.headers = vec![
            header("X-Custom", "sekret-token", true),
            header("Cookie", "sid=1", true),
            header("X-Off", "no", false),
        ];
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"p":"sekret-token"}"#.into()),
            form_data: None,
            file_path: None,
        });
        req.auth = auth;
        req
    }

    #[test]
    fn masks_secrets_everywhere_and_appends_enabled_query_params() {
        let d = build_debug_request(
            &request(Auth::None),
            None,
            None,
            &secrets(&["sekret-token"]),
        );
        assert_eq!(d.method, "POST");
        assert!(
            d.url.contains("page=2") && !d.url.contains("off=x"),
            "{}",
            d.url
        );
        assert!(!d.url.contains("sekret-token"));
        assert_eq!(
            d.headers
                .iter()
                .find(|h| h.key == "X-Custom")
                .map(|h| h.value.as_str()),
            Some("••••••")
        );
        assert_eq!(
            d.headers
                .iter()
                .find(|h| h.key == "Cookie")
                .map(|h| h.value.as_str()),
            Some("••••••")
        );
        assert!(d.headers.iter().all(|h| h.key != "X-Off"));
        assert_eq!(d.body.as_deref(), Some(r#"{"p":"••••••"}"#));
    }

    #[test]
    fn shows_auth_as_a_masked_line_never_its_value() {
        let d = build_debug_request(
            &request(Auth::Bearer {
                token: "tok-123456".into(),
            }),
            None,
            None,
            &HashSet::new(),
        );
        let auth = d
            .headers
            .iter()
            .find(|h| h.key == "Authorization")
            .expect("auth line");
        assert_eq!(auth.value, "Bearer ••••••");
        assert!(d.headers.iter().all(|h| !h.value.contains("tok-123456")));
    }

    #[test]
    fn shows_basic_auth_as_a_masked_line() {
        let d = build_debug_request(
            &request(Auth::Basic {
                username: "user-abc".into(),
                password: "pw-123456".into(),
            }),
            None,
            None,
            &HashSet::new(),
        );
        let auth = d
            .headers
            .iter()
            .find(|h| h.key == "Authorization")
            .expect("auth line");
        assert_eq!(auth.value, "Basic ••••••");
        assert!(d.headers.iter().all(|h| !h.value.contains("pw-123456")));
    }

    #[test]
    fn masks_encoded_secrets_in_the_url_and_the_error() {
        let secret = "p@ss word é1";
        let mut r = request(Auth::None);
        r.url = format!("https://example.com/{secret}/x");
        r.query_params = vec![rocket_shared::types::QueryParam {
            key: "q".into(),
            value: secret.into(),
            enabled: true,
            description: None,
        }];
        let secrets = secrets(&[secret]);
        let err = "error sending request for url (https://example.com/p@ss%20word%20%C3%A91/x?q=p%40ss+word+%C3%A91)";
        let d = build_debug_request(&r, None, Some(err), &secrets);
        for text in [d.url.as_str(), d.error.as_deref().unwrap_or("")] {
            assert!(text.contains("••••••"), "{text}");
            for form in ["p@ss", "%40", "word", "%20", "%C3%A9", "é", "+"] {
                assert!(!text.contains(form), "{form} in {text}");
            }
        }
    }

    #[test]
    fn describes_api_key_auth_without_its_value() {
        let query = Auth::ApiKey {
            key: "k".into(),
            value: "val-123456".into(),
            placement: "query".into(),
        };
        let d = build_debug_request(&request(query), None, None, &HashSet::new());
        assert!(d
            .headers
            .iter()
            .any(|h| h.key == "Auth" && h.value == "API key in query \"k\""));
        assert!(!d.url.contains("val-123456"));

        let in_header = Auth::ApiKey {
            key: "X-Api".into(),
            value: "val-123456".into(),
            placement: "header".into(),
        };
        let d = build_debug_request(&request(in_header), None, None, &HashSet::new());
        assert!(d
            .headers
            .iter()
            .any(|h| h.key == "X-Api" && h.value == "••••••"));
    }

    #[test]
    fn shows_no_auth_line_for_an_api_key_with_an_unknown_placement() {
        let auth = Auth::ApiKey {
            key: "X-Api".into(),
            value: "val-123456".into(),
            placement: "cookie".into(),
        };
        let d = build_debug_request(&request(auth), None, None, &HashSet::new());
        assert!(d
            .headers
            .iter()
            .all(|h| h.key != "X-Api" && h.key != "Auth"));
    }

    #[test]
    fn lists_enabled_form_entries_as_masked_lines() {
        use rocket_shared::types::{FormDataEntry, FormDataType};
        let entry = |key: &str, value: &str, enabled: bool| FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type: FormDataType::Text,
            enabled,
            content_type: None,
            description: None,
        };
        let mut req = request(Auth::None);
        req.body = Some(Body {
            mode: BodyMode::FormUrlEncoded,
            content: None,
            form_data: Some(vec![
                entry("a", "sekret-token", true),
                entry("b", "no", false),
            ]),
            file_path: None,
        });
        let d = build_debug_request(&req, None, None, &secrets(&["sekret-token"]));
        assert_eq!(d.body.as_deref(), Some("a=••••••"));
    }

    #[test]
    fn records_the_response_and_masks_its_sensitive_headers() {
        let resp = HttpResponse {
            status: 400,
            status_text: "Bad Request".into(),
            headers: vec![header("Set-Cookie", "sid=2", true)],
            body: r#"{"error":"sekret-token bad"}"#.into(),
            duration_ms: 12,
            ttfb_ms: 5,
            size_bytes: 30,
        };
        let d = build_debug_request(
            &request(Auth::None),
            Some(&resp),
            None,
            &secrets(&["sekret-token"]),
        );
        let r = d.response.expect("response");
        assert_eq!(r.status, 400);
        assert_eq!(r.headers[0].value, "••••••");
        assert!(!r.body.contains("sekret-token"));
    }

    #[test]
    fn records_the_error_when_there_is_no_response() {
        let d = build_debug_request(
            &request(Auth::None),
            None,
            Some("connection refused"),
            &HashSet::new(),
        );
        assert!(d.response.is_none());
        assert_eq!(d.error.as_deref(), Some("connection refused"));
    }

    fn response_record(body: String) -> FlowDebugRequest {
        FlowDebugRequest {
            method: "GET".into(),
            url: "https://x.test".into(),
            headers: Vec::new(),
            body: None,
            response: Some(FlowDebugResponse {
                status: 200,
                status_text: "OK".into(),
                duration_ms: 1,
                size_bytes: body.len() as u64,
                headers: Vec::new(),
                body,
                truncated: false,
            }),
            error: None,
        }
    }

    #[test]
    fn cap_exchange_keeps_a_body_at_the_limit_untouched() {
        let body = "a".repeat(EXCHANGE_BODY_LIMIT);
        let capped = cap_exchange(response_record(body.clone()));
        let response = capped.response.expect("response");
        assert_eq!(response.body, body);
        assert!(!response.truncated);
    }

    #[test]
    fn cap_exchange_cuts_a_long_body_and_keeps_the_real_size() {
        let body = "a".repeat(EXCHANGE_BODY_LIMIT + 10);
        let capped = cap_exchange(response_record(body));
        let response = capped.response.expect("response");
        assert_eq!(response.body.len(), EXCHANGE_BODY_LIMIT);
        assert!(response.truncated);
        assert_eq!(response.size_bytes, (EXCHANGE_BODY_LIMIT + 10) as u64);
    }

    #[test]
    fn cap_exchange_never_splits_a_utf8_character() {
        // "é" is two bytes. It starts one byte before the limit, so a naive
        // cut would split it.
        let mut body = "a".repeat(EXCHANGE_BODY_LIMIT - 1);
        body.push('é');
        body.push_str("tail");
        let capped = cap_exchange(response_record(body));
        let response = capped.response.expect("response");
        assert_eq!(response.body.len(), EXCHANGE_BODY_LIMIT - 1);
        assert!(response.body.chars().all(|c| c == 'a'));
        assert!(response.truncated);
    }

    #[test]
    fn cap_exchange_leaves_a_record_without_a_response_alone() {
        let mut record = response_record(String::new());
        record.response = None;
        record.error = Some("connection refused".into());
        assert_eq!(cap_exchange(record.clone()), record);
    }

    #[test]
    fn callback_exchange_shows_the_call_as_the_response_with_secrets_masked() {
        let call = crate::callback_listener::ReceivedCall {
            method: "POST".into(),
            path: "/cb/abc".into(),
            query: vec![("event".into(), "paid".into())],
            headers: vec![
                ("authorization".into(), "Bearer t".into()),
                ("x-note".into(), "sekret-token".into()),
            ],
            body: r#"{"k":"sekret-token"}"#.into(),
        };
        let record = callback_exchange(&call, 42, &secrets(&["sekret-token"]));
        assert_eq!(record.method, "POST");
        assert_eq!(record.url, "/cb/abc?event=paid");
        let response = record.response.expect("response");
        assert_eq!(response.status, 200);
        assert_eq!(response.status_text, "POST");
        assert_eq!(response.duration_ms, 42);
        assert_eq!(response.body, r#"{"k":"••••••"}"#);
        let value = |key: &str| {
            response
                .headers
                .iter()
                .find(|h| h.key == key)
                .map(|h| h.value.clone())
        };
        assert_eq!(value("authorization").as_deref(), Some("••••••"));
        assert_eq!(value("x-note").as_deref(), Some("••••••"));
    }
}
