use crate::oc::*;
use rocket_environment::environment::Environment;
use rocket_environment::variable::Variable;

impl From<OcEnvironment> for Environment {
    fn from(oc: OcEnvironment) -> Self {
        Environment {
            name: oc.name,
            variables: oc.variables.into_iter().map(Variable::from).collect(),
            color: oc.color,
            description: oc.description,
            extends: oc.extends,
            dot_env_file_path: oc.dot_env_file_path,
            // Domain uses Vec<serde_json::Value> as a placeholder for client certs.
            client_certificates: oc.client_certificates.into_iter()
                .map(|c| serde_json::to_value(c).unwrap_or_default())
                .collect(),
        }
    }
}

impl From<Environment> for OcEnvironment {
    fn from(env: Environment) -> Self {
        OcEnvironment {
            name: env.name,
            color: env.color,
            description: env.description,
            variables: env.variables.into_iter().map(OcVariable::from).collect(),
            client_certificates: env.client_certificates.into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect(),
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}
