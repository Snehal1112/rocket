use crate::oc::*;
use rocket_environment::environment::Environment;
use rocket_environment::external_secret::{ExternalSecretBinding, ExternalSecretRef};
use rocket_environment::variable::Variable;

impl From<OcExternalSecretRef> for ExternalSecretRef {
    fn from(oc: OcExternalSecretRef) -> Self {
        ExternalSecretRef {
            name: oc.name,
            secret_id: oc.secret_id,
        }
    }
}

impl From<ExternalSecretRef> for OcExternalSecretRef {
    fn from(r: ExternalSecretRef) -> Self {
        OcExternalSecretRef {
            name: r.name,
            secret_id: r.secret_id,
        }
    }
}

impl From<OcExternalSecretBinding> for ExternalSecretBinding {
    fn from(oc: OcExternalSecretBinding) -> Self {
        ExternalSecretBinding {
            alias: oc.alias,
            connection_id: oc.connection_id,
            vault_name: oc.vault_name,
            secret_names: oc
                .secret_names
                .into_iter()
                .map(ExternalSecretRef::from)
                .collect(),
        }
    }
}

impl From<ExternalSecretBinding> for OcExternalSecretBinding {
    fn from(b: ExternalSecretBinding) -> Self {
        OcExternalSecretBinding {
            alias: b.alias,
            connection_id: b.connection_id,
            vault_name: b.vault_name,
            secret_names: b
                .secret_names
                .into_iter()
                .map(OcExternalSecretRef::from)
                .collect(),
        }
    }
}

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
            external_secrets: oc
                .external_secrets
                .into_iter()
                .map(ExternalSecretBinding::from)
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
            external_secrets: env
                .external_secrets
                .into_iter()
                .map(OcExternalSecretBinding::from)
                .collect(),
            client_certificates: env.client_certificates,
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}
