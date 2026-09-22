use serde::{Deserialize, Serialize};

/// One secret name captured from a RocketVault "Fetch Secrets" action, paired
/// with the vault's own UUID so later value lookups skip a list round-trip.
/// Never carries a secret *value* — see spec §4.4/§4.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRef {
    pub name: String,
    pub secret_id: String,
}

/// Binds a RocketVault connection + vault to an environment under a
/// user-chosen alias. `{{alias.secretName}}` and
/// `rok.getSecretVar('alias.secretName')` both resolve through this binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretBinding {
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
    #[serde(default)]
    pub secret_names: Vec<ExternalSecretRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_secret_ref_serde_roundtrip() {
        let r = ExternalSecretRef {
            name: "stripe-key".to_string(),
            secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
        };
        let json = serde_json::to_string(&r).expect("serialize ExternalSecretRef");
        assert!(
            json.contains("\"secretId\""),
            "expected camelCase field, got: {json}"
        );
        let back: ExternalSecretRef =
            serde_json::from_str(&json).expect("deserialize ExternalSecretRef");
        assert_eq!(r, back);
    }

    #[test]
    fn external_secret_binding_serde_roundtrip() {
        let b = ExternalSecretBinding {
            alias: "payments".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: vec![ExternalSecretRef {
                name: "stripe-key".to_string(),
                secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            }],
        };
        let json = serde_json::to_string(&b).expect("serialize ExternalSecretBinding");
        assert!(
            json.contains("\"connectionId\""),
            "expected camelCase field, got: {json}"
        );
        assert!(
            json.contains("\"vaultName\""),
            "expected camelCase field, got: {json}"
        );
        let back: ExternalSecretBinding =
            serde_json::from_str(&json).expect("deserialize ExternalSecretBinding");
        assert_eq!(b, back);
    }

    #[test]
    fn external_secret_binding_defaults_secret_names_to_empty() {
        let json = r#"{"alias":"payments","connectionId":"conn-1","vaultName":"prod-vault"}"#;
        let b: ExternalSecretBinding =
            serde_json::from_str(json).expect("deserialize without secretNames");
        assert!(b.secret_names.is_empty());
    }
}
