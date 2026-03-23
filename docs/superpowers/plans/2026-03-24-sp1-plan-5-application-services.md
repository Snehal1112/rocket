# SP1 Plan 5: Application Services Layer

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the application service layer — use cases that orchestrate across bounded contexts. The key service is `RequestExecutionService` which coordinates environment resolution → HTTP execution → history save → cookie update.

**Architecture:** Each service takes domain trait objects via constructor (dependency injection). Services contain orchestration logic only — no I/O, no domain logic. Testable with mock repositories.

**Tech Stack:** Rust, async-trait, tokio, rocket-shared + all domain crates

---

## File Structure

```
crates/rocket-app/src/
  lib.rs
  collection_service.rs         # Collection CRUD orchestration
  environment_service.rs        # Environment CRUD orchestration
  execution_service.rs          # Cross-context: execute request use case
  history_service.rs            # History CRUD orchestration
  template_service.rs           # Template CRUD orchestration
  cookie_service.rs             # Cookie CRUD orchestration
```

---

## Chunk 1: Single-context services

### Task 1: CollectionService

**Files:**
- Create: `crates/rocket-app/src/collection_service.rs`
- Test: inline with mock repo

- [ ] **Step 1: Write the failing test with a mock repository**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    /// In-memory mock for testing
    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()) }
        }
    }

    impl CollectionRepository for MockCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            let cols = self.collections.lock().unwrap();
            Ok(cols.iter().map(|c| CollectionSummary::new(&c.name, "", c.request_count())).collect())
        }
        fn get(&self, name: &str) -> DomainResult<Collection> {
            let cols = self.collections.lock().unwrap();
            cols.iter().find(|c| c.name == name).cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn create(&self, name: &str) -> DomainResult<Collection> {
            let mut cols = self.collections.lock().unwrap();
            if cols.iter().any(|c| c.name == name) {
                return Err(DomainError::AlreadyExists(name.into()));
            }
            let col = Collection::new(name);
            cols.push(col.clone());
            Ok(col)
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            cols.retain(|c| c.name != name);
            Ok(())
        }
        fn rename(&self, old: &str, new: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            if let Some(c) = cols.iter_mut().find(|c| c.name == old) {
                c.name = new.to_string();
                Ok(())
            } else {
                Err(DomainError::NotFound(old.into()))
            }
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<Request> { unimplemented!() }
        fn save_request(&self, _: &str, _: &str, _: &Request) -> DomainResult<()> { unimplemented!() }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
    }

    fn make_service() -> CollectionService {
        CollectionService::new(
            Box::new(MockCollectionRepo::new()),
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
}
```

- [ ] **Step 2: Implement CollectionService**

`crates/rocket-app/src/collection_service.rs`:
```rust
use rocket_collection::*;
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get(name)
    }

    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let col = self.repo.create(name)?;
        self.events.publish(DomainEvent::CollectionCreated { name: name.to_string() });
        Ok(col)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.events.publish(DomainEvent::CollectionDeleted { name: name.to_string() });
        Ok(())
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        self.repo.rename(old_name, new_name)?;
        self.events.publish(DomainEvent::CollectionRenamed {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        });
        Ok(())
    }

    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<()> {
        self.repo.save_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_request(collection, path)?;
        self.events.publish(DomainEvent::RequestDeleted {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo.move_item(src_collection, src_path, dst_collection, dst_path)?;
        self.events.publish(DomainEvent::ItemMoved {
            src_collection: src_collection.to_string(),
            src_path: src_path.to_string(),
            dst_collection: dst_collection.to_string(),
            dst_path: dst_path.to_string(),
        });
        Ok(())
    }
}

// Tests included inline — see Step 1 above
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-app -- collection_service::tests
```
Expected: PASS — 3 tests.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "feat(app): CollectionService with mock-based tests"
```

---

### Task 2: EnvironmentService, HistoryService, TemplateService, CookieService

Same pattern as CollectionService — thin orchestration over a single repo + event publishing. Each service:
- Takes `Box<dyn *Repository>` + `Box<dyn EventPublisher>`
- Delegates CRUD to repo
- Publishes relevant domain events
- Tested with in-memory mock repos

**Files:**
- Create: `crates/rocket-app/src/environment_service.rs`
- Create: `crates/rocket-app/src/history_service.rs`
- Create: `crates/rocket-app/src/template_service.rs`
- Create: `crates/rocket-app/src/cookie_service.rs`

- [ ] **Step 1: Implement all four services + tests (same pattern)**

Each has 2-3 tests: create/list, validation, event publishing.

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-app
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-app/src/
git commit -m "feat(app): environment, history, template, cookie services"
```

---

## Chunk 2: Cross-context execution service

### Task 3: RequestExecutionService

This is the most important use case — it orchestrates across 4 bounded contexts.

**Files:**
- Create: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::{Environment, Variable};
    use rocket_http::*;
    use rocket_history::*;
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::Mutex;

    // Mock HttpExecutor that returns a fixed response
    struct MockExecutor {
        response: HttpResponse,
    }

    #[async_trait::async_trait]
    impl HttpExecutor for MockExecutor {
        async fn execute(&self, _req: &HttpRequest) -> DomainResult<HttpResponse> {
            Ok(self.response.clone())
        }
    }

    // ... mock repos ...

    #[tokio::test]
    async fn execute_resolves_variables_in_url() {
        // Setup: environment with BASE_URL variable
        // Execute request with url "{{BASE_URL}}/users"
        // Assert: the executor received the resolved URL
    }

    #[tokio::test]
    async fn execute_saves_history() {
        // Execute a request
        // Assert: history repo contains 1 entry
    }

    #[tokio::test]
    async fn execute_publishes_event() {
        // Execute a request
        // Assert: RequestExecuted event was published
    }
}
```

- [ ] **Step 2: Implement RequestExecutionService**

`crates/rocket-app/src/execution_service.rs`:
```rust
use rocket_environment::{resolve, EnvironmentRepository, Environment};
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse, CookieRepository, RequestOptions};
use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::{Auth, Header, HttpMethod};
use std::collections::HashMap;

pub struct ExecuteRequestInput {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<rocket_shared::types::Body>,
    pub auth: Auth,
    pub options: RequestOptions,
    pub environment_name: Option<String>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
}

pub struct RequestExecutionService {
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Box<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
}

impl RequestExecutionService {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Box<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self { env_repo, executor, history_repo, cookie_repo, events }
    }

    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
        // Step 1: Resolve environment variables
        let vars = self.build_variable_map(&input.environment_name)?;

        let resolved_url = resolve(&input.url, &vars).output;
        let resolved_headers: Vec<Header> = input.headers.iter().map(|h| {
            Header {
                key: resolve(&h.key, &vars).output,
                value: resolve(&h.value, &vars).output,
                enabled: h.enabled,
            }
        }).collect();

        // Step 2: Build the HttpRequest
        let http_request = HttpRequest {
            method: input.method,
            url: resolved_url.clone(),
            headers: resolved_headers,
            body: input.body.clone(),
            auth: input.auth.clone(),
            options: input.options,
        };

        // Step 3: Execute
        let response = self.executor.execute(&http_request).await?;

        // Step 4: Save history
        let entry = HistoryEntry::new(
            input.method.to_string(),
            &resolved_url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        let entry = if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry.with_collection(col, name)
        } else {
            entry
        };
        let _ = self.history_repo.save(&entry); // Don't fail the request if history save fails

        // Step 5: Publish event
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: resolved_url,
            status: response.status,
            duration_ms: response.duration_ms,
        });

        Ok(response)
    }

    fn build_variable_map(&self, env_name: &Option<String>) -> DomainResult<HashMap<String, String>> {
        let mut vars = HashMap::new();
        if let Some(name) = env_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }
        Ok(vars)
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-app -- execution_service::tests
```
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(app): RequestExecutionService — cross-context request execution"
```

---

### Task 4: Wire up rocket-app lib.rs

**Files:**
- Modify: `crates/rocket-app/src/lib.rs`

- [ ] **Step 1: Export all services**

```rust
pub mod collection_service;
pub mod cookie_service;
pub mod environment_service;
pub mod execution_service;
pub mod history_service;
pub mod template_service;

pub use collection_service::CollectionService;
pub use cookie_service::CookieService;
pub use environment_service::EnvironmentService;
pub use execution_service::{ExecuteRequestInput, RequestExecutionService};
pub use history_service::HistoryService;
pub use template_service::TemplateService;
```

- [ ] **Step 2: Full workspace test**

```bash
cargo test --workspace
cargo clippy --workspace
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-app/src/lib.rs
git commit -m "feat(app): wire up all service exports"
```

---

## Milestone Checklist — Plan 5

- [ ] CollectionService: 3+ tests with mock repo
- [ ] EnvironmentService: 2+ tests
- [ ] HistoryService: 2+ tests
- [ ] TemplateService: 2+ tests
- [ ] CookieService: 2+ tests
- [ ] RequestExecutionService: 3+ tests (variable resolution, history save, event publish)
- [ ] All services use dependency injection (Box<dyn Trait>)
- [ ] Full workspace: `cargo test --workspace` — all pass
