//! Builds the masked record of a request a Flow step sent.

use crate::callback_listener::ReceivedCall;
use crate::execution_service::sensitive_auth_label;
use crate::redaction::{is_sensitive_header, redact_secrets, redact_url_secrets, REDACTED};
use rocket_http::{HttpRequest, HttpResponse};
use rocket_shared::events::{
    FlowDebugHeader, FlowDebugRequest, FlowDebugResponse, FlowRejectedCall,
};
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
        body_truncated: false,
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

/// The largest response body an exchange record keeps, in bytes.
pub(crate) const EXCHANGE_BODY_LIMIT: usize = 262_144;

/// Cuts the request and response bodies to `EXCHANGE_BODY_LIMIT` bytes at
/// a UTF-8 boundary and marks each cut side as truncated.
pub(crate) fn cap_exchange(mut record: FlowDebugRequest) -> FlowDebugRequest {
    if let Some(body) = record.body.as_mut() {
        record.body_truncated = cap_body(body);
    }
    if let Some(response) = record.response.as_mut() {
        if cap_body(&mut response.body) {
            response.truncated = true;
        }
    }
    record
}

/// Cuts `body` to `EXCHANGE_BODY_LIMIT` bytes at a UTF-8 boundary. Returns
/// true when it cut anything.
fn cap_body(body: &mut String) -> bool {
    cap_text(body, EXCHANGE_BODY_LIMIT)
}

/// Cuts `text` to at most `limit` bytes at a UTF-8 boundary. Returns true
/// when it cut anything.
pub(crate) fn cap_text(text: &mut String, limit: usize) -> bool {
    if text.len() <= limit {
        return false;
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    true
}

/// The path and query of a received call. The path of a callback endpoint is
/// its bearer token, so it is shown as `/cb/…`.
fn call_url(call: &ReceivedCall) -> String {
    let path = if call.path.starts_with("/cb/") {
        "/cb/…".to_string()
    } else {
        call.path.clone()
    };
    let query: Vec<String> = call.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    if query.is_empty() {
        path
    } else {
        format!("{path}?{}", query.join("&"))
    }
}

/// A received call's headers. A sensitive header is always masked, and every
/// other value has its secrets masked.
fn mask_call_headers(
    headers: &[(String, String)],
    secret_values: &HashSet<String>,
) -> Vec<FlowDebugHeader> {
    headers
        .iter()
        .map(|(key, value)| FlowDebugHeader {
            key: key.clone(),
            value: if is_sensitive_header(key) {
                REDACTED.to_string()
            } else {
                redact_secrets(value, secret_values)
            },
        })
        .collect()
}

/// The record of an accepted callback. The call itself is the response,
/// so a reader sees what arrived; the request side holds its method and
/// path.
pub(crate) fn callback_exchange(
    call: &ReceivedCall,
    duration_ms: u64,
    secret_values: &HashSet<String>,
) -> FlowDebugRequest {
    cap_exchange(FlowDebugRequest {
        method: call.method.clone(),
        url: redact_url_secrets(&call_url(call), secret_values),
        headers: Vec::new(),
        body: None,
        body_truncated: false,
        response: Some(FlowDebugResponse {
            status: 200,
            status_text: call.method.clone(),
            duration_ms,
            size_bytes: call.body.len() as u64,
            headers: mask_call_headers(&call.headers, secret_values),
            body: redact_secrets(&call.body, secret_values),
            truncated: false,
        }),
        error: None,
    })
}

/// The largest turned-down call body a live progress event carries, in bytes.
pub(crate) const LIVE_REJECTED_BODY_LIMIT: usize = 2_048;

/// The masked record of a call a Wait for callback node turned down. The body
/// is masked first and cut to `body_limit` second.
pub(crate) fn rejected_call(
    call: &ReceivedCall,
    secret_values: &HashSet<String>,
    body_limit: usize,
    reason: &str,
) -> FlowRejectedCall {
    let mut body = redact_secrets(&call.body, secret_values);
    let body_truncated = cap_text(&mut body, body_limit);
    FlowRejectedCall {
        method: call.method.clone(),
        url: redact_url_secrets(&call_url(call), secret_values),
        headers: mask_call_headers(&call.headers, secret_values),
        body,
        body_truncated,
        reason: reason.to_string(),
    }
}

/// The live copy of an already masked turned-down call, with the body cut
/// to `LIVE_REJECTED_BODY_LIMIT` at a char boundary.
pub(crate) fn live_rejected_call(record: &FlowRejectedCall) -> FlowRejectedCall {
    let mut live = record.clone();
    if cap_text(&mut live.body, LIVE_REJECTED_BODY_LIMIT) {
        live.body_truncated = true;
    }
    live
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
    fn the_debug_record_never_carries_client_certificate_material() {
        use rocket_http::{CertificateSource, ResolvedClientCertificate};
        let mut req = request(Auth::None);
        req.options.client_certificates = vec![ResolvedClientCertificate::pem(
            "api.example.com",
            CertificateSource::Inline(zeroize::Zeroizing::new(b"PEM-BODY-LEAK-CHECK".to_vec())),
            CertificateSource::Inline(zeroize::Zeroizing::new(b"KEY-BODY-LEAK-CHECK".to_vec())),
            Some("pass-LEAK-CHECK".into()),
        )];
        let record = build_debug_request(&req, None, None, &secrets(&[]));
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(!json.contains("LEAK-CHECK"), "{json}");
        assert!(!json.contains("clientCertificates"), "{json}");
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
            ..Default::default()
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
            body_truncated: false,
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
    fn cap_exchange_cuts_a_long_request_body_at_a_utf8_boundary() {
        let mut body = "a".repeat(EXCHANGE_BODY_LIMIT - 1);
        body.push('é');
        body.push_str("tail");
        let mut record = response_record(String::new());
        record.body = Some(body);
        let capped = cap_exchange(record);
        let sent = capped.body.expect("request body");
        assert_eq!(sent.len(), EXCHANGE_BODY_LIMIT - 1);
        assert!(sent.chars().all(|c| c == 'a'));
        assert!(capped.body_truncated);
    }

    #[test]
    fn cap_exchange_keeps_a_request_body_at_the_limit_untouched() {
        let body = "a".repeat(EXCHANGE_BODY_LIMIT);
        let mut record = response_record(String::new());
        record.body = Some(body.clone());
        let capped = cap_exchange(record);
        assert_eq!(capped.body.as_deref(), Some(body.as_str()));
        assert!(!capped.body_truncated);
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
        assert_eq!(record.url, "/cb/…?event=paid", "the token path is hidden");
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

    #[test]
    fn callback_exchange_masks_a_secret_in_the_query_plain_or_encoded() {
        let secret = "p@ss word é1";
        let call = crate::callback_listener::ReceivedCall {
            method: "GET".into(),
            path: "/cb/abc".into(),
            query: vec![
                ("plain".into(), secret.into()),
                ("enc".into(), "p%40ss%20word%20%C3%A91".into()),
            ],
            headers: Vec::new(),
            body: String::new(),
        };
        let record = callback_exchange(&call, 1, &secrets(&[secret]));
        assert_eq!(record.url, "/cb/…?plain=••••••&enc=••••••");
    }

    fn turned_down(body: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".into(),
            path: "/cb/abc".into(),
            query: vec![("token".into(), "sekret-token".into())],
            headers: vec![
                ("Authorization".into(), "Bearer x".into()),
                ("X-Echo".into(), "sekret-token".into()),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: body.into(),
        }
    }

    #[test]
    fn a_rejected_call_masks_sensitive_headers_secrets_and_the_url() {
        let call = turned_down(r#"{"t":"sekret-token"}"#);
        let record = rejected_call(
            &call,
            &secrets(&["sekret-token"]),
            EXCHANGE_BODY_LIMIT,
            "no match",
        );
        assert_eq!(record.method, "POST");
        assert_eq!(record.url, format!("/cb/…?token={REDACTED}"));
        assert_eq!(record.headers[0].value, REDACTED);
        assert_eq!(record.headers[1].value, REDACTED);
        assert_eq!(record.headers[2].value, "application/json");
        assert_eq!(record.body, format!(r#"{{"t":"{REDACTED}"}}"#));
        assert!(!record.body_truncated);
        assert_eq!(record.reason, "no match");
    }

    #[test]
    fn a_rejected_call_body_is_masked_before_it_is_cut() {
        let body = format!("{}sekret-token", "a".repeat(LIVE_REJECTED_BODY_LIMIT - 4));
        let record = rejected_call(
            &turned_down(&body),
            &secrets(&["sekret-token"]),
            LIVE_REJECTED_BODY_LIMIT,
            "no match",
        );
        assert!(record.body_truncated);
        assert!(record.body.len() <= LIVE_REJECTED_BODY_LIMIT);
        assert!(!record.body.contains("sek"), "half a secret leaked");
    }

    #[test]
    fn the_live_copy_cuts_the_masked_body_and_flags_it() {
        let body = format!(
            "{}é{}",
            "a".repeat(LIVE_REJECTED_BODY_LIMIT - 1),
            "b".repeat(50)
        );
        let full = rejected_call(
            &turned_down(&body),
            &secrets(&[]),
            EXCHANGE_BODY_LIMIT,
            "no",
        );
        assert!(!full.body_truncated);
        let live = live_rejected_call(&full);
        assert!(live.body_truncated);
        assert!(live.body.len() <= LIVE_REJECTED_BODY_LIMIT);
        assert_eq!(full.body.len(), body.len(), "the full record is untouched");
    }

    #[test]
    fn a_path_outside_the_callback_prefix_is_kept() {
        let mut call = turned_down("");
        call.path = "/other".into();
        call.query.clear();
        let record = rejected_call(&call, &secrets(&[]), EXCHANGE_BODY_LIMIT, "no match");
        assert_eq!(record.url, "/other");
    }
}
