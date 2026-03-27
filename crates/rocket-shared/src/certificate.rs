use serde::{Deserialize, Serialize};

/// Client certificate — PEM or PKCS12 format, discriminated by `type` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCertificate {
    #[serde(rename = "pem", rename_all = "camelCase")]
    Pem {
        domain: String,
        certificate_file_path: String,
        private_key_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    #[serde(rename = "pkcs12", rename_all = "camelCase")]
    Pkcs12 {
        domain: String,
        #[serde(rename = "pkcs12FilePath")]
        pkcs12_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_certificate_serde() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path: "/certs/client-key.pem".into(),
            passphrase: None,
        };
        let json = serde_json::to_string(&cert).unwrap();
        assert!(json.contains("\"type\":\"pem\""));
        assert!(json.contains("\"certificateFilePath\""));
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn pkcs12_certificate_serde() {
        let cert = ClientCertificate::Pkcs12 {
            domain: "secure.example.com".into(),
            pkcs12_file_path: "/certs/client.p12".into(),
            passphrase: Some("secret".into()),
        };
        let json = serde_json::to_string(&cert).unwrap();
        assert!(json.contains("\"type\":\"pkcs12\""));
        assert!(json.contains("\"pkcs12FilePath\""));
        assert!(json.contains("\"passphrase\":\"secret\""));
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn pem_with_passphrase() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path: "/certs/client-key.pem".into(),
            passphrase: Some("my-pass".into()),
        };
        let json = serde_json::to_string(&cert).unwrap();
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn certificate_dispatch_on_type() {
        let pem_json = r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#;
        let pkcs_json = r#"{"type":"pkcs12","domain":"b.com","pkcs12FilePath":"/c.p12"}"#;

        let pem: ClientCertificate = serde_json::from_str(pem_json).unwrap();
        assert!(matches!(pem, ClientCertificate::Pem { .. }));

        let pkcs: ClientCertificate = serde_json::from_str(pkcs_json).unwrap();
        assert!(matches!(pkcs, ClientCertificate::Pkcs12 { .. }));
    }
}
