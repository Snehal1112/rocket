//! Collection export use cases.

use std::collections::HashSet;

use chrono::SecondsFormat;
use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_environment::{flatten_with_repo, EnvironmentRepositoryFactory, Variable};
use rocket_export::{
    bruno_zip, environment_notes, postman_bundle, postman_collection, postman_environment, slug,
    CollectionExportTree, CollectionFileSource, ExportAction, ExportFolder, ExportItem,
    ExportNote, ExportReport, PostmanOptions,
};
use rocket_shared::error::{DomainError, DomainResult};

/// Choices of a Postman export.
#[derive(Debug, Clone, Default)]
pub struct PostmanExportOptions {
    /// Keep script text as written instead of translating `rok.*` calls.
    pub preserve_scripts: bool,
    /// Add the collection's environments, which turns the result into a ZIP.
    pub include_environments: bool,
}

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
    environments: Option<Box<dyn EnvironmentRepositoryFactory>>,
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

/// `<slug>.postman_environment.json`, with ` -2`, `-3` ... when two names share a slug.
fn unique_env_file(used: &mut HashSet<String>, stem: &str) -> String {
    let mut candidate = stem.to_string();
    let mut n = 2;
    while used.contains(&candidate) {
        candidate = format!("{stem}-{n}");
        n += 1;
    }
    used.insert(candidate.clone());
    format!("{candidate}.postman_environment.json")
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
            environments: None,
        }
    }

    /// Adds the environment reader that `include_environments` needs.
    pub fn with_environments(mut self, factory: Box<dyn EnvironmentRepositoryFactory>) -> Self {
        self.environments = Some(factory);
        self
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

    /// Exports a Postman collection. With environments included and at least one
    /// present, the result is a ZIP of the collection file and the environment files.
    pub fn export_postman(
        &self,
        collection: &str,
        opts: &PostmanExportOptions,
    ) -> DomainResult<ExportOutput> {
        let tree = self.build_tree(collection)?;
        let postman_opts = PostmanOptions {
            preserve_scripts: opts.preserve_scripts,
        };
        let (value, mut report) = postman_collection(&tree, &postman_opts)?;
        let bytes = serde_json::to_vec_pretty(&value)
            .map_err(|e| DomainError::Internal(format!("could not write JSON: {e}")))?;
        let stem = slug(&tree.name);
        let file_name = format!("{stem}.postman_collection.json");
        let envs = if opts.include_environments {
            self.postman_environment_files(collection, &mut report)?
        } else {
            Vec::new()
        };
        if envs.is_empty() {
            return Ok(ExportOutput {
                bytes,
                file_name,
                report,
            });
        }
        Ok(ExportOutput {
            bytes: postman_bundle((&file_name, &bytes), envs)?,
            file_name: format!("{stem}-postman.zip"),
            report,
        })
    }

    /// One `(file name, bytes)` per environment of the collection, `extends` flattened.
    /// A read or keychain error fails the export. Secret values are cleared at once.
    fn postman_environment_files(
        &self,
        collection: &str,
        report: &mut ExportReport,
    ) -> DomainResult<Vec<(String, Vec<u8>)>> {
        let factory = self.environments.as_deref().ok_or_else(|| {
            DomainError::Internal("collection export has no environment repository".into())
        })?;
        let repo = factory.for_collection(collection);
        let exported_at = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut used = HashSet::new();
        let mut files = Vec::new();
        for env in repo.list_strict()? {
            let flat = flatten_with_repo(&env, repo.as_ref())?;
            let path = format!("environments/{}", env.name);
            for warning in &flat.warnings {
                report.items.push(ExportNote {
                    path: path.clone(),
                    feature: "extends".into(),
                    action: ExportAction::Skipped,
                    detail: Some(warning.clone()),
                });
            }
            let mut vars: Vec<Variable> = flat.variables.into_iter().map(|f| f.variable).collect();
            for v in vars.iter_mut().filter(|v| v.secret) {
                v.value.clear();
            }
            report.items.extend(environment_notes(&env.name, &vars));
            let doc = postman_environment(&env.name, &vars, &exported_at, env!("CARGO_PKG_VERSION"));
            let bytes = serde_json::to_vec_pretty(&doc)
                .map_err(|e| DomainError::Internal(format!("could not write JSON: {e}")))?;
            files.push((unique_env_file(&mut used, &slug(&env.name)), bytes));
        }
        Ok(files)
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
    use rocket_environment::{
        Environment, EnvironmentRepository, EnvironmentRepositoryFactory, Variable,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
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
            example_names: Vec::new(),
        })];
        let err = service(NullCollectionRepo::new().on_get_strict(move |_| Ok(c.clone())))
            .build_tree("demo")
            .expect_err("summary is an error");
        assert!(matches!(err, DomainError::Internal(_)));
    }

    #[test]
    fn export_postman_names_the_file() {
        let repo = NullCollectionRepo::new().on_get_strict(|_| Ok(Collection::new("My API")));
        let out = service(repo)
            .export_postman("My API", &PostmanExportOptions::default())
            .expect("exports");
        assert_eq!(out.file_name, "my-api.postman_collection.json");
        let v: serde_json::Value = serde_json::from_slice(&out.bytes).expect("json");
        assert_eq!(v["info"]["name"], "My API");
    }

    #[test]
    fn export_postman_without_repository_is_an_error() {
        let svc = CollectionExportService::new(Box::new(EmptySource));
        assert!(matches!(
            svc.export_postman("x", &PostmanExportOptions::default()),
            Err(DomainError::Internal(_))
        ));
    }

    #[derive(Clone, Default)]
    struct EnvState {
        envs: Vec<Environment>,
        list_error: Option<String>,
        strict_calls: Arc<AtomicUsize>,
    }

    struct MapEnvRepo(EnvState);

    impl EnvironmentRepository for MapEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.envs.clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .envs
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn list_strict(&self) -> DomainResult<Vec<Environment>> {
            self.0.strict_calls.fetch_add(1, Ordering::SeqCst);
            match &self.0.list_error {
                Some(message) => Err(DomainError::Internal(message.clone())),
                None => Ok(self.0.envs.clone()),
            }
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct MapEnvFactory(EnvState);

    impl EnvironmentRepositoryFactory for MapEnvFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(MapEnvRepo(self.0.clone()))
        }
    }

    fn with_envs(state: EnvState) -> CollectionExportService {
        let repo = NullCollectionRepo::new().on_get_strict(|_| Ok(Collection::new("My API")));
        service(repo).with_environments(Box::new(MapEnvFactory(state)))
    }

    fn env_options() -> PostmanExportOptions {
        PostmanExportOptions {
            preserve_scripts: false,
            include_environments: true,
        }
    }

    fn env_with(name: &str, extends: Option<&str>, vars: Vec<Variable>) -> Environment {
        let mut e = Environment::new(name);
        e.extends = extends.map(str::to_string);
        e.variables = vars;
        e
    }

    fn zip_entry(bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        let mut file = archive.by_name(name).expect("entry");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut file, &mut text).expect("utf8");
        text
    }

    #[test]
    fn no_environments_gives_plain_json() {
        let out = with_envs(EnvState::default())
            .export_postman("My API", &env_options())
            .expect("exports");
        assert!(out.file_name.ends_with(".postman_collection.json"));
        serde_json::from_slice::<serde_json::Value>(&out.bytes).expect("plain json");
    }

    #[test]
    fn include_environments_off_gives_plain_json_and_reads_nothing() {
        let state = EnvState {
            envs: vec![env_with("a", None, vec![]), env_with("b", None, vec![])],
            ..Default::default()
        };
        let calls = Arc::clone(&state.strict_calls);
        let out = with_envs(state)
            .export_postman("My API", &PostmanExportOptions::default())
            .expect("exports");
        assert_eq!(out.file_name, "my-api.postman_collection.json");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn environments_are_bundled_in_a_zip() {
        let state = EnvState {
            envs: vec![env_with("Prod EU", None, vec![Variable::new("HOST", "x.test")])],
            ..Default::default()
        };
        let out = with_envs(state)
            .export_postman("My API", &env_options())
            .expect("exports");
        assert_eq!(out.file_name, "my-api-postman.zip");
        let env: serde_json::Value = serde_json::from_str(&zip_entry(
            &out.bytes,
            "environments/prod-eu.postman_environment.json",
        ))
        .expect("json");
        assert_eq!(env["name"], "Prod EU");
        assert_eq!(env["values"][0]["key"], "HOST");
        assert_eq!(env["_postman_exported_using"], format!("Rocket/{}", env!("CARGO_PKG_VERSION")));
        zip_entry(&out.bytes, "my-api.postman_collection.json");
    }

    #[test]
    fn environment_extends_is_flattened() {
        let state = EnvState {
            envs: vec![
                env_with(
                    "base",
                    None,
                    vec![Variable::new("HOST", "base.test"), Variable::new("A", "1")],
                ),
                env_with("child", Some("base"), vec![Variable::new("HOST", "child.test")]),
            ],
            ..Default::default()
        };
        let out = with_envs(state)
            .export_postman("My API", &env_options())
            .expect("exports");
        let child: serde_json::Value = serde_json::from_str(&zip_entry(
            &out.bytes,
            "environments/child.postman_environment.json",
        ))
        .expect("json");
        let values = child["values"].as_array().expect("values");
        let get = |k: &str| values.iter().find(|v| v["key"] == k).expect("key")["value"].clone();
        assert_eq!(values.len(), 2);
        assert_eq!(get("HOST"), "child.test");
        assert_eq!(get("A"), "1");
    }

    #[test]
    fn environment_read_error_fails_export() {
        let state = EnvState {
            list_error: Some("keychain locked".into()),
            ..Default::default()
        };
        let err = with_envs(state)
            .export_postman("My API", &env_options())
            .expect_err("read error surfaces");
        assert!(err.to_string().contains("keychain locked"));
    }

    #[test]
    fn environments_report_blanked_secrets_and_never_export_their_values() {
        let mut one = Variable::new("TOKEN", "hydrated-one");
        one.secret = true;
        let mut two = Variable::new("KEY", "hydrated-two");
        two.secret = true;
        let state = EnvState {
            envs: vec![env_with("prod", None, vec![one, two])],
            ..Default::default()
        };
        let out = with_envs(state)
            .export_postman("My API", &env_options())
            .expect("exports");
        let blanked: Vec<_> = out
            .report
            .items
            .iter()
            .filter(|n| n.action == ExportAction::Blanked && n.path == "environments/prod")
            .collect();
        assert_eq!(blanked.len(), 2);
        let env = zip_entry(&out.bytes, "environments/prod.postman_environment.json");
        assert!(!env.contains("hydrated"));
    }

    #[test]
    fn include_environments_without_a_factory_is_an_error() {
        let repo = NullCollectionRepo::new().on_get_strict(|_| Ok(Collection::new("My API")));
        assert!(matches!(
            service(repo).export_postman("My API", &env_options()),
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
