# Flow Auth Node — Plan 1: Domain, IPC shape, TS type

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 7.** Previous plan: none (start here).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-02-credentials.md`**
**Recommended model: Sonnet.**

**Goal:** Add a `FlowNodeKind::Auth` node to the flow domain, validate it, persist it, carry it over IPC, and type it in the frontend — without any run-time behavior yet.

**Architecture:** `rocket-flow` gets the new node variant, an `auth` handle name and three validation rules. `rocket-infra` persists it with no code change (serde). `src-tauri` gets a DTO variant. `rocket-app` gets a one-line placeholder arm so the workspace compiles (Plan 3 replaces it). The frontend gets the TypeScript type and the exhaustive-switch fixes so `yarn tsc` stays green.

**Tech Stack:** Rust (serde, serde_yaml), Tauri v2 IPC, TypeScript.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- A flow has at most one Auth node with `apply_to_inherit = true`.
- Tokens are never written to disk: the node stores auth *configuration* only.
- No `unwrap()` in production Rust paths; never shell out to `git`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on persistence structs.
- Persistence is `.yml` only.
- Conventional commits (`feat:`, `fix:`, `test:`, `chore:`).
- Before each commit run `cargo fmt` (the plan's code is not rustfmt-checked) and re-run the task's tests.
- If `yarn check` reports formatting or import-order findings, run `yarn lint` (auto-fix), then re-run `yarn check`.
- Verification per task: `cargo check`, focused `cargo test`, `yarn tsc --noEmit` when TS changes.

## File Structure

| File | Change |
|---|---|
| `crates/rocket-flow/src/node.rs` | Add `FlowNodeKind::Auth`, `default_true` |
| `crates/rocket-flow/src/handle.rs` | Add `AUTH` handle constant |
| `crates/rocket-flow/src/validate.rs` | Exhaustive-match fixes, rules V11–V13, tests |
| `crates/rocket-infra/src/fs_flow_repo.rs` | YAML round-trip test |
| `src-tauri/src/commands/flow.rs` | `FlowNodeKindDto::Auth` + conversions + test |
| `crates/rocket-app/src/flow_execution_service.rs` | Placeholder `Auth` arm in `execute_node` |
| `src/lib/tauri-api.ts` | `Auth` variant of `FlowNodeKind` |
| `src/lib/flow-handles.ts` | `AUTH_HANDLE` |
| `src/lib/flow-wiring.ts` | Handle rules for `Auth` |
| `src/components/flow/properties/NodePropertiesPanel.tsx` | Placeholder `Auth` case |

---

### Task 1: `rocket-flow` — node variant, handle, validation

**Files:**
- Modify: `crates/rocket-flow/src/node.rs`
- Modify: `crates/rocket-flow/src/handle.rs`
- Modify: `crates/rocket-flow/src/validate.rs`

**Interfaces:**
- Produces (used by Plans 2–5):
  - `FlowNodeKind::Auth { label: String, auth: rocket_shared::types::Auth, apply_to_inherit: bool }`
  - `rocket_flow::handle::AUTH: &str = "auth"` (the `target_field` of an Auth→Request wire)

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 3, Auth types). The node reuses the existing `Auth` enum unchanged.

- [ ] **Step 2: Write the failing node tests**

Append inside `mod tests` of `crates/rocket-flow/src/node.rs` (before its closing `}`):

```rust
    #[test]
    fn flow_node_kind_auth_tagged_roundtrip() {
        let kind = FlowNodeKind::Auth {
            label: "Sign in".to_string(),
            auth: rocket_shared::types::Auth::Bearer {
                token: "{{token}}".to_string(),
            },
            apply_to_inherit: false,
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Auth\""), "got: {json}");
        assert!(json.contains("\"authType\":\"bearer\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_auth_defaults_apply_to_inherit_to_true() {
        let json = r#"{"kind":"Auth","label":"Sign in","auth":{"authType":"basic","username":"u","password":"p"}}"#;
        let kind: FlowNodeKind = serde_json::from_str(json).expect("deserialize FlowNodeKind");
        match kind {
            FlowNodeKind::Auth {
                apply_to_inherit, ..
            } => assert!(apply_to_inherit),
            other => panic!("expected an Auth node, got {other:?}"),
        }
    }
```

Append inside `mod tests` of `crates/rocket-flow/src/handle.rs`:

```rust
    #[test]
    fn auth_handle_name_matches_the_spec() {
        assert_eq!(AUTH, "auth");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-flow -j4 auth`
Expected: compile error (`no variant named Auth`, `cannot find value AUTH`).

- [ ] **Step 4: Add the handle constant**

In `crates/rocket-flow/src/handle.rs`, after the `TRIGGER` constant, add:

```rust
/// The `target_field` of a wire from an Auth node into a Request node. The
/// wire sets the request's auth. It carries no expression.
pub const AUTH: &str = "auth";
```

- [ ] **Step 5: Add the node variant**

In `crates/rocket-flow/src/node.rs`, add this variant at the end of `FlowNodeKind` (after `Transform { .. }`):

```rust
    /// Authenticates once per run and hands the credential to requests. It has
    /// no inputs and one exit, `result`. Only the auth *configuration* is
    /// stored; a token is never written to the flow file.
    Auth {
        label: String,
        /// Any auth type except `none` and `inherit`.
        auth: rocket_shared::types::Auth,
        /// When true (the default), every Request node whose auth is
        /// `inherit` uses this node's credential. At most one Auth node in a
        /// flow may have this on.
        #[serde(default = "default_true")]
        apply_to_inherit: bool,
    },
```

Update the doc comment above `FlowNodeKind` by appending the sentence: `` `Auth` nodes authenticate once per run and supply the credential to requests.``

Below the `is_false` function add:

```rust
fn default_true() -> bool {
    true
}
```

- [ ] **Step 6: Fix the exhaustive matches and add the rules in `validate.rs`**

In `kind_name`, add the arm `FlowNodeKind::Auth { .. } => "Auth",`.

In `source_handle_exists`, add `| FlowNodeKind::Auth { .. }` to the arm that returns `source_handle == handle::RESULT`, so it reads:

```rust
        FlowNodeKind::Request { .. }
        | FlowNodeKind::Input { .. }
        | FlowNodeKind::WaitForCallback { .. }
        | FlowNodeKind::Transform { .. }
        | FlowNodeKind::Auth { .. } => source_handle == handle::RESULT,
```

In `validate`, directly after the rule that rejects wires into `Input` nodes (`"Input nodes cannot receive wires"`), insert:

```rust
    check_edges(flow, &kinds, |_, target, _| {
        matches!(target, FlowNodeKind::Auth { .. })
            .then(|| "Auth nodes cannot receive wires".to_string())
    })?;
    check_edges(flow, &kinds, |edge, target, source| {
        let from_auth = matches!(source, FlowNodeKind::Auth { .. });
        let into_request = matches!(target, FlowNodeKind::Request { .. });
        (edge.target_field == handle::AUTH && !(from_auth && into_request)).then(|| {
            format!(
                "an '{}' wire must go from an Auth node into a Request node",
                handle::AUTH
            )
        })
    })?;
```

At the end of `validate`, after `check_wait_nodes(flow)?;`, add `check_auth_nodes(flow)?;`.

Add this function after `check_wait_nodes`:

```rust
/// V13: an Auth node holds a concrete auth type, and at most one Auth node
/// applies to inherited auth.
fn check_auth_nodes(flow: &Flow) -> Result<(), FlowGraphError> {
    use rocket_shared::types::Auth;
    let mut applying = false;
    for node in &flow.nodes {
        let FlowNodeKind::Auth {
            auth,
            apply_to_inherit,
            ..
        } = &node.kind
        else {
            continue;
        };
        if matches!(auth, Auth::None | Auth::Inherit) {
            return Err(invalid_node(
                node,
                "the Auth node needs an auth type other than none or inherit".to_string(),
            ));
        }
        if *apply_to_inherit {
            if applying {
                return Err(invalid_node(
                    node,
                    "only one Auth node can apply to inherited auth; turn this one or the other off"
                        .to_string(),
                ));
            }
            applying = true;
        }
    }
    Ok(())
}
```

Update the module comment at the top of the file: change `(rules V1-V10)` to `(rules V1-V13)`.

- [ ] **Step 7: Add the validation tests**

Append inside `mod tests` of `validate.rs`:

```rust
    fn auth_node(id: &str, auth: rocket_shared::types::Auth, apply_to_inherit: bool) -> FlowNode {
        node(
            id,
            FlowNodeKind::Auth {
                label: id.to_string(),
                auth,
                apply_to_inherit,
            },
        )
    }

    fn bearer() -> rocket_shared::types::Auth {
        rocket_shared::types::Auth::Bearer {
            token: "{{token}}".to_string(),
        }
    }

    #[test]
    fn an_auth_node_alone_is_valid() {
        let f = flow(vec![auth_node("a", bearer(), true)], vec![]);
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn an_auth_wire_into_a_request_is_valid() {
        let f = flow(
            vec![auth_node("a", bearer(), true), request("r")],
            vec![edge("e1", "a", handle::RESULT, "r", handle::AUTH)],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn an_auth_node_may_feed_a_header_wire() {
        let f = flow(
            vec![auth_node("a", bearer(), true), request("r")],
            vec![edge(
                "e1",
                "a",
                handle::RESULT,
                "r",
                "headers[Authorization].value",
            )],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn an_auth_node_cannot_receive_a_wire() {
        let f = flow(
            vec![request("a"), auth_node("x", bearer(), false)],
            vec![edge("e1", "a", handle::RESULT, "x", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn an_auth_wire_must_come_from_an_auth_node() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::AUTH)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn an_auth_wire_must_end_at_a_request() {
        let f = flow(
            vec![auth_node("a", bearer(), true), output("o")],
            vec![edge("e1", "a", handle::RESULT, "o", handle::AUTH)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn an_auth_node_has_only_a_result_exit() {
        let f = flow(
            vec![auth_node("a", bearer(), true), request("r")],
            vec![edge("e1", "a", handle::TRUE, "r", handle::TRIGGER)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn two_auth_nodes_cannot_both_apply_to_inherit() {
        let f = flow(
            vec![
                auth_node("a", bearer(), true),
                auth_node("b", bearer(), true),
            ],
            vec![],
        );
        assert_eq!(invalid_node_id(validate(&f)), "b");
    }

    #[test]
    fn two_auth_nodes_may_coexist_when_only_one_applies() {
        let f = flow(
            vec![
                auth_node("a", bearer(), true),
                auth_node("b", bearer(), false),
            ],
            vec![],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn an_auth_node_needs_a_concrete_auth_type() {
        use rocket_shared::types::Auth;
        for auth in [Auth::None, Auth::Inherit] {
            let f = flow(vec![auth_node("a", auth, false)], vec![]);
            assert_eq!(invalid_node_id(validate(&f)), "a");
        }
    }
```

- [ ] **Step 8: Run the crate tests**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS, including the 10 new validation tests and 3 new node/handle tests.

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-flow/src/node.rs crates/rocket-flow/src/handle.rs crates/rocket-flow/src/validate.rs
git commit -m "feat(flow): add Auth node kind with validation rules"
```

---

### Task 2: Persistence and IPC DTO

**Files:**
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (tests only)
- Modify: `src-tauri/src/commands/flow.rs`

**Interfaces:**
- Consumes: `FlowNodeKind::Auth` from Task 1.
- Produces: IPC JSON shape `{"kind":"Auth","label":…,"auth":{…authType…},"applyToInherit":bool}` (used by Plans 4–6).

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing YAML round-trip test**

In `crates/rocket-infra/src/fs_flow_repo.rs`, add inside `mod tests` after `transform_node_roundtrips_a_multiline_script`:

```rust
    #[test]
    fn auth_node_roundtrips_through_yaml_without_a_token_field() {
        use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
        use rocket_shared::types::Auth;

        let (_dir, repo) = setup();
        let mut flow = sample("Auth Flow");
        flow.nodes.push(FlowNode {
            id: "a1".to_string(),
            kind: FlowNodeKind::Auth {
                label: "Sign in".to_string(),
                auth: Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                    access_token_url: "https://idp.example.com/token".to_string(),
                    refresh_token_url: None,
                    credentials: OAuth2ClientCredentials {
                        client_id: "{{clientId}}".to_string(),
                        client_secret: "{{clientSecret}}".to_string(),
                        placement: None,
                    },
                    scope: Some("read".to_string()),
                    additional_parameters: None,
                    token_config: None,
                    settings: None,
                })),
                apply_to_inherit: true,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Auth Flow").expect("get"), flow);

        let raw = serde_yaml::to_string(&flow).expect("serialize flow to yaml");
        assert!(!raw.to_lowercase().contains("accesstoken:"), "got: {raw}");
        assert!(raw.contains("{{clientSecret}}"), "got: {raw}");
    }
```

- [ ] **Step 3: Run it**

Run: `cargo test -p rocket-infra -j4 auth_node_roundtrips`
Expected: PASS (persistence is plain serde, so no production change is needed). If it fails to deserialize, the cause is serde's handling of the nested internally tagged enums; report the error instead of changing `Auth`.

- [ ] **Step 4: Write the failing DTO test**

In `src-tauri/src/commands/flow.rs`, add inside `mod tests` after `transform_node_dto_keeps_tag_and_roundtrips`:

```rust
    #[test]
    fn auth_node_dto_uses_camel_case_and_roundtrips() {
        let kind = FlowNodeKind::Auth {
            label: "Sign in".to_string(),
            auth: rocket_shared::types::Auth::Bearer {
                token: "t".to_string(),
            },
            apply_to_inherit: false,
        };
        let dto: FlowNodeKindDto = kind.clone().into();
        let json = serde_json::to_string(&dto).expect("serialize FlowNodeKindDto");
        assert!(json.contains(r#""kind":"Auth""#), "got: {json}");
        assert!(json.contains(r#""applyToInherit":false"#), "got: {json}");
        assert!(json.contains(r#""authType":"bearer""#), "got: {json}");
        let back: FlowNodeKindDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(FlowNodeKind::from(back), kind);
    }

    #[test]
    fn auth_node_dto_defaults_apply_to_inherit_to_true() {
        let json = r#"{"kind":"Auth","label":"Sign in","auth":{"authType":"basic","username":"u","password":"p"}}"#;
        let dto: FlowNodeKindDto = serde_json::from_str(json).expect("deserialize");
        match FlowNodeKind::from(dto) {
            FlowNodeKind::Auth {
                apply_to_inherit, ..
            } => assert!(apply_to_inherit),
            other => panic!("expected an Auth node, got {other:?}"),
        }
    }
```

- [ ] **Step 5: Run it to verify it fails**

Run: `cargo test -p rocket auth_node_dto` (the `src-tauri` package is named `rocket`).
Expected: compile error (`no variant named Auth` on `FlowNodeKindDto`).

- [ ] **Step 6: Add the DTO variant and conversions**

In `src-tauri/src/commands/flow.rs`, add this variant at the end of `FlowNodeKindDto` (after `Transform { .. }`):

```rust
    Auth {
        label: String,
        auth: rocket_shared::types::Auth,
        #[serde(default = "default_true")]
        apply_to_inherit: bool,
    },
```

Above `FlowNodeKindDto`'s definition (or anywhere at module level) add:

```rust
fn default_true() -> bool {
    true
}
```

In `impl From<FlowNodeKind> for FlowNodeKindDto`, add before the closing of the `match`:

```rust
            FlowNodeKind::Auth {
                label,
                auth,
                apply_to_inherit,
            } => FlowNodeKindDto::Auth {
                label,
                auth,
                apply_to_inherit,
            },
```

In `impl From<FlowNodeKindDto> for FlowNodeKind`, add:

```rust
            FlowNodeKindDto::Auth {
                label,
                auth,
                apply_to_inherit,
            } => FlowNodeKind::Auth {
                label,
                auth,
                apply_to_inherit,
            },
```

- [ ] **Step 7: Run the DTO tests**

Run the command from Step 5.
Expected: both `auth_node_dto_*` tests PASS. (`rocket-app` does not compile yet if you run the whole workspace; Task 3 fixes that.)

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/src/fs_flow_repo.rs src-tauri/src/commands/flow.rs
git commit -m "feat(flow): persist and transport the Auth node"
```

---

### Task 3: Compile placeholder and frontend typing

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (the `execute_node` match)
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/flow-handles.ts`
- Modify: `src/lib/flow-wiring.ts`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx`
- Test: `src/lib/__tests__/flow-wiring.test.ts`

**Interfaces:**
- Consumes: IPC shape from Task 2.
- Produces: TS `FlowNodeKind` member `{ kind: 'Auth'; label: string; auth: Auth; applyToInherit: boolean }`; `AUTH_HANDLE = 'auth'` in `flow-handles.ts`. Plan 3 replaces the Rust placeholder arm; Plan 5 replaces the panel placeholder.

- [ ] **Step 1: Add the Rust placeholder arm**

In `execute_node` in `crates/rocket-app/src/flow_execution_service.rs`, add this arm just before the closing brace of the `match &node.kind` (after the `FlowNodeKind::Transform { .. }` arm):

```rust
            // Replaced by flow-auth-node plan 03 (executor).
            FlowNodeKind::Auth { label, .. } => Err(DomainError::InvalidInput(format!(
                "Auth node '{label}' cannot run yet"
            ))),
```

- [ ] **Step 2: Check the whole workspace compiles**

Run: `cargo check -j4`
Expected: PASS. If the compiler reports another non-exhaustive `match` on `FlowNodeKind`, add an arm that treats `Auth` like `Transform` for that match and note it in the commit message.

- [ ] **Step 3: Write the failing frontend tests**

Append to `src/lib/__tests__/flow-wiring.test.ts` (reuse the file's existing imports; add `AUTH_HANDLE` to the `@/lib/flow-handles` import if the file imports from there, otherwise add `import { AUTH_HANDLE } from '@/lib/flow-handles';`):

```ts
describe('Auth node wiring', () => {
  const authNode: FlowNode = {
    id: 'a',
    kind: {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: true,
    },
    position: { x: 0, y: 0 },
  };
  const requestNode: FlowNode = {
    id: 'r',
    kind: {
      kind: 'Request',
      label: 'Get',
      source: { type: 'Saved', requestPath: 'x.yml' },
    },
    position: { x: 0, y: 0 },
  };

  it('lets an Auth node feed a Request auth handle', () => {
    expect(
      isValidFlowConnection(
        { source: 'a', target: 'r', sourceHandle: 'result', targetHandle: AUTH_HANDLE },
        [authNode, requestNode],
        [],
      ),
    ).toBe(true);
  });

  it('does not let anything wire into an Auth node', () => {
    expect(
      isValidFlowConnection(
        { source: 'r', target: 'a', sourceHandle: 'result', targetHandle: 'url' },
        [authNode, requestNode],
        [],
      ),
    ).toBe(false);
  });
});
```

If the file does not already import `isValidFlowConnection` and `FlowNode`, add `import { isValidFlowConnection } from '@/lib/flow-wiring';` and `import type { FlowNode } from '@/lib/tauri-api';`.

- [ ] **Step 4: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-wiring.test.ts`
Expected: FAIL (type errors or `AUTH_HANDLE` missing).

- [ ] **Step 5: Add the TS type and handle**

In `src/lib/tauri-api.ts`, add this member to the end of the `FlowNodeKind` union (after the `Transform` member, before the `;`):

```ts
  | {
      kind: 'Auth';
      label: string;
      /** The auth configuration. A token is never stored here. */
      auth: Auth;
      /** When true, every Request in the flow whose auth is inherit uses this credential. */
      applyToInherit: boolean;
    }
```

In `src/lib/flow-handles.ts`, after `TRIGGER_HANDLE`, add:

```ts
export const AUTH_HANDLE = 'auth';
```

In `src/lib/flow-wiring.ts`:
- add `AUTH_HANDLE,` to the import list from `@/lib/flow-handles`;
- in `sourceHandleExists`, add `case 'Auth':` next to `case 'Transform':` (it returns `handle === RESULT_HANDLE`);
- change `const REQUEST_TARGETS = ['url', 'headers', 'body', TRIGGER_HANDLE];` to `['url', 'headers', 'body', TRIGGER_HANDLE, AUTH_HANDLE]`;
- in `targetAccepts`, change `case 'Input':` to `case 'Input':\n    case 'Auth':` (both return `false`);
- make an `auth` wire dataless: change `isDataLessTarget` to
  `return targetHandle === INPUT_HANDLE || targetHandle === TRIGGER_HANDLE || targetHandle === AUTH_HANDLE;`
- inside `isValidFlowConnection`, directly after the `targetAccepts` check, add:

```ts
  // An auth wire comes only from an Auth node, and carries one credential per request.
  if (targetHandle === AUTH_HANDLE) {
    if (sourceNode.kind.kind !== 'Auth') return false;
    if (edges.some((e) => e.targetNodeId === target && e.targetField === AUTH_HANDLE)) return false;
  }
```

- [ ] **Step 6: Add the properties-panel placeholder**

In `src/components/flow/properties/NodePropertiesPanel.tsx`, add this case after `case 'Transform':` (before the closing brace of the `switch`):

```tsx
    case 'Auth':
      // Replaced by flow-auth-node plan 05 (frontend node).
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
```

- [ ] **Step 7: Run the checks**

Run: `yarn test src/lib/__tests__/flow-wiring.test.ts && yarn tsc --noEmit && yarn check`
Expected: all PASS. If `tsc` reports another non-exhaustive switch over `kind.kind`, add the `Auth` case beside `Transform`.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs src/lib/tauri-api.ts src/lib/flow-handles.ts src/lib/flow-wiring.ts src/lib/__tests__/flow-wiring.test.ts src/components/flow/properties/NodePropertiesPanel.tsx
git commit -m "feat(flow): type the Auth node in the frontend and keep the backend compiling"
```

---

**End of Plan 1.** Verify: `cargo check -j4`, `cargo test -p rocket-flow -j4`, `yarn tsc --noEmit`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-02-credentials.md`** (backend credential resolution and token fetcher).
