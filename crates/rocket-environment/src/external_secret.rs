use rocket_shared::error::{DomainError, DomainResult};
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

/// Checks that every binding can actually be referenced as
/// `{{alias.secretName}}`. The alias must be non-empty, unique within the
/// environment, and made of letters, digits, `_` or `-` only. A dot would make
/// the `alias.secretName` key ambiguous. Each binding also needs a connection
/// and a vault name, otherwise "Fetch Secrets" and send-time resolution have
/// nothing to talk to.
pub fn validate_external_secret_bindings(bindings: &[ExternalSecretBinding]) -> DomainResult<()> {
    let mut seen = std::collections::HashSet::new();
    for binding in bindings {
        let alias = binding.alias.as_str();
        if alias.is_empty() {
            return Err(DomainError::InvalidInput(
                "external secret binding alias must not be empty".to_string(),
            ));
        }
        if !alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DomainError::InvalidInput(format!(
                "external secret alias '{alias}' may only contain letters, digits, '_' or '-'"
            )));
        }
        if !seen.insert(alias) {
            return Err(DomainError::InvalidInput(format!(
                "external secret alias '{alias}' is used by more than one binding"
            )));
        }
        if binding.connection_id.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "external secret binding '{alias}' has no connection selected"
            )));
        }
        if binding.vault_name.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "external secret binding '{alias}' has no vault name"
            )));
        }
    }
    Ok(())
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

    fn binding(alias: &str) -> ExternalSecretBinding {
        ExternalSecretBinding {
            alias: alias.to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: Vec::new(),
        }
    }

    #[test]
    fn validate_bindings_accepts_valid_aliases() {
        let bindings = vec![binding("payments"), binding("stripe_live-2")];
        assert!(validate_external_secret_bindings(&bindings).is_ok());
    }

    #[test]
    fn validate_bindings_rejects_empty_dotted_or_duplicate_alias() {
        for bad in [
            vec![binding("")],
            vec![binding("pay.ments")],
            vec![binding("pay ments")],
            vec![binding("payments"), binding("payments")],
        ] {
            let err = validate_external_secret_bindings(&bad).expect_err("must reject");
            assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
        }
    }

    #[test]
    fn validate_bindings_rejects_missing_connection_or_vault() {
        let mut no_conn = binding("payments");
        no_conn.connection_id = String::new();
        let mut no_vault = binding("payments");
        no_vault.vault_name = "  ".to_string();
        assert!(validate_external_secret_bindings(&[no_conn]).is_err());
        assert!(validate_external_secret_bindings(&[no_vault]).is_err());
    }
}
