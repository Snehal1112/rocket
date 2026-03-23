use rocket_shared::types::Header;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    pub body: String,
    pub duration_ms: u64,
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
}
