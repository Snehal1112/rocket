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

        // Walk request files.
        self.walk_requests(
            path,
            path,
            &resolved_name,
            self.collection_repo.as_ref(),
            &mut report,
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
    ) -> ImportResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();

            if p.is_dir() {
                // Skip environments — handled separately.
                if p.file_name().is_some_and(|n| n == "environments") {
                    continue;
                }
                // Create subfolder metadata and recurse.
                let folder_rel = p.strip_prefix(root).unwrap_or(&p);
                let folder_path = folder_rel.to_string_lossy().to_string();
                let _ = repo.create_folder(collection_name, &folder_path);
                self.walk_requests(root, &p, collection_name, repo, report)?;
                continue;
            }

            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            // A gRPC request points at a `.proto` file by path, so the proto files
            // travel with the requests. They are not requests and are not counted.
            if ext == "proto" {
                let rel = p.strip_prefix(root).unwrap_or(&p);
                let dest = self
                    .workspace_path
                    .join("collections")
                    .join(collection_name)
                    .join(rel);
                copy_proto_file(&p, &dest, rel, report);
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

                    let out_path = rel_path.with_extension("yml").to_string_lossy().to_string();
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
                    let env = env_converter::convert(&env_name, &doc);
                    let _ = env_repo.save(&env);
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

        Ok(report)
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
            let mut env = rocket_environment::Environment::new(&postman_env.name);
            for v in &postman_env.values {
                let mut var = rocket_environment::Variable::new(&v.key, &v.value);
                var.enabled = v.enabled;
                env.set_variable(var);
            }
            self.env_factory
                .make(&col_name)
                .save(&env)
                .map_err(ImportError::DomainError)?;
        }

        self.write_postman_items(&collection.item, &col_name, "", &mut report)?;

        Ok(report)
    }

    fn write_postman_items(
        &self,
        items: &[crate::postman::ast::PostmanItem],
        col_name: &str,
        path_prefix: &str,
        report: &mut ImportReport,
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

                    self.write_postman_items(&folder.item, col_name, &folder_path, report)?;
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

        let mut env = Environment::new(&postman_env.name);
        for v in &postman_env.values {
            let mut var = Variable::new(&v.key, &v.value);
            var.enabled = v.enabled;
            env.set_variable(var);
        }

        self.env_factory
            .make(collection_name)
            .save(&env)
            .map_err(ImportError::DomainError)?;

        report.imported = postman_env.values.len();
        Ok(report)
    }

    /// Recursively copy `.yml` files from `src_dir` into `dest_root`, preserving structure.
    ///
    /// Skips:
    ///   - `opencollection.yml` at the collection root (written by `repo.create`).
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
                std::fs::create_dir_all(&dest_path)?;
                self.copy_collection_files(src_root, &src_path, dest_root, report)?;
                continue;
            }

            let ext = src_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            // gRPC requests refer to `.proto` files by path, so copy them as well.
            // They are not requests and are not counted.
            if ext == "proto" {
                copy_proto_file(&src_path, &dest_path, rel, report);
                continue;
            }
            if !matches!(ext, "yml" | "yaml") {
                continue;
            }

            let name = src_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Root opencollection.yml is already written by repo.create().
            if src_path == src_root.join("opencollection.yml") {
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

/// Copies a `.proto` file next to the requests. A symlink is never followed, because a
/// cloned collection could point one at a private file, and a copy that fails is reported
/// instead of aborting the whole import.
fn copy_proto_file(src: &Path, dest: &Path, rel: &Path, report: &mut ImportReport) {
    let mut skip = |why: String| {
        report.skipped.push(SkippedItem {
            path: rel.to_string_lossy().to_string(),
            reason: SkipReason::ParseError(why),
        });
    };
    match std::fs::symlink_metadata(src) {
        Ok(meta) if meta.file_type().is_symlink() => {
            skip("a symlinked .proto file is not copied".into());
        }
        Ok(meta) if meta.is_file() => {
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    skip(format!("could not copy the .proto file: {e}"));
                    return;
                }
            }
            if let Err(e) = std::fs::copy(src, dest) {
                skip(format!("could not copy the .proto file: {e}"));
            }
        }
        Ok(_) => {}
        Err(e) => skip(format!("could not read the .proto file: {e}")),
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
/// Most envelope bytes one WSDL import generates, summed over all requests.
pub(crate) const MAX_WSDL_BYTES: usize = 32 * 1024 * 1024;

/// File stems the collection store treats as metadata, not requests.
const REQUEST_RESERVED: &[&str] = &[
    "folder",
    "_order",
    "opencollection",
    "workspace",
    "collection",
];
/// Folder names hidden in the tree at any depth.
const PORT_RESERVED: &[&str] = &["environments"];
/// Folder names hidden at the collection root. Services live at the root.
const SERVICE_RESERVED: &[&str] = &["environments", "flows"];

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

/// A path segment for `name` that is free in `used` and not reserved. Records it.
///
/// Matching is case-insensitive. A taken or reserved name gets `-2`, `-3`, ... and is
/// shortened first so the result stays within the length limit. Returns the segment
/// and the suffix number (1 when none was needed).
fn unique_segment(
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
