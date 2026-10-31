use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rocket_collection::CollectionRepository;
use rocket_environment::EnvironmentRepository;

use crate::bru;
use crate::converter::{environment as env_converter, request as req_converter};
use crate::error::{ImportError, ImportResult};
use crate::report::{ImportReport, SkipReason, SkippedItem};

/// Which generation of Bruno format a directory uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrunoFormat {
    /// Bruno 3.0+ — uses workspace.yml / opencollection.yml (OpenCollection-compatible).
    Modern,
    /// Bruno 2.x — uses bruno.json markers everywhere.
    Legacy,
}

/// Returns the Bruno format if `path` is a workspace root, or `None`.
pub(crate) fn detect_workspace(path: &Path) -> Option<BrunoFormat> {
    if path.join("workspace.yml").exists() {
        Some(BrunoFormat::Modern)
    } else if path.join("bruno.json").exists() {
        Some(BrunoFormat::Legacy)
    } else {
        None
    }
}

/// Returns the Bruno format if `path` is a collection root, or `None`.
pub(crate) fn detect_collection(path: &Path) -> Option<BrunoFormat> {
    if path.join("opencollection.yml").exists() {
        Some(BrunoFormat::Modern)
    } else if path.join("bruno.json").exists() {
        Some(BrunoFormat::Legacy)
    } else {
        None
    }
}

/// Makes an imported environment name safe to use as a file name.
/// A name that had to change gets a numeric suffix if it collides with an existing one.
fn unique_env_name(repo: &dyn EnvironmentRepository, raw: &str) -> String {
    let clean = rocket_environment::Environment::sanitize_name(raw);
    if clean == raw {
        return clean;
    }
    let mut candidate = clean.clone();
    let mut n = 2;
    while repo.get(&candidate).is_ok() {
        candidate = format!("{clean} ({n})");
        n += 1;
    }
    candidate
}

/// Orchestrates the full Bruno import pipeline.
/// Creates an `EnvironmentRepository` scoped to a single collection's `environments/` dir.
pub trait EnvironmentRepositoryFactory: Send + Sync {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository>;
}

pub struct ImportService {
    workspace_path: PathBuf,
    collection_repo: Box<dyn CollectionRepository>,
    env_factory: Box<dyn EnvironmentRepositoryFactory>,
}

impl ImportService {
    pub fn new(
        workspace_path: PathBuf,
        collection_repo: Box<dyn CollectionRepository>,
        env_factory: Box<dyn EnvironmentRepositoryFactory>,
    ) -> Self {
        Self {
            workspace_path,
            collection_repo,
            env_factory,
        }
    }

    /// Test-only constructor — wires up `FsCollectionRepo` and `FsEnvironmentRepo` directly.
    #[cfg(test)]
    pub fn new_with_workspace_path(path: &Path) -> Self {
        use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};

        struct FsFactory(PathBuf);
        impl EnvironmentRepositoryFactory for FsFactory {
            fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
                Box::new(FsEnvironmentRepo::new(
                    self.0
                        .join("collections")
                        .join(collection_name)
                        .join("environments"),
                ))
            }
        }

        let workspace_path = path.to_path_buf();
        let collection_repo = Box::new(FsCollectionRepo::new_standalone(
            workspace_path.join("collections"),
        ));
        let env_factory = Box::new(FsFactory(workspace_path.clone()));
        Self {
            workspace_path,
            collection_repo,
            env_factory,
        }
    }

    /// Import a single Bruno collection directory into the given workspace.
    pub fn import_collection(&self, path: &Path, workspace_id: &str) -> ImportResult<ImportReport> {
        self.import_collection_with_name(path, workspace_id, None)
    }

    /// Import a collection, optionally overriding the derived name.
    fn import_collection_with_name(
        &self,
        path: &Path,
        workspace_id: &str,
        name_hint: Option<&str>,
    ) -> ImportResult<ImportReport> {
        match detect_collection(path) {
            None => Err(ImportError::NotABrunoDirectory(path.to_path_buf())),
            Some(BrunoFormat::Modern) => {
                self.import_modern_collection(path, workspace_id, name_hint)
            }
            Some(BrunoFormat::Legacy) => {
                self.import_legacy_collection(path, workspace_id, name_hint)
            }
        }
    }

    /// Import a legacy (Bruno 2.x) collection directory.
    fn import_legacy_collection(
        &self,
        path: &Path,
        _workspace_id: &str,
        name_hint: Option<&str>,
    ) -> ImportResult<ImportReport> {
        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };

        let col_name = name_hint
            .map(|n| n.to_string())
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "imported".into());

        let resolved_name = self.resolve_collection_name(&col_name)?;
        self.collection_repo
            .create(&resolved_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(resolved_name.clone());

        // Walk request files. The set holds the output paths this run has taken.
        let mut used = HashSet::new();
        self.walk_requests(
            path,
            path,
            &resolved_name,
            self.collection_repo.as_ref(),
            &mut report,
            &mut used,
        )?;

        // Import environments.
        let env_dir = path.join("environments");
        if env_dir.is_dir() {
            self.import_environments(&env_dir, &resolved_name, &mut report)?;
        }

        Ok(report)
    }

    /// Import a Bruno workspace directory (containing multiple collection dirs).
    pub fn import_workspace(
        &self,
        path: &Path,
        create_new_workspace: bool,
        target_workspace_id: Option<&str>,
    ) -> ImportResult<ImportReport> {
        self.import_workspace_with_name(path, create_new_workspace, target_workspace_id, None)
    }

    fn import_workspace_with_name(
        &self,
        path: &Path,
        create_new_workspace: bool,
        target_workspace_id: Option<&str>,
        name_hint: Option<&str>,
    ) -> ImportResult<ImportReport> {
        if detect_workspace(path).is_none() {
            return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
        }

        let mut combined = ImportReport {
            detected_type: "workspace".to_string(),
            ..Default::default()
        };

        if create_new_workspace {
            let ws_name = name_hint
                .map(|n| n.to_string())
                .or_else(|| path.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "imported-workspace".into());
            combined.created_workspace = Some(ws_name);
        }

        // Bruno workspaces may nest collections under a `collections/` directory.
        let scan_root = if path.join("collections").is_dir() {
            path.join("collections")
        } else {
            path.to_path_buf()
        };

        for entry in std::fs::read_dir(&scan_root)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() && detect_collection(&p).is_some() {
                let id = target_workspace_id.unwrap_or("default");
                match self.import_collection(&p, id) {
                    Ok(r) => {
                        combined.total_files += r.total_files;
                        combined.imported += r.imported;
                        combined.skipped.extend(r.skipped);
                        combined.created_collections.extend(r.created_collections);
                    }
                    Err(e) => {
                        combined.skipped.push(SkippedItem {
                            path: p.to_string_lossy().to_string(),
                            reason: SkipReason::ParseError(e.to_string()),
                        });
                    }
                }
            }
        }

        Ok(combined)
    }

    /// Auto-detect whether `path` is a workspace or collection and import accordingly.
    ///
    /// Modern markers are unambiguous (`opencollection.yml` vs `workspace.yml`).
    /// For legacy Bruno 2.x, both workspace and collection roots use `bruno.json`, so
    /// collection detection runs first — a dir with `.bru` request files is treated as
    /// a collection before trying workspace detection.
    /// Returns `NotABrunoDirectory` if neither marker is found.
    ///
    /// `name_hint` overrides the directory-derived name (useful for ZIP imports where the
    /// extracted temp path has no meaningful name).
    #[tracing::instrument(name = "bruno_import", skip(self), fields(source_path = %path.display()))]
    pub fn import_auto(
        &self,
        path: &Path,
        workspace_id: &str,
        create_new_workspace: bool,
    ) -> ImportResult<ImportReport> {
        let report = self.import_auto_with_name(path, workspace_id, create_new_workspace, None)?;
        tracing::info!(
            total_files = report.total_files,
            imported = report.imported,
            skipped = report.skipped.len(),
            detected_type = %report.detected_type,
            "import completed"
        );
        Ok(report)
    }

    fn import_auto_with_name(
        &self,
        path: &Path,
        workspace_id: &str,
        create_new_workspace: bool,
        name_hint: Option<&str>,
    ) -> ImportResult<ImportReport> {
        if detect_collection(path).is_some() {
            self.import_collection_with_name(path, workspace_id, name_hint)
        } else if detect_workspace(path).is_some() {
            self.import_workspace_with_name(
                path,
                create_new_workspace,
                Some(workspace_id),
                name_hint,
            )
        } else {
            Err(ImportError::NotABrunoDirectory(path.to_path_buf()))
        }
    }

    /// Extract a Bruno ZIP to a temp directory and call `import_auto` on the inner path.
    ///
    /// The `TempDir` is held for the duration of the import and cleaned up automatically
    /// when this method returns. The collection/workspace name is derived from the ZIP
    /// filename so flat-root archives get a meaningful name instead of a temp path.
    #[tracing::instrument(name = "bruno_import_zip", skip(self), fields(source_path = %zip_path.display()))]
    pub fn import_auto_from_zip(
        &self,
        zip_path: &Path,
        workspace_id: &str,
        create_new_workspace: bool,
    ) -> ImportResult<ImportReport> {
        let (_temp, inner) = crate::bru::zip_extractor::extract_to_temp(zip_path)?;

        // Use ZIP file stem as name hint (e.g. "Lockstep-Inbox.zip" -> "Lockstep-Inbox").
        let zip_name = zip_path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string());
        let name_hint = zip_name.as_deref();

        let report =
            self.import_auto_with_name(&inner, workspace_id, create_new_workspace, name_hint)?;
        tracing::info!(
            total_files = report.total_files,
            imported = report.imported,
            skipped = report.skipped.len(),
            detected_type = %report.detected_type,
            "zip import completed"
        );
        Ok(report)
    }

    fn walk_requests(
        &self,
        root: &Path,
        dir: &Path,
        collection_name: &str,
        repo: &dyn CollectionRepository,
        report: &mut ImportReport,
        used: &mut HashSet<String>,
    ) -> ImportResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();

            if p.is_dir() {
                // Skip environments — handled separately.
                if p.file_name().is_some_and(|n| n == "environments") {
                    continue;
                }
                // Never import dependency trees.
                if p.file_name().is_some_and(|n| n == "node_modules") {
                    continue;
                }
                // Create subfolder metadata and recurse.
                let folder_rel = p.strip_prefix(root).unwrap_or(&p);
                let folder_path = folder_rel.to_string_lossy().to_string();
                let _ = repo.create_folder(collection_name, &folder_path);
                self.walk_requests(root, &p, collection_name, repo, report, used)?;
                continue;
            }

            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            // A gRPC request points at a `.proto` file by path, so the proto files
            // travel with the requests. Shared `.js` script files are loaded by scripts
            // with `require()`, so they travel too. Neither is a request or counted.
            if ext == "proto" || ext == "js" {
                let label = if ext == "js" { ".js" } else { ".proto" };
                let rel = p.strip_prefix(root).unwrap_or(&p);
                let dest = self
                    .workspace_path
                    .join("collections")
                    .join(collection_name)
                    .join(rel);
                copy_support_file(&p, &dest, rel, label, report);
                continue;
            }
            if !matches!(ext, "bru" | "yml" | "yaml") {
                continue;
            }
            // Skip Bruno metadata files.
            // Skip Bruno metadata files — not requests.
            if p.file_name().is_some_and(|n| {
                matches!(
                    n.to_str(),
                    Some("bruno.json" | "_order.yml" | "folder.bru" | "collection.bru")
                )
            }) {
                continue;
            }

            report.total_files += 1;
            let rel_path = p.strip_prefix(root).unwrap_or(&p);
            let rel_str = rel_path.to_string_lossy().to_string();

            match bru::parse_file(&p) {
                Err(e) => {
                    report.skipped.push(SkippedItem {
                        path: rel_str,
                        reason: SkipReason::ParseError(e.to_string()),
                    });
                }
                Ok(doc) => {
                    let (item_opt, skipped_reasons) = req_converter::convert_item(&doc);

                    for reason in skipped_reasons {
                        report.skipped.push(SkippedItem {
                            path: rel_str.clone(),
                            reason,
                        });
                    }

                    let out_path =
                        unique_import_path(used, &rel_path.with_extension("yml").to_string_lossy());
                    match item_opt {
                        Some(req_converter::Converted::Http(req)) => {
                            let _ = repo.save_request(collection_name, &out_path, &req);
                            report.imported += 1;
                        }
                        Some(req_converter::Converted::GraphQl(gql)) => {
                            match repo.save_graphql_request(collection_name, &out_path, &gql) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
                        Some(req_converter::Converted::Grpc(mut grpc)) => {
                            // Point the proto path at the copy inside the new collection.
                            if let Some(raw) = grpc.proto_file_path.clone() {
                                let bru_dir = p.parent().unwrap_or(root);
                                if let Some(rebased) = rebase_proto_path(root, bru_dir, &raw) {
                                    grpc.proto_file_path = Some(rebased);
                                }
                            }
                            match repo.save_grpc_request(collection_name, &out_path, &grpc) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
                        Some(req_converter::Converted::WebSocket(ws)) => {
                            match repo.save_websocket_request(collection_name, &out_path, &ws) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn import_environments(
        &self,
        env_dir: &Path,
        collection_name: &str,
        report: &mut ImportReport,
    ) -> ImportResult<()> {
        let env_repo = self.env_factory.make(collection_name);
        for entry in std::fs::read_dir(env_dir)? {
            let entry = entry?;
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "bru" | "yml" | "yaml") {
                continue;
            }

            let env_name = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "env".into());

            match bru::parse_env_file(&p) {
                Err(e) => {
                    report.skipped.push(SkippedItem {
                        path: p.to_string_lossy().to_string(),
                        reason: SkipReason::ParseError(e.to_string()),
                    });
                }
                Ok(doc) => {
                    let safe_name = unique_env_name(env_repo.as_ref(), &env_name);
                    let env = env_converter::convert(&safe_name, &doc);
                    if let Err(e) = env_repo.save(&env) {
                        report.skipped.push(SkippedItem {
                            path: p.to_string_lossy().to_string(),
                            reason: SkipReason::ParseError(e.to_string()),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve_collection_name(&self, name: &str) -> ImportResult<String> {
        let existing: std::collections::HashSet<String> = self
            .collection_repo
            .list()
            .map_err(ImportError::DomainError)?
            .into_iter()
            .map(|s| s.name)
            .collect();
        if !existing.contains(name) {
            return Ok(name.to_string());
        }
        let mut i = 1u32;
        loop {
            let candidate = format!("{name}-{i}");
            if !existing.contains(&candidate) {
                return Ok(candidate);
            }
            i += 1;
        }
    }

    /// Like `resolve_collection_name`, but the name is also safe on every platform: a Windows
    /// device name such as `CON`, or a name that differs from an existing collection only by
    /// letter case, gets the next free `-1`, `-2`, ... suffix.
    fn resolve_portable_collection_name(&self, wanted: &str) -> ImportResult<String> {
        let base = wsdl_path_segment(wanted);
        let existing: std::collections::HashSet<String> = self
            .collection_repo
            .list()
            .map_err(ImportError::DomainError)?
            .into_iter()
            .map(|s| s.name.to_lowercase())
            .collect();
        let mut candidate = base.clone();
        let mut i = 0u32;
        while existing.contains(&candidate.to_lowercase()) || is_windows_device_name(&candidate) {
            i += 1;
            candidate = format!("{base}-{i}");
        }
        Ok(candidate)
    }

    /// Import a Bruno 3.0+ (OpenCollection-compatible) collection by direct file copy.
    ///
    /// Skips parsing — modern Bruno files are already OpenCollection YAML.
    pub(crate) fn import_modern_collection(
        &self,
        src: &Path,
        _workspace_id: &str,
        name_hint: Option<&str>,
    ) -> ImportResult<ImportReport> {
        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };

        let col_name = name_hint
            .map(|n| n.to_string())
            .or_else(|| src.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "imported".into());

        let resolved_name = self.resolve_collection_name(&col_name)?;
        self.collection_repo
            .create(&resolved_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(resolved_name.clone());

        let dest_root = self.workspace_path.join("collections").join(&resolved_name);
        self.copy_collection_files(src, src, &dest_root, &mut report)?;
        self.report_files_that_cannot_open(&resolved_name, &dest_root, &mut report);

        Ok(report)
    }

    /// Reads every copied file back with Rocket's own readers. A file that fails is still on
    /// disk, but it is reported so the user knows it will not open.
    fn report_files_that_cannot_open(
        &self,
        collection: &str,
        dest_root: &Path,
        report: &mut ImportReport,
    ) {
        let mut failures: Vec<(String, String)> = Vec::new();
        if let Err(e) = self.collection_repo.get_settings(collection) {
            failures.push(("opencollection.yml".into(), e.to_string()));
        }
        if let Err(e) = self.env_factory.make(collection).list_strict() {
            failures.push(("environments".into(), e.to_string()));
        }
        let mut files = Vec::new();
        collect_yml_files(dest_root, dest_root, &mut files);
        files.sort();
        for rel in files {
            let in_environments = rel.starts_with("environments/");
            if in_environments || rel == "opencollection.yml" {
                continue;
            }
            let result = if rel == "folder.yml" || rel.ends_with("/folder.yml") {
                let dir = rel.trim_end_matches("folder.yml").trim_end_matches('/');
                self.collection_repo
                    .get_folder_settings(collection, dir)
                    .map(|_| ())
            } else {
                self.open_request(collection, &rel)
            };
            if let Err(e) = result {
                failures.push((rel, e.to_string()));
            }
        }
        for (path, why) in failures {
            report.skipped.push(SkippedItem {
                path,
                reason: SkipReason::ParseError(format!("copied, but Rocket cannot open it: {why}")),
            });
        }
    }

    fn open_request(&self, collection: &str, rel: &str) -> rocket_shared::error::DomainResult<()> {
        use rocket_collection::RequestKind;
        let repo = &self.collection_repo;
        match repo.request_kind(collection, rel)? {
            RequestKind::Http => repo.get_request(collection, rel).map(|_| ()),
            RequestKind::GraphQl => repo.get_graphql_request(collection, rel).map(|_| ()),
            RequestKind::Grpc => repo.get_grpc_request(collection, rel).map(|_| ()),
            RequestKind::WebSocket => repo.get_websocket_request(collection, rel).map(|_| ()),
        }
    }

    /// Import a Postman Collection JSON (v2.0 or v2.1) into the workspace.
    pub fn import_postman_collection(
        &self,
        json_path: &Path,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        use crate::converter::postman as pc;
        use crate::postman::parse_postman_json;

        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };

        let collection = parse_postman_json(json_path)?;
        let col_name = self.resolve_collection_name(&collection.info.name)?;

        self.collection_repo
            .create(&col_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(col_name.clone());

        let variables = pc::convert_collection_variables(&collection.variable);
        let auth = collection.auth.as_ref().and_then(|a| {
            if a.auth_type == "oauth2" {
                report.skipped.push(SkippedItem {
                    path: col_name.clone(),
                    reason: SkipReason::UnsupportedAuthType("oauth2".into()),
                });
                None
            } else {
                pc::convert_auth(a)
            }
        });
        if !variables.is_empty() || auth.is_some() {
            use rocket_collection::settings::CollectionSettings;
            let settings = CollectionSettings {
                auth,
                variables,
                ..Default::default()
            };
            self.collection_repo
                .save_settings(&col_name, &settings)
                .map_err(ImportError::DomainError)?;
        }

        for postman_env in &collection.environment {
            let repo = self.env_factory.make(&col_name);
            let safe_name = unique_env_name(repo.as_ref(), &postman_env.name);
            let mut env = rocket_environment::Environment::new(&safe_name);
            for v in &postman_env.values {
                let mut var = rocket_environment::Variable::new(&v.key, &v.value);
                var.enabled = v.enabled;
                env.set_variable(var);
            }
            repo.save(&env).map_err(ImportError::DomainError)?;
        }

        let mut used = HashSet::new();
        self.write_postman_items(&collection.item, &col_name, "", &mut report, &mut used)?;

        Ok(report)
    }

    fn write_postman_items(
        &self,
        items: &[crate::postman::ast::PostmanItem],
        col_name: &str,
        path_prefix: &str,
        report: &mut ImportReport,
        used: &mut HashSet<String>,
    ) -> ImportResult<()> {
        use crate::converter::postman as pc;
        use crate::postman::ast::PostmanItem;

        for item in items {
            match item {
                PostmanItem::Request(req_item) => {
                    report.total_files += 1;
                    let (req, mut skipped) = pc::convert_request_item(req_item);
                    report.skipped.append(&mut skipped);

                    let slug = sanitize_postman_filename(&req_item.name);
                    let request_path = if path_prefix.is_empty() {
                        slug
                    } else {
                        format!("{}/{}", path_prefix, slug)
                    };

                    let request_path = unique_import_path(used, &request_path);
                    self.collection_repo
                        .save_request(col_name, &request_path, &req)
                        .map_err(ImportError::DomainError)?;
                    report.imported += 1;
                }
                PostmanItem::Folder(folder) => {
                    let folder_slug = sanitize_postman_filename(&folder.name);
                    let folder_path = if path_prefix.is_empty() {
                        folder_slug
                    } else {
                        format!("{}/{}", path_prefix, folder_slug)
                    };

                    self.collection_repo
                        .create_folder(col_name, &folder_path)
                        .map_err(ImportError::DomainError)?;

                    self.write_postman_items(&folder.item, col_name, &folder_path, report, used)?;
                }
            }
        }
        Ok(())
    }

    /// Import a WSDL 1.1 file as a collection: one folder per service, one folder per
    /// port, one request per SOAP operation. Non-SOAP bindings and unreadable imports
    /// are reported, not fatal.
    pub fn import_wsdl(&self, path: &Path, _workspace_id: &str) -> ImportResult<ImportReport> {
        self.import_wsdl_with_limits(path, MAX_WSDL_REQUESTS, MAX_WSDL_BYTES)
    }

    /// Same as `import_wsdl` with explicit limits, so tests can use small values.
    ///
    /// Only creating the collection is fatal. A folder or request that cannot be
    /// written becomes a skipped item and the import goes on.
    fn import_wsdl_with_limits(
        &self,
        path: &Path,
        max_requests: usize,
        max_bytes: usize,
    ) -> ImportResult<ImportReport> {
        use crate::converter::wsdl as wc;

        let model = crate::wsdl::parse_wsdl_file(path)?;
        let file_label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "wsdl".to_string());
        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };
        for warning in &model.warnings {
            report.skipped.push(SkippedItem {
                path: file_label.clone(),
                reason: SkipReason::UnsupportedRequestType(warning.clone()),
            });
        }

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "wsdl".to_string());
        let col_name = self.resolve_collection_name(&stem)?;
        self.collection_repo
            .create(&col_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(col_name.clone());

        let total_ops: usize = model
            .services
            .iter()
            .flat_map(|s| &s.ports)
            .map(|p| p.operations.len())
            .sum();
        // Operations handled so far, written or skipped.
        let mut handled = 0usize;
        let mut bytes = 0usize;
        let mut limit_hit = false;

        let mut used_services = std::collections::HashSet::new();
        'services: for service in &model.services {
            let (service_dir, _) =
                unique_segment(&mut used_services, &service.name, SERVICE_RESERVED);
            let service_failure = self
                .collection_repo
                .create_folder(&col_name, &service_dir)
                .err()
                .map(|e| format!("service folder could not be created: {e}"));
            let mut used_ports = std::collections::HashSet::new();
            for port in &service.ports {
                let (port_seg, _) = unique_segment(&mut used_ports, &port.name, PORT_RESERVED);
                let port_dir = format!("{service_dir}/{port_seg}");
                let port_failure = match &service_failure {
                    Some(msg) => Some(msg.clone()),
                    None => self
                        .collection_repo
                        .create_folder(&col_name, &port_dir)
                        .err()
                        .map(|e| format!("port folder could not be created: {e}")),
                };
                if let Some(msg) = port_failure {
                    report.total_files += port.operations.len();
                    handled += port.operations.len();
                    report.skipped.push(SkippedItem {
                        path: port_dir.clone(),
                        reason: SkipReason::ParseError(msg),
                    });
                    continue;
                }
                let mut used = std::collections::HashSet::new();
                for (i, op) in port.operations.iter().enumerate() {
                    if handled >= max_requests || bytes >= max_bytes {
                        limit_hit = true;
                        break 'services;
                    }
                    handled += 1;
                    report.total_files += 1;
                    let mut req = wc::convert_operation(port, op, &model.schemas, (i + 1) as u32);
                    bytes += req
                        .body
                        .as_ref()
                        .and_then(|b| b.content.as_ref())
                        .map_or(0, |c| c.len());
                    let (slug, k) = unique_segment(&mut used, &op.name, REQUEST_RESERVED);
                    if k > 1 {
                        req.name = format!("{} ({k})", op.name);
                    }
                    let request_path = format!("{port_dir}/{slug}");
                    match self
                        .collection_repo
                        .save_request(&col_name, &request_path, &req)
                    {
                        Ok(_) => report.imported += 1,
                        Err(e) => report.skipped.push(SkippedItem {
                            path: request_path,
                            reason: SkipReason::ParseError(format!(
                                "request could not be written: {e}"
                            )),
                        }),
                    }
                }
            }
        }
        if limit_hit {
            let left = total_ops.saturating_sub(handled);
            report.skipped.push(SkippedItem {
                path: file_label,
                reason: SkipReason::UnsupportedRequestType(format!(
                    "request limit reached, {left} operations not imported"
                )),
            });
        }
        Ok(report)
    }

    /// The folder that holds this workspace's collections.
    pub(crate) fn collections_root(&self) -> PathBuf {
        self.workspace_path.join("collections")
    }

    /// Import an OpenAPI 3.0, 3.1 or Swagger 2.0 document (JSON or YAML text) as a new
    /// collection, grouped into folders by tag or by URL path.
    ///
    /// Only creating the collection is fatal. A folder or request that cannot be written
    /// becomes a skipped item and the import goes on. At most `MAX_OPENAPI_OPERATIONS`
    /// operations are written; the rest are reported.
    pub fn import_openapi_text(
        &self,
        text: &str,
        source_label: &str,
        group_by: crate::openapi::GroupBy,
        _workspace_id: &str,
    ) -> ImportResult<OpenApiImportOutcome> {
        use crate::converter::openapi as oc;
        use crate::openapi::{layout, parse_openapi};
        use rocket_collection::settings::CollectionVariable;
        use std::collections::{HashMap, HashSet};

        let doc = parse_openapi(text)?;
        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };
        for warning in &doc.warnings {
            report.skipped.push(SkippedItem {
                path: source_label.to_string(),
                reason: SkipReason::NotFollowed(warning.clone()),
            });
        }

        let stem = Path::new(source_label)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let wanted = [doc.title.trim(), stem.as_str(), "openapi"]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("openapi");
        let col_name = self.resolve_portable_collection_name(wanted)?;
        self.collection_repo
            .create(&col_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(col_name.clone());

        // `baseUrl` plus one empty secret variable for every placeholder an auth block uses.
        let mut settings = self
            .collection_repo
            .get_settings(&col_name)
            .unwrap_or_default();
        let base_url = doc.servers.first().cloned().unwrap_or_default();
        settings.variables.push(CollectionVariable {
            key: oc::BASE_URL_VARIABLE.to_string(),
            value: base_url.clone(),
            initial_value: base_url,
            enabled: true,
            secret: false,
        });
        for scheme in &doc.security_schemes {
            for key in oc::security_variable_names(scheme) {
                if settings.variables.iter().all(|v| v.key != key) {
                    settings.variables.push(CollectionVariable {
                        key,
                        value: String::new(),
                        initial_value: String::new(),
                        enabled: true,
                        secret: true,
                    });
                }
            }
        }
        if let Err(e) = self.collection_repo.save_settings(&col_name, &settings) {
            report.skipped.push(SkippedItem {
                path: source_label.to_string(),
                reason: SkipReason::ParseError(format!(
                    "collection variables could not be saved: {e}"
                )),
            });
        }

        let total = doc.operations.len();
        if total == 0 {
            report.skipped.push(SkippedItem {
                path: source_label.to_string(),
                reason: SkipReason::UnsupportedRequestType(
                    "the spec has no operations".to_string(),
                ),
            });
        }

        let mut folder_dirs: HashMap<Vec<String>, String> = HashMap::new();
        let mut used_folders: HashMap<String, HashSet<String>> = HashMap::new();
        let mut used_requests: HashMap<String, HashSet<String>> = HashMap::new();
        let mut next_seq: HashMap<String, u32> = HashMap::new();
        let mut operations = Vec::new();

        for (index, op) in doc.operations.iter().enumerate() {
            if index >= MAX_OPENAPI_OPERATIONS {
                report.skipped.push(SkippedItem {
                    path: source_label.to_string(),
                    reason: SkipReason::UnsupportedRequestType(format!(
                        "operation limit reached, {} operations not imported",
                        total - index
                    )),
                });
                break;
            }
            report.total_files += 1;

            let names = layout::folder_names(op, group_by);
            let dir = match ensure_openapi_folder(
                self.collection_repo.as_ref(),
                &col_name,
                &names,
                &mut folder_dirs,
                &mut used_folders,
            ) {
                Ok(dir) => dir,
                Err(message) => {
                    report.skipped.push(SkippedItem {
                        path: op.key.clone(),
                        reason: SkipReason::ParseError(message),
                    });
                    continue;
                }
            };

            let seq = next_seq.entry(dir.clone()).or_insert(0);
            *seq += 1;
            let mut converted = oc::convert_operation(op, &doc.security_schemes, *seq);
            for reason in converted.skipped.drain(..) {
                report.skipped.push(SkippedItem {
                    path: op.key.clone(),
                    reason,
                });
            }
            let baseline_hash = oc::baseline_hash(&converted.request);
            let (slug, k) = unique_segment(
                used_requests.entry(dir.clone()).or_default(),
                &converted.request.name,
                REQUEST_RESERVED,
            );
            if k > 1 {
                converted.request.name = format!("{} ({k})", converted.request.name);
            }
            let request_path = if dir.is_empty() {
                slug
            } else {
                format!("{dir}/{slug}")
            };
            match self
                .collection_repo
                .save_request(&col_name, &request_path, &converted.request)
            {
                Ok(_) => {
                    report.imported += 1;
                    operations.push(ImportedOperation {
                        key: op.key.clone(),
                        request_path,
                        uid: converted.request.uid.clone(),
                        baseline_hash,
                        baseline: converted.request,
                    });
                }
                Err(e) => report.skipped.push(SkippedItem {
                    path: request_path,
                    reason: SkipReason::ParseError(format!("request could not be written: {e}")),
                }),
            }
        }

        Ok(OpenApiImportOutcome {
            report,
            collection: col_name,
            operations,
            base_url: doc.servers.first().cloned().unwrap_or_default(),
        })
    }

    /// Import a Postman environment JSON file into an existing collection.
    pub fn import_postman_environment(
        &self,
        json_path: &Path,
        collection_name: &str,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        use crate::postman::parse_postman_environment;
        use rocket_environment::{Environment, Variable};

        let mut report = ImportReport {
            detected_type: "environment".to_string(),
            ..Default::default()
        };

        let postman_env = parse_postman_environment(json_path)?;

        let repo = self.env_factory.make(collection_name);
        let safe_name = unique_env_name(repo.as_ref(), &postman_env.name);
        let mut env = Environment::new(&safe_name);
        for v in &postman_env.values {
            let mut var = Variable::new(&v.key, &v.value);
            var.enabled = v.enabled;
            env.set_variable(var);
        }

        repo.save(&env).map_err(ImportError::DomainError)?;

        report.imported = postman_env.values.len();
        Ok(report)
    }

    /// Recursively copy `.yml` files from `src_dir` into `dest_root`, preserving structure.
    ///
    /// Skips:
    ///   - `opencollection.yml` at the collection root, except that its collection-level
    ///     defaults and docs are merged into the file `repo.create` wrote.
    ///   - `workspace.yml` anywhere (workspace marker, not a request).
    ///   - `_order.yml` (Bruno internal ordering file).
    ///     Files inside `environments/` are counted separately and not added to `report.imported`.
    fn copy_collection_files(
        &self,
        src_root: &Path,
        src_dir: &Path,
        dest_root: &Path,
        report: &mut ImportReport,
    ) -> ImportResult<()> {
        for entry in std::fs::read_dir(src_dir)? {
            let entry = entry?;
            let src_path = entry.path();
            let rel = src_path.strip_prefix(src_root).unwrap_or(&src_path);
            let dest_path = dest_root.join(rel);

            if src_path.is_dir() {
                // Never import dependency trees.
                if src_path.file_name().is_some_and(|n| n == "node_modules") {
                    continue;
                }
                std::fs::create_dir_all(&dest_path)?;
                self.copy_collection_files(src_root, &src_path, dest_root, report)?;
                continue;
            }

            let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            // gRPC requests refer to `.proto` files by path, and scripts `require()` shared
            // `.js` files, so copy both as well. They are not requests and are not counted.
            if ext == "proto" || ext == "js" {
                let label = if ext == "js" { ".js" } else { ".proto" };
                copy_support_file(&src_path, &dest_path, rel, label, report);
                continue;
            }
            if !matches!(ext, "yml" | "yaml") {
                continue;
            }

            let name = src_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // The root file was written by repo.create(), which owns the identity keys. The
            // collection-level defaults and docs of the source are merged into it.
            if src_path == src_root.join("opencollection.yml") {
                merge_root_defaults(&src_path, &dest_path, report);
                continue;
            }
            if name == "workspace.yml" || name == "_order.yml" {
                continue;
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src_path, &dest_path)?;

            // Count only request files (not environment entries).
            let in_environments = rel.components().any(|c| c.as_os_str() == "environments");
            if !in_environments {
                report.imported += 1;
            }
        }
        Ok(())
    }
}

/// Collects the `.yml` and `.yaml` files under `dir` as `/`-separated paths relative to `root`.
fn collect_yml_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_yml_files(root, &path, out);
            continue;
        }
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
            out.push(parts.join("/"));
        }
    }
}

/// Keys of a source `opencollection.yml` that carry collection-level content. The identity
/// keys (`opencollection`, `info`, `uid`) stay as `repo.create` wrote them.
const ROOT_CONTENT_KEYS: [&str; 5] = ["request", "docs", "config", "bundled", "extensions"];

/// Copies the collection-level headers, auth, variables, scripts, docs and config of a source
/// root file into the new root file. A file that cannot be read or parsed is reported and the
/// import goes on without it.
fn merge_root_defaults(src: &Path, dest: &Path, report: &mut ImportReport) {
    let mut skip = |why: String| {
        report.skipped.push(SkippedItem {
            path: "opencollection.yml".to_string(),
            reason: SkipReason::ParseError(why),
        });
    };
    let parse = |path: &Path| -> Result<serde_yaml::Value, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_yaml::from_str(&text).map_err(|e| e.to_string())
    };
    let source = match parse(src) {
        Ok(v) => v,
        Err(e) => return skip(format!("collection-level settings were not imported: {e}")),
    };
    let mut target = match parse(dest) {
        Ok(v) => v,
        Err(e) => return skip(format!("collection-level settings were not imported: {e}")),
    };
    let (Some(from), Some(into)) = (source.as_mapping(), target.as_mapping_mut()) else {
        return;
    };
    let mut changed = false;
    for key in ROOT_CONTENT_KEYS {
        if let Some(value) = from.get(key) {
            into.insert(serde_yaml::Value::from(key), value.clone());
            changed = true;
        }
    }
    if !changed {
        return;
    }
    match serde_yaml::to_string(&target) {
        Ok(text) => {
            if let Err(e) = std::fs::write(dest, text) {
                skip(format!("collection-level settings were not imported: {e}"));
            }
        }
        Err(e) => skip(format!("collection-level settings were not imported: {e}")),
    }
}

/// Copies a support file (`.proto` or `.js`, named by `label`) next to the requests. A
/// symlink is never followed, because a cloned collection could point one at a private
/// file, and a copy that fails is reported instead of aborting the whole import.
fn copy_support_file(src: &Path, dest: &Path, rel: &Path, label: &str, report: &mut ImportReport) {
    let mut skip = |why: String| {
        report.skipped.push(SkippedItem {
            path: rel.to_string_lossy().to_string(),
            reason: SkipReason::ParseError(why),
        });
    };
    match std::fs::symlink_metadata(src) {
        Ok(meta) if meta.file_type().is_symlink() => {
            skip(format!("a symlinked {label} file is not copied"));
        }
        Ok(meta) if meta.is_file() => {
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    skip(format!("could not copy the {label} file: {e}"));
                    return;
                }
            }
            if let Err(e) = std::fs::copy(src, dest) {
                skip(format!("could not copy the {label} file: {e}"));
            }
        }
        Ok(_) => {}
        Err(e) => skip(format!("could not read the {label} file: {e}")),
    }
}

/// Finds the `.proto` file a Bruno request names and returns its path relative to
/// the collection root, with forward slashes. Bruno paths are relative to the
/// request file or to the collection root, so both are tried. Returns `None` for an
/// absolute path, a missing file, or a file outside the collection, and the caller
/// keeps the path as written.
fn rebase_proto_path(root: &Path, request_dir: &Path, raw: &str) -> Option<String> {
    let raw_path = Path::new(raw);
    if raw_path.is_absolute() {
        return None;
    }
    let canonical_root = root.canonicalize().ok()?;
    [request_dir.join(raw_path), root.join(raw_path)]
        .iter()
        .filter_map(|candidate| candidate.canonicalize().ok())
        .find(|real| real.is_file() && real.starts_with(&canonical_root))
        .and_then(|real| {
            real.strip_prefix(&canonical_root).ok().map(|rel| {
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            })
        })
}

/// Sanitize a Postman item name for use as a folder/file path component.
fn sanitize_postman_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Longest folder or file name segment, in characters.
const MAX_SEGMENT_CHARS: usize = 100;
/// Most requests one WSDL import writes, counted over all services and ports.
pub(crate) const MAX_WSDL_REQUESTS: usize = 2000;
/// Most operations one OpenAPI import writes. The rest are reported, not imported.
pub(crate) const MAX_OPENAPI_OPERATIONS: usize = 5_000;

/// One request written by an OpenAPI import, with what a later sync needs to find it again.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedOperation {
    /// The operation key from the spec (`operationId`, else `METHOD /path`).
    pub key: String,
    /// The request's path inside the collection, without the file extension.
    pub request_path: String,
    pub uid: String,
    /// Hash of the imported fields at import time, see `converter::openapi::baseline_hash`.
    pub baseline_hash: String,
    /// The request exactly as it was saved, including a " (n)" name suffix.
    pub baseline: rocket_collection::Request,
}

/// The result of an OpenAPI import.
#[derive(Debug, Clone)]
pub struct OpenApiImportOutcome {
    pub report: ImportReport,
    /// The name of the collection that was created.
    pub collection: String,
    pub operations: Vec<ImportedOperation>,
    /// The `servers[0]` URL of the spec, empty when there is none.
    pub base_url: String,
}

/// Most envelope bytes one WSDL import generates, summed over all requests.
pub(crate) const MAX_WSDL_BYTES: usize = 32 * 1024 * 1024;

/// File stems the collection store treats as metadata, not requests.
pub(crate) const REQUEST_RESERVED: &[&str] = &[
    "folder",
    "_order",
    "opencollection",
    "workspace",
    "collection",
];
/// Folder names hidden in the tree at any depth.
pub(crate) const PORT_RESERVED: &[&str] = &["environments"];
/// Folder names hidden at the collection root. Services live at the root.
pub(crate) const SERVICE_RESERVED: &[&str] = &["environments", "flows"];

/// Folder and file name segment for a WSDL name. Never empty, at most 100 characters.
fn wsdl_path_segment(name: &str) -> String {
    let s: String = sanitize_postman_filename(name)
        .chars()
        .take(MAX_SEGMENT_CHARS)
        .collect();
    if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

/// Create the folder chain for `names` (outermost first) under the collection and return its
/// path, `""` for the collection root. Each level gets a safe, unique, non-reserved name, and a
/// chain that was created before is reused.
fn ensure_openapi_folder(
    repo: &dyn CollectionRepository,
    collection: &str,
    names: &[String],
    dirs: &mut std::collections::HashMap<Vec<String>, String>,
    used: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Result<String, String> {
    let mut dir = String::new();
    for depth in 1..=names.len() {
        let key = names[..depth].to_vec();
        if let Some(existing) = dirs.get(&key) {
            dir = existing.clone();
            continue;
        }
        let reserved = if depth == 1 {
            SERVICE_RESERVED
        } else {
            PORT_RESERVED
        };
        let (segment, _) = unique_segment(
            used.entry(dir.clone()).or_default(),
            &names[depth - 1],
            reserved,
        );
        let path = if dir.is_empty() {
            segment
        } else {
            format!("{dir}/{segment}")
        };
        repo.create_folder(collection, &path)
            .map_err(|e| format!("folder {path} could not be created: {e}"))?;
        dirs.insert(key, path.clone());
        dir = path;
    }
    Ok(dir)
}

/// True for names Windows treats as devices, with or without an extension.
fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    match stem.as_str() {
        "con" | "prn" | "aux" | "nul" => true,
        _ => {
            let b = stem.as_bytes();
            b.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && b[3].is_ascii_digit()
                && b[3] != b'0'
        }
    }
}

/// A request file path that no earlier file of this import took. Records it.
///
/// Paths compare by folded form (Unicode NFC, lowercase), so a case-only clash gets
/// a number (`get 2.yml`). A free path is returned unchanged. A folder never clashes
/// with a file, because the full path is the key.
fn unique_import_path(used: &mut HashSet<String>, path: &str) -> String {
    let normalized = rocket_collection::request_filename_for(path);
    if used.insert(rocket_collection::fold_name(&normalized)) {
        return normalized;
    }
    let stem = normalized
        .strip_suffix(".yml")
        .or_else(|| normalized.strip_suffix(".yaml"))
        .unwrap_or(&normalized);
    (2u32..)
        .map(|n| rocket_collection::candidate_filename(stem, n))
        .find(|c| used.insert(rocket_collection::fold_name(c)))
        .unwrap_or(normalized)
}

/// A path segment for `name` that is free in `used` and not reserved. Records it.
///
/// Matching is case-insensitive. A taken or reserved name gets `-2`, `-3`, ... and is
/// shortened first so the result stays within the length limit. Returns the segment
/// and the suffix number (1 when none was needed).
pub(crate) fn unique_segment(
    used: &mut std::collections::HashSet<String>,
    name: &str,
    reserved: &[&str],
) -> (String, u32) {
    let base = wsdl_path_segment(name);
    let mut slug = base.clone();
    let mut n = 1u32;
    loop {
        let key = slug.to_lowercase();
        let blocked = reserved.contains(&key.as_str()) || is_windows_device_name(&key);
        if !blocked && used.insert(key) {
            return (slug, n);
        }
        n += 1;
        let suffix = format!("-{n}");
        let keep = MAX_SEGMENT_CHARS.saturating_sub(suffix.len());
        let head: String = base.chars().take(keep).collect();
        slug = format!("{head}{suffix}");
    }
}

#[cfg(test)]
mod modern_tests {
    use super::*;
    use tempfile::TempDir;

    fn make_modern_collection(src: &std::path::Path) {
        std::fs::write(
            src.join("opencollection.yml"),
            "opencollection: \"1.0.0\"\ninfo:\n  name: my-col\n",
        )
        .unwrap();
        std::fs::write(
            src.join("get-users.yml"),
            "name: Get Users\nmethod: GET\nurl: https://api.example.com/users\n",
        )
        .unwrap();
        let env_dir = src.join("environments");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("local.yml"), "name: local\nvars: []\n").unwrap();
    }

    #[test]
    fn modern_collection_copies_files_without_parsing() {
        let src_dir = TempDir::new().unwrap();
        let col_src = src_dir.path().join("my-col");
        std::fs::create_dir_all(&col_src).unwrap();
        make_modern_collection(&col_src);

        let ws_dir = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws_dir.path());

        let report = service
            .import_modern_collection(&col_src, "default", None)
            .expect("modern import should succeed");

        assert_eq!(report.detected_type, "collection");
        assert!(report.created_collections.contains(&"my-col".to_string()));
        assert!(ws_dir
            .path()
            .join("collections/my-col/opencollection.yml")
            .exists());
        assert!(ws_dir
            .path()
            .join("collections/my-col/get-users.yml")
            .exists());
        assert!(ws_dir
            .path()
            .join("collections/my-col/environments/local.yml")
            .exists());
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn modern_collection_skips_root_opencollection_yml() {
        let src_dir = TempDir::new().unwrap();
        let col_src = src_dir.path().join("col");
        std::fs::create_dir_all(&col_src).unwrap();
        make_modern_collection(&col_src);

        let ws_dir = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws_dir.path());
        service
            .import_modern_collection(&col_src, "default", None)
            .unwrap();

        let oc_path = ws_dir.path().join("collections/col/opencollection.yml");
        assert!(oc_path.exists());
    }
}

#[cfg(test)]
mod auto_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn import_auto_detects_legacy_collection() {
        // Use a named subdirectory so the collection name doesn't start with a dot.
        let root = TempDir::new().unwrap();
        let src = root.path().join("my-collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bruno.json"), "{}").unwrap();
        std::fs::write(
            src.join("req.bru"),
            "meta {\n  name: R\n  type: http\n  seq: 1\n}\nget {\n  url: https://ex.com\n}\n",
        )
        .unwrap();

        let ws = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws.path());
        let report = service.import_auto(&src, "default", false).unwrap();

        assert_eq!(report.detected_type, "collection");
        assert_eq!(report.imported, 1);
    }

    #[test]
    fn import_workspace_finds_collections_inside_collections_subdir() {
        // Reproduces the real Bruno workspace ZIP structure:
        //   workspace.yml
        //   collections/
        //     MyCollection/
        //       bruno.json
        //       req.bru
        let root = TempDir::new().unwrap();
        let ws_root = root.path().join("my-workspace");
        let col_dir = ws_root.join("collections").join("MyCollection");
        std::fs::create_dir_all(&col_dir).unwrap();
        std::fs::write(ws_root.join("workspace.yml"), "name: test\n").unwrap();
        std::fs::write(col_dir.join("bruno.json"), "{}").unwrap();
        std::fs::write(
            col_dir.join("req.bru"),
            "meta {\n  name: R\n  type: http\n  seq: 1\n}\nget {\n  url: https://ex.com\n}\n",
        )
        .unwrap();

        let ws = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws.path());
        let report = service
            .import_workspace(&ws_root, false, Some("default"))
            .expect("workspace import should succeed");

        assert_eq!(report.detected_type, "workspace");
        assert_eq!(
            report.imported, 1,
            "should import the request inside collections/"
        );
        assert!(
            report
                .created_collections
                .iter()
                .any(|c| c.contains("MyCollection")),
            "should create MyCollection, got: {:?}",
            report.created_collections
        );
    }

    #[test]
    fn import_auto_returns_error_for_invalid_dir() {
        let dir = TempDir::new().unwrap();
        let ws = TempDir::new().unwrap();
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service.import_auto(dir.path(), "default", false);
        assert!(
            matches!(result, Err(ImportError::NotABrunoDirectory(_))),
            "expected NotABrunoDirectory"
        );
    }
}

#[cfg(test)]
mod detection_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_workspace_modern() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("workspace.yml"), "").unwrap();
        assert!(matches!(
            detect_workspace(d.path()),
            Some(BrunoFormat::Modern)
        ));
    }

    #[test]
    fn detect_workspace_legacy() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("bruno.json"), "{}").unwrap();
        assert!(matches!(
            detect_workspace(d.path()),
            Some(BrunoFormat::Legacy)
        ));
    }

    #[test]
    fn detect_workspace_none() {
        let d = TempDir::new().unwrap();
        assert!(detect_workspace(d.path()).is_none());
    }

    #[test]
    fn detect_collection_modern() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("opencollection.yml"), "").unwrap();
        assert!(matches!(
            detect_collection(d.path()),
            Some(BrunoFormat::Modern)
        ));
    }

    #[test]
    fn detect_collection_legacy() {
        let d = TempDir::new().unwrap();
        std::fs::write(d.path().join("bruno.json"), "{}").unwrap();
        assert!(matches!(
            detect_collection(d.path()),
            Some(BrunoFormat::Legacy)
        ));
    }

    #[test]
    fn detect_collection_none() {
        let d = TempDir::new().unwrap();
        assert!(detect_collection(d.path()).is_none());
    }
}

#[cfg(test)]
mod wsdl_tests {
    use super::*;
    use tempfile::TempDir;

    fn calc_wsdl() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wsdl/calc.wsdl")
    }

    fn yml_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let p = entry.expect("entry").path();
            if p.is_dir() {
                yml_files(&p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("yml")
                && !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("folder.yml" | "opencollection.yml")
                )
            {
                // Folder and collection metadata files are not requests.
                out.push(p);
            }
        }
    }

    #[test]
    fn import_wsdl_writes_service_port_operation_tree() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let report = service
            .import_wsdl(&calc_wsdl(), "default")
            .expect("import should succeed");

        assert_eq!(report.detected_type, "collection");
        assert_eq!(report.created_collections, vec!["calc".to_string()]);
        assert_eq!(report.total_files, 4);
        assert_eq!(report.imported, 4);
        assert!(report.skipped.is_empty(), "got: {:?}", report.skipped);

        let col = ws.path().join("collections/calc");
        assert!(col.join("Calculator/CalcSoap").is_dir());
        assert!(col.join("Calculator/CalcSoap12").is_dir());
        let mut files = Vec::new();
        yml_files(&col.join("Calculator"), &mut files);
        assert_eq!(files.len(), 4, "got: {files:?}");

        let soap12 = std::fs::read_to_string(
            files
                .iter()
                .find(|p| p.to_string_lossy().contains("CalcSoap12"))
                .expect("a soap 1.2 request file"),
        )
        .expect("read");
        assert!(soap12.contains("application/soap+xml"), "got: {soap12}");
        assert!(
            !soap12.to_lowercase().contains("soapaction"),
            "got: {soap12}"
        );
    }

    #[test]
    fn import_wsdl_resolves_collection_name_conflicts() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        service.import_wsdl(&calc_wsdl(), "default").expect("first");
        let second = service
            .import_wsdl(&calc_wsdl(), "default")
            .expect("second");
        assert_eq!(second.created_collections, vec!["calc-1".to_string()]);
    }

    #[test]
    fn warnings_become_report_items() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let wsdl = std::fs::read_to_string(calc_wsdl())
            .expect("read fixture")
            .replace(
                r#"<xsd:import namespace="http://example.com/types" schemaLocation="calc-types.xsd"/>"#,
                r#"<xsd:import namespace="http://example.com/types" schemaLocation="https://example.invalid/t.xsd"/>"#,
            );
        let path = src.path().join("remote.wsdl");
        std::fs::write(&path, wsdl).expect("write");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert!(
            report.skipped.iter().any(|s| matches!(
                &s.reason,
                SkipReason::UnsupportedRequestType(m) if m.contains("not fetched")
            )),
            "got: {:?}",
            report.skipped
        );
    }

    #[test]
    fn duplicate_operation_names_get_suffixes() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let wsdl = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
            xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
            xmlns:xsd="http://www.w3.org/2001/XMLSchema"
            xmlns:tns="urn:o" targetNamespace="urn:o">
          <message name="A"><part name="p" type="xsd:string"/></message>
          <message name="B"><part name="p" type="xsd:int"/></message>
          <portType name="P">
            <operation name="Get"><input name="ByName" message="tns:A"/></operation>
            <operation name="Get"><input name="ById" message="tns:B"/></operation>
          </portType>
          <binding name="Bd" type="tns:P"><soap:binding style="document"/>
            <operation name="Get"><soap:operation soapAction="a"/><input name="ByName"><soap:body use="literal"/></input></operation>
            <operation name="Get"><soap:operation soapAction="b"/><input name="ById"><soap:body use="literal"/></input></operation>
          </binding>
          <service name="S"><port name="Pt" binding="tns:Bd"><soap:address location="http://h/o"/></port></service>
        </definitions>"#;
        let path = src.path().join("overload.wsdl");
        std::fs::write(&path, wsdl).expect("write");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 2);
        let mut files = Vec::new();
        yml_files(&ws.path().join("collections/overload"), &mut files);
        assert_eq!(
            files.len(),
            2,
            "overloads must not overwrite each other: {files:?}"
        );
        let text = |name: &str| {
            let f = files
                .iter()
                .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(name))
                .unwrap_or_else(|| panic!("missing {name} in {files:?}"));
            std::fs::read_to_string(f).expect("read")
        };
        let first = text("Get.yml");
        let second = text("Get-2.yml");
        assert!(second.contains("Get (2)"), "got: {second}");
        assert!(!first.contains("Get (2)"), "got: {first}");
        assert!(
            first.contains("a\"") || first.contains("\"a"),
            "got: {first}"
        );
        assert!(
            second.contains("b\"") || second.contains("\"b"),
            "got: {second}"
        );
        assert_ne!(first, second);
    }

    /// Build a WSDL with `ports` ports sharing one binding of `ops` operations.
    fn shared_binding_wsdl(ports: usize, ops: usize) -> String {
        let mut pt_ops = String::new();
        let mut b_ops = String::new();
        for i in 0..ops {
            pt_ops.push_str(&format!(
                r#"<operation name="Op{i}"><input message="tns:M"/></operation>"#
            ));
            b_ops.push_str(&format!(
                r#"<operation name="Op{i}"><soap:operation soapAction="a{i}"/><input><soap:body use="literal"/></input></operation>"#
            ));
        }
        let mut port_xml = String::new();
        for i in 0..ports {
            port_xml.push_str(&format!(
                r#"<port name="P{i}" binding="tns:B"><soap:address location="http://h/{i}"/></port>"#
            ));
        }
        format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
            xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
            xmlns:xsd="http://www.w3.org/2001/XMLSchema"
            xmlns:tns="urn:m" targetNamespace="urn:m">
          <message name="M"><part name="p" type="xsd:string"/></message>
          <portType name="PT">{pt_ops}</portType>
          <binding name="B" type="tns:PT"><soap:binding style="document"/>{b_ops}</binding>
          <service name="S">{port_xml}</service>
        </definitions>"#
        )
    }

    fn write_wsdl(dir: &Path, file: &str, text: &str) -> PathBuf {
        let path = dir.join(file);
        std::fs::write(&path, text).expect("write");
        path
    }

    fn limit_items(report: &ImportReport) -> Vec<&SkippedItem> {
        report
            .skipped
            .iter()
            .filter(|s| {
                matches!(&s.reason, SkipReason::UnsupportedRequestType(m) if m.contains("request limit reached"))
            })
            .collect()
    }

    #[test]
    fn many_ports_sharing_a_binding_stop_at_the_request_cap() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let path = write_wsdl(src.path(), "many.wsdl", &shared_binding_wsdl(20, 50));
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl_with_limits(&path, 130, usize::MAX)
            .expect("import should succeed");
        assert_eq!(report.imported, 130);
        let items = limit_items(&report);
        assert_eq!(items.len(), 1, "got: {:?}", report.skipped);
        assert!(
            matches!(&items[0].reason, SkipReason::UnsupportedRequestType(m) if m.contains("870 operations not imported")),
            "got: {:?}",
            items[0]
        );
        let mut files = Vec::new();
        yml_files(&ws.path().join("collections/many"), &mut files);
        assert_eq!(files.len(), 130);
    }

    #[test]
    fn total_envelope_byte_budget_stops_the_import() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let path = write_wsdl(src.path(), "bytes.wsdl", &shared_binding_wsdl(2, 10));
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl_with_limits(&path, usize::MAX, 1)
            .expect("import should succeed");
        assert_eq!(report.imported, 1);
        assert_eq!(limit_items(&report).len(), 1, "got: {:?}", report.skipped);
    }

    #[test]
    fn a_normal_import_hits_no_limit() {
        let ws = TempDir::new().expect("tempdir");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&calc_wsdl(), "default")
            .expect("import should succeed");
        assert!(limit_items(&report).is_empty());
    }

    #[test]
    fn ports_per_service_are_capped_with_a_warning() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let path = write_wsdl(src.path(), "ports.wsdl", &shared_binding_wsdl(300, 1));
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 256);
        assert!(
            report.skipped.iter().any(|s| matches!(
                &s.reason,
                SkipReason::UnsupportedRequestType(m) if m.contains("only the first 256")
            )),
            "got: {:?}",
            report.skipped
        );
    }

    /// One operation per name, all in service `S`, port `Pt`.
    fn named_ops_wsdl(service: &str, port: &str, names: &[&str]) -> String {
        let mut pt_ops = String::new();
        let mut b_ops = String::new();
        for n in names {
            pt_ops.push_str(&format!(
                r#"<operation name="{n}"><input message="tns:M"/></operation>"#
            ));
            b_ops.push_str(&format!(
                r#"<operation name="{n}"><soap:operation soapAction="act-{n}"/><input><soap:body use="literal"/></input></operation>"#
            ));
        }
        format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
            xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
            xmlns:xsd="http://www.w3.org/2001/XMLSchema"
            xmlns:tns="urn:m" targetNamespace="urn:m">
          <message name="M"><part name="p" type="xsd:string"/></message>
          <portType name="PT">{pt_ops}</portType>
          <binding name="B" type="tns:PT"><soap:binding style="document"/>{b_ops}</binding>
          <service name="{service}"><port name="{port}" binding="tns:B"><soap:address location="http://h/x"/></port></service>
        </definitions>"#
        )
    }

    #[test]
    fn overlong_operation_names_are_truncated() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let long_a = format!("{}A", "x".repeat(299));
        let long_b = format!("{}B", "x".repeat(299));
        let path = write_wsdl(
            src.path(),
            "long.wsdl",
            &named_ops_wsdl("S", "Pt", &[&long_a, &long_b]),
        );
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 2, "got: {:?}", report.skipped);
        assert!(report.skipped.is_empty(), "got: {:?}", report.skipped);
        let mut files = Vec::new();
        yml_files(&ws.path().join("collections/long"), &mut files);
        assert_eq!(files.len(), 2, "names must stay distinct: {files:?}");
        for f in &files {
            let stem = f.file_stem().and_then(|n| n.to_str()).expect("stem");
            assert!(stem.chars().count() <= 100, "got: {stem}");
        }
    }

    #[test]
    fn overlong_service_and_port_names_are_truncated() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let path = write_wsdl(
            src.path(),
            "longdirs.wsdl",
            &named_ops_wsdl(&"s".repeat(300), &"p".repeat(300), &["Get"]),
        );
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 1, "got: {:?}", report.skipped);
    }

    #[test]
    fn unique_segment_stays_unique_and_short_after_truncation() {
        let mut used = std::collections::HashSet::new();
        let a = unique_segment(&mut used, &format!("{}1", "x".repeat(200)), &[]);
        let b = unique_segment(&mut used, &format!("{}2", "x".repeat(200)), &[]);
        let c = unique_segment(&mut used, &format!("{}3", "x".repeat(200)), &[]);
        assert_ne!(a.0, b.0);
        assert_ne!(b.0, c.0);
        assert_ne!(a.0, c.0);
        for seg in [&a.0, &b.0, &c.0] {
            assert!(seg.chars().count() <= 100, "got: {seg}");
        }
        assert_eq!((a.1, b.1, c.1), (1, 2, 3));
    }

    #[test]
    fn unique_import_path_keeps_a_free_name_unchanged() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(unique_import_path(&mut used, "api/get.yml"), "api/get.yml");
    }

    #[test]
    fn unique_import_path_numbers_a_clash_and_ignores_case() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(unique_import_path(&mut used, "api/Get.yml"), "api/Get.yml");
        assert_eq!(
            unique_import_path(&mut used, "api/get.yml"),
            "api/get 2.yml"
        );
        assert_eq!(
            unique_import_path(&mut used, "api/GET.yml"),
            "api/GET 3.yml"
        );
    }

    #[test]
    fn unique_import_path_ignores_folders() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(unique_import_path(&mut used, "a.yml"), "a.yml");
        assert_eq!(unique_import_path(&mut used, "a/b.yml"), "a/b.yml");
    }

    /// Imports a legacy Bruno collection holding one `.bru` file per name, each
    /// with its own URL, and returns the request `.yml` files found in the result.
    fn import_bru_names(names: &[&str]) -> Vec<(String, String)> {
        let src_root = TempDir::new().expect("src dir");
        let src = src_root.path().join("col");
        std::fs::create_dir_all(&src).expect("create col dir");
        std::fs::write(src.join("bruno.json"), r#"{"version":"1","name":"c"}"#)
            .expect("write bruno.json");
        for (i, name) in names.iter().enumerate() {
            let body = format!(
                "meta {{\n  name: {name}\n  type: http\n  seq: {}\n}}\n\nget {{\n  url: https://example.com/{i}\n  body: none\n  auth: none\n}}\n",
                i + 1
            );
            std::fs::write(src.join(format!("{name}.bru")), body).expect("write bru");
        }
        let ws = TempDir::new().expect("ws dir");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_collection(&src, "default")
            .expect("import should succeed");
        let col = ws
            .path()
            .join("collections")
            .join(&report.created_collections[0]);
        let mut found: Vec<(String, String)> = std::fs::read_dir(&col)
            .expect("read collection")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| x == "yml")
                    && p.file_name().is_some_and(|n| n != "opencollection.yml")
            })
            .map(|p| {
                (
                    p.file_name().expect("name").to_string_lossy().to_string(),
                    std::fs::read_to_string(&p).expect("read request"),
                )
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn bruno_import_keeps_both_files_when_case_variants_clash() {
        let found = import_bru_names(&["Get", "get"]);
        assert_eq!(
            found.len(),
            2,
            "got: {:?}",
            found.iter().map(|f| &f.0).collect::<Vec<_>>()
        );
        assert!(found.iter().any(|f| f.0.ends_with(" 2.yml")));
        for i in 0..2 {
            let url = format!("https://example.com/{i}");
            assert!(found.iter().any(|f| f.1.contains(&url)), "missing {url}");
        }
    }

    #[test]
    fn bruno_import_numbers_a_third_clash() {
        let found = import_bru_names(&["Get", "get", "GET"]);
        assert_eq!(found.len(), 3);
    }

    /// Delegates to the real repository but fails to save any request named `Get`.
    struct FailingRepo(rocket_infra::FsCollectionRepo);

    use rocket_collection::{
        Collection, CollectionSettings, CollectionSummary, CollectionVariable, Request,
    };
    use rocket_shared::error::{DomainError, DomainResult};

    impl CollectionRepository for FailingRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            self.0.list()
        }
        fn get(&self, name: &str) -> DomainResult<Collection> {
            self.0.get(name)
        }
        fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
            self.0.get_summaries(name)
        }
        fn create(&self, name: &str) -> DomainResult<Collection> {
            self.0.create(name)
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.delete(name)
        }
        fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
            self.0.rename(old_name, new_name)
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            self.0.get_request(collection, path)
        }
        fn save_request(
            &self,
            collection: &str,
            path: &str,
            request: &Request,
        ) -> DomainResult<String> {
            if path.ends_with("/Get") {
                return Err(DomainError::Io("disk full".to_string()));
            }
            self.0.save_request(collection, path, request)
        }
        fn rename_request(&self, c: &str, old: &str, new: &str) -> DomainResult<()> {
            self.0.rename_request(c, old, new)
        }
        fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.0.delete_request(collection, path)
        }
        fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.0.create_folder(collection, path)
        }
        fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
            self.0.delete_folder(collection, path)
        }
        fn move_item(&self, sc: &str, sp: &str, dc: &str, dp: &str) -> DomainResult<()> {
            self.0.move_item(sc, sp, dc, dp)
        }
        fn reorder_items(&self, c: &str, f: &str, names: &[String]) -> DomainResult<()> {
            self.0.reorder_items(c, f, names)
        }
        fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
            self.0.get_settings(name)
        }
        fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<()> {
            self.0.save_settings(name, settings)
        }
        fn get_folder_chain_variables(
            &self,
            collection: &str,
            request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_chain_variables(collection, request_path)
        }
        fn get_folder_chain_variables_strict(
            &self,
            collection: &str,
            request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            self.0
                .get_folder_chain_variables_strict(collection, request_path)
        }
        fn get_folder_variables(
            &self,
            collection: &str,
            folder_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_variables(collection, folder_path)
        }
        fn save_folder_variables(
            &self,
            collection: &str,
            folder_path: &str,
            vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0.save_folder_variables(collection, folder_path, vars)
        }
        fn get_request_variables(
            &self,
            collection: &str,
            request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_request_variables(collection, request_path)
        }
        fn save_request_variables(
            &self,
            collection: &str,
            request_path: &str,
            vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            self.0
                .save_request_variables(collection, request_path, vars)
        }
    }

    #[test]
    fn a_failed_request_write_is_reported_and_the_rest_are_written() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let path = write_wsdl(
            src.path(),
            "fail.wsdl",
            &named_ops_wsdl("S", "Pt", &["Get", "Put"]),
        );
        let mut service = ImportService::new_with_workspace_path(ws.path());
        service.collection_repo = Box::new(FailingRepo(
            rocket_infra::FsCollectionRepo::new_standalone(ws.path().join("collections")),
        ));
        let report = service
            .import_wsdl(&path, "default")
            .expect("import should survive a failed write");
        assert_eq!(report.created_collections, vec!["fail".to_string()]);
        assert_eq!(report.imported, 1, "got: {:?}", report.skipped);
        assert_eq!(report.total_files, 2);
        assert!(
            report.skipped.iter().any(|s| s.path.ends_with("Pt/Get")
                && matches!(&s.reason, SkipReason::ParseError(m) if m.contains("could not be written"))),
            "got: {:?}",
            report.skipped
        );
        assert!(ws.path().join("collections/fail/S/Pt/Put.yml").is_file());
    }

    fn read_named(files: &[PathBuf], name: &str) -> String {
        let f = files
            .iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(name))
            .unwrap_or_else(|| panic!("missing {name} in {files:?}"));
        std::fs::read_to_string(f).expect("read")
    }

    #[test]
    fn reserved_and_colliding_operation_names_stay_distinct() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let names = [
            "folder",
            "_order",
            "Get",
            "get",
            "opencollection",
            "NUL",
            "com1",
        ];
        let path = write_wsdl(
            src.path(),
            "reserved.wsdl",
            &named_ops_wsdl("S", "Pt", &names),
        );
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 7, "got: {:?}", report.skipped);
        assert!(report.skipped.is_empty(), "got: {:?}", report.skipped);

        let port = ws.path().join("collections/reserved/S/Pt");
        let mut files = Vec::new();
        yml_files(&port, &mut files);
        assert_eq!(files.len(), 7, "got: {files:?}");
        for expected in [
            "folder-2.yml",
            "_order-2.yml",
            "Get.yml",
            "get-2.yml",
            "opencollection-2.yml",
            "NUL-2.yml",
            "com1-2.yml",
        ] {
            let text = read_named(&files, expected);
            assert!(
                text.contains("Envelope"),
                "{expected} is not a request: {text}"
            );
        }
        let folder = std::fs::read_to_string(port.join("folder.yml")).expect("folder.yml");
        assert!(
            !folder.contains("Envelope"),
            "folder.yml was overwritten: {folder}"
        );
    }

    #[test]
    fn reserved_service_and_port_names_get_visible_distinct_folders() {
        for (service, port, svc_dir, port_dir) in [
            ("environments", "Pt", "environments-2", "Pt"),
            ("flows", "Pt", "flows-2", "Pt"),
            ("CON", "NUL", "CON-2", "NUL-2"),
            ("S", "environments", "S", "environments-2"),
        ] {
            let ws = TempDir::new().expect("tempdir");
            let src = TempDir::new().expect("tempdir");
            let path = write_wsdl(
                src.path(),
                "svc.wsdl",
                &named_ops_wsdl(service, port, &["Get"]),
            );
            let report = ImportService::new_with_workspace_path(ws.path())
                .import_wsdl(&path, "default")
                .expect("import should succeed");
            assert_eq!(report.imported, 1, "{service}/{port}: {:?}", report.skipped);
            let file = ws
                .path()
                .join(format!("collections/svc/{svc_dir}/{port_dir}/Get.yml"));
            assert!(file.is_file(), "missing {file:?}");
        }
    }

    #[test]
    fn services_differing_only_by_case_get_distinct_folders() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let one = named_ops_wsdl("Svc", "Pt", &["Get"]);
        let two = named_ops_wsdl("svc", "Pt", &["Get"]);
        // Splice the second service into the first document.
        let svc = two
            .split("<service")
            .nth(1)
            .and_then(|r| r.split("</service>").next())
            .expect("service block");
        let merged = one.replace(
            "</definitions>",
            &format!("<service{svc}</service></definitions>"),
        );
        let path = write_wsdl(src.path(), "case.wsdl", &merged);
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 2, "got: {:?}", report.skipped);
        let base = ws.path().join("collections/case");
        assert!(base.join("Svc/Pt/Get.yml").is_file());
        assert!(base.join("svc-2/Pt/Get.yml").is_file());
    }
}

#[cfg(test)]
mod proto_path_tests {
    use super::*;
    use tempfile::TempDir;

    fn layout() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("protos")).expect("mkdir");
        std::fs::create_dir_all(dir.path().join("calls")).expect("mkdir");
        std::fs::write(
            dir.path().join("protos/greeter.proto"),
            "syntax = \"proto3\";\n",
        )
        .expect("write");
        dir
    }

    #[test]
    fn a_path_relative_to_the_request_file_is_rebased_to_the_collection_root() {
        let dir = layout();
        let rebased = rebase_proto_path(
            dir.path(),
            &dir.path().join("calls"),
            "../protos/greeter.proto",
        );
        assert_eq!(rebased.as_deref(), Some("protos/greeter.proto"));
    }

    #[test]
    fn a_path_relative_to_the_collection_root_is_found_too() {
        let dir = layout();
        let rebased = rebase_proto_path(
            dir.path(),
            &dir.path().join("calls"),
            "protos/greeter.proto",
        );
        assert_eq!(rebased.as_deref(), Some("protos/greeter.proto"));
    }

    #[test]
    fn missing_absolute_and_outside_paths_stay_as_written() {
        let dir = layout();
        let outside = TempDir::new().expect("tempdir");
        std::fs::write(outside.path().join("other.proto"), "syntax = \"proto3\";\n")
            .expect("write");
        let calls = dir.path().join("calls");
        assert_eq!(
            rebase_proto_path(dir.path(), &calls, "protos/missing.proto"),
            None
        );
        assert_eq!(
            rebase_proto_path(
                dir.path(),
                &calls,
                &outside.path().join("other.proto").to_string_lossy()
            ),
            None
        );
        let escaping = format!(
            "../../{}/other.proto",
            outside.path().file_name().expect("name").to_string_lossy()
        );
        assert_eq!(rebase_proto_path(dir.path(), &calls, &escaping), None);
    }
}

#[cfg(test)]
mod open_check_tests {
    use super::*;
    use tempfile::TempDir;

    const GOOD: &str =
        "info:\n  name: Good\n  type: http\nhttp:\n  method: GET\n  url: https://x.test\n";
    const BAD: &str = "info:\n  name: Bad\n  type: http\nhttp:\n  method: GET\n  url: https://x.test\n  headers:\n    - value: orphan\n";

    fn import(files: &[(&str, &str)]) -> ImportReport {
        let src_dir = TempDir::new().expect("tmp");
        let src = src_dir.path().join("col");
        std::fs::create_dir_all(src.join("sub")).expect("mkdir");
        std::fs::write(
            src.join("opencollection.yml"),
            "opencollection: \"1.0.0\"\ninfo:\n  name: col\n",
        )
        .expect("root");
        for (name, text) in files {
            std::fs::write(src.join(name), text).expect("write");
        }
        let ws = TempDir::new().expect("ws");
        ImportService::new_with_workspace_path(ws.path())
            .import_modern_collection(&src, "default", None)
            .expect("import")
    }

    #[test]
    fn a_copied_request_that_cannot_open_is_reported() {
        let report = import(&[("good.yml", GOOD), ("sub/bad.yml", BAD)]);
        assert_eq!(report.imported, 2);
        assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
        assert_eq!(report.skipped[0].path, "sub/bad.yml");
        assert!(matches!(
            report.skipped[0].reason,
            SkipReason::ParseError(_)
        ));
    }

    #[test]
    fn requests_that_open_add_nothing_to_the_report() {
        let report = import(&[("good.yml", GOOD)]);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }
}
