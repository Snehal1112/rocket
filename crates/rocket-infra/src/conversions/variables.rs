use crate::oc::*;
use rocket_collection::settings::CollectionVariable;
use rocket_environment::variable::Variable;
use rocket_shared::variable_value::VariableValue;

impl From<OcVariable> for CollectionVariable {
    fn from(v: OcVariable) -> Self {
        let current = v.value.as_ref().map(|vv| vv.data().to_string()).unwrap_or_default();
        // Fall back to the current value if initial is absent (backward compat with old files).
        let initial = v.initial.as_ref()
            .map(|vv| vv.data().to_string())
            .unwrap_or_else(|| current.clone());
        CollectionVariable {
            key:           v.name,
            value:         current,
            initial_value: initial,
            enabled:       !v.disabled.unwrap_or(false),
            secret:        false,
        }
    }
}

impl From<CollectionVariable> for OcVariable {
    fn from(cv: CollectionVariable) -> Self {
        OcVariable {
            name:        cv.key,
            value:       if cv.value.is_empty() { None } else { Some(VariableValue::simple(cv.value)) },
            initial:     if cv.initial_value.is_empty() { None } else { Some(VariableValue::simple(cv.initial_value)) },
            description: None,
            disabled:    if cv.enabled { None } else { Some(true) },
        }
    }
}

// For collection variables use CollectionVariable::from(oc_variable) instead — this
// impl is only for environment variables and silently drops the `initial` field.
impl From<OcVariable> for Variable {
    fn from(oc: OcVariable) -> Self {
        Variable {
            key: oc.name,
            value: oc.value.as_ref().map(|v| v.data().to_string()).unwrap_or_default(),
            enabled: !oc.disabled.unwrap_or(false),
            secret: false,
            description: oc.description,
            value_variants: None,
            secret_type: None,
        }
    }
}

impl From<Variable> for OcVariable {
    fn from(v: Variable) -> Self {
        OcVariable {
            name: v.key,
            value: Some(VariableValue::simple(v.value)),
            // Environment variables don't have a separate initial value concept.
            initial: None,
            description: v.description,
            // Omit disabled entirely when enabled (cleaner YAML output).
            disabled: if v.enabled { None } else { Some(true) },
        }
    }
}

impl From<OcSecretVariable> for Variable {
    fn from(oc: OcSecretVariable) -> Self {
        Variable {
            key: oc.name,
            // Secrets don't store values in YAML.
            value: String::new(),
            enabled: !oc.disabled.unwrap_or(false),
            secret: true,
            description: oc.description,
            value_variants: None,
            secret_type: oc.secret_type,
        }
    }
}
