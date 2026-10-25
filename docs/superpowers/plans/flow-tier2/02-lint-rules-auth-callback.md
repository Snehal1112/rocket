# Flow Tier 2 Auth and Callback Lint Rules Implementation Plan (Plan 2 of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three non-blocking lint rules to the Flow lint tier: an Auth node that has no effect and an `auth` wire that replaces a saved request's own auth (F-24), and a saved request that sends `{{callback.<name>}}` without being wired before that Wait for callback node (F-25). Pin the new codes on the client and update the docs.

**Architecture:** Plan 1 added the app-layer `CollectionLintContext` (`crates/rocket-app/src/flow_lint_context.rs`) and wired `FlowService::lint` and the `lint_flow` command to use it. This plan adds two defaulted methods to the `LintContext` trait, so Plan 1's types do not change shape. The three rules live in `rocket-flow` `lint.rs` and only ask the context yes/no questions. The facts about a saved request (explicit auth, callback names used) are computed by free functions in a new `rocket-app` module `flow_lint_facts.rs`, which take a `&dyn CollectionRepository`. The context implements the two new methods by delegating to them. Task 3 is frontend tests and docs; the client already lists the three codes in `BACKEND_LINT_CODES`.

**Tech Stack:** Rust (`rocket-flow`, `rocket-app`), React, TypeScript, Vitest, Biome. `cargo ... -j4`, `yarn`.

**Spec:** `docs/roadmaps/flow-roadmap.md` rows F-24 and F-25 (Tier 2). Engine facts: `crates/rocket-app/src/flow_auth.rs` (`FlowCredentials::apply_to_inherit`), `crates/rocket-app/src/flow_execution_service.rs` (~1629-1665, the Request arm), `crates/rocket-flow/src/validate.rs` V13, V14 and V15, `docs/superpowers/specs/opencollection-spec-reference.md` sections 3.1 and 6.1.

## Global Constraints

- Rust gates: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never run `cargo fmt` over a crate. Check only your own files with `rustfmt --check --edition 2021 <file>`; `lint.rs` and `flow_service.rs` already have drift (roadmap F-118), so read the diff and fix only hunks you wrote.
- No unwrap or expect calls in production code. Test code may use `.expect("message")`.
- Serde `camelCase` only on IPC DTOs. This plan adds no serialized type.
- Frontend gates: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run --maxWorkers=2`. shadcn/ui and lucide-react only; this plan adds no UI component.
- Comments are short full sentences ending in a punctuation mark. No emojis.
- Lint messages and hints name nodes by label only. They never quote a callback name, a URL, a token, an expression or an auth field. Tests pin this.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A`.
- Tasks 1 and 2 touch auth, the request model and the lint context. Each starts with the OpenCollection read line.
- Plan 1 runs before this plan and I could not see it. Each of Tasks 1 and 2 starts with a step that re-reads `crates/rocket-app/src/flow_lint_context.rs` and `crates/rocket-flow/src/lint.rs` as Plan 1 left them. Where this plan says "the context's repository and collection", use the field names Plan 1 chose. Code in this plan that touches Plan 1's context is marked `// Adapt:`.

## Review Focus

- An Auth node that applies to inherited auth is never flagged while any Request might take its credential. An unknown saved request (`None` from the context) counts as "might take it", so a context that knows nothing never warns (Task 1).
- The `auth` wire lint says the wire replaces the request's own auth, because the engine lets the wire win (`request_input.auth = credential.clone()`), and it fires only when the context says the saved request has explicit auth. Inline requests have no auth field and never trigger it (Task 1).
- "Explicit" means the effective auth (`runtime_auth`, else `auth`) is neither `none` nor `inherit`, the same rule `FlowCredentials::apply_to_inherit` uses (Task 1).
- A saved request that sends `{{callback.x}}` is flagged only when it has no path to the Wait named `x`, which is the V15 direction (the request registers the URL before the Wait runs). A wired-before request, a request that does not use the variable, and an unknown request are all silent (Task 2).
- Disabled headers, query params and form entries are not scanned, so a disabled field cannot cause a warning (Task 2).
- No lint message contains a Wait name, token or URL, and a cyclic graph does not hang the callback rule (Task 2).
- The client keeps the three codes in `BACKEND_LINT_CODES`, never reports them itself, and the issue list shows an edge-bound lint as a clickable node item (Task 3).

---

### Task 1: Auth lints (F-24)

**Files:**
- Modify: `crates/rocket-flow/src/lint.rs` (constants ~16-24, `LintContext` ~50-60, `validate_with_warnings` ~69-84, `GraphIndex` ~87-114, new rule fns after `no_path_lint` ~268, tests ~544-1321)
- Create: `crates/rocket-app/src/flow_lint_facts.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module list ~19-30)
- Modify: `crates/rocket-app/src/flow_lint_context.rs` (Plan 1's file, one new trait method)
- Test: `crates/rocket-flow/src/lint.rs` (`mod tests`)
- Test: `crates/rocket-app/src/flow_lint_facts.rs` (`mod tests`)
- Test: `crates/rocket-app/src/flow_lint_context.rs` (`mod tests`, one test)

**Interfaces:**
- Consumes: `FlowNodeKind::Auth { label, auth, apply_to_inherit }` and `FlowNodeKind::Request { label, source, .. }`, `RequestSource::{Saved { request_path }, Inline { request }}` (`rocket-flow`), `handle::AUTH`, `rocket_collection::{CollectionRepository, Request}`, `rocket_shared::types::Auth`, `test_doubles::ConfigurableCollectionRepo` (`with_request`).
- Produces in `lint.rs`:
  - `pub const AUTH_NO_EFFECT: &str = "auth_no_effect";`
  - `pub const AUTH_WIRE_OVERRIDES_AUTH: &str = "auth_wire_overrides_auth";`
  - Trait method `fn saved_request_auth_is_explicit(&self, _request_path: &str) -> Option<bool> { None }` on `LintContext`. `Some(true)` means the saved request's own auth is neither none nor inherit.
- Produces in `flow_lint_facts.rs`:
  - `pub(crate) fn request_auth_is_explicit(request: &Request) -> bool`
  - `pub(crate) fn saved_request_auth_is_explicit(repo: &dyn CollectionRepository, collection: &str, request_path: &str) -> Option<bool>` (`None` when the read fails for any reason).

How the engine applies credentials, which fixes the rules (from `flow_execution_service.rs` ~1629-1665 and `flow_auth.rs` ~107-115):

1. A Request first calls `credentials.apply_to_inherit(&mut request_input.auth)`. It replaces the auth with the Auth node's credential only when the request's auth is `Inherit` or `None`. Inline requests are built with `Auth::None`, so they always take it. A saved request's auth is `runtime_auth.unwrap_or(auth)` (`runner_sequence.rs` ~197).
2. Then every `auth` wire into the request does `request_input.auth = credential.clone()`. The wire wins over inherit and over the request's own explicit auth. The request's own auth is not ignored in a silent way; it is replaced.

So an Auth node has an effect when it has an outgoing `auth` wire (the wire always applies), or when `apply_to_inherit` is on and at least one Request could take it. A Request cannot take it when an `auth` wire already sets its auth (the inherited credential is replaced by the wire) or when its saved auth is explicit. Because the check must not give false positives, "could take it" is true for an inline request, for a saved request whose auth is not explicit, and for a saved request the context cannot read.

- [ ] **Step 0: Orient**

Run `git log --oneline -8` and confirm Plan 1 is merged on this branch. Read `docs/superpowers/specs/opencollection-spec-reference.md`. Re-read `crates/rocket-app/src/flow_lint_context.rs` and `crates/rocket-flow/src/lint.rs` as Plan 1 left them and write down: the struct name and fields of `CollectionLintContext` (the repository reference and the collection name), how it looks up a saved request for `saved_request_exists`, whether `lint.rs` tests already define a fake `LintContext`, and whether Plan 1 changed `GraphIndex` or the node loop in `validate_with_warnings`. Adapt every name below to what is there. If Plan 1's tests already have a fake context struct, extend it with the two new methods and delete my `AuthCallbackContext` in Step 1 instead of adding a second fake.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/lint.rs` `mod tests`, after the `auth` helper (~line 713), add the helpers and tests below. `AuthCallbackContext` also serves Task 2.

```rust
    /// A context that answers from maps, so a test controls every fact.
    #[derive(Default)]
    struct AuthCallbackContext {
        explicit_auth: HashMap<String, bool>,
        callbacks: HashMap<String, Vec<String>>,
    }

    impl LintContext for AuthCallbackContext {
        fn saved_request_auth_is_explicit(&self, request_path: &str) -> Option<bool> {
            self.explicit_auth.get(request_path).copied()
        }

        fn saved_request_callbacks(
            &self,
            request_path: &str,
            names: &[&str],
        ) -> Option<Vec<String>> {
            self.callbacks.get(request_path).map(|used| {
                used.iter()
                    .filter(|name| names.contains(&name.as_str()))
                    .cloned()
                    .collect()
            })
        }
    }

    fn explicit(paths: &[(&str, bool)]) -> AuthCallbackContext {
        AuthCallbackContext {
            explicit_auth: paths.iter().map(|(p, v)| (p.to_string(), *v)).collect(),
            ..AuthCallbackContext::default()
        }
    }

    fn lint_with(f: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
        validate_with_warnings(f, ctx)
    }

    fn inline_req(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                source: RequestSource::Inline {
                    request: crate::node::InlineRequestData {
                        method: "GET".to_string(),
                        url: "https://example.test/x".to_string(),
                        headers: Vec::new(),
                        body: None,
                    },
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    #[test]
    fn an_auth_node_that_applies_to_an_inline_request_is_not_flagged() {
        let f = flow(
            vec![auth("a", true), inline_req("r"), output("out")],
            vec![edge("e1", "r", handle::RESULT, "out", "value")],
        );
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_idle_auth_node_with_no_wire_is_one_warning_naming_the_label() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![edge("e1", "r", handle::RESULT, "out", "value")],
        );
        let lints = lint(&f);
        let found = only(&lints, AUTH_NO_EFFECT);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("a"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'a'"), "{}", found[0].message);
        assert!(found[0].hint.is_some());
    }

    #[test]
    fn an_auth_node_with_an_auth_wire_has_an_effect_even_when_not_applied() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_applying_auth_node_in_a_flow_without_requests_is_flagged() {
        let f = flow(vec![auth("a", true), input("in"), output("out")], Vec::new());
        assert_eq!(only(&lint(&f), AUTH_NO_EFFECT).len(), 1);
    }

    #[test]
    fn an_applying_auth_node_is_flagged_when_every_saved_request_has_explicit_auth() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        let ctx = explicit(&[("r1.yml", true), ("r2.yml", true)]);
        assert_eq!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).len(), 1);
    }

    #[test]
    fn an_applying_auth_node_is_not_flagged_when_one_saved_request_inherits() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        let ctx = explicit(&[("r1.yml", true), ("r2.yml", false)]);
        assert!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_unknown_saved_request_never_causes_an_auth_no_effect_lint() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        // r2 is unknown to the context, so it might inherit.
        let ctx = explicit(&[("r1.yml", true)]);
        assert!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).is_empty());
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn a_request_taking_another_auth_wire_does_not_use_the_inherited_credential() {
        let f = flow(
            vec![auth("a1", true), auth("a2", false), inline_req("r"), output("out")],
            vec![
                edge("e1", "a2", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint(&f);
        let found = only(&lints, AUTH_NO_EFFECT);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("a1"));
    }

    #[test]
    fn an_auth_wire_into_a_saved_request_with_explicit_auth_is_one_edge_warning() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let ctx = explicit(&[("r.yml", true)]);
        let lints = lint_with(&f, &ctx);
        let found = only(&lints, AUTH_WIRE_OVERRIDES_AUTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("r"));
        assert_eq!(found[0].edge_id.as_deref(), Some("e1"));
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'a'") && found[0].message.contains("'r'"));
        assert!(found[0].message.contains("replaces"), "{}", found[0].message);
    }

    #[test]
    fn an_auth_wire_is_silent_when_the_request_inherits_or_is_unknown_or_inline() {
        let saved = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(&lint_with(&saved, &explicit(&[("r.yml", false)])), AUTH_WIRE_OVERRIDES_AUTH).is_empty());
        assert!(only(&lint(&saved), AUTH_WIRE_OVERRIDES_AUTH).is_empty());
        let inline = flow(
            vec![auth("a", false), inline_req("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(&lint_with(&inline, &explicit(&[("r", true)])), AUTH_WIRE_OVERRIDES_AUTH).is_empty());
    }

    #[test]
    fn auth_lints_never_quote_the_credential() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let idle = flow(vec![auth("a", false), request("r")], Vec::new());
        let mut all = lint_with(&f, &explicit(&[("r.yml", true)]));
        all.extend(lint(&idle));
        assert!(!all.is_empty());
        for l in &all {
            let text = format!("{} {}", l.message, l.hint.as_deref().unwrap_or(""));
            assert!(!text.contains("{{token}}"), "leaked in: {text}");
        }
    }
```

Then change the existing test `an_auth_node_that_applies_to_inherited_auth_is_not_flagged` (~line 779). Its `idle` flow now also gets the new `auth_no_effect` warning, so filter by code:

```rust
        let idle = flow(
            vec![auth("a1", false), request("r1"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        let no_path: Vec<FlowLint> = lint(&idle)
            .into_iter()
            .filter(|l| l.code == NO_PATH_TO_OUTPUT)
            .collect();
        assert_eq!(keys(&no_path), vec![(Some("a1"), None, NO_PATH_TO_OUTPUT)]);
```

In `crates/rocket-app/src/flow_lint_facts.rs` (new file) add only the test module for now, so the red state is clear:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::ConfigurableCollectionRepo;
    use rocket_shared::types::{Auth, HttpMethod};

    fn request_with(runtime: Option<Auth>, auth: Auth) -> Request {
        let mut r = Request::new("r", HttpMethod::Get, "https://example.test/x");
        r.auth = auth;
        r.runtime_auth = runtime;
        r
    }

    fn bearer() -> Auth {
        Auth::Bearer {
            token: "t".to_string(),
        }
    }

    #[test]
    fn none_and_inherit_are_not_explicit() {
        assert!(!request_auth_is_explicit(&request_with(None, Auth::None)));
        assert!(!request_auth_is_explicit(&request_with(None, Auth::Inherit)));
    }

    #[test]
    fn a_concrete_auth_is_explicit() {
        assert!(request_auth_is_explicit(&request_with(None, bearer())));
    }

    #[test]
    fn the_runtime_auth_wins_over_the_stored_auth() {
        assert!(!request_auth_is_explicit(&request_with(Some(Auth::Inherit), bearer())));
        assert!(request_auth_is_explicit(&request_with(Some(bearer()), Auth::None)));
    }

    #[test]
    fn the_repo_helper_reads_the_saved_request_and_answers_none_when_it_cannot() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request("demo", "a.yml", request_with(None, bearer()));
        repo.with_request("demo", "b.yml", request_with(None, Auth::None));
        assert_eq!(saved_request_auth_is_explicit(&*repo, "demo", "a.yml"), Some(true));
        assert_eq!(saved_request_auth_is_explicit(&*repo, "demo", "b.yml"), Some(false));
        assert_eq!(saved_request_auth_is_explicit(&*repo, "demo", "missing.yml"), None);
        repo.fail_request_reads();
        assert_eq!(saved_request_auth_is_explicit(&*repo, "demo", "a.yml"), None);
    }
}
```

In Plan 1's `flow_lint_context.rs` `mod tests`, add one test using Plan 1's own fixture builder. The shape below uses `ConfigurableCollectionRepo`; replace the constructor call with the one Plan 1's tests use:

```rust
    #[test]
    fn the_context_answers_explicit_auth_from_the_collection() {
        use crate::test_doubles::ConfigurableCollectionRepo;
        use rocket_collection::Request;
        use rocket_shared::types::{Auth, HttpMethod};

        let repo = ConfigurableCollectionRepo::new();
        let mut explicit = Request::new("a", HttpMethod::Get, "https://example.test/x");
        explicit.auth = Auth::Bearer {
            token: "t".to_string(),
        };
        repo.with_request("demo", "a.yml", explicit);
        // Plan 1's test helpers: `sources`, `flow_of`, `inline_node` and `ENV` live in this module.
        let src = sources(&repo, None, None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(ctx.saved_request_auth_is_explicit("a.yml"), Some(true));
        assert_eq!(ctx.saved_request_auth_is_explicit("gone.yml"), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint`
Expected: compile FAILURE (`AUTH_NO_EFFECT`, `AUTH_WIRE_OVERRIDES_AUTH`, `saved_request_auth_is_explicit` and `saved_request_callbacks` are undefined). `AuthCallbackContext` overrides `saved_request_callbacks`, which Task 2 uses; it is added to the trait in this task as well so the fake compiles (Step 3a).
Run: `cargo test -j4 -p rocket-app flow_lint_facts`
Expected: compile FAILURE (`request_auth_is_explicit` and the module are not defined).

- [ ] **Step 3: Write the implementation**

3a. `crates/rocket-flow/src/lint.rs`. Add imports and constants:

```rust
use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::node::{FlowNodeKind, RequestSource};
```

```rust
/// An Auth node that no Request can take its credential from.
pub const AUTH_NO_EFFECT: &str = "auth_no_effect";
/// An `auth` wire into a saved request that already has its own explicit auth.
pub const AUTH_WIRE_OVERRIDES_AUTH: &str = "auth_wire_overrides_auth";
```

Add to `LintContext` (the second method is used by Task 2; adding it here keeps the trait change in one place):

```rust
    /// Whether the saved request at `request_path` has its own auth, meaning
    /// an auth other than none and inherit.
    fn saved_request_auth_is_explicit(&self, _request_path: &str) -> Option<bool> {
        None
    }

    /// Which of the Wait for callback `names` the saved request at
    /// `request_path` sends as `{{callback.<name>}}`. The answer holds only
    /// names from `names`, never request text. `None` when it cannot tell.
    fn saved_request_callbacks(&self, _request_path: &str, _names: &[&str]) -> Option<Vec<String>> {
        None
    }
```

Extend `GraphIndex` (adapt if Plan 1 changed it; keep its fields and add these):

```rust
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
    /// Ids with a path to an Output, or `None` when the flow has no Output.
    reaching_output: Option<HashSet<&'a str>>,
    /// The first node of each id.
    nodes_by_id: HashMap<&'a str, &'a FlowNode>,
    /// The `auth` wires into each node.
    auth_wires_into: HashMap<&'a str, Vec<&'a FlowEdge>>,
    /// Ids of nodes with at least one outgoing `auth` wire.
    auth_sources: HashSet<&'a str>,
    /// The nodes each node has a wire to, of any kind.
    successors: HashMap<&'a str, Vec<&'a str>>,
    /// `(name, node)` of each Wait for callback node, first of a name wins.
    waits: Vec<(&'a str, &'a FlowNode)>,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        let mut auth_wires_into: HashMap<&'a str, Vec<&'a FlowEdge>> = HashMap::new();
        let mut auth_sources: HashSet<&'a str> = HashSet::new();
        let mut successors: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
            successors
                .entry(edge.source_node_id.as_str())
                .or_default()
                .push(edge.target_node_id.as_str());
            if edge.target_field == handle::AUTH {
                auth_wires_into
                    .entry(edge.target_node_id.as_str())
                    .or_default()
                    .push(edge);
                auth_sources.insert(edge.source_node_id.as_str());
            }
        }
        let mut nodes_by_id: HashMap<&'a str, &'a FlowNode> =
            HashMap::with_capacity(flow.nodes.len());
        let mut waits: Vec<(&'a str, &'a FlowNode)> = Vec::new();
        let mut wait_names: HashSet<&'a str> = HashSet::new();
        for node in &flow.nodes {
            nodes_by_id.entry(node.id.as_str()).or_insert(node);
            if let FlowNodeKind::WaitForCallback { name, .. } = &node.kind {
                if !name.is_empty() && wait_names.insert(name.as_str()) {
                    waits.push((name.as_str(), node));
                }
            }
        }
        Self {
            wired_exits,
            reaching_output: nodes_reaching_output(flow),
            nodes_by_id,
            auth_wires_into,
            auth_sources,
            successors,
            waits,
        }
    }

    fn is_wired(&self, node_id: &str, exit: &str) -> bool {
        self.wired_exits
            .get(node_id)
            .is_some_and(|exits| exits.contains(exit))
    }

    /// Every id reachable by following wires forward from `start`. The
    /// visited set makes a cycle safe.
    fn downstream_of(&self, start: &str) -> HashSet<&'a str> {
        let mut seen: HashSet<&'a str> = HashSet::new();
        let mut stack: Vec<&str> = vec![start];
        while let Some(id) = stack.pop() {
            for &next in self.successors.get(id).into_iter().flatten() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        seen
    }
}
```

In `validate_with_warnings`, remove the `let _ = ctx;` line and its comment if Plan 1 has not already, and extend the node loop (keep Plan 1's lines; the new ones go last, so the rule order stays fixed):

```rust
        lints.extend(exit_lints(node, &index));
        lints.extend(no_path_lint(node, &index));
        // Plan 1's rules for this node go here, before the lines below.
        lints.extend(auth_no_effect_lint(node, flow, &index, ctx));
        lints.extend(auth_wire_lints(node, &index, ctx));
```

Add after `no_path_lint`:

```rust
/// An Auth node that nothing uses. It is used when it has an `auth` wire, or
/// when it applies to inherited auth and some Request might take it. A
/// Request might take it when no `auth` wire sets its auth and it is inline,
/// or a saved request that does not have explicit auth, or one the context
/// cannot read. So the lint fires only when it is certain that no Request does.
fn auth_no_effect_lint(
    node: &FlowNode,
    flow: &Flow,
    index: &GraphIndex<'_>,
    ctx: &dyn LintContext,
) -> Option<FlowLint> {
    let FlowNodeKind::Auth {
        apply_to_inherit, ..
    } = &node.kind
    else {
        return None;
    };
    if index.auth_sources.contains(node.id.as_str()) {
        return None;
    }
    if *apply_to_inherit && inherited_auth_may_apply(flow, index, ctx) {
        return None;
    }
    let label = display_label(node);
    if *apply_to_inherit {
        Some(warning(
            AUTH_NO_EFFECT,
            node,
            format!("No Request in this flow takes the credential of '{label}'."),
            "Every Request has its own auth or an 'auth' wire, or the flow has no Request. Remove the node, or let a request inherit auth.",
        ))
    } else {
        Some(warning(
            AUTH_NO_EFFECT,
            node,
            format!(
                "'{label}' is not wired to any Request and does not apply to inherited auth, so no request uses its credential."
            ),
            "Wire its result into a Request's 'auth' input, or let it apply to requests that inherit auth.",
        ))
    }
}

/// False only when no Request can take the inherited credential.
fn inherited_auth_may_apply(flow: &Flow, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> bool {
    flow.nodes.iter().any(|n| {
        let FlowNodeKind::Request { source, .. } = &n.kind else {
            return false;
        };
        // An `auth` wire sets the auth after the inherited credential, so it wins.
        if index.auth_wires_into.contains_key(n.id.as_str()) {
            return false;
        }
        match source {
            RequestSource::Inline { .. } => true,
            RequestSource::Saved { request_path } => {
                ctx.saved_request_auth_is_explicit(request_path) != Some(true)
            }
        }
    })
}

/// One lint per `auth` wire from an Auth node into a saved request that has
/// its own explicit auth. At run time the wire wins, so the request's own
/// auth is replaced. Inline requests have no auth of their own.
fn auth_wire_lints(node: &FlowNode, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> Vec<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return Vec::new();
    };
    let Some(wires) = index.auth_wires_into.get(node.id.as_str()) else {
        return Vec::new();
    };
    if ctx.saved_request_auth_is_explicit(request_path) != Some(true) {
        return Vec::new();
    }
    wires
        .iter()
        .filter_map(|edge| {
            let source = index.nodes_by_id.get(edge.source_node_id.as_str())?;
            if !matches!(source.kind, FlowNodeKind::Auth { .. }) {
                return None;
            }
            let mut lint = warning(
                AUTH_WIRE_OVERRIDES_AUTH,
                node,
                format!(
                    "The credential of '{}' replaces the own auth of '{}'.",
                    display_label(source),
                    display_label(node)
                ),
                "The wire wins at run time. Set the request's auth to inherit, or remove the wire.",
            );
            lint.edge_id = Some(edge.id.clone());
            Some(lint)
        })
        .collect()
}
```

3b. Create `crates/rocket-app/src/flow_lint_facts.rs` (put the test module from Step 1 below this code):

```rust
//! Facts about a saved request that the Flow lint tier asks for. Each
//! function answers yes or no and never returns request text, so a lint
//! cannot quote a URL, a header or a token.

use rocket_collection::{CollectionRepository, Request};
use rocket_shared::types::Auth;

/// The auth a flow run starts from: `runtime_auth` when set, else `auth`.
/// This is the same pick `build_step_input` makes.
fn effective_auth(request: &Request) -> &Auth {
    request.runtime_auth.as_ref().unwrap_or(&request.auth)
}

/// True when the request has its own auth. `None` and `Inherit` are not
/// explicit, because the flow engine treats both as "take the inherited
/// credential" (see `FlowCredentials::apply_to_inherit`).
pub(crate) fn request_auth_is_explicit(request: &Request) -> bool {
    !matches!(effective_auth(request), Auth::None | Auth::Inherit)
}

/// Reads the saved request and answers `request_auth_is_explicit`. `None`
/// when the read fails for any reason, so an unreadable request never warns.
pub(crate) fn saved_request_auth_is_explicit(
    repo: &dyn CollectionRepository,
    collection: &str,
    request_path: &str,
) -> Option<bool> {
    repo.get_request(collection, request_path)
        .ok()
        .map(|request| request_auth_is_explicit(&request))
}
```

3c. `crates/rocket-app/src/lib.rs`: add `pub(crate) mod flow_lint_facts;` after `pub(crate) mod flow_debug;` in the `flow_*` block, next to Plan 1's `flow_lint_context` line.

3d. In `flow_lint_context.rs` add to the `impl LintContext for CollectionLintContext` block:

```rust
    fn saved_request_auth_is_explicit(&self, request_path: &str) -> Option<bool> {
        // Adapt: pass the repository and collection fields Plan 1 holds.
        crate::flow_lint_facts::saved_request_auth_is_explicit(
            self.repo,
            self.collection,
            request_path,
        )
    }
```

- [ ] **Step 4: Run the tests to verify they pass, then the gates**

Run: `cargo test -j4 -p rocket-flow lint` (all PASS, including the edited `an_auth_node_that_applies_to_inherited_auth_is_not_flagged`).
Run: `cargo test -j4 -p rocket-app flow_lint` (facts and context tests PASS) and `cargo test -j4 -p rocket-app flow_service` (Plan 1's `lint` service tests still PASS; if one compares a whole lint list for a flow with an Auth node, filter it by code as in Step 1).
Run: `cargo check -j4 -p rocket-flow`, `cargo check -j4 -p rocket-app`, `cargo check -j4 -p rocket` only if `src-tauri` implements `LintContext` (it should not).
Run: `rustfmt --check --edition 2021 crates/rocket-app/src/flow_lint_facts.rs`. For `lint.rs`, run it too and fix only hunks you wrote.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_lint_facts.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/flow_lint_context.rs && git commit --only -m "feat(flow): lint auth nodes with no effect and auth wires that replace request auth" -- crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_lint_facts.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/flow_lint_context.rs`

---

### Task 2: Callback ordering lint for saved requests (F-25)

**Files:**
- Modify: `crates/rocket-flow/src/lint.rs` (constant, `validate_with_warnings` loop, new fn, tests)
- Modify: `crates/rocket-flow/src/validate.rs` (V15 doc comment ~388-392 only)
- Modify: `crates/rocket-app/src/flow_lint_facts.rs` (new functions and tests)
- Modify: `crates/rocket-app/src/flow_lint_context.rs` (Plan 1's file, one new trait method and one test)
- Test: `crates/rocket-flow/src/lint.rs`, `crates/rocket-app/src/flow_lint_facts.rs`, `crates/rocket-app/src/flow_lint_context.rs`

**Interfaces:**
- Consumes: `LintContext::saved_request_callbacks` (added to the trait in Task 1, no implementation yet except the test fake), `GraphIndex::{waits, downstream_of}` (Task 1), `rocket_flow::validate::mentions_callback(text: &str, name: &str) -> bool`, `rocket_collection::Request` (`url`, `headers`, `query_params`, `path_params`, `body`), `rocket_shared::types::{Header, QueryParam, PathParam, Body, FormDataEntry}`.
- Produces in `lint.rs`: `pub const CALLBACK_NOT_WIRED: &str = "callback_not_wired";`. One warning per (saved request, Wait) pair, on the request node, naming both by label.
- Produces in `flow_lint_facts.rs`:
  - `pub(crate) fn request_callback_names(request: &Request, names: &[&str]) -> Vec<String>`
  - `pub(crate) fn saved_request_callbacks(repo: &dyn CollectionRepository, collection: &str, request_path: &str, names: &[&str]) -> Option<Vec<String>>`

What `{{callback.<name>}}` means (from `flow_callbacks.rs` and `validate.rs` V15): each Wait for callback node opens a listener at run start and the engine adds `callback.<name>` = the listener URL to the variables of every request in the run. A request that sends that URL (in its url, a header or the body) is the one that registers it with a third party, so it must run BEFORE the Wait starts waiting: the request must be an ancestor of the Wait in the wire graph. V15 enforces this as an error for inline requests, and skips saved requests because `rocket-flow` cannot read the collection. The brief for this plan words the order the other way round ("downstream of the Wait"); that contradicts V15, `RunCallbacks` and the existing error message, so this plan implements the V15 direction. Recorded as a deviation in the report.

- [ ] **Step 0: Orient**

Read `docs/superpowers/specs/opencollection-spec-reference.md` (sections 2.3, 2.9 and 6). Re-read `crates/rocket-app/src/flow_lint_context.rs` and `crates/rocket-flow/src/lint.rs` as Plan 1 and Task 1 left them, and adapt every `// Adapt:` line below.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/lint.rs` `mod tests` (after the Task 1 tests):

```rust
    fn wait_node(id: &str, label: &str, name: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::WaitForCallback {
                label: label.to_string(),
                name: name.to_string(),
                timeout_ms: 60_000,
                accept_when: None,
            },
        )
    }

    /// `path` sends `{{callback.<name>}}` for each given name.
    fn sends(path: &str, names: &[&str]) -> AuthCallbackContext {
        AuthCallbackContext {
            callbacks: HashMap::from([(
                path.to_string(),
                names.iter().map(|n| n.to_string()).collect(),
            )]),
            ..AuthCallbackContext::default()
        }
    }

    #[test]
    fn a_saved_request_wired_before_its_wait_is_not_flagged() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment"), output("out")],
            vec![
                edge("e1", "r", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_saved_request_with_no_path_to_its_wait_is_one_warning_naming_both_labels() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment"), output("out")],
            vec![edge("e1", "w", handle::RESULT, "out", "value")],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("r"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'r'") && found[0].message.contains("'Pay wait'"));
        assert!(found[0].hint.is_some());
    }

    #[test]
    fn a_request_downstream_of_its_wait_is_flagged() {
        let f = flow(
            vec![wait_node("w", "Pay wait", "payment"), request("r"), output("out")],
            vec![
                edge("e1", "w", handle::RESULT, "r", handle::TRIGGER),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert_eq!(only(&lints, CALLBACK_NOT_WIRED).len(), 1);
    }

    #[test]
    fn a_request_reaching_its_wait_through_other_nodes_is_not_flagged() {
        let f = flow(
            vec![request("r1"), request("r2"), wait_node("w", "Pay wait", "payment")],
            vec![
                edge("e1", "r1", handle::RESULT, "r2", handle::TRIGGER),
                edge("e2", "r2", handle::RESULT, "w", handle::TRIGGER),
            ],
        );
        let lints = lint_with(&f, &sends("r1.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_request_that_does_not_use_the_variable_or_is_unknown_is_silent() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        assert!(only(&lint_with(&f, &sends("r.yml", &[])), CALLBACK_NOT_WIRED).is_empty());
        assert!(only(&lint_with(&f, &sends("other.yml", &["payment"])), CALLBACK_NOT_WIRED).is_empty());
        assert!(only(&lint(&f), CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn only_the_wait_without_a_path_is_named() {
        let f = flow(
            vec![
                request("r"),
                wait_node("w1", "First wait", "one"),
                wait_node("w2", "Second wait", "two"),
            ],
            vec![edge("e1", "r", handle::RESULT, "w1", handle::TRIGGER)],
        );
        let lints = lint_with(&f, &sends("r.yml", &["one", "two"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("'Second wait'"), "{}", found[0].message);
    }

    #[test]
    fn an_inline_request_is_left_to_the_validate_rule() {
        let f = flow(
            vec![inline_req("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        assert!(only(&lint_with(&f, &sends("r", &["payment"])), CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_cycle_does_not_hang_the_callback_rule() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            vec![
                edge("e1", "r", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "r", handle::TRIGGER),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn the_callback_lint_never_quotes_the_wait_name() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        let text = format!("{} {}", found[0].message, found[0].hint.as_deref().unwrap_or(""));
        assert!(!text.contains("payment") && !text.contains("callback."), "{text}");
    }
```

In `flow_lint_facts.rs` `mod tests` add (the `use` lines go with the existing ones):

```rust
    use rocket_shared::types::{Body, BodyMode, FormDataEntry, Header, PathParam, QueryParam};

    fn plain(url: &str) -> Request {
        Request::new("r", HttpMethod::Get, url)
    }

    #[test]
    fn a_callback_in_the_url_is_found() {
        let r = plain("{{callback.payment}}/hook");
        assert_eq!(request_callback_names(&r, &["payment", "other"]), vec!["payment"]);
    }

    #[test]
    fn spaces_inside_the_braces_are_allowed() {
        let r = plain("{{ callback.payment }}");
        assert_eq!(request_callback_names(&r, &["payment"]), vec!["payment"]);
    }

    #[test]
    fn headers_query_path_and_body_are_scanned() {
        let mut r = plain("https://example.test/x");
        r.headers.push(Header::new("X-Hook", "{{callback.a}}"));
        r.query_params.push(QueryParam {
            key: "cb".to_string(),
            value: "{{callback.b}}".to_string(),
            enabled: true,
            description: None,
        });
        r.path_params.push(PathParam {
            name: "p".to_string(),
            value: "{{callback.c}}".to_string(),
            description: None,
        });
        r.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"url":"{{callback.d}}"}"#.to_string()),
            form_data: None,
            file_path: None,
        });
        assert_eq!(
            request_callback_names(&r, &["a", "b", "c", "d", "e"]),
            vec!["a", "b", "c", "d"]
        );
    }

    #[test]
    fn form_data_is_scanned() {
        let mut r = plain("https://example.test/x");
        r.body = Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "cb".to_string(),
                value: "{{callback.a}}".to_string(),
                entry_type: rocket_shared::types::FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        assert_eq!(request_callback_names(&r, &["a"]), vec!["a"]);
    }

    #[test]
    fn disabled_fields_are_not_scanned() {
        let mut r = plain("https://example.test/x");
        let mut header = Header::new("X-Hook", "{{callback.a}}");
        header.enabled = false;
        r.headers.push(header);
        r.query_params.push(QueryParam {
            key: "cb".to_string(),
            value: "{{callback.a}}".to_string(),
            enabled: false,
            description: None,
        });
        assert!(request_callback_names(&r, &["a"]).is_empty());
    }

    #[test]
    fn a_longer_name_does_not_match_a_shorter_one() {
        let r = plain("{{callback.payment_done}}");
        assert!(request_callback_names(&r, &["payment"]).is_empty());
    }

    #[test]
    fn the_repo_helper_answers_none_when_it_cannot_read() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request("demo", "a.yml", plain("{{callback.payment}}"));
        assert_eq!(
            saved_request_callbacks(&*repo, "demo", "a.yml", &["payment"]),
            Some(vec!["payment".to_string()])
        );
        assert_eq!(saved_request_callbacks(&*repo, "demo", "missing.yml", &["payment"]), None);
    }
```

If `FormDataType` has other variant names, use the one the type defines (`grep -n "enum FormDataType" -A6 crates/rocket-shared/src/types.rs`).

In `flow_lint_context.rs` `mod tests` add:

```rust
    #[test]
    fn the_context_answers_which_callbacks_a_saved_request_sends() {
        use crate::test_doubles::ConfigurableCollectionRepo;
        use rocket_collection::Request;
        use rocket_shared::types::HttpMethod;

        let repo = ConfigurableCollectionRepo::new();
        repo.with_request(
            "demo",
            "hook.yml",
            Request::new("hook", HttpMethod::Post, "https://example.test/{{callback.payment}}"),
        );
        // Plan 1's test helpers: `sources`, `flow_of`, `inline_node` and `ENV` live in this module.
        let src = sources(&repo, None, None);
        let flow = flow_of(vec![inline_node("a")]);
        let ctx = CollectionLintContext::new(&src, "demo", ENV, &flow);
        assert_eq!(
            ctx.saved_request_callbacks("hook.yml", &["payment", "other"]),
            Some(vec!["payment".to_string()])
        );
        assert_eq!(ctx.saved_request_callbacks("gone.yml", &["payment"]), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint` expect compile FAILURE (`CALLBACK_NOT_WIRED` undefined).
Run: `cargo test -j4 -p rocket-app flow_lint` expect compile FAILURE (`request_callback_names` and `saved_request_callbacks` undefined; the context test fails to find the method override and would return the default `None`).

- [ ] **Step 3: Write the implementation**

3a. `lint.rs` constant:

```rust
/// A saved request sends a Wait for callback URL but has no path to that Wait.
pub const CALLBACK_NOT_WIRED: &str = "callback_not_wired";
```

Loop line (after Task 1's):

```rust
        lints.extend(callback_lints(node, &index, ctx));
```

Rule fn, placed after `auth_wire_lints`:

```rust
/// One lint per Wait for callback node whose `{{callback.<name>}}` a saved
/// request sends without a path to that Wait. The request registers the URL,
/// so it must run before the Wait starts (rule V15 checks inline requests).
/// Disabled fields and scripts are not scanned, so a lint here is certain.
fn callback_lints(node: &FlowNode, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> Vec<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return Vec::new();
    };
    if index.waits.is_empty() {
        return Vec::new();
    }
    let names: Vec<&str> = index.waits.iter().map(|(name, _)| *name).collect();
    let Some(used) = ctx.saved_request_callbacks(request_path, &names) else {
        return Vec::new();
    };
    if used.is_empty() {
        return Vec::new();
    }
    let downstream = index.downstream_of(&node.id);
    index
        .waits
        .iter()
        .filter(|(name, wait)| {
            used.iter().any(|u| u.as_str() == *name) && !downstream.contains(wait.id.as_str())
        })
        .map(|(_, wait)| {
            warning(
                CALLBACK_NOT_WIRED,
                node,
                format!(
                    "'{}' sends the callback URL of '{}', but it is not wired before that Wait for callback node.",
                    display_label(node),
                    display_label(wait)
                ),
                "Wire this request's 'result' exit into the Wait for callback node's 'trigger' input, so the URL is sent before the wait starts.",
            )
        })
        .collect()
}
```

3b. `validate.rs`: replace the sentence "Saved requests cannot be read in this pure crate, so they are skipped, and so are values fed in through wired fields." in the V15 doc comment with "Saved requests cannot be read in this pure crate, so the `callback_not_wired` lint in `lint.rs` covers them. Values fed in through wired fields are skipped." Change nothing else in that file.

3c. `flow_lint_facts.rs`: add imports and functions above the test module:

```rust
use rocket_flow::validate::mentions_callback;
```

```rust
/// The texts of a request that are sent to the server. Disabled headers,
/// query parameters and form entries are left out, because they are not sent.
fn sent_texts(request: &Request) -> Vec<&str> {
    let mut texts = vec![request.url.as_str()];
    for header in request.headers.iter().filter(|h| h.enabled) {
        texts.push(&header.key);
        texts.push(&header.value);
    }
    for param in request.query_params.iter().filter(|q| q.enabled) {
        texts.push(&param.key);
        texts.push(&param.value);
    }
    for param in &request.path_params {
        texts.push(&param.value);
    }
    if let Some(body) = &request.body {
        if let Some(content) = &body.content {
            texts.push(content);
        }
        for entry in body.form_data.iter().flatten().filter(|e| e.enabled) {
            texts.push(&entry.key);
            texts.push(&entry.value);
        }
    }
    texts
}

/// Which of `names` the request sends as `{{callback.<name>}}`, in the order
/// of `names`. Scripts and auth fields are not scanned.
pub(crate) fn request_callback_names(request: &Request, names: &[&str]) -> Vec<String> {
    let texts = sent_texts(request);
    names
        .iter()
        .filter(|name| texts.iter().any(|text| mentions_callback(text, name)))
        .map(|name| name.to_string())
        .collect()
}

/// Reads the saved request and answers `request_callback_names`. `None`
/// when the read fails for any reason.
pub(crate) fn saved_request_callbacks(
    repo: &dyn CollectionRepository,
    collection: &str,
    request_path: &str,
    names: &[&str],
) -> Option<Vec<String>> {
    repo.get_request(collection, request_path)
        .ok()
        .map(|request| request_callback_names(&request, names))
}
```

3d. `flow_lint_context.rs`:

```rust
    fn saved_request_callbacks(&self, request_path: &str, names: &[&str]) -> Option<Vec<String>> {
        // Adapt: pass the repository and collection fields Plan 1 holds.
        crate::flow_lint_facts::saved_request_callbacks(
            self.repo,
            self.collection,
            request_path,
            names,
        )
    }
```

- [ ] **Step 4: Run the tests to verify they pass, then the gates**

Run: `cargo test -j4 -p rocket-flow lint` and `cargo test -j4 -p rocket-flow validate` (the V15 tests are unchanged and PASS).
Run: `cargo test -j4 -p rocket-app flow_lint` and `cargo test -j4 -p rocket-app flow_service`.
Run: `cargo check -j4 -p rocket-flow`, `cargo check -j4 -p rocket-app`.
Run: `rustfmt --check --edition 2021 crates/rocket-app/src/flow_lint_facts.rs` and fix only hunks you wrote in `lint.rs` and `validate.rs`.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/lint.rs crates/rocket-flow/src/validate.rs crates/rocket-app/src/flow_lint_facts.rs crates/rocket-app/src/flow_lint_context.rs && git commit --only -m "feat(flow): lint saved requests that send a callback URL without a path to its wait" -- crates/rocket-flow/src/lint.rs crates/rocket-flow/src/validate.rs crates/rocket-app/src/flow_lint_facts.rs crates/rocket-app/src/flow_lint_context.rs`

---

### Task 3: Client pins and docs (F-24, F-25)

**Files:**
- Test: `src/lib/__tests__/flow-lint.test.ts` (codes test ~100-104, merge tests)
- Test: `src/lib/__tests__/flow-issues.test.ts` (`computeFlowIssues: rules the backend owns` ~173-190)
- Test: `src/components/flow/__tests__/FlowIssuesButton.test.tsx` (end of the describe)
- Modify: `crates/rocket-flow/CLAUDE.md` (Lint tier section)
- Modify: `docs/roadmaps/flow-roadmap.md` (rows F-24 and F-25, follow-up F-122)

**Interfaces:**
- Consumes: `BACKEND_LINT_CODES`, `mergeFlowIssues`, `toFlowIssue` (`src/lib/flow-lint.ts`), `computeFlowIssues` (`src/lib/flow-issues.ts`), `FlowIssuesButton`. There is no code-to-text map: the issue list shows `issue.message` as the backend sent it, and `useBackendFlowLints` passes every lint through `toFlowIssue`. So no mapping code is needed. `BACKEND_LINT_CODES` already holds `auth_no_effect`, `auth_wire_overrides_auth` and `callback_not_wired`, so these tests pin existing behaviour and are expected to pass on the first run.
- Produces: no new exports.

- [ ] **Step 1: Write the tests**

`flow-lint.test.ts`, inside the last `describe`, after "lists the codes the backend owns":

```ts
  it('lists the F-24 and F-25 codes', () => {
    expect(BACKEND_LINT_CODES).toEqual(
      expect.arrayContaining(['auth_no_effect', 'auth_wire_overrides_auth', 'callback_not_wired']),
    );
  });

  it('keeps an edge-bound auth lint and a node lint on the same request apart', () => {
    const overrides = issue({
      code: 'auth_wire_overrides_auth',
      nodeId: 'req',
      edgeId: 'e1',
      message: 'The credential of Sign in replaces the own auth of Get.',
    });
    const secondWire = issue({ ...overrides, edgeId: 'e2' });
    const callback = issue({ code: 'callback_not_wired', nodeId: 'req', message: 'Get sends it.' });
    const present = { nodeIds: new Set(['req']), edgeIds: new Set(['e1', 'e2']) };
    expect(mergeFlowIssues([], [overrides, secondWire, callback], present)).toEqual([
      overrides,
      secondWire,
      callback,
    ]);
  });

  it('drops an auth wire lint once its wire is deleted', () => {
    const overrides = issue({ code: 'auth_wire_overrides_auth', nodeId: 'req', edgeId: 'e1' });
    const present = { nodeIds: new Set(['req']), edgeIds: new Set<string>() };
    expect(mergeFlowIssues([], [overrides], present)).toEqual([]);
  });

  it('maps a backend auth wire lint without loss', () => {
    expect(
      toFlowIssue({
        code: 'auth_wire_overrides_auth',
        severity: 'warning',
        nodeId: 'req',
        edgeId: 'e1',
        message: 'm',
        hint: 'h',
      }),
    ).toEqual({
      code: 'auth_wire_overrides_auth',
      severity: 'warning',
      nodeId: 'req',
      edgeId: 'e1',
      message: 'm',
      hint: 'h',
    });
  });
```

`flow-issues.test.ts`, inside `describe('computeFlowIssues: rules the backend owns', ...)` after the existing test:

```ts
  it('does not report the auth and callback rules on the client', () => {
    const nodes = [
      node('a', {
        kind: 'Auth',
        label: 'Sign in',
        auth: { authType: 'bearer', token: 't' },
        applyToInherit: false,
      }),
      node('r', {
        kind: 'Request',
        label: 'Get',
        source: { type: 'Saved', requestPath: 'get.yml' },
      }),
      node('w', { kind: 'WaitForCallback', label: 'Hook', name: 'hook', timeoutMs: 60000 }),
      output('out'),
    ];
    const edges = [wire('e1', 'a', 'r', 'auth'), wire('e2', 'w', 'out', 'value')];
    const codes = computeFlowIssues(nodes, edges).map((i) => i.code);
    for (const code of ['auth_no_effect', 'auth_wire_overrides_auth', 'callback_not_wired']) {
      expect(codes).not.toContain(code);
    }
  });
```

`FlowIssuesButton.test.tsx`, before the closing `});` of the describe:

```tsx
  it('lists an auth wire lint under the request and selects the request', async () => {
    const authNodes: FlowNode[] = [
      {
        id: 'req',
        position: { x: 0, y: 0 },
        kind: { kind: 'Request', label: 'Get orders', source: { type: 'Saved', requestPath: 'o.yml' } },
      },
    ];
    const lints: FlowIssue[] = [
      {
        code: 'auth_wire_overrides_auth',
        severity: 'warning',
        nodeId: 'req',
        edgeId: 'e1',
        message: "The credential of 'Sign in' replaces the own auth of 'Get orders'.",
      },
    ];
    const onSelectNode = vi.fn();
    render(<FlowIssuesButton issues={lints} nodes={authNodes} onSelectNode={onSelectNode} />);
    await userEvent.click(screen.getByRole('button', { name: '1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    expect(list).toHaveTextContent('replaces the own auth');
    await userEvent.click(within(list).getByRole('button'));
    await waitFor(() => expect(onSelectNode).toHaveBeenCalledWith('req'));
  });
```

If the trigger's accessible name for one warning differs from `1 warning`, read it from `issueCountLabel` in `flow-issues.ts` and use that text.

- [ ] **Step 2: Run the tests**

Run: `yarn test flow-lint flow-issues FlowIssuesButton --run --maxWorkers=2`
Expected: all PASS on the first run, because these tests pin behaviour that already exists. To prove the code-list test can fail, remove `'callback_not_wired'` from `BACKEND_LINT_CODES` for one run and see it fail, then restore it.

- [ ] **Step 3: Update the docs (no code)**

`crates/rocket-flow/CLAUDE.md`, in the Lint tier section: add to the rules list "`auth_no_effect` (an Auth node with no `auth` wire that applies to no Request: not applying, or applying while every Request has an `auth` wire or explicit saved auth; a saved request the context cannot read counts as taking it), `auth_wire_overrides_auth` (an `auth` wire into a saved request whose own auth is explicit; the wire wins at run time and replaces it; one lint per wire, with `edge_id`), `callback_not_wired` (a saved request sends `{{callback.<name>}}` but has no path to the Wait named `<name>`, the V15 direction; inline requests stay with V15)". Replace the last sentence "No rule uses it yet (F-21, F-22)." with a sentence that lists the `LintContext` methods now in use: `saved_request_auth_is_explicit` and `saved_request_callbacks` (F-24, F-25), plus whatever Plan 1 added for F-21 and F-22. Add one line: "The app layer computes the saved-request facts in `rocket-app` `flow_lint_facts.rs`; effective auth is `runtime_auth` else `auth`, and disabled fields are not scanned."

`docs/roadmaps/flow-roadmap.md`: set F-24 and F-25 status to `done` and put the date, the commit hashes from Tasks 1 and 2, and these notes in the last column. F-24: "Codes `auth_no_effect`, `auth_wire_overrides_auth` in `lint.rs`; facts in `flow_lint_facts.rs`. The wire replaces the request's own auth (engine: wire wins). Unknown saved request never warns." F-25: "Code `callback_not_wired`. Saved requests only; inline stays V15. Run-start engine check not built (see Plan 2 open decisions)." In follow-up F-122 remove F-24 and F-25 from the "still open" list. Do not touch other rows.

- [ ] **Step 4: Run the gates**

Run: `yarn tsc --noEmit` and `yarn check`. Both must be clean. Docs need no gate.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add src/lib/__tests__/flow-lint.test.ts src/lib/__tests__/flow-issues.test.ts src/components/flow/__tests__/FlowIssuesButton.test.tsx crates/rocket-flow/CLAUDE.md docs/roadmaps/flow-roadmap.md && git commit --only -m "test(flow): pin the auth and callback lint codes on the client and document the rules" -- src/lib/__tests__/flow-lint.test.ts src/lib/__tests__/flow-issues.test.ts src/components/flow/__tests__/FlowIssuesButton.test.tsx crates/rocket-flow/CLAUDE.md docs/roadmaps/flow-roadmap.md`

(`docs/roadmaps/flow-roadmap.md` is untracked on `main` at the time of writing. If it is still untracked, leave it out of the `git add` and the pathspec and tell the user the roadmap edit stays in the working tree.)

---

## Open decisions

1. **Order direction of the callback lint.** The brief said the request must run after the Wait. The engine and V15 say the opposite: the request registers the URL and must be an ancestor of the Wait. Recommended default: follow V15 (as written above).
2. **Run-start engine check for callback ordering.** The roadmap says the engine can also check it at run start. Recommended default: out of scope. The lint already covers saved requests at edit time, and a run-start check would need the engine to read every saved request before the first node and decide whether to fail a run that V15 and the lint only warn about for saved requests. If wanted, make it a separate item that reuses `request_callback_names`.
3. **Scanned fields.** Url, enabled headers, enabled query params, path params, body content and enabled form entries. Not scanned: scripts, auth fields, folder and collection headers, GraphQL, gRPC and WebSocket requests. Recommended default: keep this list, because it never gives a false positive, and note the gap in the CLAUDE.md line. Scripts could be added later by scanning for the literal `callback.` text.
4. **Wording of the auth wire lint.** "Replaces" is exact: the engine sets the request auth from the wire after any other step. Recommended default: warning severity, wire-level (`edge_id` set, `node_id` the request) so the canvas can mark the wire and the issue list can select the request.
5. **Unknown means silent.** A saved request the context cannot read counts as "might inherit" for `auth_no_effect` and as "not explicit" for the wire lint. Recommended default: yes; a dangling path is already reported by F-21.
6. **Folder and collection auth.** A saved request with `none` or `inherit` is treated as inheriting even when the collection has no auth at all, because the flow credential still wins over nothing. Recommended default: yes, same as the engine.
7. **Repeated reads.** One lint call may read a saved request up to three times (exists, auth, callbacks). Recommended default: accept, since lint runs debounced at 500 ms; add a per-call cache inside `CollectionLintContext` only if profiling shows a cost.
8. **Lint cost.** `callback_lints` does one forward walk per saved request that uses a callback, so it is not strictly linear. Recommended default: accept, the walk runs only for such requests.

## Self-review

- Spec coverage: F-24 is Task 1 (both lints), F-25 is Task 2, the client check, the CLAUDE.md rules list and the roadmap notes are Task 3. The run-start check is recorded as an open decision.
- Placeholders: none. The only open spots are the `// Adapt:` lines and the "Plan 1's names" step, which exist because Plan 1 was not visible; each names exactly what to substitute.
- Name consistency: `AUTH_NO_EFFECT`, `AUTH_WIRE_OVERRIDES_AUTH`, `CALLBACK_NOT_WIRED`, `saved_request_auth_is_explicit`, `saved_request_callbacks`, `request_auth_is_explicit`, `request_callback_names`, `GraphIndex::{nodes_by_id, auth_wires_into, auth_sources, successors, waits, downstream_of}`, `AuthCallbackContext`, `inline_req`, `wait_node` and `sends` are spelled the same in every task. The three code strings match `BACKEND_LINT_CODES` in `src/lib/flow-lint.ts`.
- Review Focus: each line maps to a named test in Tasks 1 to 3.
- Task 1 changes one existing test (`an_auth_node_that_applies_to_inherited_auth_is_not_flagged`), because its idle Auth node now also gets `auth_no_effect`.

## Next plan to execute

`03-hard-errors.md`.
