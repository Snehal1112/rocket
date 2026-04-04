use std::path::{Path, PathBuf};

use rocket_collection::CollectionRepository;
use rocket_environment::EnvironmentRepository;
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};

use crate::bru;
use crate::converter::{environment as env_converter, request as req_converter};
use crate::error::{ImportError, ImportResult};
use crate::report::{ImportReport, SkipReason, SkippedItem};

/// Orchestrates the full Bruno import pipeline.
pub struct ImportService {
    workspace_path: PathBuf,
}

impl ImportService {
    /// Use the active workspace path from the environment, falling back to `.`.
    pub fn new() -> Self {
        Self {
            workspace_path: default_workspace_path(),
        }
    }

    /// Construct with an explicit workspace path (mainly for tests).
    pub fn new_with_workspace_path(path: &Path) -> Self {
        Self { workspace_path: path.to_path_buf() }
    }

    /// Import a single Bruno collection directory into the given workspace.
    pub fn import_collection(
        &self,
        path: &Path,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        if !path.join("bruno.json").exists() {
            return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
        }

        let mut report = ImportReport::default();

        let col_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported".into());

        let resolved_name = self.resolve_collection_name(&col_name);
        let repo = self.make_collection_repo();

        repo.create(&resolved_name).map_err(ImportError::DomainError)?;
        report.created_collections.push(resolved_name.clone());

        // Walk request files.
        self.walk_requests(path, path, &resolved_name, &repo, &mut report)?;

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
        if !path.join("bruno.json").exists() {
            return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
        }

        let mut combined = ImportReport::default();

        if create_new_workspace {
            let ws_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "imported-workspace".into());
            combined.created_workspace = Some(ws_name);
        }

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() && p.join("bruno.json").exists() {
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
                if p.file_name().map_or(false, |n| n == "environments") {
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
            if !matches!(ext, "bru" | "yml" | "yaml") {
                continue;
            }
            // Skip Bruno metadata files.
            if p.file_name().map_or(false, |n| n == "bruno.json" || n == "_order.yml") {
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
                    let (req_opt, skipped_reasons) = req_converter::convert(&doc);

                    for reason in skipped_reasons {
                        report.skipped.push(SkippedItem {
                            path: rel_str.clone(),
                            reason,
                        });
                    }

                    if let Some(req) = req_opt {
                        let out_path = rel_path.with_extension("yml").to_string_lossy().to_string();
                        let _ = repo.save_request(collection_name, &out_path, &req);
                        report.imported += 1;
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
        let env_repo = self.make_env_repo(collection_name);
        for entry in std::fs::read_dir(env_dir)? {
            let entry = entry?;
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "bru" | "yml" | "yaml") {
                continue;
            }

            let env_name = p.file_stem()
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

    fn resolve_collection_name(&self, name: &str) -> String {
        let col_dir = self.workspace_path.join("collections");
        if !col_dir.join(name).exists() {
            return name.to_string();
        }
        let mut i = 1u32;
        loop {
            let candidate = format!("{name}-{i}");
            if !col_dir.join(&candidate).exists() {
                return candidate;
            }
            i += 1;
        }
    }

    fn make_collection_repo(&self) -> FsCollectionRepo {
        FsCollectionRepo::new(self.workspace_path.join("collections"))
    }

    fn make_env_repo(&self, collection_name: &str) -> FsEnvironmentRepo {
        FsEnvironmentRepo::new(
            self.workspace_path
                .join("collections")
                .join(collection_name)
                .join("environments"),
        )
    }
}

fn default_workspace_path() -> PathBuf {
    PathBuf::from(
        std::env::var("ROCKET_WORKSPACE_PATH").unwrap_or_else(|_| ".".into()),
    )
}
