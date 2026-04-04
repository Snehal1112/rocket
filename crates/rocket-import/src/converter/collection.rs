use rocket_collection::CollectionVariable;

use crate::bru::ast::BruKeyValue;

/// Convert Bruno collection-level key-value pairs into domain CollectionVariables.
/// Deferred: not yet called from ImportService; wired once workspace-level bruno.json parsing lands.
#[allow(dead_code)]
pub fn convert_variables(vars: &[BruKeyValue]) -> Vec<CollectionVariable> {
    vars.iter().map(|kv| CollectionVariable {
        key: kv.key.clone(),
        value: kv.value.clone(),
        initial_value: String::new(),
        enabled: !kv.disabled,
        secret: false,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bru::ast::BruKeyValue;

    #[test]
    fn converts_enabled_var() {
        let kvs = vec![BruKeyValue { key: "base".into(), value: "url".into(), disabled: false }];
        let vars = convert_variables(&kvs);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "base");
        assert_eq!(vars[0].value, "url");
        assert!(vars[0].enabled);
    }

    #[test]
    fn disabled_var_maps_to_enabled_false() {
        let kvs = vec![BruKeyValue { key: "unused".into(), value: "x".into(), disabled: true }];
        let vars = convert_variables(&kvs);
        assert!(!vars[0].enabled);
    }

    #[test]
    fn empty_input_produces_empty_vec() {
        assert!(convert_variables(&[]).is_empty());
    }
}
