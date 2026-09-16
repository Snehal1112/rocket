use crate::oc::*;
use rocket_environment::environment::Environment;
use rocket_environment::variable::Variable;

impl From<OcEnvironment> for Environment {
    fn from(oc: OcEnvironment) -> Self {
        Environment {
            name: oc.name,
            variables: oc
                .variables
                .into_iter()
                .map(|entry| match entry {
                    OcEnvVariableEntry::Secret(s) => Variable::from(s),
                    OcEnvVariableEntry::Plain(v) => Variable::from(v),
                })
                .collect(),
            color: oc.color,
            description: oc.description,
            extends: oc.extends,
            dot_env_file_path: oc.dot_env_file_path,
            client_certificates: oc.client_certificates,
        }
    }
}

impl From<Environment> for OcEnvironment {
    fn from(env: Environment) -> Self {
        OcEnvironment {
            name: env.name,
            color: env.color,
            description: env.description,
            variables: env
                .variables
                .into_iter()
                .map(|v| {
                    if v.secret {
                        // `v.value` is deliberately dropped: a secret value belongs
                        // in the SecretStore and must never reach a YAML struct.
                        // FsEnvironmentRepo::save routes it there before calling this.
                        OcEnvVariableEntry::Secret(OcSecretVariable {
                            secret: true,
                            name: v.key,
                            description: v.description,
                            disabled: if v.enabled { None } else { Some(true) },
                            secret_type: v.secret_type,
                        })
                    } else {
                        OcEnvVariableEntry::Plain(OcVariable::from(v))
                    }
                })
                .collect(),
            client_certificates: env.client_certificates,
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}
