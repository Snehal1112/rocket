# Opt-In Host Guard for Script-Driven Request Mutations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give security-conscious teams an opt-in, per-workspace setting that blocks a `BeforeRequest` script's `req.setUrl()` mutation from redirecting a request to an internal/loopback/metadata-endpoint host, without ever restricting a user's own manually-typed URL.

**Architecture:** A new `RequestGuardPolicy` struct nests inside `rocket-workspace`'s existing `WorkspaceConfig` (persisted in `workspace.yml`, mirroring how `WorkspaceEnvironmentsConfig` already nests there). A new pure function `is_blocked_host` lives in a new `rocket-app` module (`request_guard.rs`) since it is host-range decision logic, not I/O, and `rocket-app` is where business-rule decisions belong per the DDD boundary rule. `RequestExecutionService::check_request_guard` is called from `execute()` at the exact point a `BeforeRequest` script's `request_mutations.url` is applied to the outgoing `HttpRequest` (today at `execution_service.rs:599-602`), comparing the *resolved host* before and after the mutation — never the user's originally-typed URL. The policy travels from `workspace.yml` to the executor via a new `request_guard_policy` field on `ExecuteRequestInput`, populated on the frontend from the active workspace's config immediately before `executeRequest()` is invoked. The settings UI is a new toolbar popover (`WorkspaceSecurityPopover.tsx`) with two shadcn `Switch` toggles, modeled on the existing `SandboxPopover.tsx` toolbar-popover pattern and the existing `Switch` usage in `AssertionsTab.tsx`.

**Tech Stack:** Rust (Cargo workspace: `rocket-workspace`, `rocket-app`, `src-tauri`), `std::net::{Ipv4Addr, Ipv6Addr}` for IP-range checks, `url` crate for URL/host parsing, React + TypeScript, `@tanstack/react-query`, shadcn/ui `Switch`/`Popover`, Zustand.

**Spec:** [docs/superpowers/specs/2026-09-16-request-mutation-host-guard-spec.md](../specs/2026-09-16-request-mutation-host-guard-spec.md)

## Global Constraints

- Default behavior is fully permissive (both policy flags `false`) — this must never change existing script/redirect behavior for a workspace that hasn't opted in.
- The guard only ever inspects a `BeforeRequest` script's `request_mutations.url` — the user's own manually-typed URL is never checked, regardless of policy (spec §3.1, §4).
- The check operates on the *resolved host*, not raw URL strings — a script rewriting only the path/query of the same host must never be blocked (spec §4, non-goal 3).
- `is_blocked_host` must check, at minimum: `127.0.0.0/8`, `::1`, `169.254.0.0/16` (including `169.254.169.254`), and the literal string `localhost`. When `also_block_private_ranges` is true, additionally `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`.
- No new crate dependency for IP-range checks — `ipnet` is only a *transitive* dependency (via `hyper-util`/`reqwest`) in `Cargo.lock`, not declared in any workspace `Cargo.toml`, so use `std::net::Ipv4Addr`/`Ipv6Addr` range checks per the spec's stated preference.
- `url = "2"` is already a direct dependency of `rocket-infra` (not workspace-managed); add the same direct dependency to `rocket-app`'s `Cargo.toml` rather than introducing a new version or promoting it to `[workspace.dependencies]`.
- Serde: `camelCase` rename on `RequestGuardPolicy` and the `WorkspaceConfig.requestGuardPolicy` field follows the established precedent already set by `WorkspaceConfig`/`WorkspaceEnvironmentsConfig` themselves (both persistence structs that already use `camelCase` because `workspace.yml` round-trips 1:1 to the frontend `WorkspaceConfig` TS interface via `get_workspace_config`) — this is a deliberate, precedented exception to the general "no camelCase on persistence structs" rule, not a violation of it.
- Rust: never `unwrap()` in production paths.
- Backward compatibility: any existing `workspace.yml` without a `requestGuardPolicy` key must deserialize with the policy defaulting to fully permissive (`#[serde(default)]`).

---

## Verified facts (read at plan-writing time, not assumed from the spec)

- `WorkspaceConfig` lives in `crates/rocket-workspace/src/config.rs:39-50`. It has no existing generic "settings" sub-struct — `environments: WorkspaceEnvironmentsConfig` is the closest sibling pattern (a small nested settings struct with its own `#[serde(default)]` field). `RequestGuardPolicy` follows that exact pattern rather than the spec's suggested `rocket_shared::types` location — `rocket-shared` houses generic `DomainError`/events/HTTP primitives, not workspace-scoped settings, and `rocket-workspace` already depends on `rocket-shared` (error types only), so nothing here creates a new dependency edge.
- The exact application site of a `BeforeRequest` script's URL mutation is `crates/rocket-app/src/execution_service.rs:599-602`, inside `if let Some(ref mutations) = result.request_mutations { if let Some(ref url) = mutations.url { http_request.url = url.clone(); ... } }` — confirmed, matches the spec's estimate exactly.
- `DomainError::InvalidInput(String)` exists at `crates/rocket-shared/src/error.rs:10`, confirmed.
- `ipnet` appears in `Cargo.lock` only as a transitive dependency of `hyper-util` (itself pulled in by `reqwest`) — it is not declared in any crate's `Cargo.toml` or in `[workspace.dependencies]`. `url = "2"` **is** already a direct dependency, but only of `rocket-infra`, not `rocket-app` (where the check needs to live). `crates/rocket-infra/src/reqwest_executor.rs` does not itself do any host/IP filtering today (confirmed by spec's own audit).
- No dedicated "workspace settings" React component exists anywhere in `src/components/` today. `getWorkspaceConfig` is declared in `src/lib/tauri-api.ts:983-984` but is not called from any component. The closest real analogs are: (1) `src/components/layout/SandboxPopover.tsx`, a toolbar `Popover` for an opt-in security-relevant mode toggle, mounted in `src/components/layout/WorkspaceToolbar.tsx`; (2) `Switch` usage in `src/components/request/AssertionsTab.tsx:149-153` and `src/components/request/VarsTab.tsx:144-148`; (3) the `multiWorkspaceMode` react-query mutation pattern in `src/lib/queries/workspace-queries.ts` (`useMultiWorkspaceMode`/`setMultiWorkspaceMode`) and its wiring in `src/components/layout/CollectionsSidebar.tsx:414-433`. This plan builds a new `WorkspaceSecurityPopover.tsx` following pattern (1) for placement/shell and pattern (2) for the toggle rows, wired through pattern (3)'s query/mutation style — there is no existing workspace-settings surface to "add fields into," contrary to the spec's assumption, so a new (small, consistent) one is created.

---

## Task 1: `RequestGuardPolicy` struct in `rocket-workspace`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-workspace/src/config.rs`
- Test: `crates/rocket-workspace/src/config.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `rocket_workspace::config::RequestGuardPolicy` (re-exported as `rocket_workspace::RequestGuardPolicy` — verify/add the re-export in `crates/rocket-workspace/src/lib.rs` if `config::*` isn't already re-exported at crate root; `WorkspaceConfig` already is, so mirror however it's exposed).
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
  #[serde(rename_all = "camelCase")]
  pub struct RequestGuardPolicy {
      pub block_script_redirects_to_internal_hosts: bool,
      pub also_block_private_ranges: bool,
  }
  ```

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-workspace/src/config.rs`:

```rust
    #[test]
    fn request_guard_policy_default_is_fully_permissive() {
        let policy = RequestGuardPolicy::default();
        assert!(!policy.block_script_redirects_to_internal_hosts);
        assert!(!policy.also_block_private_ranges);
    }

    #[test]
    fn request_guard_policy_serde_roundtrip() {
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let yaml = serde_yaml::to_string(&policy).unwrap();
        assert!(yaml.contains("blockScriptRedirectsToInternalHosts: true"));
        assert!(yaml.contains("alsoBlockPrivateRanges: true"));
        let back: RequestGuardPolicy = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn request_guard_policy_deserializes_from_empty_yaml_as_permissive() {
        // Backward compatibility: a workspace.yml predating this field must not
        // fail to load, and must not implicitly enable the guard.
        let policy: RequestGuardPolicy = serde_yaml::from_str("{}").unwrap();
        assert_eq!(policy, RequestGuardPolicy::default());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-workspace request_guard_policy`
Expected: FAIL with `cannot find type \`RequestGuardPolicy\` in this scope`.

- [ ] **Step 3: Add the struct**

Insert immediately after the `WorkspaceEnvironmentsConfig` struct definition (after line 32) in `crates/rocket-workspace/src/config.rs`:

```rust
/// Opt-in security policy for BeforeRequest script URL mutations. When both
/// flags are false (the default), behavior is unchanged from today: a script
/// may redirect a request to any host, exactly like a user typing the URL
/// manually — see docs/superpowers/specs/2026-09-16-request-mutation-host-guard-spec.md.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestGuardPolicy {
    /// When true, a BeforeRequest script's `req.setUrl` mutation is checked
    /// against the blocked ranges below before the request is dispatched.
    #[serde(default)]
    pub block_script_redirects_to_internal_hosts: bool,
    /// Additionally block RFC1918 private ranges (10/8, 172.16/12, 192.168/16),
    /// not just loopback/link-local. Off by default even when the guard itself
    /// is enabled, since many legitimate internal APIs live on private ranges.
    #[serde(default)]
    pub also_block_private_ranges: bool,
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-workspace request_guard_policy`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-workspace/src/config.rs
git commit -m "feat: add RequestGuardPolicy struct to rocket-workspace"
```

---

## Task 2: Nest `RequestGuardPolicy` into `WorkspaceConfig`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-workspace/src/config.rs`

**Interfaces:**
- Consumes: `RequestGuardPolicy` from Task 1.
- Produces: `WorkspaceConfig.request_guard_policy: RequestGuardPolicy` field, and `WorkspaceConfig::new` initializes it to `RequestGuardPolicy::default()`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn workspace_config_new_has_permissive_request_guard_policy() {
        let cfg = WorkspaceConfig::new("Test");
        assert_eq!(cfg.request_guard_policy, RequestGuardPolicy::default());
    }

    #[test]
    fn workspace_config_request_guard_policy_serde_roundtrip() {
        let mut cfg = WorkspaceConfig::new("My Project");
        cfg.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: WorkspaceConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn workspace_config_deserialize_minimal_yaml_defaults_request_guard_policy() {
        // A workspace.yml written before this feature existed has no
        // requestGuardPolicy key at all — it must still load, permissively.
        let yaml = "name: Minimal\n";
        let cfg: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.request_guard_policy, RequestGuardPolicy::default());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-workspace workspace_config_request_guard_policy workspace_config_new_has_permissive workspace_config_deserialize_minimal_yaml_defaults`
Expected: FAIL with `no field \`request_guard_policy\` on type \`WorkspaceConfig\``.

- [ ] **Step 3: Add the field**

In `crates/rocket-workspace/src/config.rs`, modify `WorkspaceConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<CollectionReference>,
    #[serde(default)]
    pub environments: WorkspaceEnvironmentsConfig,
    /// Name of the selected global environment (workspace/environments/<n>.yml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_environment: Option<String>,
    /// Opt-in security policy for BeforeRequest script URL mutations. Defaults
    /// to fully permissive (today's behavior) for every existing workspace.
    #[serde(default)]
    pub request_guard_policy: RequestGuardPolicy,
}
```

And update the constructor:

```rust
impl WorkspaceConfig {
    /// Create a new workspace config with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            collections: Vec::new(),
            environments: WorkspaceEnvironmentsConfig::default(),
            global_environment: None,
            request_guard_policy: RequestGuardPolicy::default(),
        }
    }
    // ... existing methods unchanged
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-workspace`
Expected: PASS (all tests in the crate, including the pre-existing `workspace_config_full_serde_roundtrip` and `workspace_config_deserialize_minimal_yaml`, which must still pass unmodified).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-workspace/src/config.rs
git commit -m "feat: nest RequestGuardPolicy into WorkspaceConfig"
```

---

## Task 3: `is_blocked_host` pure function in `rocket-app`

**Files:**
- Create: `crates/rocket-app/src/request_guard.rs`
- Modify: `crates/rocket-app/src/lib.rs` (register the module)
- Modify: `crates/rocket-app/Cargo.toml` (add `url` dependency)
- Test: `crates/rocket-app/src/request_guard.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn is_blocked_host(host: &str, also_block_private: bool) -> bool` in `rocket_app::request_guard` (re-exported as `rocket_app::is_blocked_host` isn't necessary — `RequestExecutionService` in the same crate can use `crate::request_guard::is_blocked_host`).

- [ ] **Step 1: Add the `url` dependency**

In `crates/rocket-app/Cargo.toml`, add to `[dependencies]` (after `reqwest.workspace = true`):

```toml
url = "2"
```

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-app/src/request_guard.rs`:

```rust
//! Pure host-blocking decision logic for the opt-in request-mutation host
//! guard (docs/superpowers/specs/2026-09-16-request-mutation-host-guard-spec.md).
//! No I/O — consumed by `RequestExecutionService::check_request_guard`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Returns true if `host` should be blocked under the given policy.
///
/// Checks, at minimum: 127.0.0.0/8, ::1, 169.254.0.0/16 (including the
/// 169.254.169.254 cloud metadata endpoint), and the literal string
/// "localhost". When `also_block_private` is true, additionally checks
/// 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
pub fn is_blocked_host(host: &str, also_block_private: bool) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => is_blocked_ipv4(v4, also_block_private),
        Ok(IpAddr::V6(v6)) => is_blocked_ipv6(v6),
        Err(_) => false,
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr, also_block_private: bool) -> bool {
    if ip.is_loopback() || ip.is_link_local() {
        return true;
    }
    also_block_private && ip.is_private()
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ipv4_loopback() {
        assert!(is_blocked_host("127.0.0.1", false));
    }

    #[test]
    fn blocks_ipv6_loopback() {
        assert!(is_blocked_host("::1", false));
    }

    #[test]
    fn blocks_link_local_metadata_endpoint() {
        // 169.254.169.254 — the standard cloud metadata endpoint. Highest-value
        // entry in the blocklist per the spec.
        assert!(is_blocked_host("169.254.169.254", false));
    }

    #[test]
    fn blocks_other_link_local_addresses() {
        assert!(is_blocked_host("169.254.1.1", false));
    }

    #[test]
    fn blocks_localhost_string_case_insensitively() {
        assert!(is_blocked_host("localhost", false));
        assert!(is_blocked_host("LOCALHOST", false));
    }

    #[test]
    fn private_ranges_not_blocked_without_flag() {
        assert!(!is_blocked_host("10.0.0.5", false));
        assert!(!is_blocked_host("172.16.5.5", false));
        assert!(!is_blocked_host("192.168.1.1", false));
    }

    #[test]
    fn private_ranges_blocked_with_flag() {
        assert!(is_blocked_host("10.0.0.5", true));
        assert!(is_blocked_host("172.16.5.5", true));
        assert!(is_blocked_host("192.168.1.1", true));
    }

    #[test]
    fn loopback_and_link_local_blocked_even_without_private_flag() {
        // also_block_private_ranges only affects RFC1918 — loopback/link-local
        // are always blocked once the guard itself is on.
        assert!(is_blocked_host("127.0.0.1", true));
        assert!(is_blocked_host("169.254.169.254", true));
    }

    #[test]
    fn public_ip_not_blocked() {
        assert!(!is_blocked_host("8.8.8.8", true));
    }

    #[test]
    fn ordinary_hostname_not_blocked() {
        assert!(!is_blocked_host("api.example.com", true));
        assert!(!is_blocked_host("internal-service.corp", true));
    }
}
```

- [ ] **Step 3: Register the module**

In `crates/rocket-app/src/lib.rs`, add `pub mod request_guard;` alphabetically among the existing `pub mod` lines (after `pub mod oauth2_service;`, before `pub mod security_audit_service;`). No `pub use` re-export is needed — it stays an internal-to-crate helper consumed by `execution_service.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app request_guard::`
Expected: PASS (10 tests). (They cannot have "failed first" in the usual red-green sense since the module didn't exist — verify instead that `cargo check -p rocket-app` failed before Step 3/2 combined, i.e. run `cargo test -p rocket-app request_guard::` once before writing the module to confirm "no such module" as the red step, matching this task's actual order: write file with tests included, so the red step is implicit compile failure prior to this task starting.)

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/request_guard.rs crates/rocket-app/src/lib.rs crates/rocket-app/Cargo.toml
git commit -m "feat: add is_blocked_host host-range guard logic"
```

---

## Task 4: `RequestExecutionService::check_request_guard`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`
- Test: `crates/rocket-app/src/execution_service.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `rocket_workspace::RequestGuardPolicy` (Task 1/2), `crate::request_guard::is_blocked_host` (Task 3).
- Produces: `RequestExecutionService::check_request_guard(&self, original_url: &str, mutated_url: &str, policy: &RequestGuardPolicy) -> DomainResult<()>` (private method — used internally by Task 5, and directly by this task's own unit tests since it's in the same module).

- [ ] **Step 1: Write the failing tests**

Add a new test module section inside the existing `#[cfg(test)] mod tests` block in `crates/rocket-app/src/execution_service.rs` (near the other `before_request_script_*` tests, e.g. right after `before_request_script_string_body_respects_explicit_content_type`):

```rust
    #[test]
    fn check_request_guard_noop_when_policy_disabled() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy::default();
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://169.254.169.254/latest/meta-data/",
            &policy,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_blocks_metadata_endpoint_when_enabled() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://169.254.169.254/latest/meta-data/",
            &policy,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("169.254.169.254"), "error should name the blocked host: {msg}");
    }

    #[test]
    fn check_request_guard_allows_private_range_when_flag_off() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://192.168.1.1/",
            &policy,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_blocks_private_range_when_both_flags_on() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result = svc.check_request_guard(
            "https://example.com/",
            "http://192.168.1.1/",
            &policy,
        );
        assert!(result.is_err());
    }

    #[test]
    fn check_request_guard_ignores_same_host_path_only_rewrite() {
        use rocket_workspace::RequestGuardPolicy;
        // A script that only rewrites the path/query of a host the user already
        // declared themselves (even an internal one) must never be blocked —
        // only a host *change* introduced by the script is in scope.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let result = svc.check_request_guard(
            "http://192.168.1.1/foo",
            "http://192.168.1.1/bar",
            &policy,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn check_request_guard_errors_on_unparseable_mutated_url() {
        use rocket_workspace::RequestGuardPolicy;
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let result = svc.check_request_guard("https://example.com/", "not a url", &policy);
        assert!(result.is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app check_request_guard`
Expected: FAIL with `no method named \`check_request_guard\` found`.

- [ ] **Step 3: Implement `check_request_guard`**

Add this private method to `impl RequestExecutionService` in `crates/rocket-app/src/execution_service.rs`, immediately before `pub async fn execute`:

```rust
    /// Validates a BeforeRequest script's URL mutation against the workspace's
    /// opt-in `RequestGuardPolicy`. Only ever inspects `mutated_url` — the
    /// user's own manually-typed URL never reaches this method (see call site
    /// in `execute()`, which only calls this when a script actually set a new
    /// URL). Compares resolved hosts, not raw URL strings: a script that only
    /// rewrites the path/query of the same host the user already declared is
    /// never blocked, regardless of whether that host happens to be internal.
    fn check_request_guard(
        &self,
        original_url: &str,
        mutated_url: &str,
        policy: &rocket_workspace::RequestGuardPolicy,
    ) -> DomainResult<()> {
        if !policy.block_script_redirects_to_internal_hosts {
            return Ok(());
        }

        let mutated = url::Url::parse(mutated_url).map_err(|e| {
            rocket_shared::error::DomainError::InvalidInput(format!(
                "script produced an invalid URL: {e}"
            ))
        })?;
        let Some(mutated_host) = mutated.host_str() else {
            // No host component (e.g. a relative/opaque URL) — nothing to check.
            return Ok(());
        };

        // If the script only changed the path/query of the host the user
        // already declared, this is not a redirect in the sense the guard
        // cares about.
        if let Ok(original) = url::Url::parse(original_url) {
            if original.host_str() == Some(mutated_host) {
                return Ok(());
            }
        }

        if crate::request_guard::is_blocked_host(mutated_host, policy.also_block_private_ranges) {
            return Err(rocket_shared::error::DomainError::InvalidInput(format!(
                "blocked: script redirected request to internal host '{mutated_host}' \
                 (workspace policy blocks script-driven redirects to internal hosts)"
            )));
        }
        Ok(())
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app check_request_guard`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat: add RequestExecutionService::check_request_guard"
```

---

## Task 5: Wire the guard into `execute()` via `ExecuteRequestInput`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`
- Test: `crates/rocket-app/src/execution_service.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `RequestExecutionService::check_request_guard` (Task 4), `rocket_workspace::RequestGuardPolicy` (Task 1/2).
- Produces: `ExecuteRequestInput.request_guard_policy: RequestGuardPolicy` (new field, `#[serde(default)]`). This is what Task 7 mirrors on the TypeScript side.

- [ ] **Step 1: Write the failing tests**

Add to `#[cfg(test)] mod tests` in `crates/rocket-app/src/execution_service.rs` (near `before_request_script_invalid_method_is_surfaced_as_script_error`):

```rust
    #[tokio::test]
    async fn ac1_policy_disabled_sends_redirect_unmodified() {
        use rocket_scripting::{RequestMutations, ScriptResult};

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://169.254.169.254/latest/meta-data/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        // request_guard_policy left at its Default — fully permissive.
        let output = svc.execute(input).await.expect("execute should succeed — policy is off");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac2_policy_enabled_blocks_redirect_to_metadata_endpoint() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://169.254.169.254/latest/meta-data/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let err = svc.execute(input).await.expect_err("must be blocked");
        assert!(err.to_string().contains("169.254.169.254"));
    }

    #[tokio::test]
    async fn ac3_policy_enabled_without_private_flag_allows_private_redirect() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://192.168.1.1/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let output = svc.execute(input).await.expect("private ranges must be allowed by default");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac4_both_flags_enabled_blocks_private_redirect() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_workspace::RequestGuardPolicy;

        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                url: Some("http://192.168.1.1/".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let err = svc.execute(input).await.expect_err("must be blocked with both flags on");
        assert!(err.to_string().contains("192.168.1.1"));
    }

    #[tokio::test]
    async fn ac5_manual_loopback_url_never_blocked_regardless_of_policy() {
        use rocket_workspace::RequestGuardPolicy;

        // No pre_request_script at all — this is exactly what a user manually
        // typing http://localhost:8080/ into the URL bar looks like to the
        // service. The guard must never inspect input.url itself.
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        let mut input = sample_input("http://localhost:8080/", None);
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let output = svc.execute(input).await.expect("manual URLs are never checked");
        assert_eq!(output.response.status, 200);
    }

    #[tokio::test]
    async fn ac6_script_without_seturl_is_unaffected_by_policy() {
        use rocket_scripting::{RequestMutations, ScriptResult};
        use rocket_shared::types::HttpMethod;
        use rocket_workspace::RequestGuardPolicy;

        // Script only calls req.setMethod — no URL mutation at all.
        let result = ScriptResult {
            request_mutations: Some(RequestMutations {
                method: Some("POST".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(MockBeforeRequestEngine::returning(result)));

        let mut input = sample_input("https://example.com", None);
        input.method = HttpMethod::Get;
        input.pre_request_script = Some("// pre".into());
        input.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let output = svc.execute(input).await.expect("no URL mutation means nothing to check");
        assert_eq!(output.response.status, 200);
    }
```

Also update the `sample_input` test helper (around line 1184-1206) to initialize the new field:

```rust
    fn sample_input(url: &str, env_name: Option<&str>) -> ExecuteRequestInput {
        ExecuteRequestInput {
            method: HttpMethod::Get,
            url: url.to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: rocket_shared::types::Auth::None,
            options: RequestOptions::default(),
            environment_name: env_name.map(str::to_string),
            collection: None,
            request_name: None,
            pre_request_script: None,
            post_response_script: None,
            tests_script: None,
            request_path: None,
            global_env_name: None,
            assertions: vec![],
            tags: vec![],
            path_params: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app ac1_ ac2_ ac3_ ac4_ ac5_ ac6_`
Expected: FAIL — compile error, `ExecuteRequestInput` has no field `request_guard_policy` and struct literal is missing it in `sample_input`.

- [ ] **Step 3: Add the field and call site**

Add the field to `ExecuteRequestInput` in `crates/rocket-app/src/execution_service.rs` (after the `actions` field, which is the last field before the struct closes — check the current end of the struct and append there):

```rust
    /// Opt-in per-workspace policy: when a BeforeRequest script redirects the
    /// request via req.setUrl(), validate the new host against a blocklist of
    /// internal/loopback ranges before dispatch. Defaults to fully permissive.
    #[serde(default)]
    pub request_guard_policy: rocket_workspace::RequestGuardPolicy,
```

Then wire the check into the mutation-application block at (currently) `execution_service.rs:599-602`:

```rust
                // Apply request mutations.
                if let Some(ref mutations) = result.request_mutations {
                    if let Some(ref url) = mutations.url {
                        let original_url = http_request.url.clone();
                        self.check_request_guard(&original_url, url, &input.request_guard_policy)?;
                        http_request.url = url.clone();
                    }
```

(Everything else in that block — method, headers, timeout, body, max_redirects — is unchanged.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app`
Expected: PASS — the full `rocket-app` suite, including all pre-existing tests (the `sample_input` helper change must not break any test that calls it) and the six new `ac*_` tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat: enforce RequestGuardPolicy on BeforeRequest URL mutations"
```

---

## Task 6: `WorkspaceService` + Tauri command to update the policy

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`
- Modify: `src-tauri/src/commands/workspaces.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `rocket_workspace::RequestGuardPolicy` (Task 1/2). Reads reuse the existing `get_workspace_config`/`WorkspaceService::get_workspace_config` (already returns the whole `WorkspaceConfig`, which now includes `request_guard_policy` — no new read command needed).
- Produces: `WorkspaceService::update_request_guard_policy(&self, id: &str, policy: RequestGuardPolicy) -> DomainResult<()>`; Tauri command `update_request_guard_policy(workspace_id: String, policy: RequestGuardPolicy)`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/rocket-app/src/workspace_service.rs`, immediately after `get_workspace_config_returns_config` (after line 585). This mirrors that test's exact setup: the module-level `make_service(&tmp) -> WorkspaceService` helper (defined at line 389) and `svc.create(name, path)` for a fresh workspace, same as `get_workspace_config_returns_config` and `update_description_sets_value` already do:

```rust
    #[test]
    fn update_request_guard_policy_persists_to_workspace_yml() {
        use rocket_workspace::RequestGuardPolicy;

        let tmp = TempDir::new().expect("tempdir");
        let svc = make_service(&tmp);
        let ws = svc.create("Guarded", tmp.path().join("guarded-ws")).expect("create should succeed");

        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        svc.update_request_guard_policy(&ws.id, policy.clone())
            .expect("update should succeed");

        let loaded = svc.get_workspace_config(&ws.id).expect("load should succeed");
        assert_eq!(loaded.request_guard_policy, policy);
    }

    #[test]
    fn update_request_guard_policy_nonexistent_workspace_fails() {
        use rocket_workspace::RequestGuardPolicy;

        let tmp = TempDir::new().expect("tempdir");
        let svc = make_service(&tmp);
        let result = svc.update_request_guard_policy("nope", RequestGuardPolicy::default());
        assert!(result.is_err());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-app update_request_guard_policy`
Expected: FAIL with `no method named \`update_request_guard_policy\` found`.

- [ ] **Step 3: Implement the service method**

Add to `impl WorkspaceService` in `crates/rocket-app/src/workspace_service.rs`, immediately after `get_workspace_config` (after line 184):

```rust
    /// Update the opt-in RequestGuardPolicy for a workspace. Unlike
    /// `update_description`, this is not cached in the registry — the policy
    /// only ever lives in workspace.yml, mirroring `set_multi_workspace_mode`'s
    /// simplicity (a plain settings toggle, no domain event published).
    pub fn update_request_guard_policy(
        &self,
        workspace_id: &str,
        policy: rocket_workspace::RequestGuardPolicy,
    ) -> DomainResult<()> {
        let registry = self.repo.load()?;
        let workspace = registry
            .find_by_id(workspace_id)
            .ok_or_else(|| DomainError::NotFound(workspace_id.into()))?;
        let mut config = self.config_repo.load(&workspace.path)?;
        config.request_guard_policy = policy;
        self.config_repo.save(&workspace.path, &config)
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rocket-app update_request_guard_policy`
Expected: PASS (2 tests).

- [ ] **Step 5: Add the Tauri command**

In `src-tauri/src/commands/workspaces.rs`, add the import and command, mirroring `set_multi_workspace_mode`'s exact shape:

```rust
use rocket_workspace::{RequestGuardPolicy, Workspace, WorkspaceConfig};
```

(replacing the existing `use rocket_workspace::{Workspace, WorkspaceConfig};` line at the top of the file)

```rust
#[tauri::command]
pub fn update_request_guard_policy(
    workspace_id: String,
    policy: RequestGuardPolicy,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.update_request_guard_policy(&workspace_id, policy)
}
```

Add this immediately after the existing `set_multi_workspace_mode` command (after line 130).

- [ ] **Step 6: Register the command**

In `src-tauri/src/lib.rs`, add `commands::workspaces::update_request_guard_policy,` to the `invoke_handler` list, immediately after the existing `commands::workspaces::set_multi_workspace_mode,` line (line 377).

- [ ] **Step 7: Verify the crate builds**

Run: `cargo check -p rocket-app && cargo check -p src-tauri`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs src-tauri/src/commands/workspaces.rs src-tauri/src/lib.rs
git commit -m "feat: add update_request_guard_policy service method and Tauri command"
```

---

## Task 7: TypeScript DTOs and `tauri-api.ts` binding

**Files:**
- Modify: `src/lib/tauri-api.ts`

**Interfaces:**
- Produces: `RequestGuardPolicy` TS interface, `WorkspaceConfig.requestGuardPolicy`, `ExecuteRequestInput.requestGuardPolicy?`, `updateRequestGuardPolicy(workspaceId, policy)` API function.

- [ ] **Step 1: Add the `RequestGuardPolicy` interface and extend `WorkspaceConfig`**

In `src/lib/tauri-api.ts`, immediately before the `WorkspaceConfig` interface (before line 435):

```typescript
export interface RequestGuardPolicy {
  blockScriptRedirectsToInternalHosts: boolean;
  alsoBlockPrivateRanges: boolean;
}
```

Then extend `WorkspaceConfig` (lines 435-440):

```typescript
export interface WorkspaceConfig {
  name: string;
  description?: string | null;
  collections: CollectionReference[];
  environments: WorkspaceEnvironmentsConfig;
  requestGuardPolicy: RequestGuardPolicy;
}
```

- [ ] **Step 2: Extend `ExecuteRequestInput`**

Add to the `ExecuteRequestInput` interface (after the `actions?: ActionEntry[];` line, around line 276):

```typescript
  /** Opt-in per-workspace policy checked against BeforeRequest req.setUrl() redirects. */
  requestGuardPolicy?: RequestGuardPolicy;
```

- [ ] **Step 3: Add the `updateRequestGuardPolicy` API function**

Immediately after the existing `getWorkspaceConfig` export (after line 984):

```typescript
export const updateRequestGuardPolicy = (workspaceId: string, policy: RequestGuardPolicy) =>
  invoke<void>('update_request_guard_policy', { workspaceId, policy });
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors (the new fields are additive; `requestGuardPolicy?` on `ExecuteRequestInput` is optional so no existing call site breaks).

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat: add RequestGuardPolicy types and updateRequestGuardPolicy binding"
```

---

## Task 8: React Query hooks for the workspace's `RequestGuardPolicy`

**Files:**
- Modify: `src/lib/queries/workspace-queries.ts`

**Interfaces:**
- Consumes: `getWorkspaceConfig`, `updateRequestGuardPolicy` (Task 7).
- Produces: `workspaceKeys.config(id)`, `useWorkspaceConfig(id)`, `useUpdateRequestGuardPolicy()`.

- [ ] **Step 1: Add the query key, query hook, and mutation hook**

In `src/lib/queries/workspace-queries.ts`, update the import list to include the two new bindings:

```typescript
import {
  closeWorkspace,
  createWorkspace,
  deleteWorkspace,
  getActiveWorkspace,
  getMultiWorkspaceMode,
  getWorkspaceConfig,
  listWorkspaces,
  openWorkspaceFromDisk,
  pinWorkspace,
  renameWorkspace,
  type RequestGuardPolicy,
  setMultiWorkspaceMode,
  switchWorkspace,
  unpinWorkspace,
  updateRequestGuardPolicy,
  updateWorkspaceDescription,
} from '@/lib/tauri-api';
```

Extend `workspaceKeys`:

```typescript
export const workspaceKeys = {
  all: ['workspaces'] as const,
  active: ['workspaces', 'active'] as const,
  multiMode: ['workspaces', 'multiMode'] as const,
  config: (id: string) => ['workspaces', id, 'config'] as const,
};
```

Add the query hook, immediately after `useMultiWorkspaceMode`:

```typescript
export function useWorkspaceConfig(workspaceId: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.config(workspaceId ?? ''),
    queryFn: () => getWorkspaceConfig(workspaceId as string),
    enabled: Boolean(workspaceId),
  });
}
```

Add the mutation hook, alongside the other `use*Workspace` mutations (e.g. after `usePinWorkspace`):

```typescript
export function useUpdateRequestGuardPolicy(workspaceId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (policy: RequestGuardPolicy) =>
      updateRequestGuardPolicy(workspaceId as string, policy),
    onSuccess: () => {
      if (workspaceId) qc.invalidateQueries({ queryKey: workspaceKeys.config(workspaceId) });
    },
  });
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/queries/workspace-queries.ts
git commit -m "feat: add useWorkspaceConfig and useUpdateRequestGuardPolicy hooks"
```

---

## Task 9: Thread the policy into `sendRequest`

**Files:**
- Modify: `src/lib/execute-request.ts`

**Interfaces:**
- Consumes: `getWorkspaceConfig` (Task 7), `useWorkspaceStore` (existing — `activeWorkspaceId`).
- Produces: `executeRequest(...)` calls in `sendRequest` now include `requestGuardPolicy`.

- [ ] **Step 1: Import `getWorkspaceConfig` and `useWorkspaceStore`**

In `src/lib/execute-request.ts`, add to the existing `@/lib/tauri-api` import block (alongside `executeRequest`, `getCollectionSettings`, etc.):

```typescript
  getWorkspaceConfig,
```

Add a new top-level import for the workspace store:

```typescript
import { useWorkspaceStore } from '@/stores/workspace-store';
```

- [ ] **Step 2: Fetch the active workspace's policy before dispatch**

In `sendRequest`, immediately before the `try { const result = await executeRequest({ ... }` block (before line 457), add:

```typescript
  // Fetch the active workspace's opt-in RequestGuardPolicy. Failure here must
  // never block sending a request — fall back to the fully-permissive default,
  // matching today's behavior, rather than surfacing an unrelated error.
  const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  let requestGuardPolicy: { blockScriptRedirectsToInternalHosts: boolean; alsoBlockPrivateRanges: boolean } = {
    blockScriptRedirectsToInternalHosts: false,
    alsoBlockPrivateRanges: false,
  };
  if (activeWorkspaceId) {
    try {
      const config = await getWorkspaceConfig(activeWorkspaceId);
      requestGuardPolicy = config.requestGuardPolicy;
    } catch {
      // Non-critical — keep the permissive default.
    }
  }
```

Then add the field to the `executeRequest({...})` call (alongside `globalEnvName`, `requestName`, etc.):

```typescript
      globalEnvName,
      requestName,
      requestGuardPolicy,
```

(Insert `requestGuardPolicy,` on its own line within the existing object literal — match whatever trailing fields already exist at that call site rather than assuming exact line position, since other in-flight work in this file may have shifted them.)

- [ ] **Step 3: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/execute-request.ts
git commit -m "feat: thread active workspace's RequestGuardPolicy into sendRequest"
```

---

## Task 10: `WorkspaceSecurityPopover` settings UI

**Files:**
- Create: `src/components/layout/WorkspaceSecurityPopover.tsx`
- Modify: `src/components/layout/WorkspaceToolbar.tsx`

**Interfaces:**
- Consumes: `useWorkspaceConfig`, `useUpdateRequestGuardPolicy` (Task 8), `useWorkspaceStore` (existing).
- Produces: `WorkspaceSecurityPopover` component, mounted in `WorkspaceToolbar`.

- [ ] **Step 1: Write the component**

Create `src/components/layout/WorkspaceSecurityPopover.tsx`, following `SandboxPopover.tsx`'s toolbar-icon-button + `Popover` shell, and `AssertionsTab.tsx`'s `Switch` usage:

```tsx
import { ShieldAlert } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Switch } from '@/components/ui/switch';
import { useUpdateRequestGuardPolicy, useWorkspaceConfig } from '@/lib/queries/workspace-queries';
import { cn } from '@/lib/utils';
import { useWorkspaceStore } from '@/stores/workspace-store';

export function WorkspaceSecurityPopover() {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: config } = useWorkspaceConfig(activeWorkspaceId);
  const updatePolicy = useUpdateRequestGuardPolicy(activeWorkspaceId);

  const blockInternal = config?.requestGuardPolicy?.blockScriptRedirectsToInternalHosts ?? false;
  const blockPrivate = config?.requestGuardPolicy?.alsoBlockPrivateRanges ?? false;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7 hover:bg-toolbar-hover'
          title='Request Guard'
          aria-label='Request Guard'
        >
          <ShieldAlert
            className={cn(
              'h-4 w-4 transition-colors duration-200',
              blockInternal
                ? 'text-green-500 dark:text-green-400'
                : 'text-muted-foreground',
            )}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-72 p-0 overflow-hidden' align='end'>
        {/* Header */}
        <div className='flex items-center gap-2 px-4 py-2.5 border-b border-border/60'>
          <ShieldAlert className='h-3 w-3 shrink-0 text-muted-foreground' />
          <p className='text-[11px] font-semibold tracking-wider uppercase text-muted-foreground'>
            Request Guard
          </p>
        </div>

        {/* Toggles */}
        <div className='p-3 space-y-3'>
          <p className='text-xs text-muted-foreground'>
            Block a BeforeRequest script's <code>req.setUrl()</code> redirect from
            reaching internal hosts. Your own manually-typed URLs are never affected.
          </p>

          <div className='flex items-start justify-between gap-3'>
            <div className='flex-1 min-w-0'>
              <p className='text-xs font-medium'>Block internal redirects</p>
              <p className='text-[11px] text-muted-foreground'>
                Loopback, link-local, and the cloud metadata endpoint.
              </p>
            </div>
            <Switch
              checked={blockInternal}
              disabled={!activeWorkspaceId || updatePolicy.isPending}
              onCheckedChange={(checked) => {
                updatePolicy.mutate({
                  blockScriptRedirectsToInternalHosts: checked,
                  alsoBlockPrivateRanges: checked ? blockPrivate : false,
                });
              }}
              aria-label='Block script redirects to internal hosts'
            />
          </div>

          <div className='flex items-start justify-between gap-3'>
            <div className='flex-1 min-w-0'>
              <p className='text-xs font-medium'>Also block private ranges</p>
              <p className='text-[11px] text-muted-foreground'>
                10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
              </p>
            </div>
            <Switch
              checked={blockPrivate}
              disabled={!activeWorkspaceId || !blockInternal || updatePolicy.isPending}
              onCheckedChange={(checked) => {
                updatePolicy.mutate({
                  blockScriptRedirectsToInternalHosts: blockInternal,
                  alsoBlockPrivateRanges: checked,
                });
              }}
              aria-label='Also block private IP ranges'
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 2: Mount it in the toolbar**

In `src/components/layout/WorkspaceToolbar.tsx`, add the import and render it next to `SandboxPopover`:

```tsx
import { CollectionDropdown } from './CollectionDropdown';
import { EnvironmentSwitcher } from './EnvironmentSwitcher';
import { GitToolbarButton } from './GitToolbarButton';
import { SandboxPopover } from './SandboxPopover';
import { WorkspaceSecurityPopover } from './WorkspaceSecurityPopover';

export function WorkspaceToolbar() {
  return (
    <div className='h-9 border-b border-border bg-card px-3 flex items-center justify-between shrink-0'>
      {/* Left side */}
      <div className='flex items-center gap-2'>
        <CollectionDropdown />
      </div>

      {/* Right side */}
      <div className='flex items-center gap-1'>
        <GitToolbarButton />
        <SandboxPopover />
        <WorkspaceSecurityPopover />
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors. (`yarn check` is read-only Biome lint/format — if it reports formatting issues on the new file, run `yarn format` and re-check rather than hand-fixing whitespace.)

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/WorkspaceSecurityPopover.tsx src/components/layout/WorkspaceToolbar.tsx
git commit -m "feat: add WorkspaceSecurityPopover with request guard toggles"
```

---

## Task 11: Full verification pass

**Files:** none (verification only).

- [ ] **Step 1: Run the full Rust suite**

Run: `cargo test -p rocket-workspace && cargo test -p rocket-app`
Expected: PASS — every test added in Tasks 1-6, plus every pre-existing test in both crates (the `sample_input`/`WorkspaceConfig::new` changes must not have broken anything).

- [ ] **Step 2: Run a full workspace `cargo check`**

Run: `cargo check`
Expected: no errors across the whole workspace, including `src-tauri`.

- [ ] **Step 3: Run the frontend checks**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors (this is acceptance criterion 7 from the spec).

- [ ] **Step 4: Manually confirm the acceptance criteria mapping**

Cross-check against spec §6:
- AC1 → `ac1_policy_disabled_sends_redirect_unmodified` (Task 5)
- AC2 → `ac2_policy_enabled_blocks_redirect_to_metadata_endpoint` (Task 5)
- AC3 → `ac3_policy_enabled_without_private_flag_allows_private_redirect` (Task 5)
- AC4 → `ac4_both_flags_enabled_blocks_private_redirect` (Task 5)
- AC5 → `ac5_manual_loopback_url_never_blocked_regardless_of_policy` (Task 5)
- AC6 → `ac6_script_without_seturl_is_unaffected_by_policy` (Task 5)
- AC7 → this task's Steps 1-3

- [ ] **Step 5: Commit (only if any of the above required fixes)**

If Steps 1-3 required any code changes to pass, stage and commit those fixes individually with a `fix:` message before considering the plan complete. If everything passed as-is, there is nothing to commit for this task.
