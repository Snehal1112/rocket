use std::fs;
use std::path::PathBuf;

use rocket_collection::Collection;
use rocket_shared::action::HttpRequestExample;
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::oc::{
    OcGraphQLRequest, OcGrpcRequest, OcHttpRequest, OcHttpRequestExample, OcWebSocketRequest,
};

use super::paths::resolve_request_path;
use super::FsCollectionRepo;

/// Longest allowed example name, in characters.
const MAX_NAME_CHARS: usize = 120;
/// Largest allowed size of one example once written as YAML, in bytes.
const MAX_EXAMPLE_BYTES: usize = 1_048_576;

/// Checks the name and the size of one example.
fn validate_example(example: &HttpRequestExample) -> DomainResult<()> {
    validate_name(&example.name)?;
    let oc = to_oc(example)?;
    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize example: {e}")))?;
    if yaml.len() > MAX_EXAMPLE_BYTES {
        return Err(DomainError::InvalidInput(format!(
            "example is larger than {MAX_EXAMPLE_BYTES} bytes"
        )));
    }
    Ok(())
}

/// A name is display text: trimmed, 1 to 120 characters, no control characters.
fn validate_name(name: &str) -> DomainResult<()> {
    let trimmed = name.trim();
    let chars = trimmed.chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS {
        return Err(DomainError::InvalidInput(format!(
            "example name must be 1 to {MAX_NAME_CHARS} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(DomainError::InvalidInput(
            "example name cannot contain control characters".into(),
        ));
    }
    Ok(())
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Same mapping as `conversions/request.rs`, but a bad shape is an error, not a silent drop.
fn to_oc(example: &HttpRequestExample) -> DomainResult<OcHttpRequestExample> {
    let bad = |part: &str, e: serde_yaml::Error| {
        DomainError::InvalidInput(format!("example {part} has an invalid shape: {e}"))
    };
    let request = match example.request.clone() {
        Some(v) => Some(serde_yaml::from_value(v).map_err(|e| bad("request", e))?),
        None => None,
    };
    let response = match example.response.clone() {
        Some(v) => Some(serde_yaml::from_value(v).map_err(|e| bad("response", e))?),
        None => None,
    };
    Ok(OcHttpRequestExample {
        name: example.name.trim().to_string(),
        description: example.description.clone(),
        request,
        response,
    })
}

fn to_domain(oc: OcHttpRequestExample) -> HttpRequestExample {
    HttpRequestExample {
        name: oc.name,
        description: oc.description,
        request: oc.request.and_then(|r| serde_yaml::to_value(r).ok()),
        response: oc.response.and_then(|r| serde_yaml::to_value(r).ok()),
    }
}

/// Reads the examples of an HTTP request file. Other protocols are `InvalidInput`.
fn parse_examples(content: &str) -> DomainResult<Vec<OcHttpRequestExample>> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => return Ok(req.examples.unwrap_or_default()),
        Err(e) => e,
    };
    if serde_yaml::from_str::<OcGraphQLRequest>(content).is_ok()
        || serde_yaml::from_str::<OcWebSocketRequest>(content).is_ok()
        || serde_yaml::from_str::<OcGrpcRequest>(content).is_ok()
    {
        return Err(DomainError::InvalidInput(
            "examples are only supported on HTTP requests".into(),
        ));
    }
    Err(DomainError::Internal(format!(
        "Failed to parse request file: {http_err}"
    )))
}

/// Returns the file text with only the `examples` key replaced. The other keys are kept as
/// parsed. An empty list removes the key.
fn with_examples(content: &str, examples: Vec<OcHttpRequestExample>) -> DomainResult<String> {
    let to_err = |e: serde_yaml::Error| {
        DomainError::Internal(format!("Failed to serialize request file: {e}"))
    };
    let mut doc: serde_yaml::Value = serde_yaml::from_str(content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| DomainError::Internal("request file is not a mapping".into()))?;
    let key = serde_yaml::Value::String("examples".into());
    if examples.is_empty() {
        map.remove(&key);
    } else {
        map.insert(key, serde_yaml::to_value(&examples).map_err(to_err)?);
    }
    serde_yaml::to_string(&doc).map_err(to_err)
}

/// Finds the request file and reads it. A missing file is `NotFound`.
fn read_request(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<(PathBuf, String)> {
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, request_path)?;
    match fs::read_to_string(&file_path) {
        Ok(content) => Ok((file_path, content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(DomainError::NotFound(format!("request '{request_path}'")))
        }
        Err(e) => Err(e.into()),
    }
}

/// Reads the request file under the collection lock, lets `edit` change the examples and
/// writes the file back. If `edit` fails, nothing is written.
fn edit_examples(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    edit: impl FnOnce(&mut Vec<OcHttpRequestExample>) -> DomainResult<()>,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let (file_path, content) = read_request(repo, collection, request_path)?;
    let mut examples = parse_examples(&content)?;
    edit(&mut examples)?;
    let yaml = with_examples(&content, examples)?;
    atomic_write(&file_path, yaml.as_bytes())?;
    Ok(())
}

pub(super) fn list_request_examples(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<HttpRequestExample>> {
    Collection::validate_name(collection)?;
    let (_, content) = read_request(repo, collection, request_path)?;
    Ok(parse_examples(&content)?
        .into_iter()
        .map(to_domain)
        .collect())
}

pub(super) fn add_request_example(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    example: HttpRequestExample,
) -> DomainResult<()> {
    validate_example(&example)?;
    let oc = to_oc(&example)?;
    edit_examples(repo, collection, request_path, move |examples| {
        if examples.iter().any(|e| same_name(&e.name, &oc.name)) {
            return Err(DomainError::AlreadyExists(format!("example '{}'", oc.name)));
        }
        examples.push(oc);
        Ok(())
    })
}

pub(super) fn rename_request_example(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    old_name: &str,
    new_name: &str,
) -> DomainResult<()> {
    validate_name(new_name)?;
    let new_name = new_name.trim().to_string();
    edit_examples(repo, collection, request_path, |examples| {
        let index = examples
            .iter()
            .position(|e| e.name == old_name)
            .ok_or_else(|| DomainError::NotFound(format!("example '{old_name}'")))?;
        let taken = examples
            .iter()
            .enumerate()
            .any(|(i, e)| i != index && same_name(&e.name, &new_name));
        if taken {
            return Err(DomainError::AlreadyExists(format!("example '{new_name}'")));
        }
        examples[index].name = new_name;
        Ok(())
    })
}

pub(super) fn delete_request_example(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    name: &str,
) -> DomainResult<()> {
    edit_examples(repo, collection, request_path, |examples| {
        let index = examples
            .iter()
            .position(|e| e.name == name)
            .ok_or_else(|| DomainError::NotFound(format!("example '{name}'")))?;
        examples.remove(index);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dashmap::DashMap;
    use rocket_collection::{
        CollectionRepository, CollectionVariable, Request, RequestScriptPhase,
    };
    use rocket_shared::description::Documentation;
    use rocket_shared::types::{Auth, HttpMethod};
    use tempfile::TempDir;

    use super::*;

    const COL: &str = "my-api";
    const PATH: &str = "get-users.yml";

    fn setup() -> (TempDir, FsCollectionRepo) {
        let dir = TempDir::new().expect("temp dir");
        let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
        repo.create(COL).expect("create collection");
        let req = Request::new("Get Users", HttpMethod::Get, "https://x.io/users");
        repo.save_request(COL, PATH, &req).expect("save request");
        (dir, repo)
    }

    fn file_text(dir: &TempDir) -> String {
        fs::read_to_string(dir.path().join(COL).join(PATH)).expect("read file")
    }

    fn example(name: &str) -> HttpRequestExample {
        let request = serde_yaml::from_str("url: https://x.io\nmethod: GET\n").expect("request");
        let response = serde_yaml::from_str("status: 200\n").expect("response");
        HttpRequestExample {
            name: name.into(),
            description: None,
            request: Some(request),
            response: Some(response),
        }
    }

    fn names(repo: &FsCollectionRepo) -> Vec<String> {
        repo.list_request_examples(COL, PATH)
            .expect("list")
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    #[test]
    fn add_then_list_returns_the_example() {
        let (_dir, repo) = setup();
        repo.add_request_example(COL, PATH, example("200 OK"))
            .expect("add");
        let listed = repo.list_request_examples(COL, PATH).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "200 OK");
        let request = listed[0].request.as_ref().expect("request");
        assert_eq!(request["url"], "https://x.io");
        assert_eq!(request["method"], "GET");
        let response = listed[0].response.as_ref().expect("response");
        assert_eq!(response["status"], 200);
    }

    #[test]
    fn add_keeps_every_other_field() {
        let (dir, repo) = setup();
        let mut req = Request::new("Get Users", HttpMethod::Get, "https://x.io/users");
        req.uid = "uid-fixed-1".into();
        req.pre_request_script = Some("console.log('pre');".into());
        req.docs = Some(Documentation::text("Some docs".to_string()));
        req.auth = Auth::Bearer {
            token: "tok".into(),
        };
        repo.save_request(COL, PATH, &req).expect("save request");
        let vars = vec![CollectionVariable {
            key: "host".into(),
            value: "x.io".into(),
            initial_value: "x.io".into(),
            enabled: true,
            secret: false,
        }];
        repo.save_request_variables(COL, PATH, vars.clone())
            .expect("save vars");
        repo.save_request_script(
            COL,
            PATH,
            RequestScriptPhase::Tests,
            "rok.test('t', () => {});".into(),
        )
        .expect("save script");

        let before_req = repo.get_request(COL, PATH).expect("get before");
        let before_vars = repo.get_request_variables(COL, PATH).expect("vars before");
        let before_value: serde_yaml::Value =
            serde_yaml::from_str(&file_text(&dir)).expect("parse before");
        assert_eq!(before_vars.len(), 1, "fixture must hold a variable");
        assert_eq!(before_req.uid, "uid-fixed-1", "fixture must hold the uid");
        assert!(before_req.docs.is_some(), "fixture must hold docs");
        assert!(before_req.pre_request_script.is_some());

        repo.add_request_example(COL, PATH, example("200 OK"))
            .expect("add");

        let mut after_req = repo.get_request(COL, PATH).expect("get after");
        let after_vars = repo.get_request_variables(COL, PATH).expect("vars after");
        let mut after_value: serde_yaml::Value =
            serde_yaml::from_str(&file_text(&dir)).expect("parse after");
        assert_eq!(after_req.examples.len(), 1);
        after_req.examples.clear();
        assert_eq!(after_req, before_req);
        assert_eq!(after_vars, before_vars);
        // Every top-level key except `examples` is the same value as before.
        after_value
            .as_mapping_mut()
            .expect("mapping")
            .remove("examples");
        assert_eq!(after_value, before_value);
    }

    #[test]
    fn duplicate_name_is_rejected_case_insensitively_and_writes_nothing() {
        let (dir, repo) = setup();
        repo.add_request_example(COL, PATH, example("200 OK"))
            .expect("add");
        let before = fs::read(dir.path().join(COL).join(PATH)).expect("bytes");
        let err = repo
            .add_request_example(COL, PATH, example("200 ok"))
            .expect_err("duplicate");
        assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
        let after = fs::read(dir.path().join(COL).join(PATH)).expect("bytes");
        assert_eq!(before, after);
    }

    #[test]
    fn bad_names_are_rejected() {
        let (_dir, repo) = setup();
        for name in ["", "   ", &"a".repeat(121), "two\nlines"] {
            let err = repo
                .add_request_example(COL, PATH, example(name))
                .expect_err("bad name");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{name:?}");
        }
        assert!(names(&repo).is_empty());
    }

    #[test]
    fn oversized_example_is_rejected() {
        let (_dir, repo) = setup();
        let mut big = example("big");
        let response = serde_yaml::from_str(&format!(
            "status: 200\nbody:\n  type: text\n  data: {}\n",
            "x".repeat(1_100_000)
        ))
        .expect("response");
        big.response = Some(response);
        let err = repo
            .add_request_example(COL, PATH, big)
            .expect_err("too big");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn rename_changes_only_that_name() {
        let (_dir, repo) = setup();
        repo.add_request_example(COL, PATH, example("first"))
            .expect("add first");
        repo.add_request_example(COL, PATH, example("second"))
            .expect("add second");

        repo.rename_request_example(COL, PATH, "first", "renamed")
            .expect("rename");
        assert_eq!(names(&repo), ["renamed", "second"]);

        repo.rename_request_example(COL, PATH, "renamed", "renamed")
            .expect("rename to own name");
        let err = repo
            .rename_request_example(COL, PATH, "renamed", "Second")
            .expect_err("taken");
        assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
        let err = repo
            .rename_request_example(COL, PATH, "missing", "other")
            .expect_err("missing");
        assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
        assert_eq!(names(&repo), ["renamed", "second"]);
    }

    #[test]
    fn delete_removes_the_example_and_the_key_when_last() {
        let (dir, repo) = setup();
        repo.add_request_example(COL, PATH, example("one"))
            .expect("add one");
        repo.add_request_example(COL, PATH, example("two"))
            .expect("add two");
        repo.delete_request_example(COL, PATH, "one")
            .expect("delete one");
        assert_eq!(names(&repo), ["two"]);
        repo.delete_request_example(COL, PATH, "two")
            .expect("delete two");
        assert!(names(&repo).is_empty());
        assert!(!file_text(&dir).contains("examples:"));
        let err = repo
            .delete_request_example(COL, PATH, "two")
            .expect_err("gone");
        assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
    }

    #[test]
    fn awkward_names_round_trip() {
        let (_dir, repo) = setup();
        let awkward = ["a: b # c", "- x", "'quoted'"];
        for name in awkward {
            repo.add_request_example(COL, PATH, example(name))
                .expect("add");
        }
        assert_eq!(names(&repo), awkward);
    }

    #[test]
    fn non_http_files_are_rejected() {
        let (dir, repo) = setup();
        let yml = "info:\n  name: List Users\n  type: graphql\ngraphql:\n  url: https://api.example.com/graphql\n  body:\n    query: '{ users { id } }'\n";
        fs::write(dir.path().join(COL).join("gql.yml"), yml).expect("write graphql");
        let err = repo
            .add_request_example(COL, "gql.yml", example("x"))
            .expect_err("graphql");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn missing_request_is_not_found() {
        let (_dir, repo) = setup();
        let err = repo
            .add_request_example(COL, "absent.yml", example("x"))
            .expect_err("absent");
        assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
    }
}
