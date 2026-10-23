//! Collection export use cases.

use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_export::{
    bruno_zip, postman_collection, slug, CollectionExportTree, CollectionFileSource, ExportFolder,
    ExportItem, ExportReport, PostmanOptions,
};
use rocket_shared::error::{DomainError, DomainResult};

/// The bytes of an export, the suggested file name and the report.
#[derive(Debug, Clone)]
pub struct ExportOutput {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub report: ExportReport,
}

pub struct CollectionExportService {
    files: Box<dyn CollectionFileSource>,
    collections: Option<Box<dyn CollectionRepository>>,
}

fn safe_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if cleaned.trim().is_empty() {
        "collection".to_string()
    } else {
        cleaned
    }
}

fn build_folder(
    repo: &dyn CollectionRepository,
    collection: &str,
    folder: &Folder,
    path: &str,
) -> DomainResult<ExportFolder> {
    let settings = repo.get_folder_settings(collection, path)?;
    let join = |name: &str| {
        if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        }
    };
    let mut items = Vec::new();
    for item in &folder.items {
        match item {
            CollectionItem::Request(r) => items.push(ExportItem::Request(r.clone())),
            CollectionItem::GraphQl(g) => items.push(ExportItem::GraphQl(g.clone())),
            CollectionItem::Folder(f) => {
                let dir = f.dir_name.as_deref().unwrap_or(&f.name);
                items.push(ExportItem::Folder(build_folder(
                    repo,
                    collection,
                    f,
                    &join(dir),
                )?));
            }
            CollectionItem::WebSocket(w) => items.push(ExportItem::Skipped {
                path: join(&w.name),
                kind: "websocket".into(),
            }),
            CollectionItem::Grpc(g) => items.push(ExportItem::Skipped {
                path: join(&g.name),
                kind: "grpc".into(),
            }),
            CollectionItem::OpaqueItem(o) => items.push(ExportItem::Skipped {
                path: join(&o.name),
                kind: format!("opaque ({})", o.protocol),
            }),
            CollectionItem::ScriptFile(_) => {}
            CollectionItem::Summary(_) => {
                return Err(DomainError::Internal(
                    "collection export read a request summary".into(),
                ));
            }
        }
    }
    Ok(ExportFolder {
        name: folder.name.clone(),
        settings,
        items,
    })
}

impl CollectionExportService {
    pub fn new(files: Box<dyn CollectionFileSource>) -> Self {
        Self {
            files,
            collections: None,
        }
    }

    /// Adds the typed collection reader that the Postman export needs.
    pub fn with_collections(mut self, repo: Box<dyn CollectionRepository>) -> Self {
        self.collections = Some(repo);
        self
    }

    /// Reads the whole collection into a typed tree. A failing read fails the export.
    pub fn build_tree(&self, collection: &str) -> DomainResult<CollectionExportTree> {
        let repo = self.collections.as_deref().ok_or_else(|| {
            DomainError::Internal("collection export has no collection repository".into())
        })?;
        let loaded = repo.get_strict(collection)?;
        let settings = repo.get_settings(collection)?;
        let root = build_folder(repo, collection, &loaded.root, "")?;
        Ok(CollectionExportTree {
            name: loaded.name,
            settings,
            root,
        })
    }

    pub fn export_postman(&self, collection: &str) -> DomainResult<ExportOutput> {
        let tree = self.build_tree(collection)?;
        let (value, report) = postman_collection(&tree, &PostmanOptions::default())?;
        let bytes = serde_json::to_vec_pretty(&value)
            .map_err(|e| DomainError::Internal(format!("could not write JSON: {e}")))?;
        Ok(ExportOutput {
            bytes,
            file_name: format!("{}.postman_collection.json", slug(&tree.name)),
            report,
        })
    }

    pub fn export_bruno(
        &self,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<ExportOutput> {
        let files = self.files.list_files(collection)?;
        let stem = safe_file_stem(collection);
        let (bytes, report) = bruno_zip(files, &stem, include_environments)?;
        Ok(ExportOutput {
            bytes,
            file_name: format!("{stem}.zip"),
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_export::SourceFile;

    struct OneFile;

    impl CollectionFileSource for OneFile {
        fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
            Ok(vec![SourceFile {
                rel_path: "opencollection.yml".to_string(),
                bytes: b"info:\n  name: my-api\n".to_vec(),
            }])
        }
    }

    struct EmptySource;

    impl CollectionFileSource for EmptySource {
        fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
            Ok(vec![])
        }
    }

    use crate::test_doubles::NullCollectionRepo;
    use rocket_collection::{
        Collection, GrpcRequest, OpaqueProtocolItem, Request, RequestKind, RequestSummary,
        ScriptFileItem, WebSocketRequest,
    };
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    fn service(repo: NullCollectionRepo) -> CollectionExportService {
        CollectionExportService::new(Box::new(EmptySource)).with_collections(Box::new(repo))
    }

    #[test]
    fn build_tree_fails_when_a_request_cannot_be_read() {
        let repo = NullCollectionRepo::strict()
            .on_get_strict(|_| Err(DomainError::Internal("bad request file".into())));
        let err = service(repo)
            .build_tree("demo")
            .expect_err("strict read error surfaces");
        assert!(err.to_string().contains("bad request file"));
    }

    #[test]
    fn build_tree_reads_folder_settings_per_folder() {
        let mut inner = Folder::new("inner");
        inner.dir_name = Some("b".into());
        let mut users = Folder::new("users");
        users.dir_name = Some("users-dir".into());
        users.items.push(CollectionItem::Folder(inner));
        let mut c = Collection::new("demo");
        c.root.items.push(CollectionItem::Folder(users));
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&calls);
        let repo = NullCollectionRepo::new()
            .on_get_strict(move |_| Ok(c.clone()))
            .on_get_folder_settings(move |_, p| {
                seen.lock().expect("lock").push(p.to_string());
                Ok(Default::default())
            });
        service(repo).build_tree("demo").expect("tree");
        let got = calls.lock().expect("lock").clone();
        assert_eq!(got, vec!["", "users-dir", "users-dir/b"]);
    }

    #[test]
    fn build_tree_maps_item_kinds() {
        let mut c = Collection::new("demo");
        c.root.items = vec![
            CollectionItem::Request(Box::new(Request::new("r", HttpMethod::Get, "/"))),
            CollectionItem::WebSocket(Box::new(WebSocketRequest::new("ws", "ws://x"))),
            CollectionItem::Grpc(Box::new(GrpcRequest::new("g", "x:1"))),
            CollectionItem::OpaqueItem(OpaqueProtocolItem {
                protocol: "sse".into(),
                name: "o".into(),
                raw: serde_yaml::Value::Null,
            }),
            CollectionItem::ScriptFile(ScriptFileItem::new("u.js")),
        ];
        let c2 = c.clone();
        let tree = service(NullCollectionRepo::new().on_get_strict(move |_| Ok(c2.clone())))
            .build_tree("demo")
            .expect("tree");
        assert_eq!(tree.root.items.len(), 4);
        assert!(matches!(tree.root.items[0], ExportItem::Request(_)));
        assert!(
            matches!(&tree.root.items[1], ExportItem::Skipped { kind, .. } if kind == "websocket")
        );
        assert!(matches!(&tree.root.items[2], ExportItem::Skipped { kind, .. } if kind == "grpc"));
        assert!(matches!(&tree.root.items[3], ExportItem::Skipped { .. }));

        c.root.items = vec![CollectionItem::Summary(RequestSummary {
            uid: "u".into(),
            name: "s".into(),
            method: "GET".into(),
            url: "/".into(),
            file_name: None,
            kind: RequestKind::Http,
        })];
        let err = service(NullCollectionRepo::new().on_get_strict(move |_| Ok(c.clone())))
            .build_tree("demo")
            .expect_err("summary is an error");
        assert!(matches!(err, DomainError::Internal(_)));
    }

    #[test]
    fn export_postman_names_the_file() {
        let repo = NullCollectionRepo::new().on_get_strict(|_| Ok(Collection::new("My API")));
        let out = service(repo).export_postman("My API").expect("exports");
        assert_eq!(out.file_name, "my-api.postman_collection.json");
        let v: serde_json::Value = serde_json::from_slice(&out.bytes).expect("json");
        assert_eq!(v["info"]["name"], "My API");
    }

    #[test]
    fn export_postman_without_repository_is_an_error() {
        let svc = CollectionExportService::new(Box::new(EmptySource));
        assert!(matches!(
            svc.export_postman("x"),
            Err(DomainError::Internal(_))
        ));
    }

    #[test]
    fn export_bruno_names_the_file() {
        let service = CollectionExportService::new(Box::new(OneFile));
        let out = service.export_bruno("my-api", true).expect("exports");
        assert_eq!(out.file_name, "my-api.zip");
        assert!(!out.bytes.is_empty());
    }
}
