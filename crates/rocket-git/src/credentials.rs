use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitCredentials {
    #[serde(rename_all = "camelCase")]
    SshKey { private_key_path: String, passphrase: Option<String> },
    SshAgent,
    #[serde(rename_all = "camelCase")]
    UserPass { username: String, password: String },
    Token { token: String },
}
