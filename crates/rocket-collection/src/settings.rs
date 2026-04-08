use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

/// A collection-scoped variable (like Postman/Bruno collection variables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key: String,
    pub value: String,
    /// Initial/default value committed to Git; fallback when value is empty.
    #[serde(default)]
    pub initial_value: String,
    pub enabled: bool,
    /// Mark as secret to hide in the UI (like Bruno).
    #[serde(default)]
    pub secret: bool,
}

/// Per-collection default auth, headers, and variables, stored in collection.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    /// Optional human-readable description for this collection.
    #[serde(default)]
    pub description: Option<String>,

    /// Optional markdown readme for this collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,

    /// Optional auth applied to all requests in this collection.
    #[serde(default)]
    pub auth: Option<Auth>,

    /// Default headers prepended to every request in this collection.
    #[serde(default)]
    pub headers: Vec<Header>,

    /// Collection-scoped variables, resolved alongside environment variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
}
