# CollectionService Event Publishing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `CollectionService` mutation publish the `DomainEvent` that already describes what it did, so the collection tree in the sidebar refreshes deterministically after its own operation succeeds (fixing: deleting a request via the tree's context menu doesn't remove it from the UI).

**Architecture:** `CollectionService` (in `rocket-app`) gains a `Box<dyn EventPublisher>` constructor parameter — the same trait-injection pattern every other service in the crate already uses — and calls `.publish(...)` after each mutating repo call succeeds. Six of the needed `DomainEvent` variants already exist but are never constructed; six more (folder/settings/variables events) are new. All route through the existing `TauriEventBus` to the `"collection-changed"` Tauri channel the frontend already listens on correctly. No frontend changes.

**Tech Stack:** Rust (rocket-shared, rocket-app, src-tauri crates), `cargo test`/`cargo check`.

**Spec:** `docs/superpowers/specs/2026-09-20-collection-service-event-publishing-design.md`

## Global Constraints

- No frontend changes — `CollectionNode.tsx`'s listener is already correct for these event shapes (verified in the spec's investigation).
- `NotifyFileWatcher` (`crates/rocket-infra/src/file_watcher.rs`) is unchanged — it remains a fallback, not replaced.
- Every event publish happens only after its repo call succeeds (`?` already short-circuits on error — publish goes immediately after the repo call line, never before).
- Follow the crate's existing `Box<dyn EventPublisher>` constructor-injection pattern exactly (see `crates/rocket-app/src/environment_service.rs`) — no new pattern.
- Out of scope: `cookie_service.rs`/`template_service.rs`'s identical dead-publisher gap, and the unconsumed `runner-*`/`history-changed`/`script-*` frontend channels.
- Test code uses `.expect("message")` for fallible setup calls, following this project's Rust safety convention and existing precedent (`execution_service.rs`'s `RecordingPublisher`).

---

## Task 1: New `DomainEvent` variants in `rocket-shared`

**Files:**
- Modify: `crates/rocket-shared/src/events.rs:11-13` (insert after `ItemMoved`)
- Test: `crates/rocket-shared/src/events.rs` (same file, `#[cfg(test)] mod tests` at end)

**Interfaces:**
- Produces: `DomainEvent::FolderCreated { collection: String, path: String }`, `DomainEvent::FolderDeleted { collection: String, path: String }`, `DomainEvent::ItemsReordered { collection: String, folder_path: String }`, `DomainEvent::CollectionSettingsSaved { collection: String }`, `DomainEvent::FolderVariablesSaved { collection: String, folder_path: String }`, `DomainEvent::RequestVariablesSaved { collection: String, request_path: String }` — all consumed by Tasks 5, 6, and 7.

- [ ] **Step 1: Write the failing wire-shape tests**

Add to the `#[cfg(test)] mod tests` block at the end of `crates/rocket-shared/src/events.rs` (after the existing `runner_finished_wire_shape` test, which ends the file):

```rust
    #[test]
    fn folder_created_wire_shape() {
        let event = DomainEvent::FolderCreated { collection: "my-api".into(), path: "auth".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"folderCreated","collection":"my-api","path":"auth"}"#);
    }

    #[test]
    fn folder_deleted_wire_shape() {
        let event = DomainEvent::FolderDeleted { collection: "my-api".into(), path: "auth".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"folderDeleted","collection":"my-api","path":"auth"}"#);
    }

    #[test]
    fn items_reordered_wire_shape() {
        let event = DomainEvent::ItemsReordered { collection: "my-api".into(), folder_path: "auth".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"itemsReordered","collection":"my-api","folder_path":"auth"}"#);
    }

    #[test]
    fn collection_settings_saved_wire_shape() {
        let event = DomainEvent::CollectionSettingsSaved { collection: "my-api".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"collectionSettingsSaved","collection":"my-api"}"#);
    }

    #[test]
    fn folder_variables_saved_wire_shape() {
        let event = DomainEvent::FolderVariablesSaved { collection: "my-api".into(), folder_path: "auth".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"folderVariablesSaved","collection":"my-api","folder_path":"auth"}"#);
    }

    #[test]
    fn request_variables_saved_wire_shape() {
        let event = DomainEvent::RequestVariablesSaved { collection: "my-api".into(), request_path: "users.yml".into() };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"requestVariablesSaved","collection":"my-api","request_path":"users.yml"}"#);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-shared folder_created_wire_shape folder_deleted_wire_shape items_reordered_wire_shape collection_settings_saved_wire_shape folder_variables_saved_wire_shape request_variables_saved_wire_shape`
Expected: FAIL with `no variant named 'FolderCreated' found for enum 'DomainEvent'` (and similarly for the other five).

- [ ] **Step 3: Add the six new variants**

In `crates/rocket-shared/src/events.rs`, find this block (lines 11-13):

```rust
    // Request events
    RequestSaved { collection: String, path: String },
    RequestDeleted { collection: String, path: String },
    ItemMoved { src_collection: String, src_path: String, dst_collection: String, dst_path: String },
```

Replace it with:

```rust
    // Request events
    RequestSaved { collection: String, path: String },
    RequestDeleted { collection: String, path: String },
    ItemMoved { src_collection: String, src_path: String, dst_collection: String, dst_path: String },

    // Folder events
    FolderCreated { collection: String, path: String },
    FolderDeleted { collection: String, path: String },
    ItemsReordered { collection: String, folder_path: String },

    // Collection settings/variable events
    CollectionSettingsSaved { collection: String },
    FolderVariablesSaved { collection: String, folder_path: String },
    RequestVariablesSaved { collection: String, request_path: String },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-shared folder_created_wire_shape folder_deleted_wire_shape items_reordered_wire_shape collection_settings_saved_wire_shape folder_variables_saved_wire_shape request_variables_saved_wire_shape`
Expected: PASS (6 tests)

- [ ] **Step 5: Run the full crate test suite to confirm nothing else broke**

Run: `cargo test -p rocket-shared`
Expected: PASS (all tests, including the pre-existing `domain_event_serialization` etc.)

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(events): add folder and settings/variables DomainEvent variants"
```

---

## Task 2: Wire `EventPublisher` into `CollectionService` + collection-level events

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs:1-61` (imports, struct, constructors, `create`/`delete`/`rename`)
- Modify: `crates/rocket-app/src/collection_service.rs` test module (`make_service()`, `delete_emits_security_audit_event`, plus new test helpers/tests)

**Interfaces:**
- Consumes: `rocket_shared::events::{DomainEvent, EventPublisher, NullEventPublisher}` (already exist).
- Produces: `CollectionService::new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self` and `CollectionService::new_with_audit(repo, events, audit) -> Self` — **breaking signature change**, consumed by Tasks 3-6 (same file) and Task 7 (`src-tauri/src/lib.rs`). Also produces the `RecordingEventPublisher`/`SharedEventPublisher` test helpers (in the test module), reused by Tasks 3-6.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `crates/rocket-app/src/collection_service.rs`, right after the existing `struct CapturingPublisher { ... }` / `impl SecurityAuditPublisher for CapturingPublisher { ... }` block (currently lines 292-299):

```rust
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
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("my-api").expect("create");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(e, DomainEvent::CollectionCreated { name } if name == "my-api")),
            "expected CollectionCreated, got {:?}", *published
        );
    }

    #[test]
    fn delete_emits_collection_deleted() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("temp").expect("create");
        svc.delete("temp").expect("delete");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(e, DomainEvent::CollectionDeleted { name } if name == "temp")),
            "expected CollectionDeleted, got {:?}", *published
        );
    }

    #[test]
    fn rename_emits_collection_renamed() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
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
```

Also note the two existing constructor call sites in the test module (`make_service()` and `delete_emits_security_audit_event`) will need updating — that happens in Step 5 below. Leave them as-is for now, so Step 2 observes a real compile failure caused only by the new tests above.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: FAIL to compile — `this function takes 1 argument but 2 arguments were supplied` for the three new `CollectionService::new(...)` calls (the constructor doesn't take an `events` param yet).

- [ ] **Step 3: Wire `EventPublisher` into the struct and constructors**

In `crates/rocket-app/src/collection_service.rs`, replace the imports at the top (lines 1-8):

```rust
use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{Collection, CollectionRepository, CollectionSummary, CollectionVariable, Request};
use rocket_shared::description::Documentation;
use rocket_shared::error::DomainResult;
use std::sync::Arc;
```

with:

```rust
use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{Collection, CollectionRepository, CollectionSummary, CollectionVariable, Request};
use rocket_shared::description::Documentation;
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use std::sync::Arc;
```

Replace the struct and constructors (lines 10-28):

```rust
pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>) -> Self {
        Self {
            repo,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Box<dyn CollectionRepository>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self { repo, audit }
    }
```

with:

```rust
pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self {
            repo,
            events,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Box<dyn CollectionRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self { repo, events, audit }
    }
```

- [ ] **Step 4: Publish events from `create`/`delete`/`rename`**

Replace (current `create`/`delete`/`rename`):

```rust
    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        self.repo.create(name)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: name.to_string() },
        );
        Ok(())
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        self.repo.rename(old_name, new_name)
    }
```

with:

```rust
    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let collection = self.repo.create(name)?;
        self.events.publish(DomainEvent::CollectionCreated { name: name.to_string() });
        Ok(collection)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: name.to_string() },
        );
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
```

- [ ] **Step 5: Fix the two existing test call sites**

In the test module, replace:

```rust
    fn make_service() -> CollectionService {
        CollectionService::new(Box::new(MockCollectionRepo::new()))
    }
```

with:

```rust
    fn make_service() -> CollectionService {
        CollectionService::new(Box::new(MockCollectionRepo::new()), Box::new(NullEventPublisher))
    }
```

And add `NullEventPublisher` to the test module's imports — replace:

```rust
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use std::sync::Mutex;
```

with:

```rust
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;
```

Then replace the `delete_emits_security_audit_event` test's constructor call:

```rust
        let svc = CollectionService::new_with_audit(
            Box::new(MockCollectionRepo::new()),
            publisher.clone(),
        );
```

with:

```rust
        let svc = CollectionService::new_with_audit(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: PASS (all tests in this module, including the 3 new ones and the existing 9)

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "feat(collection-service): publish DomainEvents for create/delete/rename"
```

---

## Task 3: Request-level events (fixes the reported delete bug)

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (`save_request`, `rename_request`, `update_request_docs`, `delete_request`)
- Modify: `crates/rocket-app/src/collection_service.rs` test module (`MockCollectionRepo`: `get_request`/`save_request`/`delete_request`; new tests)

**Interfaces:**
- Consumes: `RecordingEventPublisher`/`SharedEventPublisher` from Task 2 (same file, same test module).
- Produces: nothing new for other tasks — this is the leaf fix for the originally reported bug.

- [ ] **Step 1: Write the failing tests**

Add to the test module, after the `rename_emits_collection_renamed` test from Task 2:

```rust
    #[test]
    fn save_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
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
    fn delete_request_emits_request_deleted() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        svc.delete_request("my-api", "users.yml").expect("delete_request");
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
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let result = svc.delete_request("my-api", "ghost.yml");
        assert!(result.is_err(), "deleting a nonexistent request must fail");
        assert!(publisher.events.lock().expect("lock").is_empty(), "no event should publish on failure");
    }

    #[test]
    fn rename_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.rename_request("my-api", "users.yml", "List Users").expect("rename_request");
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
    fn update_request_docs_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.update_request_docs("my-api", "users.yml", Some("Fetches all users.".into())).expect("update_request_docs");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }
```

Add `HttpMethod` to the test module's imports — replace:

```rust
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;
```

with:

```rust
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::Mutex;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: FAIL — the new tests panic (from `MockCollectionRepo`'s `get_request`/`delete_request` still being `unimplemented!()`), and/or the assertions fail because no `RequestSaved`/`RequestDeleted` is published yet.

- [ ] **Step 3: Give `MockCollectionRepo` a real in-memory request store**

Replace the `MockCollectionRepo` struct and its `new()`:

```rust
    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()) }
        }
    }
```

with:

```rust
    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
        requests: Mutex<Vec<(String, String, Request)>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()), requests: Mutex::new(Vec::new()) }
        }
    }
```

Replace the `get_request`/`save_request`/`delete_request` stubs:

```rust
        fn get_request(&self, _: &str, _: &str) -> DomainResult<Request> { unimplemented!() }
        fn save_request(&self, _: &str, path: &str, _: &Request) -> DomainResult<String> { Ok(path.to_string()) }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
```

with:

```rust
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            self.requests
                .lock()
                .expect("lock")
                .iter()
                .find(|(c, p, _)| c == collection && p == path)
                .map(|(_, _, r)| r.clone())
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
            let mut requests = self.requests.lock().expect("lock");
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            requests.push((collection.to_string(), path.to_string(), request.clone()));
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
            let mut requests = self.requests.lock().expect("lock");
            let len_before = requests.len();
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            if requests.len() == len_before {
                return Err(DomainError::NotFound(format!("{collection}/{path}")));
            }
            Ok(())
        }
```

(`rename_request` stays `unimplemented!()` — `CollectionService::rename_request` never calls `repo.rename_request`; it calls `repo.get_request` then `repo.save_request`, both of which are now implemented above.)

- [ ] **Step 4: Publish events from `save_request`/`rename_request`/`update_request_docs`/`delete_request`**

Replace:

```rust
    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<Request> {
        let actual_path = self.repo.save_request(collection, path, request)?;
        self.repo.get_request(collection, &actual_path)
    }

    pub fn rename_request(&self, collection: &str, old_path: &str, new_name: &str) -> DomainResult<()> {
        // Only update the name field inside the JSON. The filename stays the same.
        // This produces a single Modify filesystem event.
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        self.repo.save_request(collection, old_path, &request)?;
        Ok(())
    }

    pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
        let mut request = self.repo.get_request(collection, path)?;
        request.docs = docs.map(Documentation::text);
        self.repo.save_request(collection, path, &request)?;
        Ok(())
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_request(collection, path)
    }
```

with:

```rust
    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<Request> {
        let actual_path = self.repo.save_request(collection, path, request)?;
        let saved = self.repo.get_request(collection, &actual_path)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path,
        });
        Ok(saved)
    }

    pub fn rename_request(&self, collection: &str, old_path: &str, new_name: &str) -> DomainResult<()> {
        // Only update the name field inside the JSON. The filename stays the same.
        // This produces a single Modify filesystem event.
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        self.repo.save_request(collection, old_path, &request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: old_path.to_string(),
        });
        Ok(())
    }

    pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
        let mut request = self.repo.get_request(collection, path)?;
        request.docs = docs.map(Documentation::text);
        self.repo.save_request(collection, path, &request)?;
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: PASS (all tests, including the 5 new ones)

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "fix(collection-service): publish RequestSaved/RequestDeleted events"
```

---

## Task 4: `move_item` event

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (`move_item`)
- Modify: `crates/rocket-app/src/collection_service.rs` test module (`MockCollectionRepo::move_item`; new test)

**Interfaces:**
- Consumes: `RecordingEventPublisher`/`SharedEventPublisher` from Task 2.

- [ ] **Step 1: Write the failing test**

Add to the test module, after `update_request_docs_emits_request_saved`:

```rust
    #[test]
    fn move_item_emits_item_moved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.move_item("my-api", "users.yml", "other-api", "users.yml").expect("move_item");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemMoved { src_collection, src_path, dst_collection, dst_path }
                    if src_collection == "my-api" && src_path == "users.yml"
                        && dst_collection == "other-api" && dst_path == "users.yml"
            )),
            "expected ItemMoved, got {:?}", *published
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-app --lib move_item_emits_item_moved`
Expected: FAIL with a panic (from `MockCollectionRepo::move_item`'s `unimplemented!()`).

- [ ] **Step 3: Implement `MockCollectionRepo::move_item`**

Replace:

```rust
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
```

with:

```rust
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
```

- [ ] **Step 4: Publish `ItemMoved` from `CollectionService::move_item`**

Replace:

```rust
    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo.move_item(src_collection, src_path, dst_collection, dst_path)
    }
```

with:

```rust
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
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: PASS (all tests)

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "feat(collection-service): publish ItemMoved event"
```

---

## Task 5: Folder events

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (`create_folder`, `delete_folder`)
- Modify: `crates/rocket-app/src/collection_service.rs` test module (`MockCollectionRepo::create_folder`/`delete_folder`; new tests)

**Interfaces:**
- Consumes: `DomainEvent::FolderCreated`/`FolderDeleted` from Task 1; `RecordingEventPublisher`/`SharedEventPublisher` from Task 2.

- [ ] **Step 1: Write the failing tests**

Add to the test module, after `move_item_emits_item_moved`:

```rust
    #[test]
    fn create_folder_emits_folder_created() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
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
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib create_folder_emits_folder_created delete_folder_emits_folder_deleted`
Expected: FAIL with a panic (from `MockCollectionRepo`'s `create_folder`/`delete_folder` `unimplemented!()`).

- [ ] **Step 3: Implement the mock stubs**

Replace:

```rust
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
```

with:

```rust
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
```

- [ ] **Step 4: Publish `FolderCreated`/`FolderDeleted`**

Replace:

```rust
    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)
    }
```

with:

```rust
    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)?;
        self.events.publish(DomainEvent::FolderCreated {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)?;
        self.events.publish(DomainEvent::FolderDeleted {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: PASS (all tests)

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "feat(collection-service): publish FolderCreated/FolderDeleted events"
```

---

## Task 6: Settings/variables/reorder events

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (`reorder_items`, `save_settings`, `save_folder_variables`, `save_request_variables`)
- Modify: `crates/rocket-app/src/collection_service.rs` test module (new tests only — the mock stubs for these four already return `Ok(())`)

**Interfaces:**
- Consumes: `DomainEvent::ItemsReordered`/`CollectionSettingsSaved`/`FolderVariablesSaved`/`RequestVariablesSaved` from Task 1; `RecordingEventPublisher`/`SharedEventPublisher` from Task 2.

- [ ] **Step 1: Write the failing tests**

Add to the test module, after `delete_folder_emits_folder_deleted`:

```rust
    #[test]
    fn reorder_items_emits_items_reordered() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.reorder_items("my-api", "auth", &["login.yml".to_string(), "logout.yml".to_string()]).expect("reorder_items");
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
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_settings("my-api", &rocket_collection::CollectionSettings::default()).expect("save_settings");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionSettingsSaved { collection } if collection == "my-api"
            )),
            "expected CollectionSettingsSaved, got {:?}", *published
        );
    }

    #[test]
    fn save_folder_variables_emits_folder_variables_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_folder_variables("my-api", "auth", vec![]).expect("save_folder_variables");
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
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_request_variables("my-api", "users.yml", vec![]).expect("save_request_variables");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestVariablesSaved { collection, request_path } if collection == "my-api" && request_path == "users.yml"
            )),
            "expected RequestVariablesSaved, got {:?}", *published
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib reorder_items_emits_items_reordered save_settings_emits_collection_settings_saved save_folder_variables_emits_folder_variables_saved save_request_variables_emits_request_variables_saved`
Expected: FAIL — each assertion fails because no event is published yet (the mock calls already return `Ok(())`, so these fail on the `assert!`, not a panic).

- [ ] **Step 3: Publish the four events**

Replace:

```rust
    pub fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
        self.repo.reorder_items(collection, folder_path, ordered_names)
    }
```

with:

```rust
    pub fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
        self.repo.reorder_items(collection, folder_path, ordered_names)?;
        self.events.publish(DomainEvent::ItemsReordered {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }
```

Replace:

```rust
    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.repo.save_settings(name, settings)
    }
```

with:

```rust
    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.repo.save_settings(name, settings)?;
        self.events.publish(DomainEvent::CollectionSettingsSaved { collection: name.to_string() });
        Ok(())
    }
```

Replace:

```rust
    pub fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_folder_variables(collection, folder_path, vars)
    }
```

with:

```rust
    pub fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_folder_variables(collection, folder_path, vars)?;
        self.events.publish(DomainEvent::FolderVariablesSaved {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }
```

Replace:

```rust
    pub fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_request_variables(collection, request_path, vars)
    }
```

with:

```rust
    pub fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_request_variables(collection, request_path, vars)?;
        self.events.publish(DomainEvent::RequestVariablesSaved {
            collection: collection.to_string(),
            request_path: request_path.to_string(),
        });
        Ok(())
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_service`
Expected: PASS (all tests)

- [ ] **Step 5: Run the whole `rocket-app` crate's test suite**

Run: `cargo test -p rocket-app`
Expected: PASS (confirms nothing in the rest of the crate broke from the constructor signature change — this is the final check before touching `src-tauri`)

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/collection_service.rs
git commit -m "feat(collection-service): publish reorder/settings/variables events"
```

---

## Task 7: Route the new events and wire a real publisher into `CollectionService`

**Files:**
- Modify: `src-tauri/src/tauri_event_bus.rs:32` (add 6 match arms)
- Modify: `src-tauri/src/lib.rs:195-203` (pass a real `TauriEventBus`, update the stale comment)

**Interfaces:**
- Consumes: `CollectionService::new_with_audit(repo, events, audit)` from Task 2; the 6 new `DomainEvent` variants from Task 1.

- [ ] **Step 1: Route the six new variants to `"collection-changed"`**

In `src-tauri/src/tauri_event_bus.rs`, find:

```rust
            DomainEvent::ItemMoved { .. } => "collection-changed",
            DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
```

Replace with:

```rust
            DomainEvent::ItemMoved { .. } => "collection-changed",
            DomainEvent::FolderCreated { .. }
            | DomainEvent::FolderDeleted { .. }
            | DomainEvent::ItemsReordered { .. }
            | DomainEvent::CollectionSettingsSaved { .. }
            | DomainEvent::FolderVariablesSaved { .. }
            | DomainEvent::RequestVariablesSaved { .. } => "collection-changed",
            DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
```

- [ ] **Step 2: Verify it compiles (the match will fail to build until this step, since the enum now has variants this file doesn't handle)**

Run: `cargo check -p rocket`
Expected: FAILS before this step with `non-exhaustive patterns` for the six new `DomainEvent` variants (confirms the match really is exhaustive-checked); PASSES after the edit above.

- [ ] **Step 3: Wire a real `TauriEventBus` into `CollectionService`**

In `src-tauri/src/lib.rs`, find:

```rust
            // Application services — no event publishing.
            // The file watcher is the single source of truth for sidebar updates.
            // SharedPathCollectionRepo resolves the base directory from
            // active_workspace_path at call time, so switching workspaces
            // automatically redirects all collection reads/writes.
            let collection_svc = CollectionService::new_with_audit(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
                audit_publisher.clone(),
            );
```

Replace with:

```rust
            // SharedPathCollectionRepo resolves the base directory from
            // active_workspace_path at call time, so switching workspaces
            // automatically redirects all collection reads/writes. The file
            // watcher (started further below) remains a fallback for changes
            // made outside the app; this service publishes its own events for
            // deterministic, immediate sidebar/tree refresh on success.
            let collection_svc = CollectionService::new_with_audit(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit_publisher.clone(),
            );
```

- [ ] **Step 4: Verify the whole app compiles**

Run: `cargo check -p rocket`
Expected: PASS

- [ ] **Step 5: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS (all crates)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tauri_event_bus.rs src-tauri/src/lib.rs
git commit -m "feat(collections): wire CollectionService into the live event bus"
```

---

## Final verification (after all 7 tasks)

- [ ] Run `cargo test --workspace` — expect PASS.
- [ ] Run `cargo check -p rocket` — expect PASS.
- [ ] Run `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx` — expect PASS unchanged (confirms the frontend listener still works with no frontend edits, per the spec's §3.6).
- [ ] Run `yarn tsc --noEmit` — expect PASS (no frontend files touched, but confirms nothing else in the tree is broken by this branch).
- [ ] Manual smoke test (`yarn tauri dev`): right-click a request in the collection tree → Delete → confirm. The request disappears from the tree immediately, without waiting or needing a manual refresh. Repeat for: create/delete/rename a collection, save a request, move an item, create/delete a folder — each should reflect in the tree immediately.
