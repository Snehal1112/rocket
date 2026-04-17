use rocket_shared::types::Header;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    pub body: String,
    /// Total time from request sent to body fully received, in milliseconds.
    pub duration_ms: u64,
    /// Time from request sent to first byte of the response headers, in milliseconds.
    pub ttfb_ms: u64,
    pub size_bytes: usize,
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

    fn sample_response(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![Header::new("content-type", "application/json")],
            body: "{}".into(),
            duration_ms: 150,
            ttfb_ms: 80,
            size_bytes: 2,
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
        };
        assert_eq!(resp.header_value("x-custom"), Some("first"));
    }
}
