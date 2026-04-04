use rocket_environment::{Environment, Variable};

use crate::bru::ast::BruDocument;

/// Convert a parsed BruDocument (from an env file) to a domain Environment.
pub fn convert(name: &str, doc: &BruDocument) -> Environment {
    let mut env = Environment::new(name);

    for kv in &doc.vars {
        let mut var = Variable::new(kv.key.clone(), kv.value.clone());
        if kv.disabled {
            var.enabled = false;
        }
        env.variables.push(var);
    }

    for secret_name in &doc.secret_vars {
        env.variables.push(Variable::secret(secret_name.clone(), ""));
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bru::ast::{BruDocument, BruKeyValue};

    #[test]
    fn converts_plain_vars() {
        let doc = BruDocument {
            vars: vec![
                BruKeyValue { key: "baseUrl".into(), value: "http://localhost:3000".into(), disabled: false },
                BruKeyValue { key: "apiKey".into(), value: "abc123".into(), disabled: true },
            ],
            ..BruDocument::default()
        };
        let env = convert("local", &doc);
        assert_eq!(env.name, "local");
        assert_eq!(env.variables.len(), 2);
        assert!(env.variables[0].enabled);
        assert!(!env.variables[1].enabled);
    }

    #[test]
    fn secret_var_names_become_secret_variables() {
        let doc = BruDocument {
            secret_vars: vec!["DB_PASSWORD".into(), "API_SECRET".into()],
            ..BruDocument::default()
        };
        let env = convert("prod", &doc);
        assert_eq!(env.variables.iter().filter(|v| v.secret).count(), 2);
    }

    #[test]
    fn secret_vars_have_empty_value() {
        let doc = BruDocument {
            secret_vars: vec!["TOKEN".into()],
            ..BruDocument::default()
        };
        let env = convert("test", &doc);
        assert_eq!(env.variables[0].value, "");
        assert!(env.variables[0].secret);
    }
}
