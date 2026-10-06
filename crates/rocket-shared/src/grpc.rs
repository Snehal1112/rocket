use serde::{Deserialize, Serialize};

/// One metadata (header or trailer) line of a gRPC call, as shown to the user.
/// Binary values (`*-bin` names) are carried as base64 text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrpcMetadataPair {
    pub name: String,
    pub value: String,
}

impl GrpcMetadataPair {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}
