use serde::Deserialize;
use crate::bru::ast::*;
use crate::error::{ImportError, ImportResult};

// ─── Request structs ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlRequest {
    pub meta: Option<BruYmlMeta>,
    pub http: Option<BruYmlHttp>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlMeta {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub request_type: Option<String>,
    pub seq: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHttp {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    pub body: Option<BruYmlBody>,
    pub auth: Option<BruYmlAuth>,
    pub script: Option<BruYmlScript>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBody {
    pub mode: Option<String>,
    pub json: Option<String>,
    pub text: Option<String>,
    pub xml: Option<String>,
    #[serde(rename = "formUrlEncoded")]
    pub form_url_encoded: Option<Vec<BruYmlFormField>>,
    pub multipart: Option<Vec<BruYmlFormField>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlFormField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAuth {
    pub mode: Option<String>,
    pub bearer: Option<BruYmlBearerAuth>,
    pub basic: Option<BruYmlBasicAuth>,
    pub awsv4: Option<BruYmlAwsV4Auth>,
    pub apikey: Option<BruYmlApiKeyAuth>,
    pub digest: Option<BruYmlBasicAuth>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBearerAuth {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBasicAuth {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAwsV4Auth {
    #[serde(rename = "accessKeyId")]
    pub access_key_id: Option<String>,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: Option<String>,
    #[serde(rename = "sessionToken")]
    pub session_token: Option<String>,
    pub service: Option<String>,
    pub region: Option<String>,
    #[serde(rename = "profileName")]
    pub profile_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlApiKeyAuth {
    pub key: Option<String>,
    pub value: Option<String>,
    pub placement: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlScript {
    pub req: Option<String>,
    pub res: Option<String>,
}

// ─── Environment structs ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlEnv {
    pub name: Option<String>,
    pub variables: Option<Vec<BruYmlEnvVar>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlEnvVar {
    pub name: String,
    pub value: Option<String>,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }
