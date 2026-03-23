# SP1 Plan 1: Workspace Scaffold + Shared Kernel

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the Cargo workspace with all 7 crates, establish the shared kernel (error types, domain events, common value objects), and verify everything compiles.

**Architecture:** Cargo workspace with 7 member crates following DDD bounded context pattern. Dependency rule: domain crates depend only on rocket-shared. rocket-infra and rocket-app depend on domain crates. src-tauri depends on rocket-app and rocket-infra.

**Tech Stack:** Rust 2021 edition, Tauri 2.0, serde, thiserror

---

## File Structure

```
Cargo.toml                              # Workspace root
crates/
  rocket-shared/
    Cargo.toml
    src/lib.rs
    src/error.rs
    src/events.rs
    src/types.rs
  rocket-collection/
    Cargo.toml
    src/lib.rs
  rocket-environment/
    Cargo.toml
    src/lib.rs
  rocket-http/
    Cargo.toml
    src/lib.rs
  rocket-history/
    Cargo.toml
    src/lib.rs
  rocket-app/
    Cargo.toml
    src/lib.rs
  rocket-infra/
    Cargo.toml
    src/lib.rs
src-tauri/
  Cargo.toml
  tauri.conf.json
  build.rs
  src/main.rs
  src/lib.rs
```

---

## Chunk 1: Workspace root + crate scaffolds

### Task 1: Create workspace Cargo.toml and all crate stubs

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/rocket-shared/Cargo.toml`
- Create: `crates/rocket-shared/src/lib.rs`
- Create: `crates/rocket-collection/Cargo.toml`
- Create: `crates/rocket-collection/src/lib.rs`
- Create: `crates/rocket-environment/Cargo.toml`
- Create: `crates/rocket-environment/src/lib.rs`
- Create: `crates/rocket-http/Cargo.toml`
- Create: `crates/rocket-http/src/lib.rs`
- Create: `crates/rocket-history/Cargo.toml`
- Create: `crates/rocket-history/src/lib.rs`
- Create: `crates/rocket-app/Cargo.toml`
- Create: `crates/rocket-app/src/lib.rs`
- Create: `crates/rocket-infra/Cargo.toml`
- Create: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/rocket-shared",
    "crates/rocket-collection",
    "crates/rocket-environment",
    "crates/rocket-http",
    "crates/rocket-history",
    "crates/rocket-app",
    "crates/rocket-infra",
    "src-tauri",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
# Shared across crates — declare versions once here
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "cookies", "multipart"] }
notify = { version = "6", features = ["macos_fsevent"] }
sha2 = "0.10"
base64 = "0.22"
dirs = "5"
log = "0.4"
env_logger = "0.11"
async-trait = "0.1"

# Internal crates
rocket-shared = { path = "crates/rocket-shared" }
rocket-collection = { path = "crates/rocket-collection" }
rocket-environment = { path = "crates/rocket-environment" }
rocket-http = { path = "crates/rocket-http" }
rocket-history = { path = "crates/rocket-history" }
rocket-app = { path = "crates/rocket-app" }
rocket-infra = { path = "crates/rocket-infra" }
```

- [ ] **Step 2: Create rocket-shared crate stub**

`crates/rocket-shared/Cargo.toml`:
```toml
[package]
name = "rocket-shared"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
chrono.workspace = true
```

`crates/rocket-shared/src/lib.rs`:
```rust
pub mod error;
pub mod events;
pub mod types;
```

- [ ] **Step 3: Create rocket-collection crate stub**

`crates/rocket-collection/Cargo.toml`:
```toml
[package]
name = "rocket-collection"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
```

`crates/rocket-collection/src/lib.rs`:
```rust
// Bounded context: Collection management
```

- [ ] **Step 4: Create rocket-environment crate stub**

`crates/rocket-environment/Cargo.toml`:
```toml
[package]
name = "rocket-environment"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
```

`crates/rocket-environment/src/lib.rs`:
```rust
// Bounded context: Environment & variable management
```

- [ ] **Step 5: Create rocket-http crate stub**

`crates/rocket-http/Cargo.toml`:
```toml
[package]
name = "rocket-http"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
chrono.workspace = true
```

`crates/rocket-http/src/lib.rs`:
```rust
// Bounded context: HTTP request execution
```

- [ ] **Step 6: Create rocket-history crate stub**

`crates/rocket-history/Cargo.toml`:
```toml
[package]
name = "rocket-history"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
chrono.workspace = true
uuid.workspace = true
```

`crates/rocket-history/src/lib.rs`:
```rust
// Bounded context: Request history & templates
```

- [ ] **Step 7: Create rocket-app crate stub**

`crates/rocket-app/Cargo.toml`:
```toml
[package]
name = "rocket-app"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
rocket-collection.workspace = true
rocket-environment.workspace = true
rocket-http.workspace = true
rocket-history.workspace = true
async-trait.workspace = true
tokio.workspace = true
log.workspace = true
```

`crates/rocket-app/src/lib.rs`:
```rust
// Application services — orchestration layer
```

- [ ] **Step 8: Create rocket-infra crate stub**

`crates/rocket-infra/Cargo.toml`:
```toml
[package]
name = "rocket-infra"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
rocket-collection.workspace = true
rocket-environment.workspace = true
rocket-http.workspace = true
rocket-history.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
tokio.workspace = true
reqwest.workspace = true
notify.workspace = true
dirs.workspace = true
sha2.workspace = true
base64.workspace = true
log.workspace = true
uuid.workspace = true
chrono.workspace = true
```

`crates/rocket-infra/src/lib.rs`:
```rust
// Infrastructure implementations — filesystem repos, HTTP client, file watcher
```

- [ ] **Step 9: Verify workspace compiles**

```bash
cargo check
```
Expected: PASS — all crates resolve, no circular deps.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/
git commit -m "feat: scaffold Cargo workspace with 7 DDD crates"
```

---

## Chunk 2: Shared kernel — error, events, types

### Task 2: Implement shared error types

**Files:**
- Create: `crates/rocket-shared/src/error.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test**

Add to bottom of `crates/rocket-shared/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_error_display_not_found() {
        let err = DomainError::NotFound("Collection 'foo'".into());
        assert_eq!(err.to_string(), "Not found: Collection 'foo'");
    }

    #[test]
    fn domain_error_display_invalid_input() {
        let err = DomainError::InvalidInput("name cannot be empty".into());
        assert_eq!(err.to_string(), "Invalid input: name cannot be empty");
    }

    #[test]
    fn domain_error_serializes_to_string() {
        let err = DomainError::NotFound("test".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Not found: test\"");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-shared -- error::tests
```
Expected: FAIL — `DomainError` not defined.

- [ ] **Step 3: Implement DomainError**

`crates/rocket-shared/src/error.rs`:
```rust
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl Serialize for DomainError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<serde_json::Error> for DomainError {
    fn from(err: serde_json::Error) -> Self {
        DomainError::Serialization(err.to_string())
    }
}

pub type DomainResult<T> = Result<T, DomainError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_error_display_not_found() {
        let err = DomainError::NotFound("Collection 'foo'".into());
        assert_eq!(err.to_string(), "Not found: Collection 'foo'");
    }

    #[test]
    fn domain_error_display_invalid_input() {
        let err = DomainError::InvalidInput("name cannot be empty".into());
        assert_eq!(err.to_string(), "Invalid input: name cannot be empty");
    }

    #[test]
    fn domain_error_serializes_to_string() {
        let err = DomainError::NotFound("test".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, "\"Not found: test\"");
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p rocket-shared -- error::tests
```
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared/src/error.rs
git commit -m "feat(shared): DomainError enum with serde + thiserror"
```

---

### Task 3: Implement shared value objects (HttpMethod, Header, Body)

**Files:**
- Create: `crates/rocket-shared/src/types.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_from_string() {
        assert_eq!(HttpMethod::from_str("GET"), Ok(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Ok(HttpMethod::Post));
        assert!(HttpMethod::from_str("INVALID").is_err());
    }

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
    }

    #[test]
    fn header_enabled_by_default() {
        let h = Header::new("Content-Type", "application/json");
        assert!(h.enabled);
    }

    #[test]
    fn body_mode_serialization() {
        let body = Body {
            mode: BodyMode::Json,
            content: Some("{\"key\":\"value\"}".into()),
            form_data: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"mode\":\"json\""));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-shared -- types::tests
```
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement shared types**

`crates/rocket-shared/src/types.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

// ============================================================
// HttpMethod
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Options => write!(f, "OPTIONS"),
            HttpMethod::Head => write!(f, "HEAD"),
        }
    }
}

impl FromStr for HttpMethod {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "PATCH" => Ok(HttpMethod::Patch),
            "DELETE" => Ok(HttpMethod::Delete),
            "OPTIONS" => Ok(HttpMethod::Options),
            "HEAD" => Ok(HttpMethod::Head),
            _ => Err(DomainError::InvalidInput(format!(
                "Invalid HTTP method: {}",
                s
            ))),
        }
    }
}

// ============================================================
// Header
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
        }
    }
}

// ============================================================
// Body
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyMode {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "formdata")]
    FormData,
    #[serde(rename = "binary")]
    Binary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub mode: BodyMode,
    pub content: Option<String>,
    pub form_data: Option<Vec<FormDataEntry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub entry_type: FormDataType,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormDataType {
    Text,
    File,
}

// ============================================================
// Auth
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "authType", rename_all = "kebab-case")]
pub enum Auth {
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    #[serde(rename_all = "camelCase")]
    ApiKey {
        key: String,
        value: String,
        add_to: ApiKeyLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

impl Default for Auth {
    fn default() -> Self {
        Auth::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_from_string() {
        assert_eq!(HttpMethod::from_str("GET"), Ok(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Ok(HttpMethod::Post));
        assert!(HttpMethod::from_str("INVALID").is_err());
    }

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
    }

    #[test]
    fn header_enabled_by_default() {
        let h = Header::new("Content-Type", "application/json");
        assert!(h.enabled);
    }

    #[test]
    fn body_mode_serialization() {
        let body = Body {
            mode: BodyMode::Json,
            content: Some("{\"key\":\"value\"}".into()),
            form_data: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"mode\":\"json\""));
    }

    #[test]
    fn auth_none_is_default() {
        assert_eq!(Auth::default(), Auth::None);
    }

    #[test]
    fn auth_basic_serialization() {
        let auth = Auth::Basic {
            username: "user".into(),
            password: "pass".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"basic\""));
        assert!(json.contains("\"username\":\"user\""));
    }

    #[test]
    fn auth_tagged_deserialization() {
        let json = r#"{"authType":"bearer","token":"abc123"}"#;
        let auth: Auth = serde_json::from_str(json).unwrap();
        assert_eq!(
            auth,
            Auth::Bearer {
                token: "abc123".into()
            }
        );
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-shared -- types::tests
```
Expected: PASS — 6 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared/src/types.rs
git commit -m "feat(shared): HttpMethod, Header, Body, Auth value objects"
```

---

### Task 4: Implement domain events

**Files:**
- Create: `crates/rocket-shared/src/events.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_event_serialization() {
        let event = DomainEvent::CollectionCreated {
            name: "my-api".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("CollectionCreated"));
        assert!(json.contains("my-api"));
    }

    #[test]
    fn event_publisher_trait_is_object_safe() {
        // Compile-time check: can we create Box<dyn EventPublisher>?
        fn _assert_object_safe(_: Box<dyn EventPublisher>) {}
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-shared -- events::tests
```
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement domain events**

`crates/rocket-shared/src/events.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DomainEvent {
    // Collection events
    CollectionCreated { name: String },
    CollectionDeleted { name: String },
    CollectionRenamed { old_name: String, new_name: String },

    // Request events
    RequestSaved { collection: String, path: String },
    RequestDeleted { collection: String, path: String },
    ItemMoved { src_collection: String, src_path: String, dst_collection: String, dst_path: String },

    // Environment events
    EnvironmentSaved { name: String },
    EnvironmentDeleted { name: String },

    // HTTP execution events
    RequestExecuted { method: String, url: String, status: u16, duration_ms: u64 },

    // File system events
    FileChanged { path: String, event_type: FileChangeKind, collection: Option<String> },

    // History events
    HistoryCleared,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    Create,
    Modify,
    Remove,
}

/// Trait for publishing domain events.
/// Implemented by TauriEventBus in infrastructure layer.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: DomainEvent);
}

/// No-op publisher for tests and contexts where events aren't needed.
pub struct NullEventPublisher;

impl EventPublisher for NullEventPublisher {
    fn publish(&self, _event: DomainEvent) {
        // Intentionally empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_event_serialization() {
        let event = DomainEvent::CollectionCreated {
            name: "my-api".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("CollectionCreated") || json.contains("collectionCreated"));
        assert!(json.contains("my-api"));
    }

    #[test]
    fn event_publisher_trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn EventPublisher>) {}
    }

    #[test]
    fn null_publisher_does_not_panic() {
        let pub_ = NullEventPublisher;
        pub_.publish(DomainEvent::HistoryCleared);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-shared -- events::tests
```
Expected: PASS — 3 tests.

- [ ] **Step 5: Run full workspace check**

```bash
cargo test --workspace
cargo clippy --workspace
```
Expected: all PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(shared): DomainEvent enum + EventPublisher trait"
```

---

## Milestone Checklist — Plan 1

- [ ] Cargo workspace compiles with 7 crates + src-tauri pending
- [ ] `rocket-shared` has: DomainError, DomainResult, HttpMethod, Header, Body, Auth, DomainEvent, EventPublisher
- [ ] All 12 tests pass: `cargo test -p rocket-shared`
- [ ] No clippy warnings: `cargo clippy --workspace`
- [ ] No circular dependencies between crates
