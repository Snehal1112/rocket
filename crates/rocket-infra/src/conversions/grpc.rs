//! Conversions between the domain `GrpcRequest` and the OC gRPC structs.

use crate::oc::*;
use rocket_collection::settings::CollectionVariable;
use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript};
use rocket_shared::types::Auth;

use super::auth::persisted_oc_auth;

/// Convert an OC gRPC request to the domain type.
///
/// The OpenCollection schema keeps gRPC auth in the `grpc` block. An auth found in
/// `runtime` (which the schema does not allow) is accepted on read and written back
/// in the `grpc` block.
pub fn oc_grpc_to_domain(oc: OcGrpcRequest) -> GrpcRequest {
    let method_type = match oc.grpc.method_type.as_deref() {
        None => GrpcMethodType::default(),
        Some(raw) => GrpcMethodType::parse(raw).unwrap_or_else(|| {
            tracing::warn!(
                method_type = raw,
                "unknown gRPC methodType, treating it as unary"
            );
            GrpcMethodType::default()
        }),
    };

    let messages = match oc.grpc.message {
        None => Vec::new(),
        Some(OcGrpcMessageOrVariants::Single(content)) => vec![GrpcMessage {
            title: String::new(),
            selected: true,
            content,
        }],
        Some(OcGrpcMessageOrVariants::Variants(variants)) => variants
            .into_iter()
            .map(|v| GrpcMessage {
                title: v.title,
                selected: v.selected,
                content: v.message,
            })
            .collect(),
    };

    let runtime_auth = oc.runtime.as_ref().and_then(|r| r.auth.clone());
    let (variables, scripts, assertions) = match oc.runtime {
        Some(rt) => (
            rt.variables
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            rt.scripts
                .into_iter()
                .map(|s| GrpcScript {
                    script_type: s.script_type,
                    code: s.code,
                })
                .collect(),
            rt.assertions,
        ),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };

    GrpcRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        file_name: None,
        seq: oc.info.seq,
        tags: oc.info.tags,
        description: oc.info.description,
        url: oc.grpc.url,
        method: oc.grpc.method,
        method_type,
        proto_file_path: oc.grpc.proto_file_path,
        metadata: oc
            .grpc
            .metadata
            .into_iter()
            .map(|m| GrpcMetadataEntry {
                key: m.name,
                value: m.value,
                enabled: !m.disabled.unwrap_or(false),
                description: m.description,
            })
            .collect(),
        messages,
        auth: oc
            .grpc
            .auth
            .or(runtime_auth)
            .map(Auth::from)
            .unwrap_or(Auth::None),
        variables,
        scripts,
        assertions,
        docs: oc.docs,
    }
}

/// The uid a file with no `uid` key gets. Stable across loads, so a tab opened from the sidebar
/// summary keeps its id when the full request is read.
pub fn derived_grpc_uid(file_name: &str) -> String {
    format!("grpc-{file_name}")
}

/// Convert a domain gRPC request back to the OC struct.
pub fn grpc_to_oc(g: &GrpcRequest) -> OcGrpcRequest {
    // One untitled message is the plain string form. Anything else keeps its titles.
    let message = match g.messages.as_slice() {
        [] => None,
        [only] if only.title.is_empty() => {
            Some(OcGrpcMessageOrVariants::Single(only.content.clone()))
        }
        many => Some(OcGrpcMessageOrVariants::Variants(
            many.iter()
                .map(|m| OcGrpcMessageVariant {
                    title: m.title.clone(),
                    selected: m.selected,
                    message: m.content.clone(),
                })
                .collect(),
        )),
    };

    let has_runtime = !g.variables.is_empty() || !g.scripts.is_empty() || !g.assertions.is_empty();
    // The schema allows only variables, scripts and assertions in `runtime`.
    let runtime = has_runtime.then(|| OcGrpcRequestRuntime {
        variables: g.variables.iter().cloned().map(OcVariable::from).collect(),
        scripts: g
            .scripts
            .iter()
            .map(|s| OcScript {
                script_type: s.script_type.clone(),
                code: s.code.clone(),
            })
            .collect(),
        assertions: g.assertions.clone(),
        auth: None,
    });

    OcGrpcRequest {
        uid: if g.uid.is_empty() {
            None
        } else {
            Some(g.uid.clone())
        },
        info: OcGrpcRequestInfo {
            name: g.name.clone(),
            description: g.description.clone(),
            request_type: Some("grpc".into()),
            seq: g.seq,
            tags: g.tags.clone(),
        },
        grpc: OcGrpcRequestDetails {
            url: g.url.clone(),
            method: g.method.clone(),
            method_type: Some(g.method_type.as_str().to_string()),
            proto_file_path: g.proto_file_path.clone(),
            metadata: g
                .metadata
                .iter()
                .map(|m| OcGrpcMetadata {
                    name: m.key.clone(),
                    value: m.value.clone(),
                    description: m.description.clone(),
                    disabled: if m.enabled { None } else { Some(true) },
                })
                .collect(),
            message,
            auth: persisted_oc_auth(g.auth.clone()),
        },
        runtime,
        docs: g.docs.clone(),
    }
}
