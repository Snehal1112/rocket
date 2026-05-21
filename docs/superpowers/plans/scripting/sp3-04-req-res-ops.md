# SP3-04 — `rocket-infra`: `req.*` + `res.*` Ops

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all `req.*` ops (read + write, with phase guard) and all `res.*` ops (read-only, unavailable in `BeforeRequest`). Replace stubs from SP3-02 and register all ops in `build_extension()`.

**Architecture:** `req.set*` ops check `ScriptInputState.phase` — if not `BeforeRequest`, they throw a JS-level error by returning an error string and relying on the bootstrap to surface it. Writes accumulate in `ScriptOutputState.request_mutations`. `res.*` ops check that `ScriptInputState.response` is `Some(_)` — if not, throw.

**Tech Stack:** Rust, `deno_core::op2`

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §4 (req/res tables)

**Depends on:** SP3-02 merged.

---

## Task 1: `req.*` read ops

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/req.rs`

- [ ] **Step 1: Replace stub with full `req` read ops**

```rust
use deno_core::{op2, OpState};
use url::Url;
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use rocket_scripting::ScriptPhase;

fn guard_before_request(state: &OpState) -> Result<(), String> {
    let phase = &state.borrow::<ScriptInputState>().phase;
    if *phase != ScriptPhase::BeforeRequest {
        Err(format!("req mutations are not allowed in {} scripts", phase))
    } else {
        Ok(())
    }
}

#[op2]
#[string]
pub fn op_req_get_url(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().request.url.clone()
}

#[op2]
#[string]
pub fn op_req_get_host(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url).ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

#[op2]
#[string]
pub fn op_req_get_path(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url).map(|u| u.path().to_string()).unwrap_or_default()
}

#[op2]
#[string]
pub fn op_req_get_query_string(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url)
        .ok()
        .and_then(|u| u.query().map(String::from))
        .unwrap_or_default()
}

/// Returns JSON array of { name, value, type } objects for path params.
#[op2]
#[string]
pub fn op_req_get_path_params(state: &OpState) -> String {
    let req = &state.borrow::<ScriptInputState>().request;
    let params: Vec<serde_json::Value> = req.path_params.iter().map(|p| {
        serde_json::json!({ "name": p.name, "value": p.value, "type": "path" })
    }).collect();
    serde_json::to_string(&params).unwrap_or_else(|_| "[]".into())
}

#[op2]
#[string]
pub fn op_req_get_method(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().request.method.to_string()
}

#[op2]
#[string]
pub fn op_req_get_name(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().request.name.clone()
}

/// Returns JSON array of tag strings.
#[op2]
#[string]
pub fn op_req_get_tags(state: &OpState) -> String {
    let tags = &state.borrow::<ScriptInputState>().request.tags;
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

#[op2]
#[string]
pub fn op_req_get_auth_mode(state: &OpState) -> String {
    // Returns the auth type discriminator string, e.g. "bearer", "basic", "none".
    let auth = &state.borrow::<ScriptInputState>().request.auth;
    auth.type_name().to_string()
}

#[op2]
#[string]
pub fn op_req_get_header(state: &OpState, #[string] name: String) -> String {
    let req = &state.borrow::<ScriptInputState>().request;
    req.headers.iter()
        .find(|h| h.name.eq_ignore_ascii_case(&name))
        .map(|h| h.value.clone())
        .unwrap_or_default()
}

/// Returns JSON object of all headers as { name: value }.
#[op2]
#[string]
pub fn op_req_get_headers(state: &OpState) -> String {
    let headers: serde_json::Map<String, serde_json::Value> = state
        .borrow::<ScriptInputState>()
        .request.headers.iter()
        .map(|h| (h.name.clone(), serde_json::Value::String(h.value.clone())))
        .collect();
    serde_json::to_string(&headers).unwrap_or_else(|_| "{}".into())
}

/// Returns request body as string. If raw=false, returns JSON-serialized form.
#[op2]
#[string]
pub fn op_req_get_body(state: &OpState, raw: bool) -> String {
    let req = &state.borrow::<ScriptInputState>().request;
    match &req.body {
        Some(body) => {
            if raw {
                body.raw_string()
            } else {
                body.as_json_string()
            }
        }
        None => if raw { String::new() } else { "null".into() },
    }
}

#[op2(fast)]
pub fn op_req_get_timeout(state: &OpState) -> u32 {
    state.borrow::<ScriptInputState>().request.settings
        .as_ref()
        .and_then(|s| s.timeout)
        .unwrap_or(0) as u32
}

#[op2]
#[string]
pub fn op_req_get_execution_mode(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().execution_mode.clone()
}

#[op2]
#[string]
pub fn op_req_get_execution_platform(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().execution_platform.clone()
}
```

> **Note to subagent:** `req.path_params`, `req.auth.type_name()`, `req.body.raw_string()`, `req.body.as_json_string()`, `req.settings.timeout` — check the actual field and method names in `crates/rocket-http/src/` before finalising. Adjust to match the real types.

- [ ] **Step 2: Add `url` crate dep to `rocket-infra/Cargo.toml`** if not already present:

```toml
url = "2"
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/
git commit -m "feat(rocket-infra): req read ops"
```

---

## Task 2: `req.*` write ops (phase-guarded)

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/req.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs`

- [ ] **Step 1: Append write ops to `req.rs`**

```rust
// ── req write ops (BeforeRequest only) ───────────────────────────────────────

/// rok: req.setUrl(url)
/// Throws a JS error if called outside BeforeRequest.
#[op2]
#[string]
pub fn op_req_set_url(state: &mut OpState, #[string] url: String) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.url = Some(url);
    out.any_request_mutation = true;
    Ok(())
}

#[op2]
#[string]
pub fn op_req_set_method(state: &mut OpState, #[string] method: String) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.method = Some(method);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_header(
    state: &mut OpState,
    #[string] name: String,
    #[string] value: String,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_set.insert(name, value);
    out.any_request_mutation = true;
    Ok(())
}

/// Accepts JSON object of { name: value } pairs.
#[op2(fast)]
pub fn op_req_set_headers(
    state: &mut OpState,
    #[string] headers_json: String,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let map: std::collections::HashMap<String, String> =
        serde_json::from_str(&headers_json).unwrap_or_default();
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_set.extend(map);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_delete_header(
    state: &mut OpState,
    #[string] name: String,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_deleted.push(name);
    out.any_request_mutation = true;
    Ok(())
}

/// Accepts JSON array of header name strings.
#[op2(fast)]
pub fn op_req_delete_headers(
    state: &mut OpState,
    #[string] names_json: String,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let names: Vec<String> = serde_json::from_str(&names_json).unwrap_or_default();
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_deleted.extend(names);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_body(
    state: &mut OpState,
    #[string] body_json: String,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let value: serde_json::Value = serde_json::from_str(&body_json).unwrap_or(serde_json::Value::Null);
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.body = Some(value);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_timeout(
    state: &mut OpState,
    ms: u32,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.timeout_ms = Some(ms as u64);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_max_redirects(
    state: &mut OpState,
    n: u32,
) -> Result<(), deno_core::error::AnyError> {
    guard_before_request(state).map_err(|e| deno_core::error::generic_error(e))?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.max_redirects = Some(n);
    out.any_request_mutation = true;
    Ok(())
}
```

- [ ] **Step 2: Register all `req` ops in `build_extension()` inside `engine.rs`**

Replace the single req stub with the full list:

```rust
// req read ops
req::op_req_get_url(),
req::op_req_get_host(),
req::op_req_get_path(),
req::op_req_get_query_string(),
req::op_req_get_path_params(),
req::op_req_get_method(),
req::op_req_get_name(),
req::op_req_get_tags(),
req::op_req_get_auth_mode(),
req::op_req_get_header(),
req::op_req_get_headers(),
req::op_req_get_body(),
req::op_req_get_timeout(),
req::op_req_get_execution_mode(),
req::op_req_get_execution_platform(),
// req write ops
req::op_req_set_url(),
req::op_req_set_method(),
req::op_req_set_header(),
req::op_req_set_headers(),
req::op_req_delete_header(),
req::op_req_delete_headers(),
req::op_req_set_body(),
req::op_req_set_timeout(),
req::op_req_set_max_redirects(),
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): req write ops with BeforeRequest phase guard"
```

---

## Task 3: `res.*` ops + integration tests for req/res

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/res.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs`

- [ ] **Step 1: Replace `res` stub with full ops**

```rust
use deno_core::{op2, OpState};
use crate::scripting::state::ScriptInputState;

fn get_response(state: &OpState) -> Result<&rocket_http::HttpResponse, deno_core::error::AnyError> {
    let input = state.borrow::<ScriptInputState>();
    input.response.as_ref()
        .ok_or_else(|| deno_core::error::generic_error(
            "res is not available in before-request scripts"
        ))
}

#[op2(fast)]
pub fn op_res_get_status(state: &OpState) -> Result<u32, deno_core::error::AnyError> {
    Ok(get_response(state)?.status as u32)
}

#[op2]
#[string]
pub fn op_res_get_status_text(state: &OpState) -> Result<String, deno_core::error::AnyError> {
    Ok(get_response(state)?.status_text.clone().unwrap_or_default())
}

#[op2]
#[string]
pub fn op_res_get_header(
    state: &OpState,
    #[string] name: String,
) -> Result<String, deno_core::error::AnyError> {
    let res = get_response(state)?;
    Ok(res.headers.iter()
        .find(|h| h.name.eq_ignore_ascii_case(&name))
        .map(|h| h.value.clone())
        .unwrap_or_default())
}

#[op2]
#[string]
pub fn op_res_get_headers(state: &OpState) -> Result<String, deno_core::error::AnyError> {
    let res = get_response(state)?;
    let map: serde_json::Map<String, serde_json::Value> = res.headers.iter()
        .map(|h| (h.name.clone(), serde_json::Value::String(h.value.clone())))
        .collect();
    Ok(serde_json::to_string(&map).unwrap_or_else(|_| "{}".into()))
}

/// Returns body as string. If raw=false, returns JSON-parsed form.
#[op2]
#[string]
pub fn op_res_get_body(
    state: &OpState,
    raw: bool,
) -> Result<String, deno_core::error::AnyError> {
    let res = get_response(state)?;
    let body_str = res.body.as_deref().unwrap_or("");
    if raw {
        Ok(body_str.to_string())
    } else {
        // Try JSON parse; if fails, return raw string so bootstrap can decide
        Ok(body_str.to_string())
    }
}

#[op2(fast)]
pub fn op_res_get_response_time(state: &OpState) -> Result<u32, deno_core::error::AnyError> {
    Ok(get_response(state)?.duration_ms.unwrap_or(0) as u32)
}
```

> **Note to subagent:** Check actual field names on `HttpResponse` in `crates/rocket-http/src/` — `status`, `status_text`, `headers`, `body`, `duration_ms` — adjust to match.

- [ ] **Step 2: Register `res` ops in `build_extension()`**

```rust
// res ops
res::op_res_get_status(),
res::op_res_get_status_text(),
res::op_res_get_header(),
res::op_res_get_headers(),
res::op_res_get_body(),
res::op_res_get_response_time(),
```

- [ ] **Step 3: Add integration tests for req mutations and res reads**

Append to `#[cfg(test)]` in `engine.rs`:

```rust
#[tokio::test]
async fn req_set_header_in_before_request() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("req.setHeader('x-custom', 'my-value')");
    let result = engine.execute(ctx).await.unwrap();
    let mutations = result.request_mutations.unwrap();
    assert_eq!(mutations.headers_set.get("x-custom").unwrap(), "my-value");
}

#[tokio::test]
async fn req_set_url_in_before_request() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("req.setUrl('https://new.example.com/api')");
    let result = engine.execute(ctx).await.unwrap();
    let mutations = result.request_mutations.unwrap();
    assert_eq!(mutations.url.unwrap(), "https://new.example.com/api");
}

#[tokio::test]
async fn req_mutation_rejected_in_after_response() {
    let engine = DenoScriptEngine::new();
    let mut ctx = minimal_ctx("req.setUrl('https://blocked.com')");
    ctx.phase = rocket_scripting::ScriptPhase::AfterResponse;
    ctx.response = Some(rocket_http::HttpResponse {
        status: 200,
        ..Default::default()
    });
    let result = engine.execute(ctx).await.unwrap();
    // Mutation was rejected — script error is set, no mutations applied
    assert!(result.error.is_some());
    assert!(result.request_mutations.is_none());
}

#[tokio::test]
async fn res_get_status_in_after_response() {
    let engine = DenoScriptEngine::new();
    let mut ctx = minimal_ctx("rok.setVar('code', String(res.getStatus()))");
    ctx.phase = rocket_scripting::ScriptPhase::AfterResponse;
    ctx.response = Some(rocket_http::HttpResponse {
        status: 201,
        ..Default::default()
    });
    let result = engine.execute(ctx).await.unwrap();
    assert_eq!(result.runtime_vars.get("code").unwrap(), "201");
}

#[tokio::test]
async fn res_unavailable_in_before_request() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("res.getStatus()");
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.error.is_some());
}
```

- [ ] **Step 4: Run all scripting tests**

```bash
cargo test -p rocket-infra scripting 2>&1 | tail -30
```

Expected: all pass.

- [ ] **Step 5: Compile clean**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
```

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): res ops + req/res integration tests"
```
