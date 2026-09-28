use std::path::PathBuf;

use rocket_shared::error::{DomainError, DomainResult};

use crate::collection::Collection;
use crate::folder_settings::FolderSettings;
use crate::graphql_request::GraphQlRequest;
use crate::grpc_request::GrpcRequest;
use crate::request::Request;
use crate::request_kind::RequestKind;
use crate::settings::{CollectionSettings, CollectionVariable};
use crate::summary::CollectionSummary;
use crate::websocket::WebSocketRequest;

/// Identifies which of a request's three script fields `save_request_script`
/// targets. A local enum (not a reuse of `rocket-scripting::ScriptPhase`) —
/// see this task's doc comment in the plan for why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}

/// Repository trait for Collection persistence.
/// Implemented by FsCollectionRepo in rocket-infra.
pub trait CollectionRepository: Send + Sync {
    /// List all collections (lightweight summaries).
    fn list(&self) -> DomainResult<Vec<CollectionSummary>>;

    /// Get full collection tree by name.
    fn get(&self, name: &str) -> DomainResult<Collection>;

    /// Get collection tree with lightweight request summaries instead of full Request bodies.
    /// Use for sidebar loads; call `get_request` for the full body when the user opens a request.
    fn get_summaries(&self, name: &str) -> DomainResult<Collection>;

    /// Create a new empty collection.
    fn create(&self, name: &str) -> DomainResult<Collection>;

    /// Delete a collection and all its contents.
    fn delete(&self, name: &str) -> DomainResult<()>;

    /// Rename a collection.
    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()>;

    /// Read a single request file by collection name and relative path.
    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request>;

    /// Save a request to a specific path within a collection.
    /// Returns the actual filename used (may differ from `path` if a unique name was generated).
    ///
    /// **Variables ownership:** `request.variables` is NOT authoritative for the stored
    /// `runtime.variables` block. Use `save_request_variables` to mutate variables; callers
    /// that pass an empty `request.variables` will have the existing on-disk variables preserved.
    fn save_request(&self, collection: &str, path: &str, request: &Request)
        -> DomainResult<String>;

    /// Rename a request file within a collection (fs::rename, single event).
    fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()>;

    /// Delete a request file.
    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Read one GraphQL request file. Repositories without GraphQL support keep this default.
    fn get_graphql_request(&self, _collection: &str, _path: &str) -> DomainResult<GraphQlRequest> {
        Err(DomainError::InvalidInput(
            "this repository does not support GraphQL requests".into(),
        ))
    }

    /// Save a GraphQL request. Returns the actual filename written, like `save_request`.
    fn save_graphql_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &GraphQlRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "this repository does not support GraphQL requests".into(),
        ))
    }

    /// Which protocol the request file at `path` holds. Repositories that only know HTTP keep this default.
    fn request_kind(&self, _collection: &str, _path: &str) -> DomainResult<RequestKind> {
        Ok(RequestKind::Http)
    }

    /// Read one WebSocket request file. Repositories that do not store
    /// WebSocket requests keep this default.
    fn get_websocket_request(
        &self,
        _collection: &str,
        _path: &str,
    ) -> DomainResult<WebSocketRequest> {
        Err(DomainError::InvalidInput(
            "websocket requests are not supported by this repository".into(),
        ))
    }

    /// Save one WebSocket request. Returns the filename actually written.
    fn save_websocket_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &WebSocketRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "websocket requests are not supported by this repository".into(),
        ))
    }

    /// Read one gRPC request file. Repositories without gRPC support keep this default.
    fn get_grpc_request(&self, _collection: &str, _path: &str) -> DomainResult<GrpcRequest> {
        Err(DomainError::InvalidInput(
            "this repository does not support gRPC requests".into(),
        ))
    }

    /// Save a gRPC request. Returns the actual filename written, like `save_request`.
    fn save_grpc_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &GrpcRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "this repository does not support gRPC requests".into(),
        ))
    }

    /// Create a folder within a collection.
    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Delete a folder and its contents.
    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Move a request or folder within or across collections.
    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()>;

    /// Write an explicit ordering for items in a folder within a collection.
    /// `folder_path` is relative to the collection root; pass `""` for the root.
    /// `ordered_names` is the full ordered list of entry names (files include `.json`).
    fn reorder_items(
        &self,
        collection: &str,
        folder_path: &str,
        ordered_names: &[String],
    ) -> DomainResult<()>;

    /// Read collection-level settings (auth, headers) from collection.json.
    /// Returns default settings if the file does not exist.
    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings>;

    /// Absolute directory of a collection. Used to scope local-file `require()`.
    /// The default body keeps test doubles compiling; real repositories override it.
    fn collection_root_path(&self, _name: &str) -> DomainResult<PathBuf> {
        Err(DomainError::Internal(
            "collection root path is not available".into(),
        ))
    }

    /// Creates `name` (`.js` appended when missing) in `folder_path` with starter
    /// content. `folder_path` is relative to the collection root, `""` for the root.
    /// Returns the collection-relative path of the new file.
    fn create_script_file(
        &self,
        _collection: &str,
        _folder_path: &str,
        _name: &str,
    ) -> DomainResult<String> {
        Err(DomainError::Internal(
            "script files are not supported".into(),
        ))
    }

    /// Reads a script file by collection-relative path.
    fn read_script_file(&self, _collection: &str, _path: &str) -> DomainResult<String> {
        Err(DomainError::Internal(
            "script files are not supported".into(),
        ))
    }

    /// Overwrites an existing script file. Never creates a file.
    fn save_script_file(&self, _collection: &str, _path: &str, _content: &str) -> DomainResult<()> {
        Err(DomainError::Internal(
            "script files are not supported".into(),
        ))
    }

    /// Renames a script file inside its folder. Returns the new relative path.
    fn rename_script_file(
        &self,
        _collection: &str,
        _path: &str,
        _new_name: &str,
    ) -> DomainResult<String> {
        Err(DomainError::Internal(
            "script files are not supported".into(),
        ))
    }

    /// Deletes a script file.
    fn delete_script_file(&self, _collection: &str, _path: &str) -> DomainResult<()> {
        Err(DomainError::Internal(
            "script files are not supported".into(),
        ))
    }

    /// Persist collection-level settings to collection.json.
    fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<()>;

    /// Walk the full folder ancestor chain for a request path and return
    /// merged variables (outer folder first; inner folder wins on collision).
    fn get_folder_chain_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>>;

    /// Read only this folder's own variables from its folder.yml (no chain walk).
    /// Returns an empty vec if the folder or its folder.yml does not exist.
    fn get_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>>;

    /// Persist folder-level variables to the folder's folder.yml.
    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()>;

    /// Read one folder's own settings from its folder.yml (no chain walk).
    /// `folder_path` is relative to the collection root, `""` for the root.
    /// The default body keeps test doubles compiling; real repositories override it.
    fn get_folder_settings(
        &self,
        _collection: &str,
        _folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        Err(DomainError::Internal(
            "folder settings not supported".into(),
        ))
    }

    /// Persist one folder's settings to its folder.yml.
    /// Keys the domain does not model (for example `request.metadata`) are kept by the implementation.
    fn save_folder_settings(
        &self,
        _collection: &str,
        _folder_path: &str,
        _settings: &FolderSettings,
    ) -> DomainResult<()> {
        Err(DomainError::Internal(
            "folder settings not supported".into(),
        ))
    }

    /// Settings of every folder above a request, outermost folder first.
    /// The default returns no folders, so repositories without folder.yml
    /// support run requests with collection settings only.
    fn get_folder_chain_settings(
        &self,
        _collection: &str,
        _request_path: &str,
    ) -> DomainResult<Vec<FolderSettings>> {
        Ok(vec![])
    }

    /// Read request-level variables from a request .yml file's runtime.variables[].
    fn get_request_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>>;

    /// Persist request-level variables to a request .yml file's runtime.variables[].
    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()>;

    /// Overwrite one of a request's three script fields (pre-request,
    /// post-response, or tests) in place, leaving the other two and every
    /// other request field untouched. Deliberately not a full
    /// read-modify-write of the whole `Request` from a caller-supplied copy —
    /// that risks clobbering a concurrent manual edit to unrelated fields,
    /// and `Request` has no optimistic-concurrency mechanism to detect that.
    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Implements only the required methods, like the older test doubles in
    /// other crates. If a new method had no default, this would not compile.
    struct MinimalRepo;

    fn unused<T>() -> DomainResult<T> {
        Err(DomainError::Internal("unused in this test".into()))
    }

    impl CollectionRepository for MinimalRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            unused()
        }
        fn get(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn get_summaries(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn create(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unused()
        }
        fn rename(&self, _old_name: &str, _new_name: &str) -> DomainResult<()> {
            unused()
        }
        fn get_request(&self, _collection: &str, _path: &str) -> DomainResult<Request> {
            unused()
        }
        fn save_request(
            &self,
            _collection: &str,
            _path: &str,
            _request: &Request,
        ) -> DomainResult<String> {
            unused()
        }
        fn rename_request(
            &self,
            _collection: &str,
            _old_path: &str,
            _new_path: &str,
        ) -> DomainResult<()> {
            unused()
        }
        fn delete_request(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn create_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn delete_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn move_item(
            &self,
            _src_collection: &str,
            _src_path: &str,
            _dst_collection: &str,
            _dst_path: &str,
        ) -> DomainResult<()> {
            unused()
        }
        fn reorder_items(
            &self,
            _collection: &str,
            _folder_path: &str,
            _ordered_names: &[String],
        ) -> DomainResult<()> {
            unused()
        }
        fn get_settings(&self, _name: &str) -> DomainResult<CollectionSettings> {
            unused()
        }
        fn save_settings(&self, _name: &str, _settings: &CollectionSettings) -> DomainResult<()> {
            unused()
        }
        fn get_folder_chain_variables(
            &self,
            _collection: &str,
            _request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn get_folder_variables(
            &self,
            _collection: &str,
            _folder_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn save_folder_variables(
            &self,
            _collection: &str,
            _folder_path: &str,
            _vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            unused()
        }
        fn get_request_variables(
            &self,
            _collection: &str,
            _request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn save_request_variables(
            &self,
            _collection: &str,
            _request_path: &str,
            _vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            unused()
        }
    }

    #[test]
    fn trait_is_object_safe() {
        // Compile-time check.
        fn _assert_object_safe(_: Box<dyn CollectionRepository>) {}
        let _boxed: Box<dyn CollectionRepository> = Box::new(MinimalRepo);
    }

    #[test]
    fn minimal_impl_gets_folder_settings_defaults() {
        let repo: Box<dyn CollectionRepository> = Box::new(MinimalRepo);

        let read = repo.get_folder_settings("c", "a/b");
        assert!(
            matches!(&read, Err(DomainError::Internal(msg)) if msg == "folder settings not supported"),
            "unexpected get_folder_settings default: {read:?}"
        );

        let saved = repo.save_folder_settings("c", "", &FolderSettings::default());
        assert!(
            matches!(&saved, Err(DomainError::Internal(msg)) if msg == "folder settings not supported"),
            "unexpected save_folder_settings default: {saved:?}"
        );
    }

    #[test]
    fn default_get_folder_chain_settings_is_empty() {
        let repo = MinimalRepo;
        let chain = repo
            .get_folder_chain_settings("c", "a/b/request.yml")
            .expect("default chain is Ok");
        assert_eq!(chain, Vec::<FolderSettings>::new());
    }

    #[test]
    fn request_script_phase_is_copy_and_comparable() {
        let a = RequestScriptPhase::PreRequest;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(RequestScriptPhase::PreRequest, RequestScriptPhase::Tests);
    }
}
