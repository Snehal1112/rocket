//! Fixture generator for the rocket-infra benches.
//!
//! It writes synthetic collections through the repository's own save methods, so the files match
//! the real on-disk format. The mix is 60% HTTP, 20% GraphQL, 10% WebSocket and 10% gRPC.
#![allow(dead_code)] // Each bench target uses a different part of this module.

use std::path::Path;

use rocket_collection::{
    CollectionRepository, GraphQlRequest, GrpcMessage, GrpcRequest, Request, WebSocketMessage,
    WebSocketMessageKind, WebSocketRequest,
};
use rocket_infra::FsCollectionRepo;
use rocket_shared::types::{Body, BodyMode, HttpMethod};
use tempfile::TempDir;

/// How many folders each fixture collection gets, as `folder-N` plus `folder-N/inner`.
const FOLDERS_SMALL: usize = 5;
const FOLDERS_MEDIUM: usize = 25;
const FOLDERS_LARGE: usize = 100;

#[derive(Debug, Clone, Copy)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Medium, Size::Large];

    pub fn requests(self) -> usize {
        match self {
            Size::Small => 50,
            Size::Medium => 500,
            Size::Large => 5000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Medium => "medium",
            Size::Large => "large",
        }
    }

    fn folders(self) -> usize {
        match self {
            Size::Small => FOLDERS_SMALL,
            Size::Medium => FOLDERS_MEDIUM,
            Size::Large => FOLDERS_LARGE,
        }
    }
}

pub struct Fixture {
    pub dir: TempDir,
    pub repo: FsCollectionRepo,
    pub collection: String,
    pub collections: usize,
    pub requests_per_collection: usize,
    pub http_paths: Vec<String>,
}

impl Fixture {
    /// Fails loudly when the fixture does not hold what its labels claim.
    pub fn verify(&self) {
        let listed = self.repo.list().expect("list collections");
        assert_eq!(listed.len(), self.collections, "collection count");
        for c in 0..self.collections {
            let name = format!("bench-{c}");
            let tree = self
                .repo
                .get_summaries(&name)
                .expect("summaries of a fixture collection");
            assert_eq!(
                tree.request_count(),
                self.requests_per_collection,
                "the request count of {name} must match the fixture size"
            );
        }
    }
}

/// One collection with the request count and folder count of `size`.
pub fn generate(size: Size) -> Fixture {
    build(1, size.requests(), size.folders())
}

/// `count` collections of `requests_each` requests each, for the `list` bench.
pub fn generate_collections(count: usize, requests_each: usize) -> Fixture {
    build(count, requests_each, 2)
}

fn build(collections: usize, requests: usize, folders: usize) -> Fixture {
    let dir = TempDir::new().expect("create temp dir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    let mut first_http = Vec::new();
    for c in 0..collections {
        let name = format!("bench-{c}");
        let http = populate(&repo, &name, requests, folders);
        if c == 0 {
            first_http = http;
        }
    }
    let expected_http = (0..requests).filter(|i| i % 10 <= 5).count();
    assert_eq!(
        first_http.len(),
        expected_http,
        "http request count of bench-0"
    );
    for path in first_http.iter().take(3).chain(first_http.last()) {
        repo.get_request("bench-0", path)
            .expect("a recorded http path must load");
    }
    let fixture = Fixture {
        dir,
        repo,
        collection: "bench-0".to_string(),
        collections,
        requests_per_collection: requests,
        http_paths: first_http,
    };
    fixture.verify();
    fixture
}

/// Writes one collection and returns the relative paths of its HTTP requests.
fn populate(repo: &FsCollectionRepo, name: &str, requests: usize, folders: usize) -> Vec<String> {
    repo.create(name).expect("create collection");
    for f in 0..folders {
        repo.create_folder(name, &format!("folder-{f}"))
            .expect("create folder");
        repo.create_folder(name, &format!("folder-{f}/inner"))
            .expect("create inner folder");
    }
    let mut http_paths = Vec::new();
    for i in 0..requests {
        let folder = i % folders;
        let dir = if i % 3 == 0 {
            format!("folder-{folder}/inner")
        } else {
            format!("folder-{folder}")
        };
        let path = format!("{dir}/req-{i}");
        match i % 10 {
            0..=5 => {
                let saved = repo
                    .save_request(name, &path, &http_request(i))
                    .expect("save http request");
                http_paths.push(saved);
            }
            6 | 7 => {
                repo.save_graphql_request(name, &path, &graphql_request(i))
                    .expect("save graphql request");
            }
            8 => {
                repo.save_websocket_request(name, &path, &websocket_request(i))
                    .expect("save websocket request");
            }
            _ => {
                repo.save_grpc_request(name, &path, &grpc_request(i))
                    .expect("save grpc request");
            }
        }
    }
    http_paths
}

fn http_request(i: usize) -> Request {
    let method = if i % 2 == 0 {
        HttpMethod::Get
    } else {
        HttpMethod::Post
    };
    let mut request = Request::new(
        format!("Request {i}"),
        method.clone(),
        format!("{{{{base_url}}}}/api/v1/items/{i}"),
    )
    .with_header("Accept", "application/json")
    .with_header("X-Request-Id", format!("{{{{request_id_{i}}}}}"));
    if method == HttpMethod::Post {
        request = request.with_body(Body {
            mode: BodyMode::Json,
            content: Some(format!("{{\"id\": {i}, \"name\": \"item-{i}\"}}")),
            form_data: None,
            file_path: None,
        });
    }
    request
}

fn graphql_request(i: usize) -> GraphQlRequest {
    GraphQlRequest::new(format!("Query {i}"), "{{base_url}}/graphql")
        .with_query(format!("query Item {{ item(id: {i}) {{ id name }} }}"))
}

fn websocket_request(i: usize) -> WebSocketRequest {
    let mut request = WebSocketRequest::new(format!("Socket {i}"), "wss://{{host}}/stream");
    request.messages.push(WebSocketMessage {
        title: "ping".to_string(),
        selected: true,
        kind: WebSocketMessageKind::Json,
        data: format!("{{\"ping\": {i}}}"),
    });
    request
}

fn grpc_request(i: usize) -> GrpcRequest {
    let mut request = GrpcRequest::new(format!("Call {i}"), "{{grpc_host}}:50051");
    request.messages.push(GrpcMessage {
        title: "hello".to_string(),
        selected: true,
        content: format!("{{\"name\": \"item-{i}\"}}"),
    });
    request
}

/// The deep case: 25 parent folders with 19 child folders each, so 500 folders in all. Every folder
/// holds 5 HTTP requests and an `_order.yml` that lists its requests and then its child folders.
pub const DEEP_PARENTS: usize = 25;
pub const DEEP_CHILDREN: usize = 19;
pub const DEEP_REQUESTS: usize = 5;
pub const DEEP_FOLDERS: usize = DEEP_PARENTS * (1 + DEEP_CHILDREN);

/// Writes the deep collection `name` into `repo`.
pub fn populate_deep(repo: &FsCollectionRepo, name: &str) {
    repo.create(name).expect("create collection");
    let root = repo.collection_root_path(name).expect("collection root");
    for p in 0..DEEP_PARENTS {
        let parent = format!("p-{p}");
        repo.create_folder(name, &parent)
            .expect("create parent folder");
        let children: Vec<String> = (0..DEEP_CHILDREN).map(|c| format!("c-{c}")).collect();
        fill_deep_folder(
            repo,
            name,
            &root,
            &parent,
            &children,
            p * (1 + DEEP_CHILDREN),
        );
        for (c, child) in children.iter().enumerate() {
            let dir = format!("{parent}/{child}");
            repo.create_folder(name, &dir).expect("create child folder");
            fill_deep_folder(
                repo,
                name,
                &root,
                &dir,
                &[],
                p * (1 + DEEP_CHILDREN) + c + 1,
            );
        }
    }
}

/// Saves the 5 requests of one deep folder and writes its `_order.yml`.
fn fill_deep_folder(
    repo: &FsCollectionRepo,
    name: &str,
    root: &Path,
    dir: &str,
    subfolders: &[String],
    seed: usize,
) {
    let mut order = String::new();
    for r in 0..DEEP_REQUESTS {
        let saved = repo
            .save_request(
                name,
                &format!("{dir}/r-{r}"),
                &http_request(seed * DEEP_REQUESTS + r),
            )
            .expect("save request");
        let file = Path::new(&saved)
            .file_name()
            .and_then(|n| n.to_str())
            .expect("saved file name")
            .to_string();
        order.push_str(&format!("- {file}\n"));
    }
    for sub in subfolders {
        order.push_str(&format!("- {sub}\n"));
    }
    std::fs::write(root.join(dir).join("_order.yml"), order).expect("write _order.yml");
}

/// One collection `bench-0` with the deep shape, in a temp dir, verified against its labels.
pub fn generate_deep() -> Fixture {
    let dir = TempDir::new().expect("create temp dir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    populate_deep(&repo, "bench-0");
    let fixture = Fixture {
        dir,
        repo,
        collection: "bench-0".to_string(),
        collections: 1,
        requests_per_collection: DEEP_FOLDERS * DEEP_REQUESTS,
        http_paths: Vec::new(),
    };
    fixture.verify();
    fixture
}
