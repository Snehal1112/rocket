# SP1 Plan 3: Environment, HTTP, and History Bounded Contexts

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the remaining 3 domain crates — rocket-environment (aggregate + resolver + repo trait), rocket-http (request/response VOs + executor trait + cookie aggregate), and rocket-history (entry + template aggregates + repo traits).

**Architecture:** Each crate is an independent bounded context. They depend only on rocket-shared. No cross-context dependencies.

**Tech Stack:** Rust, serde, rocket-shared, async-trait, chrono, uuid

---

## Chunk 1: Environment bounded context

### Task 1: Variable value object

**Files:**
- Create: `crates/rocket-environment/src/variable.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_variable_enabled_by_default() {
        let v = Variable::new("BASE_URL", "https://api.example.com");
        assert_eq!(v.key, "BASE_URL");
        assert!(v.enabled);
        assert!(!v.secret);
    }

    #[test]
    fn secret_variable() {
        let v = Variable::secret("API_KEY", "sk-12345");
        assert!(v.secret);
        assert!(v.enabled);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-environment -- variable::tests
```
Expected: FAIL.

- [ ] **Step 3: Implement Variable**

`crates/rocket-environment/src/variable.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
}

impl Variable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
        }
    }

    pub fn secret(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: true,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
            secret: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_variable_enabled_by_default() {
        let v = Variable::new("BASE_URL", "https://api.example.com");
        assert_eq!(v.key, "BASE_URL");
        assert!(v.enabled);
        assert!(!v.secret);
    }

    #[test]
    fn secret_variable() {
        let v = Variable::secret("API_KEY", "sk-12345");
        assert!(v.secret);
        assert!(v.enabled);
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-environment -- variable::tests
```
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-environment/src/variable.rs
git commit -m "feat(environment): Variable value object"
```

---

### Task 2: Environment aggregate + resolver

**Files:**
- Create: `crates/rocket-environment/src/environment.rs`
- Create: `crates/rocket-environment/src/resolver.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests for Environment**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable::Variable;

    #[test]
    fn new_environment_is_empty() {
        let env = Environment::new("production");
        assert_eq!(env.name, "production");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn set_variable_adds_new() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn set_variable_updates_existing() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.set_variable(Variable::new("HOST", "127.0.0.1"));
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].value, "127.0.0.1");
    }

    #[test]
    fn remove_variable() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.remove_variable("HOST");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn get_value_returns_enabled_only() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("ENABLED", "yes"));
        env.set_variable(Variable::disabled("DISABLED", "no"));
        assert_eq!(env.get_value("ENABLED"), Some("yes"));
        assert_eq!(env.get_value("DISABLED"), None);
    }
}
```

- [ ] **Step 2: Implement Environment aggregate**

`crates/rocket-environment/src/environment.rs`:
```rust
use serde::{Deserialize, Serialize};

use crate::variable::Variable;

/// Environment aggregate root.
/// A named set of key-value variables used for request interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub name: String,
    pub variables: Vec<Variable>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variables: Vec::new(),
        }
    }

    /// Add or update a variable. If a variable with the same key exists, replace it.
    pub fn set_variable(&mut self, variable: Variable) {
        if let Some(existing) = self.variables.iter_mut().find(|v| v.key == variable.key) {
            *existing = variable;
        } else {
            self.variables.push(variable);
        }
    }

    /// Remove a variable by key.
    pub fn remove_variable(&mut self, key: &str) {
        self.variables.retain(|v| v.key != key);
    }

    /// Get the value of an enabled variable by key.
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|v| v.key == key && v.enabled)
            .map(|v| v.value.as_str())
    }

    /// Get all enabled variables as key-value pairs.
    pub fn enabled_variables(&self) -> Vec<(&str, &str)> {
        self.variables
            .iter()
            .filter(|v| v.enabled)
            .map(|v| (v.key.as_str(), v.value.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable::Variable;

    #[test]
    fn new_environment_is_empty() {
        let env = Environment::new("production");
        assert_eq!(env.name, "production");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn set_variable_adds_new() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn set_variable_updates_existing() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.set_variable(Variable::new("HOST", "127.0.0.1"));
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].value, "127.0.0.1");
    }

    #[test]
    fn remove_variable() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("HOST", "localhost"));
        env.remove_variable("HOST");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn get_value_returns_enabled_only() {
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("ENABLED", "yes"));
        env.set_variable(Variable::disabled("DISABLED", "no"));
        assert_eq!(env.get_value("ENABLED"), Some("yes"));
        assert_eq!(env.get_value("DISABLED"), None);
    }
}
```

- [ ] **Step 3: Write failing tests for resolver**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn resolve_simple_variable() {
        let mut vars = HashMap::new();
        vars.insert("BASE_URL".to_string(), "https://api.example.com".to_string());
        let result = resolve("{{BASE_URL}}/users", &vars);
        assert_eq!(result.output, "https://api.example.com/users");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());
        let result = resolve("http://{{HOST}}:{{PORT}}/api", &vars);
        assert_eq!(result.output, "http://localhost:8080/api");
    }

    #[test]
    fn resolve_missing_variable_left_as_is() {
        let vars = HashMap::new();
        let result = resolve("{{MISSING}}/path", &vars);
        assert_eq!(result.output, "{{MISSING}}/path");
        assert_eq!(result.unresolved, vec!["MISSING"]);
    }

    #[test]
    fn resolve_no_variables() {
        let vars = HashMap::new();
        let result = resolve("plain text", &vars);
        assert_eq!(result.output, "plain text");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_whitespace_in_braces_trimmed() {
        let mut vars = HashMap::new();
        vars.insert("KEY".to_string(), "value".to_string());
        let result = resolve("{{ KEY }}", &vars);
        assert_eq!(result.output, "value");
    }
}
```

- [ ] **Step 4: Implement resolver**

`crates/rocket-environment/src/resolver.rs`:
```rust
use std::collections::HashMap;

/// Result of variable resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveResult {
    /// The output string with variables replaced.
    pub output: String,
    /// Names of variables that were referenced but not found.
    pub unresolved: Vec<String>,
}

/// Resolve `{{variable}}` placeholders in a template string.
/// Variables not found in the map are left as-is and reported in `unresolved`.
pub fn resolve(template: &str, variables: &HashMap<String, String>) -> ResolveResult {
    let mut output = String::with_capacity(template.len());
    let mut unresolved = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            let mut var_name = String::new();
            let mut found_closing = false;

            for inner in chars.by_ref() {
                if inner == '}' {
                    // Check for second '}'
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        found_closing = true;
                        break;
                    } else {
                        var_name.push(inner);
                    }
                } else {
                    var_name.push(inner);
                }
            }

            let var_name_trimmed = var_name.trim().to_string();

            if found_closing {
                if let Some(value) = variables.get(&var_name_trimmed) {
                    output.push_str(value);
                } else {
                    // Leave as-is and record as unresolved
                    output.push_str("{{");
                    output.push_str(&var_name);
                    output.push_str("}}");
                    unresolved.push(var_name_trimmed);
                }
            } else {
                // Unclosed braces — output as-is
                output.push_str("{{");
                output.push_str(&var_name);
            }
        } else {
            output.push(ch);
        }
    }

    ResolveResult { output, unresolved }
}

/// Convenience: resolve using an Environment's enabled variables.
pub fn resolve_with_env(
    template: &str,
    env: &crate::environment::Environment,
) -> ResolveResult {
    let vars: HashMap<String, String> = env
        .enabled_variables()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    resolve(template, &vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_simple_variable() {
        let mut vars = HashMap::new();
        vars.insert("BASE_URL".to_string(), "https://api.example.com".to_string());
        let result = resolve("{{BASE_URL}}/users", &vars);
        assert_eq!(result.output, "https://api.example.com/users");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());
        let result = resolve("http://{{HOST}}:{{PORT}}/api", &vars);
        assert_eq!(result.output, "http://localhost:8080/api");
    }

    #[test]
    fn resolve_missing_variable_left_as_is() {
        let vars = HashMap::new();
        let result = resolve("{{MISSING}}/path", &vars);
        assert_eq!(result.output, "{{MISSING}}/path");
        assert_eq!(result.unresolved, vec!["MISSING"]);
    }

    #[test]
    fn resolve_no_variables() {
        let vars = HashMap::new();
        let result = resolve("plain text", &vars);
        assert_eq!(result.output, "plain text");
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn resolve_whitespace_in_braces_trimmed() {
        let mut vars = HashMap::new();
        vars.insert("KEY".to_string(), "value".to_string());
        let result = resolve("{{ KEY }}", &vars);
        assert_eq!(result.output, "value");
    }
}
```

- [ ] **Step 5: Run all environment tests**

```bash
cargo test -p rocket-environment
```
Expected: PASS — 7 environment + 5 resolver tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/
git commit -m "feat(environment): Environment aggregate + Variable VO + resolver"
```

---

### Task 3: EnvironmentRepository trait

**Files:**
- Create: `crates/rocket-environment/src/repository.rs`

- [ ] **Step 1: Implement trait**

`crates/rocket-environment/src/repository.rs`:
```rust
use rocket_shared::error::DomainResult;
use crate::environment::Environment;

pub trait EnvironmentRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Environment>>;
    fn get(&self, name: &str) -> DomainResult<Environment>;
    fn save(&self, env: &Environment) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn EnvironmentRepository>) {}
    }
}
```

- [ ] **Step 2: Update lib.rs**

`crates/rocket-environment/src/lib.rs`:
```rust
pub mod environment;
pub mod repository;
pub mod resolver;
pub mod variable;

pub use environment::Environment;
pub use repository::EnvironmentRepository;
pub use resolver::{resolve, resolve_with_env, ResolveResult};
pub use variable::Variable;
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rocket-environment
git add crates/rocket-environment/src/
git commit -m "feat(environment): EnvironmentRepository trait + lib exports"
```

---

## Chunk 2: HTTP execution bounded context

### Task 4: HTTP request/response value objects

**Files:**
- Create: `crates/rocket-http/src/request.rs`
- Create: `crates/rocket-http/src/response.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement HttpRequest VO**

`crates/rocket-http/src/request.rs`:
```rust
use rocket_shared::types::{Auth, Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

/// An HTTP request ready for execution (resolved variables, all fields populated).
/// This is different from collection::Request which is a saved definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
}

fn default_true() -> bool { true }
fn default_timeout() -> u64 { 30_000 }

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            timeout_ms: 30_000,
            verify_ssl: true,
        }
    }
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        assert!(req.options.follow_redirects);
        assert_eq!(req.options.timeout_ms, 30_000);
        assert!(req.options.verify_ssl);
    }
}
```

- [ ] **Step 2: Implement HttpResponse VO**

`crates/rocket-http/src/response.rs`:
```rust
use rocket_shared::types::Header;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    pub body: String,
    pub duration_ms: u64,
    pub size_bytes: usize,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    pub fn header_value(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(key))
            .map(|h| h.value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header_value("content-type")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_response(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![Header::new("content-type", "application/json")],
            body: "{}".into(),
            duration_ms: 150,
            size_bytes: 2,
        }
    }

    #[test]
    fn status_classification() {
        assert!(sample_response(200).is_success());
        assert!(sample_response(301).is_redirect());
        assert!(sample_response(404).is_client_error());
        assert!(sample_response(500).is_server_error());
    }

    #[test]
    fn header_lookup_case_insensitive() {
        let resp = sample_response(200);
        assert_eq!(resp.header_value("Content-Type"), Some("application/json"));
        assert_eq!(resp.content_type(), Some("application/json"));
        assert_eq!(resp.header_value("x-missing"), None);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-http
```
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-http/src/request.rs crates/rocket-http/src/response.rs
git commit -m "feat(http): HttpRequest + HttpResponse value objects"
```

---

### Task 5: Cookie aggregate + HttpExecutor trait

**Files:**
- Create: `crates/rocket-http/src/cookie.rs`
- Create: `crates/rocket-http/src/executor.rs`
- Create: `crates/rocket-http/src/cookie_repository.rs`

- [ ] **Step 1: Implement CookieJar aggregate**

`crates/rocket-http/src/cookie.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<String>,
}

/// CookieJar aggregate — cookies grouped by domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieJar {
    pub domain: String,
    pub cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            cookies: Vec::new(),
        }
    }

    pub fn add(&mut self, cookie: Cookie) {
        // Replace existing cookie with same name
        if let Some(existing) = self.cookies.iter_mut().find(|c| c.name == cookie.name) {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.cookies.retain(|c| c.name != name);
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.iter().find(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cookie(name: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "val".into(),
            domain: "example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            expires: None,
        }
    }

    #[test]
    fn add_and_get_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        assert_eq!(jar.get("session").unwrap().value, "val");
    }

    #[test]
    fn add_replaces_existing() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        let mut updated = sample_cookie("session");
        updated.value = "new_val".into();
        jar.add(updated);
        assert_eq!(jar.cookies.len(), 1);
        assert_eq!(jar.get("session").unwrap().value, "new_val");
    }

    #[test]
    fn remove_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        jar.remove("session");
        assert!(jar.get("session").is_none());
    }

    #[test]
    fn clear_all() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("a"));
        jar.add(sample_cookie("b"));
        jar.clear();
        assert!(jar.cookies.is_empty());
    }
}
```

- [ ] **Step 2: Implement HttpExecutor trait**

`crates/rocket-http/src/executor.rs`:
```rust
use async_trait::async_trait;
use rocket_shared::error::DomainResult;

use crate::request::HttpRequest;
use crate::response::HttpResponse;

/// Trait for executing HTTP requests.
/// Implemented by ReqwestExecutor in rocket-infra.
#[async_trait]
pub trait HttpExecutor: Send + Sync {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn HttpExecutor>) {}
    }
}
```

- [ ] **Step 3: Implement CookieRepository trait**

`crates/rocket-http/src/cookie_repository.rs`:
```rust
use rocket_shared::error::DomainResult;
use crate::cookie::CookieJar;

pub trait CookieRepository: Send + Sync {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>>;
    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>>;
    fn save(&self, jar: &CookieJar) -> DomainResult<()>;
    fn clear(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn CookieRepository>) {}
    }
}
```

- [ ] **Step 4: Wire up lib.rs**

`crates/rocket-http/src/lib.rs`:
```rust
pub mod cookie;
pub mod cookie_repository;
pub mod executor;
pub mod request;
pub mod response;

pub use cookie::{Cookie, CookieJar};
pub use cookie_repository::CookieRepository;
pub use executor::HttpExecutor;
pub use request::{HttpRequest, RequestOptions};
pub use response::HttpResponse;
```

- [ ] **Step 5: Run all HTTP crate tests**

```bash
cargo test -p rocket-http
```
Expected: PASS — all tests (request 1 + response 2 + cookie 4 + executor 1 + cookie_repo 1 = 9).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-http/src/
git commit -m "feat(http): CookieJar aggregate + HttpExecutor + CookieRepository traits"
```

---

## Chunk 3: History bounded context

### Task 6: HistoryEntry aggregate + Template aggregate

**Files:**
- Create: `crates/rocket-history/src/entry.rs`
- Create: `crates/rocket-history/src/template.rs`
- Create: `crates/rocket-history/src/history_repository.rs`
- Create: `crates/rocket-history/src/template_repository.rs`

- [ ] **Step 1: Implement HistoryEntry**

`crates/rocket-history/src/entry.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub response_size: usize,
    pub timestamp: DateTime<Utc>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
}

impl HistoryEntry {
    pub fn new(
        method: impl Into<String>,
        url: impl Into<String>,
        status: u16,
        duration_ms: u64,
        response_size: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: method.into(),
            url: url.into(),
            status,
            duration_ms,
            response_size,
            timestamp: Utc::now(),
            collection: None,
            request_name: None,
        }
    }

    pub fn with_collection(mut self, collection: impl Into<String>, request_name: impl Into<String>) -> Self {
        self.collection = Some(collection.into());
        self.request_name = Some(request_name.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entry_has_id_and_timestamp() {
        let entry = HistoryEntry::new("GET", "https://api.example.com", 200, 150, 1024);
        assert!(!entry.id.is_empty());
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.status, 200);
        assert!(entry.collection.is_none());
    }

    #[test]
    fn entry_with_collection() {
        let entry = HistoryEntry::new("POST", "/api", 201, 50, 128)
            .with_collection("my-api", "Create User");
        assert_eq!(entry.collection, Some("my-api".into()));
        assert_eq!(entry.request_name, Some("Create User".into()));
    }
}
```

- [ ] **Step 2: Implement Template**

`crates/rocket-history/src/template.rs`:
```rust
use rocket_shared::types::{Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
}

impl Template {
    pub fn new(name: impl Into<String>, method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_template() {
        let t = Template::new("JSON POST", HttpMethod::Post, "https://api.example.com");
        assert_eq!(t.name, "JSON POST");
        assert_eq!(t.method, HttpMethod::Post);
    }
}
```

- [ ] **Step 3: Implement repository traits**

`crates/rocket-history/src/history_repository.rs`:
```rust
use rocket_shared::error::DomainResult;
use crate::entry::HistoryEntry;

pub trait HistoryRepository: Send + Sync {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>>;
    fn get(&self, id: &str) -> DomainResult<HistoryEntry>;
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()>;
    fn clear(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn HistoryRepository>) {}
    }
}
```

`crates/rocket-history/src/template_repository.rs`:
```rust
use rocket_shared::error::DomainResult;
use crate::template::Template;

pub trait TemplateRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<Template>>;
    fn get(&self, name: &str) -> DomainResult<Template>;
    fn save(&self, template: &Template) -> DomainResult<()>;
    fn delete(&self, name: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn TemplateRepository>) {}
    }
}
```

- [ ] **Step 4: Wire up lib.rs**

`crates/rocket-history/src/lib.rs`:
```rust
pub mod entry;
pub mod history_repository;
pub mod template;
pub mod template_repository;

pub use entry::HistoryEntry;
pub use history_repository::HistoryRepository;
pub use template::Template;
pub use template_repository::TemplateRepository;
```

- [ ] **Step 5: Run all history tests**

```bash
cargo test -p rocket-history
```
Expected: PASS — 5 tests (2 entry + 1 template + 2 trait).

- [ ] **Step 6: Run full workspace tests**

```bash
cargo test --workspace
cargo clippy --workspace
```
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-history/src/
git commit -m "feat(history): HistoryEntry + Template aggregates + repository traits"
```

---

## Milestone Checklist — Plan 3

- [ ] `rocket-environment`: Environment aggregate, Variable VO, resolver, EnvironmentRepository trait (13 tests)
- [ ] `rocket-http`: HttpRequest/HttpResponse VOs, CookieJar aggregate, HttpExecutor trait, CookieRepository trait (9 tests)
- [ ] `rocket-history`: HistoryEntry, Template aggregates, HistoryRepository, TemplateRepository traits (5 tests)
- [ ] All 3 crates compile independently
- [ ] Full workspace: `cargo test --workspace` — all pass
- [ ] Full workspace: `cargo clippy --workspace` — no warnings
