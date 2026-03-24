use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

/// Per-collection default auth and headers, stored in collection.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    /// Optional human-readable description for this collection.
    #[serde(default)]
    pub description: Option<String>,

    /// Optional auth applied to all requests in this collection.
    #[serde(default)]
    pub auth: Option<Auth>,

    /// Default headers prepended to every request in this collection.
    #[serde(default)]
    pub headers: Vec<Header>,
}
