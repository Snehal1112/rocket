use rocket_shared::types::Header;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    /// The body as text. Empty for a binary body, see `is_binary`.
    pub body: String,
    /// Total time from request sent to body fully received, in milliseconds.
    pub duration_ms: u64,
    /// Time from request sent to first byte of the response headers, in milliseconds.
    pub ttfb_ms: u64,
    pub size_bytes: usize,
    /// True when the body is not text, so `body` is empty and the bytes are in `body_base64`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_binary: bool,
    /// The raw bytes of a binary body, base64 encoded. `None` for a text body, and for a binary
    /// body larger than `MAX_BINARY_BODY_BYTES`, which is flagged but not carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
}

/// Largest binary body carried over IPC, in bytes.
pub const MAX_BINARY_BODY_BYTES: usize = 32 * 1024 * 1024;

/// A response body split into what the rest of the app reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyPayload {
    pub text: String,
    pub is_binary: bool,
    pub base64: Option<String>,
}

fn is_textual_type(essence: &str) -> bool {
    if essence.starts_with("text/") {
        return true;
    }
    // Match the subtype exactly. A substring test would misread office documents
    // (`...openxmlformats...`) as XML.
    let subtype = essence.split('/').nth(1).unwrap_or("");
    subtype.ends_with("+json")
        || subtype.ends_with("+xml")
        || matches!(
            subtype,
            "json"
                | "xml"
                | "javascript"
                | "x-javascript"
                | "ecmascript"
                | "yaml"
                | "x-yaml"
                | "x-www-form-urlencoded"
                | "graphql"
                | "sparql-query"
                | "x-ndjson"
                | "ndjson"
                | "sql"
                | "x-sh"
        )
}

fn is_binary_type(content_type: &str) -> bool {
    const PREFIXES: [&str; 4] = ["image/", "audio/", "video/", "font/"];
    const EXACT: [&str; 9] = [
        "application/octet-stream",
        "application/pdf",
        "application/zip",
        "application/gzip",
        "application/x-gzip",
        "application/x-tar",
        "application/x-7z-compressed",
        "application/wasm",
        "application/x-protobuf",
    ];
    PREFIXES.iter().any(|p| content_type.starts_with(p)) || EXACT.contains(&content_type)
}

/// Splits response bytes into text or a base64 payload.
///
/// A declared text type (JSON, XML, `text/*`, SVG, ...) is always text, as before, even when the
/// bytes are not valid UTF-8. A declared binary type (image, audio, video, font, PDF, archives,
/// octet-stream) is binary. Anything else, including a missing `Content-Type`, is binary when
/// the bytes are not valid UTF-8 or contain a NUL byte. A binary body larger than
/// `max_binary_bytes` is flagged but its bytes are not carried.
pub fn body_from_bytes(
    content_type: Option<&str>,
    bytes: &[u8],
    max_binary_bytes: usize,
) -> BodyPayload {
    use base64::Engine;

    let essence = content_type
        .and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let binary = if bytes.is_empty() || is_textual_type(&essence) {
        false
    } else if is_binary_type(&essence) {
        true
    } else {
        bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
    };
    if !binary {
        return BodyPayload {
            text: String::from_utf8_lossy(bytes).into_owned(),
            is_binary: false,
            base64: None,
        };
    }
    BodyPayload {
        text: String::new(),
        is_binary: true,
        base64: (bytes.len() <= max_binary_bytes)
            .then(|| base64::engine::general_purpose::STANDARD.encode(bytes)),
    }
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    pub fn header_value(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(key))
            .map(|h| h.value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header_value("content-type")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn sample_response(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![Header::new("content-type", "application/json")],
            body: "{}".into(),
            duration_ms: 150,
            ttfb_ms: 80,
            size_bytes: 2,
            ..Default::default()
        }
    }

    #[test]
    fn status_classification() {
        assert!(sample_response(200).is_success());
        assert!(sample_response(301).is_redirect());
        assert!(sample_response(404).is_client_error());
        assert!(sample_response(500).is_server_error());
    }

    #[test]
    fn header_lookup_case_insensitive() {
        let resp = sample_response(200);
        assert_eq!(resp.header_value("Content-Type"), Some("application/json"));
        assert_eq!(resp.content_type(), Some("application/json"));
        assert_eq!(resp.header_value("x-missing"), None);
    }

    #[test]
    fn status_boundaries() {
        // 2xx: 200–299
        assert!(!sample_response(199).is_success());
        assert!(sample_response(200).is_success());
        assert!(sample_response(299).is_success());
        assert!(!sample_response(300).is_success());
        // 3xx: 300–399
        assert!(!sample_response(299).is_redirect());
        assert!(sample_response(300).is_redirect());
        assert!(sample_response(399).is_redirect());
        assert!(!sample_response(400).is_redirect());
        // 4xx: 400–499
        assert!(!sample_response(399).is_client_error());
        assert!(sample_response(400).is_client_error());
        assert!(sample_response(499).is_client_error());
        assert!(!sample_response(500).is_client_error());
        // 5xx: 500–599
        assert!(!sample_response(499).is_server_error());
        assert!(sample_response(500).is_server_error());
        assert!(sample_response(599).is_server_error());
    }

    #[test]
    fn content_type_none_when_header_absent() {
        let resp = HttpResponse {
            status: 204,
            status_text: "No Content".into(),
            headers: vec![],
            body: String::new(),
            duration_ms: 5,
            ttfb_ms: 5,
            size_bytes: 0,
            ..Default::default()
        };
        assert!(resp.content_type().is_none());
    }

    #[test]
    fn header_value_first_match_returned() {
        let resp = HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![
                Header::new("x-custom", "first"),
                Header::new("x-custom", "second"),
            ],
            body: String::new(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 0,
            ..Default::default()
        };
        assert_eq!(resp.header_value("x-custom"), Some("first"));
    }

    #[test]
    fn json_body_stays_text() {
        let p = body_from_bytes(Some("application/json; charset=utf-8"), b"{\"a\":1}", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "{\"a\":1}");
        assert_eq!(p.base64, None);
    }

    #[test]
    fn png_body_is_binary_with_base64() {
        let bytes = [0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00];
        let p = body_from_bytes(Some("image/png"), &bytes, 1024);
        assert!(p.is_binary);
        assert_eq!(p.text, "");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(p.base64.expect("payload"))
            .expect("valid base64");
        assert_eq!(decoded, bytes, "bytes must arrive unchanged");
    }

    #[test]
    fn svg_stays_text_because_it_is_xml() {
        let p = body_from_bytes(Some("image/svg+xml"), b"<svg/>", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "<svg/>");
    }

    #[test]
    fn declared_text_with_invalid_utf8_stays_lossy_text() {
        let p = body_from_bytes(
            Some("text/html; charset=latin-1"),
            &[b'c', b'a', b'f', 0xe9],
            1024,
        );
        assert!(!p.is_binary);
        assert!(p.text.starts_with("caf"));
    }

    #[test]
    fn unknown_type_is_decided_by_the_bytes() {
        assert!(!body_from_bytes(Some("application/x-custom"), "héllo".as_bytes(), 1024).is_binary);
        assert!(body_from_bytes(Some("application/x-custom"), &[0xff, 0xfe, 0xfd], 1024).is_binary);
        assert!(
            body_from_bytes(None, &[b'a', 0, b'b'], 1024).is_binary,
            "NUL means binary"
        );
        assert!(!body_from_bytes(None, b"plain", 1024).is_binary);
    }

    #[test]
    fn well_known_binary_types_are_binary_even_when_the_bytes_are_ascii() {
        for ct in [
            "application/pdf",
            "application/zip",
            "application/octet-stream",
            "audio/mpeg",
            "video/mp4",
            "font/woff2",
            "image/jpeg",
        ] {
            assert!(body_from_bytes(Some(ct), b"abc", 1024).is_binary, "{ct}");
        }
    }

    #[test]
    fn binary_over_the_cap_is_flagged_but_carries_no_payload() {
        let p = body_from_bytes(Some("application/pdf"), &[1, 2, 3, 4, 5], 4);
        assert!(p.is_binary);
        assert_eq!(p.base64, None);
        assert_eq!(p.text, "");
    }

    #[test]
    fn office_documents_are_binary_not_xml() {
        let zip_like = [0x50, 0x4b, 0x03, 0x04, 0xff, 0x00];
        for ct in [
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "application/vnd.ms-excel",
        ] {
            assert!(body_from_bytes(Some(ct), &zip_like, 1024).is_binary, "{ct}");
        }
        // A vendor type with a +json or +xml suffix is still text.
        assert!(!body_from_bytes(Some("application/vnd.api+json"), b"{}", 1024).is_binary);
    }

    #[test]
    fn an_empty_body_is_text() {
        let p = body_from_bytes(Some("application/octet-stream"), b"", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "");
    }
}
