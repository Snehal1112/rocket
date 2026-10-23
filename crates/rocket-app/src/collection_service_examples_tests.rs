//! Request example methods of `CollectionService` through the real `FsCollectionRepo`.

use std::sync::Arc;

use rocket_collection::{CollectionRepository, Request};
use rocket_infra::FsCollectionRepo;
use rocket_shared::action::HttpRequestExample;
use rocket_shared::error::DomainError;
use rocket_shared::events::DomainEvent;
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use crate::collection_service::CollectionService;
use crate::test_doubles::{RecordingPublisher, SharedPublisher};

fn example(name: &str) -> HttpRequestExample {
    HttpRequestExample {
        name: name.into(),
        description: None,
        request: None,
        response: None,
    }
}

/// Collection `c` holds the saved request `Login.yml`.
fn world() -> (TempDir, CollectionService, Arc<RecordingPublisher>) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.create("c").expect("create collection");
    let request = Request::new("Login", HttpMethod::Get, "http://one".to_string());
    repo.save_request("c", "Login.yml", &request)
        .expect("save request");
    let events = RecordingPublisher::new();
    let service = CollectionService::new(
        Box::new(FsCollectionRepo::new_standalone(dir.path().to_path_buf())),
        Box::new(SharedPublisher(events.clone())),
    );
    (dir, service, events)
}

fn saved_count(events: &RecordingPublisher) -> usize {
    events
        .events()
        .iter()
        .filter(|e| {
            matches!(e, DomainEvent::RequestSaved { collection, path }
                if collection == "c" && path == "Login.yml")
        })
        .count()
}

fn names(svc: &CollectionService) -> Vec<String> {
    svc.list_request_examples("c", "Login.yml")
        .expect("list")
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[test]
fn add_example_publishes_request_saved() {
    let (_dir, svc, events) = world();
    svc.add_request_example("c", "Login.yml", example("200 OK"))
        .expect("add");
    assert_eq!(saved_count(&events), 1);
    assert_eq!(events.events().len(), 1);
}

#[test]
fn failed_add_publishes_nothing() {
    let (_dir, svc, events) = world();
    svc.add_request_example("c", "Login.yml", example("200 OK"))
        .expect("add");
    let err = svc
        .add_request_example("c", "Login.yml", example("200 ok"))
        .expect_err("duplicate");
    assert!(matches!(err, DomainError::AlreadyExists(_)), "got {err:?}");
    assert_eq!(events.events().len(), 1);
}

#[test]
fn list_publishes_nothing() {
    let (_dir, svc, events) = world();
    assert!(names(&svc).is_empty());
    assert!(events.events().is_empty());
}

#[test]
fn rename_and_delete_publish_request_saved() {
    let (_dir, svc, events) = world();
    svc.add_request_example("c", "Login.yml", example("A"))
        .expect("add");
    svc.rename_request_example("c", "Login.yml", "A", "B")
        .expect("rename");
    assert_eq!(names(&svc), vec!["B".to_string()]);
    svc.delete_request_example("c", "Login.yml", "B")
        .expect("delete");
    assert!(names(&svc).is_empty());
    assert_eq!(saved_count(&events), 3);
}

#[test]
fn failed_rename_and_delete_publish_nothing() {
    let (_dir, svc, events) = world();
    assert!(svc
        .rename_request_example("c", "Login.yml", "missing", "B")
        .is_err());
    assert!(svc
        .delete_request_example("c", "Login.yml", "missing")
        .is_err());
    assert!(events.events().is_empty());
}

const JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxIn0.c2ln";

// A login example as the frontend would send it.
const LOGIN_REQUEST: &str = r#"
method: POST
url: '{{base}}/login'
headers:
  - name: Authorization
    value: Bearer abc123
body:
  type: json
  data: '{"user": "ann", "password": "pw-s3cret"}'
"#;

const LOGIN_RESPONSE: &str = r#"
status: 200
headers:
  - name: Set-Cookie
    value: sid=cookie-val-77
body:
  type: json
  data: |-
    {
      "token": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxIn0.c2ln",
      "refresh_token": "rt-9f8e7d",
      "greeting": "hello there"
    }
"#;

#[test]
fn commit_masking_covers_an_example_written_by_the_add_path() {
    use rocket_collection::CollectionVariable;
    use rocket_shared::types::Header;

    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.create("c").expect("create collection");
    let mut request = Request::new("Login", HttpMethod::Post, "{{base}}/login".to_string());
    request
        .headers
        .push(Header::new("Authorization", "Bearer {{tok}}"));
    request.variables.push(CollectionVariable {
        key: "tok".into(),
        value: "runtime-tok".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    });
    repo.save_request("c", "Login.yml", &request)
        .expect("save request");
    let svc = CollectionService::new(
        Box::new(FsCollectionRepo::new_standalone(dir.path().to_path_buf())),
        Box::new(SharedPublisher(RecordingPublisher::new())),
    );
    let ex = HttpRequestExample {
        name: "login".into(),
        description: None,
        request: Some(serde_yaml::from_str(LOGIN_REQUEST).expect("request yaml")),
        response: Some(serde_yaml::from_str(LOGIN_RESPONSE).expect("response yaml")),
    };
    svc.add_request_example("c", "Login.yml", ex).expect("add");

    let path = dir.path().join("c").join("Login.yml");
    let text = std::fs::read_to_string(&path).expect("read file");
    let secrets = ["abc123", JWT, "rt-9f8e7d", "pw-s3cret", "cookie-val-77"];
    for s in secrets {
        assert!(text.contains(s), "{s} should be on disk");
    }

    let (masked, count) = rocket_git::sanitize::mask_examples(&text).expect("masked");
    assert!(count >= 5, "count was {count}");
    for s in secrets {
        assert!(!masked.contains(s), "{s} leaked");
    }
    for keep in [
        "hello there",
        "{{base}}/login",
        "Bearer {{tok}}",
        "runtime-tok",
    ] {
        assert!(masked.contains(keep), "{keep} was lost");
    }
    let on_disk = std::fs::read_to_string(&path).expect("read again");
    assert!(on_disk.contains("abc123"), "disk keeps the real values");
    assert_eq!(rocket_git::sanitize::mask_examples(&masked), None);
}
