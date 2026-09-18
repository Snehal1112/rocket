# Spec: Opt-In Host Guard for Script-Driven Request Mutations

**Status:** Draft
**Severity:** Low (by-design capability today; this is opt-in hardening, not a bug fix)
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 6
**Related:** independent of items 1-5 — can be implemented in any order, lowest priority.

## 1. Problem

`req.setUrl`/`req.setMethod`/`req.setBody` in a `BeforeRequest` script can redirect the app's own
declared outgoing request to an arbitrary URL, including internal/loopback/link-local addresses
(e.g. `http://169.254.169.254/latest/meta-data/...` on cloud VMs, or `http://localhost:<port>` to
hit another local service). Confirmed: `ReqwestExecutor`
(`crates/rocket-infra/src/reqwest_executor.rs`) has no host/IP allowlist or blocklist anywhere in
its client-building or request-dispatch code.

This is **the same capability the user already has** by typing a URL into the request bar
manually — RocketAPI is a general-purpose HTTP client, and legitimately needs to be able to hit
`localhost`/internal services on request. It is routed entirely through the app's one normal,
declared `ReqwestExecutor` HTTP path (confirmed by the prior audit: no side-channel network op
exists anywhere in the scripting ops surface). This is **not** a sandbox bypass. It is worth
hardening anyway for teams that specifically want to prevent an *imported, semi-trusted collection
script* from silently retargeting a request the user thought they were sending somewhere else —
which is a materially different trust boundary than the user's own manual typing.

## 2. Goal

Give security-conscious teams an **opt-in** setting that validates the *final* resolved request
(after any `BeforeRequest` script mutations) against a configurable blocklist of internal/loopback
address ranges, before dispatch — without changing default behavior or restricting the user's own
manually-entered URLs at all.

## 3. Design

### 3.1 Scope: only script-originated mutations are checked

The check must apply **only** to the parts of the request a `BeforeRequest` script actually
changed, never to what the user typed. Compare the pre-script `HttpRequest` (available as
`ScriptContext.request`, `crates/rocket-scripting/src/context.rs:24`) against the
`RequestMutations` a script produced (`crates/rocket-scripting/src/result.rs:61-83`) — if
`request_mutations.url` is `Some(new_url)` and `new_url` differs from the original, that's the
value to validate. If a script makes no URL mutation, there is nothing to check regardless of the
setting.

### 3.2 New setting: `RequestGuardPolicy`

Per-workspace setting (stored in `workspace.yml` via `WorkspaceConfig`, alongside existing
workspace-level settings — verify the exact existing settings struct location,
`rocket-workspace`, at implementation time), default fully permissive (today's behavior,
unchanged):

```rust
// crates/rocket-shared/src/types.rs (or a new small module — verify placement against
// existing conventions at implementation time)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestGuardPolicy {
    /// When true, a BeforeRequest script's `req.setUrl` mutation is checked against
    /// the blocked ranges below before the request is dispatched.
    #[serde(default)]
    pub block_script_redirects_to_internal_hosts: bool,
    /// Additionally block RFC1918 private ranges (10/8, 172.16/12, 192.168/16), not just
    /// loopback/link-local. Off by default even when the guard itself is enabled, since
    /// many legitimate internal APIs live on private ranges.
    #[serde(default)]
    pub also_block_private_ranges: bool,
}
```

### 3.3 Enforcement point

In `RequestExecutionService`, after `apply_script_side_effects` applies a `BeforeRequest` script's
`request_mutations.url` to the working `HttpRequest` (locate the exact application site — per the
prior audit, around `execution_service.rs:599-602` — verify at implementation time) and before the
mutated request reaches `self.executor.execute(...)`:

```rust
fn check_request_guard(&self, original_url: &str, mutated_url: &str, policy: &RequestGuardPolicy) -> DomainResult<()> {
    if !policy.block_script_redirects_to_internal_hosts || original_url == mutated_url {
        return Ok(());
    }
    let host = url::Url::parse(mutated_url)
        .map_err(|e| DomainError::InvalidInput(format!("script produced an invalid URL: {e}")))?
        .host_str().map(str::to_owned);
    if let Some(host) = host {
        if is_blocked_host(&host, policy.also_block_private_ranges) {
            return Err(DomainError::InvalidInput(format!(
                "blocked: script redirected request to internal host '{host}' \
                 (workspace policy blocks script-driven redirects to internal hosts)"
            )));
        }
    }
    Ok(())
}
```

This lives in `rocket-app` (business-rule decision: "does policy apply, is this host blocked"),
not `rocket-infra`/`ReqwestExecutor` — consistent with the DDD boundary rule that concrete I/O
stays in `rocket-infra` while decision logic belongs in the orchestration layer
(`.claude/rules/rust-ddd-boundaries.md`).

`is_blocked_host` checks, at minimum: `127.0.0.0/8`, `::1`, `169.254.0.0/16` (explicitly including
`169.254.169.254`, the standard cloud metadata endpoint — this is the single highest-value entry
in the list), `localhost` (string form, in addition to loopback IPs, since scripts may write the
hostname literally). When `also_block_private_ranges` is set, additionally check
`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`. Use the `ipnet`/`std::net::IpAddr` standard-library
range-containment checks (verify what's already available in the workspace's dependency tree
before adding a new crate — `reqwest`'s dependency graph likely already pulls in something
suitable; prefer `std::net::Ipv4Addr`/`Ipv6Addr` manual range checks if no crate is already
present, to avoid adding a new dependency for a handful of well-known ranges).

On block, the error surfaces through the same path a request-send failure already takes (i.e., it
becomes a script/execution error visible to the user, not a silent drop) — the user sees clearly
*why* the request didn't go out.

### 3.4 Frontend

A toggle in the workspace settings UI (shadcn/ui `Switch`, per the project's UI-primitive rules)
for `block_script_redirects_to_internal_hosts` and `also_block_private_ranges`, off by default.
Out of scope to design the exact settings-panel layout here — the implementation plan should
locate the existing workspace-settings component and add these two fields following its established
pattern, not invent a new settings surface.

## 4. Non-goals

- Not enabled by default — this changes established, intentional product behavior (general-purpose
  HTTP client parity with manual URL entry) and must be an explicit opt-in.
- Not checking the user's own manually-entered URL, ever, regardless of policy — only
  script-*mutated* URLs are in scope, per §3.1.
- Not checking `req.setHeader`/`setBody` mutations for SSRF-adjacent content (e.g. an
  `X-Forwarded-Host` header pointing internally) — header/body content isn't a redirect of the
  request's own destination, and reasoning about "dangerous header values" generically is a much
  larger, fuzzier problem than validating a concrete destination host. Out of scope.
- Not covering `req.setUrl` calls that resolve to the *same* effective host the user already typed
  (e.g. a script rewriting only the path/query) — those are unaffected by design, since `original_url == mutated_url`'s host component is what actually matters; refine the comparison to compare resolved hosts, not raw strings, during implementation (the pseudocode above compares whole URLs for the early-exit short-circuit only; the actual blocked-host check always operates on the parsed host).
- Not following HTTP redirect chains — the guard validates only the host the script's `req.setUrl()`
  names directly, before dispatch. `reqwest`'s redirect policy (`crates/rocket-infra/src/
  reqwest_executor.rs`) applies no host filtering of its own, so a script that sets the URL to an
  allowed public host which then responds with a redirect to an internal address is not caught —
  the same one-hop-of-indirection limitation a user's own manually-typed URL already has, since
  this feature deliberately never restricts that case either (see the non-goal above). Closing this
  would mean threading `is_blocked_host` into `reqwest_executor.rs`'s redirect policy, which crosses
  the `rocket-app`/`rocket-infra` boundary this spec deliberately keeps this feature out of; tracked
  as a follow-up roadmap item, not part of this plan's scope.

## 5. Interfaces (for the implementation plan)

- `rocket_shared::types::RequestGuardPolicy` — new struct (or wherever workspace-level settings conventionally live — verify against `rocket-workspace`'s existing `WorkspaceConfig` shape at implementation time).
- `RequestExecutionService::check_request_guard(&self, original_url: &str, mutated_url: &str, policy: &RequestGuardPolicy) -> DomainResult<()>` — new private method.
- `is_blocked_host(host: &str, also_block_private: bool) -> bool` — new free function, `rocket-app` (or `rocket-shared` if a more reusable location fits better — decide at implementation time; note it has no I/O and is a pure function, so either crate is DDD-compliant).

## 6. Acceptance criteria

1. With the policy disabled (default), a `BeforeRequest` script calling
   `req.setUrl('http://169.254.169.254/latest/meta-data/')` sends exactly as it does today — no
   behavior change.
2. With `block_script_redirects_to_internal_hosts: true`, the same script produces a blocked-host
   error instead of sending the request.
3. With the policy enabled but `also_block_private_ranges: false` (default), a script redirecting
   to `http://192.168.1.1/` is **not** blocked.
4. With both flags enabled, the same redirect **is** blocked.
5. A user manually typing `http://localhost:8080/` into the request URL bar (no script
   involvement) is never blocked, regardless of policy — the guard only ever inspects
   script-produced `request_mutations.url`.
6. A `BeforeRequest` script that doesn't call `req.setUrl` at all is completely unaffected,
   regardless of policy.
7. `cargo test -p rocket-app` passes; `yarn tsc --noEmit` passes for the settings UI addition.
