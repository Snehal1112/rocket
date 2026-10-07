//! End-to-end checks that scripts read folder variables through `rok.getFolderVar`.
//! They use the real filesystem repo and the real script engine, so they cover the
//! whole path from `folder.yml` to the JS sandbox.

use std::sync::Arc;

use rocket_collection::{CollectionRepository, CollectionVariable, Request as CollectionRequest};
use rocket_http::RequestOptions;
use rocket_infra::scripting::DenoScriptEngine;
use rocket_infra::FsCollectionRepo;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{Auth, HttpMethod};

use super::{ExecuteRequestInput, RequestExecutionService};
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, RecordingExecutor,
    SharedHistoryRepo,
};

const COLLECTION: &str = "my-api";
const REQUEST_PATH: &str = "outer/inner/get-user.yml";
const URL: &str = "https://example.com/users/1";

fn var(key: &str, value: &str, enabled: bool) -> CollectionVariable {
    CollectionVariable {
        key: key.into(),
        value: value.into(),
        initial_value: value.into(),
        enabled,
        secret: false,
    }
}

/// Builds `my-api/outer/inner/get-user.yml` with variables on both folders.
fn seed_collection(base: &std::path::Path) {
    let repo = FsCollectionRepo::new_standalone(base.to_path_buf());
    repo.create(COLLECTION).expect("create collection");
    repo.create_folder(COLLECTION, "outer")
        .expect("create outer folder");
    repo.create_folder(COLLECTION, "outer/inner")
        .expect("create inner folder");
    repo.save_folder_variables(
        COLLECTION,
        "outer",
        vec![
            var("shared", "outer-value", true),
            var("outerOnly", "o1", true),
            var("toggled", "outer-on", true),
        ],
    )
    .expect("save outer vars");
    repo.save_folder_variables(
        COLLECTION,
        "outer/inner",
        vec![
            var("shared", "inner-value", true),
            var("toggled", "inner-off", false),
        ],
    )
    .expect("save inner vars");
    let request = CollectionRequest::new("Get User", HttpMethod::Get, URL);
    repo.save_request(COLLECTION, REQUEST_PATH, &request)
        .expect("save request");
}

fn service(base: &std::path::Path) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        RecordingExecutor::new(),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(FsCollectionRepo::new_standalone(base.to_path_buf())),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(DenoScriptEngine::new()))
}

fn input(pre_request_script: Option<&str>) -> ExecuteRequestInput {
    ExecuteRequestInput {
        skip_folder_scripts: false,
        skip_history: false,
        flow_vars: std::collections::HashMap::new(),
        method: HttpMethod::Get,
        url: URL.into(),
        headers: vec![],
        query_params: vec![],
        body: None,
        auth: Auth::None,
        options: RequestOptions::default(),
        environment_name: None,
        collection: Some(COLLECTION.into()),
        request_name: Some("Get User".into()),
        pre_request_script: pre_request_script.map(str::to_string),
        post_response_script: None,
        tests_script: None,
        request_path: Some(REQUEST_PATH.into()),
        global_env_name: None,
        assertions: vec![],
        tags: vec![],
        path_params: vec![],
        actions: vec![],
        request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
    }
}

#[tokio::test]
async fn request_script_reads_merged_folder_chain_with_get_folder_var() {
    let dir = tempfile::tempdir().expect("tempdir");
    seed_collection(dir.path());
    let script = "console.log([rok.getFolderVar('shared'), rok.getFolderVar('outerOnly'), \
                  rok.getFolderVar('toggled'), rok.getFolderVar('missing')].join('|'))";

    let out = service(dir.path())
        .execute(input(Some(script)))
        .await
        .expect("execute");

    assert!(
        out.script_error.is_none(),
        "script error: {:?}",
        out.script_error
    );
    let lines: Vec<&str> = out
        .console_entries
        .iter()
        .map(|e| e.message.as_str())
        .collect();
    // Inner wins, outer fills gaps, a disabled inner entry does not shadow, a missing key is "".
    assert!(
        lines.contains(&"inner-value|o1|outer-on|"),
        "console: {lines:?}"
    );
}

#[tokio::test]
async fn folder_script_reads_folder_vars_with_get_folder_var() {
    let dir = tempfile::tempdir().expect("tempdir");
    seed_collection(dir.path());
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    // Read first so the save keeps the folder's variables.
    let mut settings = repo
        .get_folder_settings(COLLECTION, "outer")
        .expect("read outer settings");
    settings.pre_request_script =
        Some("console.log('outer-script:' + rok.getFolderVar('shared'))".into());
    repo.save_folder_settings(COLLECTION, "outer", &settings)
        .expect("save outer settings");

    let out = service(dir.path())
        .execute(input(None))
        .await
        .expect("execute");

    assert!(
        out.script_error.is_none(),
        "script error: {:?}",
        out.script_error
    );
    let lines: Vec<&str> = out
        .console_entries
        .iter()
        .map(|e| e.message.as_str())
        .collect();
    // The folder scope is the request's merged chain, so the outer script sees the inner value.
    assert!(
        lines.contains(&"outer-script:inner-value"),
        "console: {lines:?}"
    );
}
