use crate::collection_lock::{collection_lock, with_collection_lock};
use crate::flow_references::FlowReferenceUpdater;
use rocket_audit::{
    actor::AuditActor,
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{
    Collection, CollectionGrant, CollectionRepository, CollectionSummary, CollectionTrustStore,
    CollectionVariable, FolderSettings, GrantSource, GraphQlRequest, GrpcRequest, Request,
    RequestKind, RequestScriptPhase, WebSocketRequest,
};
use rocket_shared::action::HttpRequestExample;
use rocket_shared::description::Documentation;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher, FlowRewriteFailure};
use std::sync::Arc;

pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
    audit_actor: AuditActor,
    /// Grants follow create, rename and delete when a store is attached.
    trust: Option<Arc<dyn CollectionTrustStore>>,
    /// Flow references follow a move when an updater is attached.
    flow_refs: Option<Arc<dyn FlowReferenceUpdater>>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self {
            repo,
            events,
            audit: Arc::new(NullSecurityAuditPublisher),
            audit_actor: AuditActor::System,
            trust: None,
            flow_refs: None,
        }
    }

    /// Attaches the trust store, so a rename moves the grants, a delete removes them and a
    /// new collection gets host environment access.
    pub fn with_trust_store(mut self, store: Arc<dyn CollectionTrustStore>) -> Self {
        self.trust = Some(store);
        self
    }

    /// Attaches the flow reference updater, so a move rewrites the flows that use the path.
    pub fn with_flow_references(mut self, updater: Arc<dyn FlowReferenceUpdater>) -> Self {
        self.flow_refs = Some(updater);
        self
    }

    /// Runs `f` while holding the collection lock of `collection`, so an editor write and an
    /// OpenAPI sync apply never interleave. Lock order: this lock first, then the sync
    /// transaction's guard. The std mutex is not reentrant, so a public method locks once and
    /// calls only `*_unlocked` helpers inside. When the root is unknown (the repository has no
    /// path, or the collection is missing) `f` runs without the lock and reports its own error.
    fn locked<T>(&self, collection: &str, f: impl FnOnce() -> T) -> T {
        with_collection_lock(self.repo.as_ref(), collection, f)
    }

    /// Like `locked` for a move between two collections. Both locks are taken in path order,
    /// so two opposite moves cannot deadlock, and once when both names are one collection.
    fn locked_pair<T>(&self, a: &str, b: &str, f: impl FnOnce() -> T) -> T {
        if a == b {
            return self.locked(a, f);
        }
        let (Ok(root_a), Ok(root_b)) = (
            self.repo.collection_root_path(a),
            self.repo.collection_root_path(b),
        ) else {
            return self.locked(a, f);
        };
        let (la, lb) = (collection_lock(&root_a), collection_lock(&root_b));
        if Arc::ptr_eq(&la, &lb) {
            let _guard = la.lock();
            return f();
        }
        let (first, second) = if root_a <= root_b {
            (&la, &lb)
        } else {
            (&lb, &la)
        };
        let _g1 = first.lock();
        let _g2 = second.lock();
        f()
    }

    /// Rewrites flow references after a successful move inside one collection.
    /// A failed rewrite never fails the move.
    fn after_move(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) {
        let src = src_path.strip_suffix('/').unwrap_or(src_path);
        let dst = dst_path.strip_suffix('/').unwrap_or(dst_path);
        let Some(updater) = &self.flow_refs else {
            return;
        };
        if src_collection != dst_collection {
            if src.is_empty() {
                return;
            }
            // A failed lookup never fails the move.
            let Ok(flows) = updater.flows_using(src_collection, src) else {
                return;
            };
            if flows.is_empty() {
                return;
            }
            self.events.publish(DomainEvent::FlowRequestPathsRewritten {
                collection: src_collection.to_string(),
                from: src.to_string(),
                to: dst.to_string(),
                flows: vec![],
                failed: flows
                    .into_iter()
                    .map(|flow| FlowRewriteFailure {
                        flow,
                        message: "moved out of the collection".to_string(),
                    })
                    .collect(),
            });
            return;
        }
        if dst.is_empty() || src == dst {
            return;
        }
        let report = updater.rewrite_request_paths(src_collection, src, dst);
        if report.rewritten.is_empty() && report.failed.is_empty() {
            return;
        }
        self.events.publish(DomainEvent::FlowRequestPathsRewritten {
            collection: src_collection.to_string(),
            from: src.to_string(),
            to: dst.to_string(),
            flows: report.rewritten,
            failed: report.failed,
        });
    }

    pub fn new_with_audit(
        repo: Box<dyn CollectionRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self {
            repo,
            events,
            audit,
            audit_actor: AuditActor::System,
            trust: None,
            flow_refs: None,
        }
    }

    /// Sets who the audit rows of this service name as the actor.
    pub fn with_audit_actor(mut self, actor: AuditActor) -> Self {
        self.audit_actor = actor;
        self
    }

    pub fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get(name)
    }

    /// Get collection tree with lightweight request summaries for sidebar display.
    pub fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get_summaries(name)
    }

    /// True when anything sits at `path` on disk, case-insensitively. See
    /// `CollectionRepository::path_exists`.
    pub fn path_exists(&self, name: &str, path: &str) -> DomainResult<bool> {
        self.repo.path_exists(name, path)
    }

    /// Get the full request at `path`, including body/headers/auth/scripts.
    /// Used by the frontend to fetch full data on demand for a sidebar item
    /// that was loaded via `get_summaries`.
    pub fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        self.repo.get_request(collection, path)
    }

    /// Get one WebSocket request by collection and relative path.
    pub fn get_websocket_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<WebSocketRequest> {
        self.repo.get_websocket_request(collection, path)
    }

    /// Save a WebSocket request and return it re-read from disk, like `save_request`.
    pub fn save_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<WebSocketRequest> {
        self.locked(collection, || {
            let actual_path = self
                .repo
                .save_websocket_request(collection, path, request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path.clone(),
            });
            self.repo.get_websocket_request(collection, &actual_path)
        })
    }

    /// Get the full GraphQL request at `path`.
    pub fn get_graphql_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<GraphQlRequest> {
        self.repo.get_graphql_request(collection, path)
    }

    /// Save a GraphQL request and return it as stored (the file name may differ from `path`).
    pub fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<GraphQlRequest> {
        self.locked(collection, || {
            let actual_path = self.repo.save_graphql_request(collection, path, request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path.clone(),
            });
            self.repo.get_graphql_request(collection, &actual_path)
        })
    }

    /// Get the full gRPC request at `path`.
    pub fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        self.repo.get_grpc_request(collection, path)
    }

    /// Save a gRPC request and return it as stored (the file name may differ from `path`).
    pub fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<GrpcRequest> {
        self.locked(collection, || {
            let actual_path = self.repo.save_grpc_request(collection, path, request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path.clone(),
            });
            self.repo.get_grpc_request(collection, &actual_path)
        })
    }

    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let collection = self.repo.create(name)?;
        // A collection made in Rocket requests nothing elevated, so it only gets host
        // environment access. A failure here leaves the collection untrusted, which is safe.
        if let (Some(trust), Ok(identity)) = (&self.trust, self.repo.collection_identity(name)) {
            let grant = CollectionGrant {
                process_env: true,
                source: GrantSource::Created,
                ..Default::default()
            };
            if let Err(e) = trust.put(&identity, grant) {
                tracing::warn!(error = %e, "could not record the grant of a new collection");
            }
        }
        self.events.publish(DomainEvent::CollectionCreated {
            name: name.to_string(),
        });
        Ok(collection)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.locked(name, || {
        let identity = self.repo.collection_identity(name).ok();
        self.repo.delete(name)?;
        if let (Some(trust), Some(identity)) = (&self.trust, identity) {
            if let Err(e) = trust.remove(&identity) {
                tracing::warn!(error = %e, "could not remove the grants of a deleted collection");
            }
        }
        self.audit.publish(
            self.audit_actor.into(),
            None,
            AuditEventKind::CollectionDeleted {
                collection: name.to_string(),
            },
        );
        self.events.publish(DomainEvent::CollectionDeleted {
            name: name.to_string(),
        });
        Ok(())
        })
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        self.locked(old_name, || {
        Collection::validate_name(new_name)?;
        let old_identity = self.repo.collection_identity(old_name).ok();
        self.repo.rename(old_name, new_name)?;
        // The user renamed it in the app, so the grants follow. If the new identity cannot
        // be read the grants stay behind and the collection is untrusted, which is safe.
        if let (Some(trust), Some(old)) = (&self.trust, old_identity) {
            if let Ok(new) = self.repo.collection_identity(new_name) {
                if let Err(e) = trust.rekey(&old, &new) {
                    tracing::warn!(error = %e, "could not move the grants of a renamed collection");
                }
            }
        }
        self.events.publish(DomainEvent::CollectionRenamed {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        });
        Ok(())
        })
    }

    pub fn save_request(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        self.locked(collection, || {
            let actual_path = self.repo.save_request(collection, path, request)?;
            // Publish only after the follow-up read succeeds.
            let saved = self.repo.get_request(collection, &actual_path)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            Ok(saved)
        })
    }

    pub fn rename_request(
        &self,
        collection: &str,
        old_path: &str,
        new_name: &str,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            let kind = self.repo.request_kind(collection, old_path)?;
            if kind == RequestKind::GraphQl {
                let mut request = self.repo.get_graphql_request(collection, old_path)?;
                request.name = new_name.to_string();
                let actual_path = self
                    .repo
                    .save_graphql_request(collection, old_path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            if kind == RequestKind::WebSocket {
                let mut websocket = self.repo.get_websocket_request(collection, old_path)?;
                websocket.name = new_name.to_string();
                let actual_path = self
                    .repo
                    .save_websocket_request(collection, old_path, &websocket)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            if kind == RequestKind::Grpc {
                let mut request = self.repo.get_grpc_request(collection, old_path)?;
                request.name = new_name.to_string();
                let actual_path = self
                    .repo
                    .save_grpc_request(collection, old_path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            // Only update the name field inside the JSON. The filename stays the same.
            // This produces a single Modify filesystem event.
            let mut request = self.repo.get_request(collection, old_path)?;
            request.name = new_name.to_string();
            let actual_path = self.repo.save_request(collection, old_path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            Ok(())
        })
    }

    pub fn update_request_docs(
        &self,
        collection: &str,
        path: &str,
        docs: Option<String>,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            let kind = self.repo.request_kind(collection, path)?;
            if kind == RequestKind::GraphQl {
                let mut request = self.repo.get_graphql_request(collection, path)?;
                request.docs = docs.map(Documentation::text);
                let actual_path = self.repo.save_graphql_request(collection, path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            if kind == RequestKind::WebSocket {
                let mut websocket = self.repo.get_websocket_request(collection, path)?;
                websocket.docs = docs;
                let actual_path = self
                    .repo
                    .save_websocket_request(collection, path, &websocket)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            if kind == RequestKind::Grpc {
                let mut request = self.repo.get_grpc_request(collection, path)?;
                request.docs = docs;
                let actual_path = self.repo.save_grpc_request(collection, path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            let mut request = self.repo.get_request(collection, path)?;
            request.docs = docs.map(Documentation::text);
            let actual_path = self.repo.save_request(collection, path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            Ok(())
        })
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.delete_request(collection, path)?;
            self.events.publish(DomainEvent::RequestDeleted {
                collection: collection.to_string(),
                path: path.to_string(),
            });
            Ok(())
        })
    }

    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.create_folder(collection, path)?;
            self.events.publish(DomainEvent::FolderCreated {
                collection: collection.to_string(),
                path: path.to_string(),
            });
            Ok(())
        })
    }

    /// Creates a folder that must not exist yet. See
    /// `CollectionRepository::create_folder_exclusive`.
    pub fn create_folder_exclusive(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.create_folder_exclusive(collection, path)?;
            self.events.publish(DomainEvent::FolderCreated {
                collection: collection.to_string(),
                path: path.to_string(),
            });
            Ok(())
        })
    }

    /// Saves a request that must not exist yet. See
    /// `CollectionRepository::create_request_exclusive`.
    pub fn create_request_exclusive(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        self.locked(collection, || {
            self.create_request_exclusive_unlocked(collection, path, request)
        })
    }

    /// The body of `create_request_exclusive`. The caller holds the collection lock.
    fn create_request_exclusive_unlocked(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        let actual_path = self
            .repo
            .create_request_exclusive(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_request(collection, &actual_path)
    }

    /// Creates a new request and refuses to replace an existing one. A name
    /// that differs only in case counts as taken, as the file may be one entry
    /// on a case-insensitive filesystem.
    pub fn create_request(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        let file_path = rocket_collection::request_filename_for(path);
        self.locked(collection, || {
            if self.repo.path_exists(collection, &file_path)? {
                return Err(DomainError::AlreadyExists(format!(
                    "{collection}/{file_path}"
                )));
            }
            self.create_request_exclusive_unlocked(collection, &file_path, request)
        })
    }

    /// Creates a new GraphQL request and refuses to replace an existing file.
    /// Names are compared like `create_request`.
    pub fn create_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<GraphQlRequest> {
        let file_path = rocket_collection::request_filename_for(path);
        self.locked(collection, || {
            self.ensure_path_free(collection, &file_path)?;
            let actual_path = self
                .repo
                .create_graphql_request_exclusive(collection, &file_path, request)?;
            self.publish_request_saved(collection, &actual_path);
            self.repo.get_graphql_request(collection, &actual_path)
        })
    }

    /// Creates a new gRPC request and refuses to replace an existing file.
    pub fn create_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<GrpcRequest> {
        let file_path = rocket_collection::request_filename_for(path);
        self.locked(collection, || {
            self.ensure_path_free(collection, &file_path)?;
            let actual_path = self
                .repo
                .create_grpc_request_exclusive(collection, &file_path, request)?;
            self.publish_request_saved(collection, &actual_path);
            self.repo.get_grpc_request(collection, &actual_path)
        })
    }

    /// Creates a new WebSocket request and refuses to replace an existing file.
    pub fn create_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<WebSocketRequest> {
        let file_path = rocket_collection::request_filename_for(path);
        self.locked(collection, || {
            self.ensure_path_free(collection, &file_path)?;
            let actual_path = self
                .repo
                .create_websocket_request_exclusive(collection, &file_path, request)?;
            self.publish_request_saved(collection, &actual_path);
            self.repo.get_websocket_request(collection, &actual_path)
        })
    }

    /// Fails with `AlreadyExists` when the path is taken, in any letter case.
    fn ensure_path_free(&self, collection: &str, file_path: &str) -> DomainResult<()> {
        if self.repo.path_exists(collection, file_path)? {
            return Err(DomainError::AlreadyExists(format!(
                "{collection}/{file_path}"
            )));
        }
        Ok(())
    }

    /// Duplicates an HTTP request found by its full path. The copy gets a new
    /// uid and a unique name in the source's folder, and no file is replaced.
    pub fn duplicate_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        self.locked(collection, || {
            self.duplicate_request_unlocked(collection, path)
        })
    }

    /// The body of `duplicate_request`. The caller holds the collection lock.
    fn duplicate_request_unlocked(&self, collection: &str, path: &str) -> DomainResult<Request> {
        if self.repo.request_kind(collection, path)? != RequestKind::Http {
            return Err(DomainError::InvalidInput(
                "Only HTTP requests can be duplicated".into(),
            ));
        }
        let source = self.repo.get_request(collection, path)?;
        let (dir, file) = match path.rsplit_once('/') {
            Some((dir, file)) => (Some(dir), file),
            None => (None, path),
        };
        let stem = [".yml", ".yaml", ".json"]
            .iter()
            .find_map(|ext| file.strip_suffix(ext))
            .unwrap_or(file);
        for n in 1..=1000u32 {
            let suffix = if n == 1 {
                " copy".to_string()
            } else {
                format!(" copy {n}")
            };
            let file_name = format!("{stem}{suffix}.yml");
            let candidate = match dir {
                Some(dir) => format!("{dir}/{file_name}"),
                None => file_name,
            };
            if self
                .repo
                .path_exists(collection, &candidate)
                .unwrap_or(true)
            {
                continue;
            }
            let mut copy = source.clone();
            copy.uid = rocket_collection::generate_uid();
            copy.name = format!("{}{suffix}", source.name);
            copy.seq = None;
            copy.file_name = None;
            match self.create_request_exclusive_unlocked(collection, &candidate, &copy) {
                Ok(saved) => return Ok(saved),
                Err(DomainError::AlreadyExists(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(DomainError::Conflict(
            "No free name for the duplicate request".into(),
        ))
    }

    /// Moves an item without replacing the destination. See
    /// `CollectionRepository::move_item_no_replace`.
    pub fn move_item_no_replace(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        // The flow rewrite runs after the lock is released.
        self.locked_pair(src_collection, dst_collection, || {
            self.repo
                .move_item_no_replace(src_collection, src_path, dst_collection, dst_path)?;
            self.events.publish(DomainEvent::ItemMoved {
                src_collection: src_collection.to_string(),
                src_path: src_path.to_string(),
                dst_collection: dst_collection.to_string(),
                dst_path: dst_path.to_string(),
            });
            Ok::<(), DomainError>(())
        })?;
        self.after_move(src_collection, src_path, dst_collection, dst_path);
        Ok(())
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.delete_folder(collection, path)?;
            self.events.publish(DomainEvent::FolderDeleted {
                collection: collection.to_string(),
                path: path.to_string(),
            });
            Ok(())
        })
    }

    /// Creates a starter `.js` file. The file watcher reports the change to the UI.
    pub fn create_script_file(
        &self,
        collection: &str,
        folder_path: &str,
        name: &str,
    ) -> DomainResult<String> {
        self.locked(collection, || {
            self.repo.create_script_file(collection, folder_path, name)
        })
    }

    pub fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String> {
        self.repo.read_script_file(collection, path)
    }

    pub fn save_script_file(
        &self,
        collection: &str,
        path: &str,
        content: &str,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.save_script_file(collection, path, content)
        })
    }

    pub fn rename_script_file(
        &self,
        collection: &str,
        path: &str,
        new_name: &str,
    ) -> DomainResult<String> {
        self.locked(collection, || {
            self.repo.rename_script_file(collection, path, new_name)
        })
    }

    pub fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo.delete_script_file(collection, path)
        })
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        // The flow rewrite runs after the lock is released.
        self.locked_pair(src_collection, dst_collection, || {
            self.repo
                .move_item(src_collection, src_path, dst_collection, dst_path)?;
            self.events.publish(DomainEvent::ItemMoved {
                src_collection: src_collection.to_string(),
                src_path: src_path.to_string(),
                dst_collection: dst_collection.to_string(),
                dst_path: dst_path.to_string(),
            });
            Ok::<(), DomainError>(())
        })?;
        self.after_move(src_collection, src_path, dst_collection, dst_path);
        Ok(())
    }

    /// Overwrites one script phase of a request, then tells listeners the
    /// request changed. The other phases and fields stay as they are.
    pub fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .save_request_script(collection, request_path, phase, body)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: request_path.to_string(),
            });
            Ok(())
        })
    }

    pub fn reorder_items(
        &self,
        collection: &str,
        folder_path: &str,
        ordered_names: &[String],
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .reorder_items(collection, folder_path, ordered_names)?;
            self.events.publish(DomainEvent::ItemsReordered {
                collection: collection.to_string(),
                folder_path: folder_path.to_string(),
            });
            Ok(())
        })
    }

    pub fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
        self.repo.get_settings(name)
    }

    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.locked(name, || {
            // The capability fields are requests that only the trust service may change. Keep
            // what the file has and ignore the incoming values, so an ordinary settings save
            // can neither approve nor drop a request.
            let on_disk = self.repo.get_settings(name)?;
            let mut merged = settings.clone();
            merged.sandbox_mode = on_disk.sandbox_mode;
            merged.script_context_roots = on_disk.script_context_roots;
            merged.agent_autonomy_enabled = on_disk.agent_autonomy_enabled;
            self.repo.save_settings(name, &merged)?;
            self.events.publish(DomainEvent::CollectionSettingsSaved {
                collection: name.to_string(),
            });
            Ok(())
        })
    }

    pub fn get_folder_chain_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo
            .get_folder_chain_variables(collection, request_path)
    }

    pub fn get_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_folder_variables(collection, folder_path)
    }

    pub fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .save_folder_variables(collection, folder_path, vars)?;
            self.events.publish(DomainEvent::FolderVariablesSaved {
                collection: collection.to_string(),
                folder_path: folder_path.to_string(),
            });
            Ok(())
        })
    }

    /// Reads one folder's own settings from its folder.yml. No chain walk.
    pub fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        self.repo.get_folder_settings(collection, folder_path)
    }

    /// Writes one folder's settings, then tells listeners the folder changed.
    pub fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .save_folder_settings(collection, folder_path, settings)?;
            self.events.publish(DomainEvent::FolderSettingsSaved {
                collection: collection.to_string(),
                folder_path: folder_path.to_string(),
            });
            Ok(())
        })
    }

    pub fn get_request_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_request_variables(collection, request_path)
    }

    pub fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .save_request_variables(collection, request_path, vars)?;
            self.events.publish(DomainEvent::RequestVariablesSaved {
                collection: collection.to_string(),
                request_path: request_path.to_string(),
            });
            Ok(())
        })
    }

    /// Lists the saved examples of an HTTP request. Publishes nothing.
    pub fn list_request_examples(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<HttpRequestExample>> {
        self.repo.list_request_examples(collection, request_path)
    }

    /// Adds an example to an HTTP request, then tells listeners the request changed.
    pub fn add_request_example(
        &self,
        collection: &str,
        request_path: &str,
        example: HttpRequestExample,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .add_request_example(collection, request_path, example)?;
            self.publish_request_saved(collection, request_path);
            Ok(())
        })
    }

    /// Renames an example of an HTTP request, then tells listeners the request changed.
    pub fn rename_request_example(
        &self,
        collection: &str,
        request_path: &str,
        old_name: &str,
        new_name: &str,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .rename_request_example(collection, request_path, old_name, new_name)?;
            self.publish_request_saved(collection, request_path);
            Ok(())
        })
    }

    /// Deletes an example of an HTTP request, then tells listeners the request changed.
    pub fn delete_request_example(
        &self,
        collection: &str,
        request_path: &str,
        name: &str,
    ) -> DomainResult<()> {
        self.locked(collection, || {
            self.repo
                .delete_request_example(collection, request_path, name)?;
            self.publish_request_saved(collection, request_path);
            Ok(())
        })
    }

    fn publish_request_saved(&self, collection: &str, request_path: &str) {
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: request_path.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_references::FlowRewriteReport;
    use rocket_shared::error::DomainError;
    use rocket_shared::events::{DomainEvent, EventPublisher, NullEventPublisher};
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    use crate::test_doubles::{CollectionMethod, NullCollectionRepo};

    /// Builds a strict stateful repo. `rename_request` still panics.
    fn mock_collection_repo() -> NullCollectionRepo {
        let collections: Arc<Mutex<Vec<Collection>>> = Arc::new(Mutex::new(Vec::new()));
        let requests: Arc<Mutex<Vec<(String, String, Request)>>> = Arc::new(Mutex::new(Vec::new()));

        let cols = collections.clone();
        let list = move || {
            let cols = cols.lock().expect("lock");
            Ok(cols
                .iter()
                .map(|c| {
                    CollectionSummary::new(String::new(), &c.name, "", c.request_count(), None)
                })
                .collect())
        };
        let find = |cols: &Arc<Mutex<Vec<Collection>>>, name: &str| {
            let cols = cols.lock().expect("lock");
            cols.iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        };
        let (cols_get, cols_sum) = (collections.clone(), collections.clone());
        let cols_create = collections.clone();
        let cols_delete = collections.clone();
        let cols_rename = collections;
        let (reqs_get, reqs_save, reqs_delete) = (requests.clone(), requests.clone(), requests);

        NullCollectionRepo::strict()
            .lenient_for(&[
                CollectionMethod::CreateFolder,
                CollectionMethod::DeleteFolder,
                CollectionMethod::MoveItem,
                CollectionMethod::ReorderItems,
                CollectionMethod::GetSettings,
                CollectionMethod::SaveSettings,
                CollectionMethod::GetFolderChainVariables,
                CollectionMethod::GetFolderVariables,
                CollectionMethod::SaveFolderVariables,
                CollectionMethod::GetRequestVariables,
                CollectionMethod::SaveRequestVariables,
                CollectionMethod::SaveRequestScript,
            ])
            .on_list(list)
            .on_get(move |name| find(&cols_get, name))
            .on_get_summaries(move |name| find(&cols_sum, name))
            .on_create(move |name| {
                let mut cols = cols_create.lock().expect("lock");
                if cols.iter().any(|c| c.name == name) {
                    return Err(DomainError::AlreadyExists(name.into()));
                }
                let col = Collection::new(name);
                cols.push(col.clone());
                Ok(col)
            })
            .on_delete(move |name| {
                let mut cols = cols_delete.lock().expect("lock");
                cols.retain(|c| c.name != name);
                Ok(())
            })
            .on_rename(move |old, new| {
                let mut cols = cols_rename.lock().expect("lock");
                if let Some(c) = cols.iter_mut().find(|c| c.name == old) {
                    c.name = new.to_string();
                    Ok(())
                } else {
                    Err(DomainError::NotFound(old.into()))
                }
            })
            .on_get_request(move |collection, path| {
                reqs_get
                    .lock()
                    .expect("lock")
                    .iter()
                    .find(|(c, p, _)| c == collection && p == path)
                    .map(|(_, _, r)| r.clone())
                    .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
            })
            .on_save_request(move |collection, path, request| {
                let mut requests = reqs_save.lock().expect("lock");
                requests.retain(|(c, p, _)| !(c == collection && p == path));
                requests.push((collection.to_string(), path.to_string(), request.clone()));
                Ok(path.to_string())
            })
            .on_delete_request(move |collection, path| {
                let mut requests = reqs_delete.lock().expect("lock");
                let len_before = requests.len();
                requests.retain(|(c, p, _)| !(c == collection && p == path));
                if requests.len() == len_before {
                    return Err(DomainError::NotFound(format!("{collection}/{path}")));
                }
                Ok(())
            })
    }

    fn make_service() -> CollectionService {
        CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn create_and_list() {
        let svc = make_service();
        svc.create("my-api").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn create_validates_name() {
        let svc = make_service();
        assert!(svc.create("").is_err());
        assert!(svc.create("has/slash").is_err());
    }

    #[test]
    fn rename() {
        let svc = make_service();
        svc.create("old").unwrap();
        svc.rename("old", "new").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list[0].name, "new");
    }

    #[test]
    fn delete_removes_collection() {
        let svc = make_service();
        svc.create("temp").unwrap();
        svc.delete("temp").unwrap();
        assert!(svc.list().unwrap().is_empty());
    }

    #[test]
    fn get_existing_collection() {
        let svc = make_service();
        svc.create("my-col").unwrap();
        let col = svc.get("my-col").unwrap();
        assert_eq!(col.name, "my-col");
    }

    #[test]
    fn get_nonexistent_collection_returns_error() {
        let svc = make_service();
        let result = svc.get("ghost");
        assert!(
            result.is_err(),
            "getting a non-existent collection must fail"
        );
    }

    #[test]
    fn rename_validates_new_name() {
        let svc = make_service();
        svc.create("col").unwrap();
        assert!(
            svc.rename("col", "").is_err(),
            "empty new name must be rejected"
        );
        assert!(
            svc.rename("col", "bad/name").is_err(),
            "slash in new name must be rejected"
        );
    }

    #[test]
    fn list_empty_initially() {
        let svc = make_service();
        assert!(svc.list().unwrap().is_empty());
    }

    struct CapturingPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    struct RecordingEventPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl EventPublisher for RecordingEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().expect("lock").push(event);
        }
    }
    struct SharedEventPublisher(Arc<RecordingEventPublisher>);
    impl EventPublisher for SharedEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.0.publish(event);
        }
    }

    #[test]
    fn create_emits_collection_created() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("my-api").expect("create");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published
                .iter()
                .any(|e| matches!(e, DomainEvent::CollectionCreated { name } if name == "my-api")),
            "expected CollectionCreated, got {:?}",
            *published
        );
    }

    #[test]
    fn delete_emits_collection_deleted() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("temp").expect("create");
        svc.delete("temp").expect("delete");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published
                .iter()
                .any(|e| matches!(e, DomainEvent::CollectionDeleted { name } if name == "temp")),
            "expected CollectionDeleted, got {:?}",
            *published
        );
    }

    #[test]
    fn rename_emits_collection_renamed() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("old").expect("create");
        svc.rename("old", "new").expect("rename");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionRenamed { old_name, new_name } if old_name == "old" && new_name == "new"
            )),
            "expected CollectionRenamed, got {:?}", *published
        );
    }

    #[test]
    fn save_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn save_request_publishes_nothing_when_the_follow_up_read_fails() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let repo = NullCollectionRepo::strict()
            .on_save_request(|_, path, _| Ok(path.to_string()))
            .on_get_request(|collection, path| {
                Err(DomainError::NotFound(format!("{collection}/{path}")))
            });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        assert!(svc.save_request("my-api", "users.yml", &request).is_err());
        assert!(publisher.events.lock().expect("lock").is_empty());
    }

    #[test]
    fn delete_request_emits_request_deleted() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        svc.delete_request("my-api", "users.yml")
            .expect("delete_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestDeleted { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestDeleted, got {:?}", *published
        );
    }

    #[test]
    fn delete_request_on_missing_request_publishes_nothing() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let result = svc.delete_request("my-api", "ghost.yml");
        assert!(result.is_err(), "deleting a nonexistent request must fail");
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish on failure"
        );
    }

    #[test]
    fn rename_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.rename_request("my-api", "users.yml", "List Users")
            .expect("rename_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn get_request_returns_the_saved_request() {
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");

        let loaded = svc.get_request("my-api", "users.yml").expect("get_request");
        assert_eq!(loaded.name, "Get Users");
        assert_eq!(loaded.url, "https://api.example.com/users");
    }

    #[test]
    fn get_request_errors_for_missing_request() {
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
        );
        assert!(svc.get_request("my-api", "ghost.yml").is_err());
    }

    #[test]
    fn update_request_docs_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.update_request_docs("my-api", "users.yml", Some("Fetches all users.".into()))
            .expect("update_request_docs");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn move_item_emits_item_moved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.move_item("my-api", "users.yml", "other-api", "users.yml")
            .expect("move_item");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemMoved { src_collection, src_path, dst_collection, dst_path }
                    if src_collection == "my-api" && src_path == "users.yml"
                        && dst_collection == "other-api" && dst_path == "users.yml"
            )),
            "expected ItemMoved, got {:?}",
            *published
        );
    }

    type Recorded = Arc<Mutex<Vec<(String, Request)>>>;

    fn dup_source() -> Request {
        let mut r = Request::new("Login", HttpMethod::Get, "http://x".to_string());
        r.seq = Some(3);
        r.file_name = Some("Login.yml".into());
        r
    }

    fn dup_service(
        source: Request,
        kind: impl Fn() -> DomainResult<RequestKind> + Send + Sync + 'static,
        exists: impl Fn(&str) -> DomainResult<bool> + Send + Sync + 'static,
        create_err_first: bool,
    ) -> (CollectionService, Recorded, Arc<RecordingEventPublisher>) {
        let recorded: Recorded = Arc::new(Mutex::new(vec![]));
        let rec = Arc::clone(&recorded);
        let calls = Arc::new(Mutex::new(0u32));
        let src = source;
        let repo = NullCollectionRepo::strict()
            .on_request_kind(move |_, _| kind())
            .on_get_request(move |_, _| Ok(src.clone()))
            .on_path_exists(move |_, p| exists(p))
            .on_create_request_exclusive(move |_, p, r| {
                let mut n = calls.lock().expect("lock");
                *n += 1;
                if create_err_first && *n == 1 {
                    return Err(DomainError::AlreadyExists("x".into()));
                }
                rec.lock().expect("lock").push((p.to_string(), r.clone()));
                Ok(p.to_string())
            });
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        (svc, recorded, publisher)
    }

    fn dup_ok(path: &str) -> (Recorded, Request) {
        let (svc, rec, _) =
            dup_service(dup_source(), || Ok(RequestKind::Http), |_| Ok(false), false);
        let out = svc.duplicate_request("c", path).expect("duplicate");
        (rec, out)
    }

    #[test]
    fn create_request_refuses_a_name_that_exists_even_in_another_case() {
        let (svc, rec, publisher) = dup_service(
            dup_source(),
            || Ok(RequestKind::Http),
            |p| Ok(p.to_lowercase() == "auth/login.yml"),
            false,
        );
        let blank = Request::new("LOGIN", HttpMethod::Get, String::new());
        let err = svc
            .create_request("c", "auth/LOGIN", &blank)
            .expect_err("exists");
        assert!(matches!(err, DomainError::AlreadyExists(_)), "got {err:?}");
        assert!(rec.lock().expect("lock").is_empty());
        assert!(publisher.events.lock().expect("lock").is_empty());
    }

    #[test]
    fn create_request_writes_a_new_name_and_publishes() {
        let (svc, rec, _) =
            dup_service(dup_source(), || Ok(RequestKind::Http), |_| Ok(false), false);
        let fresh = Request::new("Fresh", HttpMethod::Get, String::new());
        svc.create_request("c", "auth/Fresh", &fresh)
            .expect("created");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].0, "auth/Fresh.yml");
    }

    /// Builds the create tests of one non-HTTP request type. The fixture counts
    /// exclusive writes and records the published events.
    macro_rules! non_http_create_tests {
        ($module:ident, $ty:ident, $create:ident, $on_create:ident, $on_get:ident) => {
            mod $module {
                use super::*;

                fn fixture(
                    exists: bool,
                ) -> (CollectionService, Arc<RecordingEventPublisher>, Arc<Mutex<u32>>) {
                    let writes = Arc::new(Mutex::new(0u32));
                    let counter = Arc::clone(&writes);
                    let repo = NullCollectionRepo::strict()
                        .on_path_exists(move |_, _| Ok(exists))
                        .$on_create(move |_, p, _| {
                            *counter.lock().expect("writes") += 1;
                            Ok(p.to_string())
                        })
                        .$on_get(|_, _| Ok($ty::new("Fresh", "https://e.example")));
                    let publisher = Arc::new(RecordingEventPublisher {
                        events: Mutex::new(vec![]),
                    });
                    let svc = CollectionService::new(
                        Box::new(repo),
                        Box::new(SharedEventPublisher(Arc::clone(&publisher))),
                    );
                    (svc, publisher, writes)
                }

                #[test]
                fn refuses_a_name_that_exists_even_in_another_case() {
                    let (svc, publisher, writes) = fixture(true);
                    let request = $ty::new("Orders", "https://e.example");
                    let err = svc
                        .$create("col", "Orders", &request)
                        .expect_err("name is taken");
                    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
                    assert_eq!(*writes.lock().expect("writes"), 0);
                    assert!(publisher.events.lock().expect("lock").is_empty());
                }

                #[test]
                fn writes_a_new_name_and_publishes() {
                    let (svc, publisher, writes) = fixture(false);
                    let request = $ty::new("Fresh", "https://e.example");
                    svc.$create("col", "Fresh", &request).expect("created");
                    assert_eq!(*writes.lock().expect("writes"), 1);
                    let events = publisher.events.lock().expect("lock");
                    assert!(matches!(
                        events.as_slice(),
                        [DomainEvent::RequestSaved { collection, path }]
                            if collection == "col" && path == "Fresh.yml"
                    ));
                }
            }
        };
    }

    non_http_create_tests!(
        create_graphql_request,
        GraphQlRequest,
        create_graphql_request,
        on_create_graphql_request_exclusive,
        on_get_graphql_request
    );
    non_http_create_tests!(
        create_grpc_request,
        GrpcRequest,
        create_grpc_request,
        on_create_grpc_request_exclusive,
        on_get_grpc_request
    );
    non_http_create_tests!(
        create_websocket_request,
        WebSocketRequest,
        create_websocket_request,
        on_create_websocket_request_exclusive,
        on_get_websocket_request
    );

    #[test]
    fn duplicate_request_creates_a_copy_with_a_new_uid_and_name() {
        let (rec, _) = dup_ok("auth/Login.yml");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec[0].0, "auth/Login copy.yml");
        assert_eq!(rec[0].1.name, "Login copy");
        assert!(!rec[0].1.uid.is_empty());
        assert_ne!(rec[0].1.uid, dup_source().uid);
    }

    #[test]
    fn duplicate_request_clears_seq() {
        let (rec, _) = dup_ok("auth/Login.yml");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec[0].1.seq, None);
        assert_eq!(rec[0].1.file_name, None);
    }

    #[test]
    fn duplicate_request_copies_every_other_field() {
        let mut source = dup_source();
        source.headers = vec![rocket_shared::types::Header {
            key: "X-A".into(),
            value: "1".into(),
            enabled: true,
            description: None,
        }];
        source.tags = vec!["t".into()];
        source.pre_request_script = Some("pre".into());
        source.post_response_script = Some("post".into());
        source.tests = Some("tests".into());
        source.variables = vec![CollectionVariable {
            key: "v".into(),
            value: "1".into(),
            initial_value: "1".into(),
            enabled: true,
            secret: false,
        }];
        let (svc, rec, _) = dup_service(
            source.clone(),
            || Ok(RequestKind::Http),
            |_| Ok(false),
            false,
        );
        svc.duplicate_request("c", "Login.yml").expect("duplicate");
        let mut expected = source;
        let got = rec.lock().expect("lock")[0].1.clone();
        expected.uid = got.uid.clone();
        expected.name = "Login copy".into();
        expected.seq = None;
        expected.file_name = None;
        assert_eq!(got, expected);
    }

    #[test]
    fn duplicate_request_rejects_a_graphql_request() {
        let (svc, rec, _) = dup_service(
            dup_source(),
            || Ok(RequestKind::GraphQl),
            |_| Ok(false),
            false,
        );
        let err = svc
            .duplicate_request("c", "Login.yml")
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(rec.lock().expect("lock").is_empty());
    }

    #[test]
    fn duplicate_request_of_a_missing_request_is_not_found() {
        let (svc, _, _) = dup_service(
            dup_source(),
            || Err(DomainError::NotFound("x".into())),
            |_| Ok(false),
            false,
        );
        let err = svc
            .duplicate_request("c", "Login.yml")
            .expect_err("missing");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn duplicate_request_publishes_request_saved() {
        let (svc, _, publisher) =
            dup_service(dup_source(), || Ok(RequestKind::Http), |_| Ok(false), false);
        svc.duplicate_request("c", "auth/Login.yml")
            .expect("duplicate");
        let published = publisher.events.lock().expect("lock");
        assert_eq!(published.len(), 1);
        assert!(matches!(
            &published[0],
            DomainEvent::RequestSaved { collection, path }
                if collection == "c" && path == "auth/Login copy.yml"
        ));
    }

    #[test]
    fn duplicate_request_skips_a_taken_name() {
        let (svc, rec, _) = dup_service(
            dup_source(),
            || Ok(RequestKind::Http),
            |p| Ok(p == "auth/Login copy.yml"),
            false,
        );
        svc.duplicate_request("c", "auth/Login.yml")
            .expect("duplicate");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec[0].0, "auth/Login copy 2.yml");
        assert_eq!(rec[0].1.name, "Login copy 2");
    }

    #[test]
    fn duplicate_request_retries_the_next_name_when_the_file_appears_meanwhile() {
        let (svc, rec, _) =
            dup_service(dup_source(), || Ok(RequestKind::Http), |_| Ok(false), true);
        svc.duplicate_request("c", "auth/Login.yml")
            .expect("duplicate");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].0, "auth/Login copy 2.yml");
    }

    #[test]
    fn duplicate_request_gives_up_after_1000_names() {
        let count = Arc::new(Mutex::new(0u32));
        let c = Arc::clone(&count);
        let (svc, rec, _) = dup_service(
            dup_source(),
            || Ok(RequestKind::Http),
            move |_| {
                *c.lock().expect("lock") += 1;
                Ok(true)
            },
            false,
        );
        let err = svc.duplicate_request("c", "Login.yml").expect_err("full");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(*count.lock().expect("lock"), 1000);
        assert!(rec.lock().expect("lock").is_empty());
    }

    #[test]
    fn duplicate_request_skips_a_candidate_when_the_existence_check_fails() {
        let (svc, rec, _) = dup_service(
            dup_source(),
            || Ok(RequestKind::Http),
            |p| {
                if p == "auth/Login copy.yml" {
                    Err(DomainError::Internal("boom".into()))
                } else {
                    Ok(false)
                }
            },
            false,
        );
        svc.duplicate_request("c", "auth/Login.yml")
            .expect("duplicate");
        assert_eq!(rec.lock().expect("lock")[0].0, "auth/Login copy 2.yml");
    }

    #[test]
    fn duplicate_request_uses_the_source_file_stem_for_the_path() {
        let mut source = dup_source();
        source.name = "A/B: C".into();
        let (svc, rec, _) = dup_service(source, || Ok(RequestKind::Http), |_| Ok(false), false);
        svc.duplicate_request("c", "auth/ab.yml")
            .expect("duplicate");
        let rec = rec.lock().expect("lock");
        assert_eq!(rec[0].0, "auth/ab copy.yml");
        assert_eq!(rec[0].1.name, "A/B: C copy");
    }

    #[test]
    fn duplicate_request_of_a_legacy_json_source_writes_a_yml_copy() {
        let (rec, _) = dup_ok("auth/Foo.json");
        assert_eq!(rec.lock().expect("lock")[0].0, "auth/Foo copy.yml");
    }

    #[test]
    fn duplicate_request_in_the_root_has_no_leading_slash() {
        let (rec, _) = dup_ok("Login.yml");
        assert_eq!(rec.lock().expect("lock")[0].0, "Login copy.yml");
    }

    struct RecordingUpdater {
        calls: Mutex<Vec<(String, String, String)>>,
        report: Mutex<FlowRewriteReport>,
        using: Mutex<DomainResult<Vec<String>>>,
        using_calls: Mutex<Vec<(String, String)>>,
    }
    impl Default for RecordingUpdater {
        fn default() -> Self {
            Self {
                calls: Mutex::new(vec![]),
                report: Mutex::new(FlowRewriteReport::default()),
                using: Mutex::new(Ok(vec![])),
                using_calls: Mutex::new(vec![]),
            }
        }
    }
    impl FlowReferenceUpdater for RecordingUpdater {
        fn rewrite_request_paths(
            &self,
            collection: &str,
            from: &str,
            to: &str,
        ) -> FlowRewriteReport {
            self.calls.lock().expect("lock").push((
                collection.to_string(),
                from.to_string(),
                to.to_string(),
            ));
            self.report.lock().expect("lock").clone()
        }
        fn flows_using(&self, collection: &str, path: &str) -> DomainResult<Vec<String>> {
            self.using_calls
                .lock()
                .expect("lock")
                .push((collection.to_string(), path.to_string()));
            match &*self.using.lock().expect("lock") {
                Ok(flows) => Ok(flows.clone()),
                Err(e) => Err(DomainError::Internal(e.to_string())),
            }
        }
    }

    fn flow_fixture(
        repo: NullCollectionRepo,
        report: FlowRewriteReport,
    ) -> (
        CollectionService,
        Arc<RecordingUpdater>,
        Arc<RecordingEventPublisher>,
    ) {
        let updater = Arc::new(RecordingUpdater::default());
        *updater.report.lock().expect("lock") = report;
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        )
        .with_flow_references(updater.clone());
        (svc, updater, publisher)
    }

    fn one_rewritten() -> FlowRewriteReport {
        FlowRewriteReport {
            rewritten: vec!["A".into()],
            failed: vec![],
        }
    }

    fn flow_events(publisher: &RecordingEventPublisher) -> usize {
        publisher
            .events
            .lock()
            .expect("lock")
            .iter()
            .filter(|e| matches!(e, DomainEvent::FlowRequestPathsRewritten { .. }))
            .count()
    }

    fn expected_call() -> Vec<(String, String, String)> {
        vec![("c".into(), "auth".into(), "identity".into())]
    }

    #[test]
    fn move_item_rewrites_flows_in_the_same_collection() {
        let (svc, updater, publisher) = flow_fixture(mock_collection_repo(), one_rewritten());
        svc.move_item("c", "auth", "c", "identity").expect("move");
        assert_eq!(*updater.calls.lock().expect("lock"), expected_call());
        let events = publisher.events.lock().expect("lock");
        assert!(events
            .iter()
            .any(|e| matches!(e, DomainEvent::ItemMoved { .. })));
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::FlowRequestPathsRewritten { collection, from, to, flows, failed }
                if collection == "c" && from == "auth" && to == "identity"
                    && flows == &vec!["A".to_string()] && failed.is_empty()
        )));
    }

    #[test]
    fn move_item_does_not_rewrite_when_the_move_fails() {
        let repo = NullCollectionRepo::new()
            .on_move_item(|_, _, _, _| Err(DomainError::NotFound("x".into())));
        let (svc, updater, publisher) = flow_fixture(repo, one_rewritten());
        assert!(svc.move_item("c", "auth", "c", "identity").is_err());
        assert!(updater.calls.lock().expect("lock").is_empty());
        assert!(publisher.events.lock().expect("lock").is_empty());
    }

    #[test]
    fn move_item_no_replace_rewrites_flows() {
        let (svc, updater, publisher) = flow_fixture(mock_collection_repo(), one_rewritten());
        svc.move_item_no_replace("c", "auth", "c", "identity")
            .expect("move");
        assert_eq!(*updater.calls.lock().expect("lock"), expected_call());
        assert_eq!(flow_events(&publisher), 1);
    }

    #[test]
    fn move_item_succeeds_when_a_flow_rewrite_fails() {
        let report = FlowRewriteReport {
            rewritten: vec![],
            failed: vec![rocket_shared::events::FlowRewriteFailure {
                flow: "B".into(),
                message: "bad".into(),
            }],
        };
        let (svc, _updater, publisher) = flow_fixture(mock_collection_repo(), report);
        svc.move_item("c", "auth", "c", "identity")
            .expect("move succeeds");
        let events = publisher.events.lock().expect("lock");
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::FlowRequestPathsRewritten { failed, .. }
                if failed.len() == 1 && failed[0].flow == "B"
        )));
    }

    #[test]
    fn move_item_publishes_no_flow_event_when_nothing_was_rewritten() {
        let (svc, _updater, publisher) =
            flow_fixture(mock_collection_repo(), FlowRewriteReport::default());
        svc.move_item("c", "auth", "c", "identity").expect("move");
        assert_eq!(flow_events(&publisher), 0);
        assert_eq!(publisher.events.lock().expect("lock").len(), 1);
    }

    #[test]
    fn move_item_without_an_updater_behaves_as_before() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.move_item("c", "auth", "c", "identity").expect("move");
        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::ItemMoved { .. }));
    }

    #[test]
    fn move_item_skips_the_rewrite_for_an_empty_destination_or_same_path() {
        let (svc, updater, _publisher) = flow_fixture(mock_collection_repo(), one_rewritten());
        svc.move_item("c", "a.yml", "c", "").expect("move");
        svc.move_item("c", "a", "c", "a/").expect("move");
        assert!(updater.calls.lock().expect("lock").is_empty());
    }

    #[test]
    fn move_item_strips_trailing_slashes_before_rewriting() {
        let (svc, updater, _publisher) = flow_fixture(mock_collection_repo(), one_rewritten());
        svc.move_item("c", "auth/", "c", "identity/").expect("move");
        assert_eq!(*updater.calls.lock().expect("lock"), expected_call());
    }

    fn other_collection_fixture(
        using: DomainResult<Vec<String>>,
    ) -> (
        CollectionService,
        Arc<RecordingUpdater>,
        Arc<RecordingEventPublisher>,
    ) {
        let (svc, updater, publisher) =
            flow_fixture(mock_collection_repo(), FlowRewriteReport::default());
        *updater.using.lock().expect("lock") = using;
        (svc, updater, publisher)
    }

    #[test]
    fn move_item_to_another_collection_reports_dangling_flows() {
        let (svc, updater, publisher) = other_collection_fixture(Ok(vec!["A".into()]));
        svc.move_item("c", "a.yml", "d", "a.yml").expect("move");
        assert!(updater.calls.lock().expect("lock").is_empty());
        assert_eq!(
            *updater.using_calls.lock().expect("lock"),
            vec![("c".to_string(), "a.yml".to_string())]
        );
        let events = publisher.events.lock().expect("lock");
        let found: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::FlowRequestPathsRewritten { .. }))
            .collect();
        assert_eq!(found.len(), 1);
        assert!(matches!(
            found[0],
            DomainEvent::FlowRequestPathsRewritten { collection, from, to, flows, failed }
                if collection == "c" && from == "a.yml" && to == "a.yml"
                    && flows.is_empty()
                    && failed.len() == 1
                    && failed[0].flow == "A"
                    && failed[0].message == "moved out of the collection"
        ));
    }

    #[test]
    fn move_item_to_another_collection_ignores_a_lookup_error() {
        let (svc, _updater, publisher) =
            other_collection_fixture(Err(DomainError::Internal("x".into())));
        svc.move_item("c", "a.yml", "d", "a.yml").expect("move");
        assert_eq!(flow_events(&publisher), 0);
    }

    #[test]
    fn move_item_to_another_collection_with_no_users_publishes_no_flow_event() {
        let (svc, _updater, publisher) = other_collection_fixture(Ok(vec![]));
        svc.move_item("c", "a.yml", "d", "a.yml").expect("move");
        assert_eq!(flow_events(&publisher), 0);
    }

    #[test]
    fn move_item_to_another_collection_with_an_empty_destination_still_reports() {
        let (svc, updater, publisher) = other_collection_fixture(Ok(vec!["A".into()]));
        svc.move_item("c", "a.yml", "d", "").expect("move");
        assert_eq!(updater.using_calls.lock().expect("lock").len(), 1);
        assert_eq!(flow_events(&publisher), 1);
    }

    #[test]
    fn create_folder_emits_folder_created() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create_folder("my-api", "auth").expect("create_folder");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderCreated { collection, path } if collection == "my-api" && path == "auth"
            )),
            "expected FolderCreated, got {:?}", *published
        );
    }

    #[test]
    fn delete_folder_emits_folder_deleted() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.delete_folder("my-api", "auth").expect("delete_folder");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderDeleted { collection, path } if collection == "my-api" && path == "auth"
            )),
            "expected FolderDeleted, got {:?}", *published
        );
    }

    #[test]
    fn reorder_items_emits_items_reordered() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.reorder_items(
            "my-api",
            "auth",
            &["login.yml".to_string(), "logout.yml".to_string()],
        )
        .expect("reorder_items");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemsReordered { collection, folder_path } if collection == "my-api" && folder_path == "auth"
            )),
            "expected ItemsReordered, got {:?}", *published
        );
    }

    #[test]
    fn save_settings_emits_collection_settings_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_settings("my-api", &rocket_collection::CollectionSettings::default())
            .expect("save_settings");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionSettingsSaved { collection } if collection == "my-api"
            )),
            "expected CollectionSettingsSaved, got {:?}",
            *published
        );
    }

    #[test]
    fn save_folder_variables_emits_folder_variables_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_folder_variables("my-api", "auth", vec![])
            .expect("save_folder_variables");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderVariablesSaved { collection, folder_path } if collection == "my-api" && folder_path == "auth"
            )),
            "expected FolderVariablesSaved, got {:?}", *published
        );
    }

    #[test]
    fn save_request_variables_emits_request_variables_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(mock_collection_repo()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_request_variables("my-api", "users.yml", vec![])
            .expect("save_request_variables");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestVariablesSaved { collection, request_path } if collection == "my-api" && request_path == "users.yml"
            )),
            "expected RequestVariablesSaved, got {:?}", *published
        );
    }

    #[test]
    fn rename_request_keeps_a_graphql_item_graphql() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GraphQlRequest::new("Old", "https://x/graphql").with_query("{ a }");
        repo.save_graphql_request("api", "q.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "q.yml", "New").expect("rename");

        let back = svc.get_graphql_request("api", "q.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.body.query, "{ a }");
    }

    #[test]
    fn delete_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher {
            captured: Mutex::new(vec![]),
        });
        let svc = CollectionService::new_with_audit(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );
        svc.create("victim").unwrap();
        svc.delete("victim").unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::CollectionDeleted { collection } if collection == "victim"
            )),
            "expected CollectionDeleted, got {:?}",
            *captured
        );
    }

    #[test]
    fn delete_records_the_configured_actor() {
        use rocket_audit::publisher::RecordingSecurityAuditPublisher;
        let publisher = Arc::new(RecordingSecurityAuditPublisher::new());
        let svc = CollectionService::new_with_audit(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        )
        .with_audit_actor(AuditActor::User);
        svc.create("victim").unwrap();
        svc.delete("victim").unwrap();
        let records = publisher.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].actor, "user");
    }

    #[test]
    fn delete_defaults_to_the_system_actor() {
        use rocket_audit::publisher::RecordingSecurityAuditPublisher;
        let publisher = Arc::new(RecordingSecurityAuditPublisher::new());
        let svc = CollectionService::new_with_audit(
            Box::new(mock_collection_repo()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );
        svc.create("victim").unwrap();
        svc.delete("victim").unwrap();
        assert_eq!(publisher.records()[0].actor, "system");
    }

    #[test]
    fn rename_request_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GrpcRequest::new("Old", "localhost:50051");
        repo.save_grpc_request("api", "call.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "call.yml", "New")
            .expect("rename");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.url, "localhost:50051");
        assert_eq!(back.uid, g.uid);
    }

    #[test]
    fn update_request_docs_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.update_request_docs("api", "call.yml", Some("Calls the greeter".into()))
            .expect("docs");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.docs.as_deref(), Some("Calls the greeter"));
    }

    #[test]
    fn save_grpc_request_returns_the_stored_request_and_publishes_an_event() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );

        let saved = svc
            .save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("call.yml"));
        let events = publisher.events.lock().expect("lock");
        assert!(
            matches!(events.as_slice(), [DomainEvent::RequestSaved { path, .. }] if path == "call.yml"),
            "{events:?}"
        );
    }
}

#[cfg(test)]
mod websocket_tests {
    use super::*;
    use rocket_collection::WebSocketRequest;
    use rocket_shared::events::NullEventPublisher;

    fn service(dir: &std::path::Path) -> CollectionService {
        CollectionService::new(
            Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                dir.to_path_buf(),
            )),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn rename_request_renames_a_websocket_request_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let saved = svc
            .save_websocket_request(
                "api",
                "chat",
                &WebSocketRequest::new("Chat", "wss://chat.example.com/ws"),
            )
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("chat.yml"));

        svc.rename_request("api", "chat.yml", "Team Chat")
            .expect("rename");

        let renamed = svc
            .get_websocket_request("api", "chat.yml")
            .expect("reload");
        assert_eq!(renamed.name, "Team Chat");
        assert_eq!(renamed.uid, saved.uid);
        assert_eq!(renamed.url, "wss://chat.example.com/ws");
    }

    #[test]
    fn update_request_docs_keeps_a_websocket_request_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");

        svc.update_request_docs("api", "chat.yml", Some("# Notes".into()))
            .expect("update docs");

        let back = svc
            .get_websocket_request("api", "chat.yml")
            .expect("reload");
        assert_eq!(back.docs.as_deref(), Some("# Notes"));
        assert_eq!(back.url, "wss://x/ws");
    }

    #[test]
    fn request_variables_round_trip_for_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");
        let var = rocket_collection::CollectionVariable {
            key: "room".into(),
            value: "general".into(),
            initial_value: "general".into(),
            enabled: true,
            secret: false,
        };

        svc.save_request_variables("api", "chat.yml", vec![var.clone()])
            .expect("save vars");

        assert_eq!(
            svc.get_request_variables("api", "chat.yml")
                .expect("get vars"),
            vec![var]
        );
        assert!(
            svc.get_websocket_request("api", "chat.yml").is_ok(),
            "still a websocket file"
        );
    }

    #[test]
    fn rename_request_still_works_for_an_http_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let req = Request::new("Get", rocket_shared::types::HttpMethod::Get, "https://x");
        svc.save_request("api", "get", &req).expect("save");

        svc.rename_request("api", "get.yml", "Fetch")
            .expect("rename");

        assert_eq!(
            svc.get_request("api", "get.yml").expect("reload").name,
            "Fetch"
        );
    }
}

#[cfg(test)]
mod folder_settings_tests {
    use super::*;
    use rocket_shared::types::Header;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<DomainEvent>>);

    struct SharedRecorder(Arc<Recorder>);

    impl EventPublisher for SharedRecorder {
        fn publish(&self, event: DomainEvent) {
            self.0 .0.lock().expect("lock").push(event);
        }
    }

    /// A service over a temp collection "api" with one folder "auth".
    fn service(dir: &std::path::Path) -> (CollectionService, Arc<Recorder>) {
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.to_path_buf());
        repo.create("api").expect("create collection");
        repo.create_folder("api", "auth").expect("create folder");
        let recorder = Arc::new(Recorder::default());
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedRecorder(Arc::clone(&recorder))),
        );
        (svc, recorder)
    }

    fn sample() -> FolderSettings {
        FolderSettings {
            headers: vec![Header::new("X-Team", "core")],
            pre_request_script: Some("console.log('auth pre');".into()),
            docs: Some("# Auth folder".into()),
            ..FolderSettings::default()
        }
    }

    #[test]
    fn save_folder_settings_persists_and_emits_folder_settings_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        svc.save_folder_settings("api", "auth", &sample())
            .expect("save_folder_settings");

        assert_eq!(
            svc.get_folder_settings("api", "auth")
                .expect("get_folder_settings"),
            sample()
        );
        let events = recorder.0.lock().expect("lock");
        assert!(
            matches!(
                events.as_slice(),
                [DomainEvent::FolderSettingsSaved { collection, folder_path }]
                    if collection == "api" && folder_path == "auth"
            ),
            "expected one FolderSettingsSaved, got {events:?}"
        );
    }

    #[test]
    fn get_folder_settings_publishes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        let settings = svc.get_folder_settings("api", "auth").expect("get");

        assert_eq!(settings, FolderSettings::default());
        assert!(recorder.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn failed_folder_settings_save_publishes_no_event() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        // The spec makes a save to a missing folder an InvalidInput error (Plan 02).
        let result = svc.save_folder_settings("api", "ghost", &sample());

        assert!(result.is_err(), "a save to a missing folder must fail");
        assert!(
            recorder.0.lock().expect("lock").is_empty(),
            "a failed save must not publish an event"
        );
    }
}

#[cfg(test)]
mod trust_tests {
    use super::*;
    use rocket_collection::settings::SandboxMode;
    use rocket_collection::CollectionSettings;
    use rocket_infra::{FsCollectionRepo, FsCollectionTrustStore};
    use rocket_shared::events::NullEventPublisher;

    fn setup() -> (
        tempfile::TempDir,
        CollectionService,
        Arc<FsCollectionTrustStore>,
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(FsCollectionTrustStore::new(dir.path().join("trust.yml")));
        let svc = CollectionService::new(
            Box::new(FsCollectionRepo::new_standalone(
                dir.path().join("collections"),
            )),
            Box::new(NullEventPublisher),
        )
        .with_trust_store(Arc::clone(&store) as Arc<dyn CollectionTrustStore>);
        (dir, svc, store)
    }

    fn identity(dir: &tempfile::TempDir, name: &str) -> rocket_collection::CollectionIdentity {
        FsCollectionRepo::new_standalone(dir.path().join("collections"))
            .collection_identity(name)
            .expect("identity")
    }

    #[test]
    fn create_grants_process_env_only() {
        let (dir, svc, store) = setup();
        svc.create("api").expect("create");
        let grant = store
            .grant_for(&identity(&dir, "api"))
            .expect("read")
            .expect("grant");
        assert!(grant.process_env);
        assert!(!grant.developer_mode && !grant.agent_run);
        assert!(grant.context_roots.is_empty());
        assert_eq!(grant.source, GrantSource::Created);
    }

    #[test]
    fn rename_moves_the_grant_and_delete_removes_it() {
        let (dir, svc, store) = setup();
        svc.create("api").expect("create");
        svc.rename("api", "api2").expect("rename");
        let grant = store
            .grant_for(&identity(&dir, "api2"))
            .expect("read")
            .expect("grant follows the rename");
        assert!(grant.process_env);

        let id = identity(&dir, "api2");
        svc.delete("api2").expect("delete");
        assert_eq!(store.grant_for(&id).expect("read"), None);
    }

    #[test]
    fn save_settings_keeps_the_capability_fields_from_disk() {
        let (dir, svc, _store) = setup();
        svc.create("api").expect("create");
        // A pull put the requests in the file.
        let repo = FsCollectionRepo::new_standalone(dir.path().join("collections"));
        repo.save_settings(
            "api",
            &CollectionSettings {
                sandbox_mode: SandboxMode::Developer,
                script_context_roots: vec!["../shared".into()],
                agent_autonomy_enabled: true,
                ..Default::default()
            },
        )
        .expect("seed");

        // An ordinary save that tries to change them, and one that drops them.
        let mut incoming = svc.get_settings("api").expect("get");
        incoming.docs = Some("hello".into());
        incoming.sandbox_mode = SandboxMode::Safe;
        incoming.script_context_roots.clear();
        incoming.agent_autonomy_enabled = false;
        svc.save_settings("api", &incoming).expect("save");

        let saved = svc.get_settings("api").expect("get");
        assert_eq!(saved.docs.as_deref(), Some("hello"));
        assert_eq!(saved.sandbox_mode, SandboxMode::Developer);
        assert_eq!(saved.script_context_roots, vec!["../shared"]);
        assert!(saved.agent_autonomy_enabled);

        // And it cannot turn them on.
        let (dir2, svc2, _s) = setup();
        svc2.create("api").expect("create");
        let mut incoming = svc2.get_settings("api").expect("get");
        incoming.sandbox_mode = SandboxMode::Developer;
        incoming.agent_autonomy_enabled = true;
        svc2.save_settings("api", &incoming).expect("save");
        let saved = svc2.get_settings("api").expect("get");
        assert_eq!(saved.sandbox_mode, SandboxMode::Safe);
        assert!(!saved.agent_autonomy_enabled);
        drop(dir2);
        drop(dir);
    }
}
