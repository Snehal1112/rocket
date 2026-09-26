use crate::oc::*;
use rocket_collection::collection::Collection;
use rocket_collection::folder::{CollectionItem, Folder, OpaqueProtocolItem};
use rocket_collection::settings::{CollectionSettings, CollectionVariable, SandboxMode};
use rocket_shared::types::{Auth, Header};

use super::auth::persisted_oc_auth;
use super::request::{oc_http_request_to_request, request_to_oc_http_request};

/// Converts one parsed OpenCollection item into a domain tree item.
/// GraphQL, gRPC and WebSocket items become `OpaqueItem`s that hold their raw
/// YAML, so nothing is lost on load. Script files are not tree items in the
/// domain model, so they return `None`.
pub fn oc_item_to_collection_item(item: OcItem) -> Option<CollectionItem> {
    match item {
        OcItem::Http(req) => Some(CollectionItem::Request(Box::new(
            oc_http_request_to_request(req),
        ))),
        OcItem::Folder(f) => Some(CollectionItem::Folder(oc_folder_to_folder(f))),
        OcItem::ScriptFile(_) => None,
        OcItem::GraphQL(gql) => {
            let name = gql.info.name.clone();
            opaque_item("graphql", name, &OcItem::GraphQL(gql))
        }
        OcItem::Grpc(grpc) => {
            let name = grpc.info.name.clone();
            opaque_item("grpc", name, &OcItem::Grpc(grpc))
        }
        OcItem::WebSocket(ws) => {
            let name = ws.info.name.clone();
            opaque_item("websocket", name, &OcItem::WebSocket(ws))
        }
    }
}

/// Wraps a non-HTTP item as an opaque tree item holding its raw YAML.
fn opaque_item(protocol: &str, name: String, item: &OcItem) -> Option<CollectionItem> {
    serde_yaml::to_value(item).ok().map(|raw| {
        CollectionItem::OpaqueItem(OpaqueProtocolItem {
            protocol: protocol.into(),
            name,
            raw,
        })
    })
}

/// Convert an OC folder to a domain Folder, recursively converting items.
pub fn oc_folder_to_folder(oc: OcFolder) -> Folder {
    let name = oc.info.name;
    let uid = oc.info.uid;
    let items = oc
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(oc_item_to_collection_item)
        .collect();

    Folder {
        uid: uid.unwrap_or_else(|| {
            tracing::warn!(folder = %name, "folder.yml is missing uid field; using empty uid");
            String::new()
        }),
        name,
        dir_name: None,
        items,
    }
}

/// Convert a domain Folder back to an OC folder.
// Used only by tests: this converts the bundled layout, which Rocket does not write.
#[allow(dead_code)]
pub fn folder_to_oc_folder(folder: Folder) -> OcFolder {
    let items: Vec<OcItem> = folder
        .items
        .into_iter()
        .filter_map(|item| match item {
            CollectionItem::Request(req) => Some(OcItem::Http(request_to_oc_http_request(&req))),
            CollectionItem::Folder(f) => Some(OcItem::Folder(folder_to_oc_folder(f))),
            // Summary items carry no body/auth — they must not be serialized to disk.
            CollectionItem::Summary(_) => None,
            CollectionItem::OpaqueItem(opaque) => Some(
                serde_yaml::from_value::<OcItem>(opaque.raw.clone()).unwrap_or_else(|_| {
                    OcItem::Folder(OcFolder {
                        info: OcFolderInfo {
                            name: opaque.name,
                            uid: None,
                            description: None,
                            folder_type: Some("folder".into()),
                            seq: None,
                            tags: Vec::new(),
                        },
                        items: None,
                        request: None,
                        docs: None,
                    })
                }),
            ),
        })
        .collect();

    OcFolder {
        info: OcFolderInfo {
            name: folder.name,
            uid: None,
            description: None,
            folder_type: Some("folder".into()),
            seq: None,
            tags: Vec::new(),
        },
        items: if items.is_empty() { None } else { Some(items) },
        request: None,
        docs: None,
    }
}

/// Convert an OC collection to a domain Collection.
// Used only by tests: this converts the bundled layout, which Rocket does not write.
#[allow(dead_code)]
pub fn oc_collection_to_collection(oc: OcCollection) -> Collection {
    let name = oc
        .info
        .as_ref()
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "Untitled".into());
    let collection_uid = oc.uid.clone();

    // Convert items into the root folder.
    let items = oc
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(oc_item_to_collection_item)
        .collect();

    let root = Folder {
        uid: collection_uid.unwrap_or_else(|| {
            tracing::warn!(collection = %name, "opencollection.yml is missing uid field; using empty uid");
            String::new()
        }),
        name: name.clone(),
        dir_name: None,
        items,
    };

    // Convert request defaults to collection settings.
    let settings = if let Some(defaults) = oc.request {
        CollectionSettings {
            docs: oc.docs,
            auth: defaults.auth.map(Auth::from),
            headers: defaults
                .headers
                .unwrap_or_default()
                .into_iter()
                .map(Header::from)
                .collect(),
            variables: defaults
                .variables
                .unwrap_or_default()
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            sandbox_mode: SandboxMode::Safe,
        }
    } else {
        CollectionSettings {
            docs: oc.docs,
            ..CollectionSettings::default()
        }
    };

    Collection {
        name,
        root,
        settings,
    }
}

/// Convert a domain Collection back to an OC collection.
// Used only by tests: this converts the bundled layout, which Rocket does not write.
#[allow(dead_code)]
pub fn collection_to_oc_collection(col: Collection) -> OcCollection {
    let items: Vec<OcItem> = col
        .root
        .items
        .into_iter()
        .filter_map(|item| match item {
            CollectionItem::Request(req) => Some(OcItem::Http(request_to_oc_http_request(&req))),
            CollectionItem::Folder(f) => Some(OcItem::Folder(folder_to_oc_folder(f))),
            // Summary items carry no body/auth — they must not be serialized to disk.
            CollectionItem::Summary(_) => None,
            CollectionItem::OpaqueItem(opaque) => Some(
                serde_yaml::from_value::<OcItem>(opaque.raw.clone()).unwrap_or_else(|_| {
                    OcItem::Folder(OcFolder {
                        info: OcFolderInfo {
                            name: opaque.name,
                            uid: None,
                            description: None,
                            folder_type: Some("folder".into()),
                            seq: None,
                            tags: Vec::new(),
                        },
                        items: None,
                        request: None,
                        docs: None,
                    })
                }),
            ),
        })
        .collect();

    let auth = col.settings.auth.and_then(persisted_oc_auth);
    let request = {
        let has_defaults = !col.settings.headers.is_empty()
            || auth.is_some()
            || !col.settings.variables.is_empty();
        if has_defaults {
            Some(OcRequestDefaults {
                headers: if col.settings.headers.is_empty() {
                    None
                } else {
                    Some(
                        col.settings
                            .headers
                            .into_iter()
                            .map(OcHttpRequestHeader::from)
                            .collect(),
                    )
                },
                metadata: None,
                auth,
                variables: if col.settings.variables.is_empty() {
                    None
                } else {
                    Some(
                        col.settings
                            .variables
                            .into_iter()
                            .map(OcVariable::from)
                            .collect(),
                    )
                },
                scripts: None,
                settings: None,
            })
        } else {
            None
        }
    };

    OcCollection {
        opencollection: Some("1.0.0".into()),
        uid: None,
        info: Some(OcInfo {
            name: col.name,
            summary: None,
            version: None,
            authors: None,
        }),
        config: None,
        items: if items.is_empty() { None } else { Some(items) },
        request,
        docs: col.settings.docs,
        bundled: None,
        extensions: None,
    }
}
