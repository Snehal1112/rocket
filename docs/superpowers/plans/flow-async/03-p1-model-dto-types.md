# Flow Async P1 — Repeat Until Model, Validation, DTO and Types — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the optional `repeat_until` setting to Request nodes end to end in the data layer: domain model, save-time validation, flow file round-trip, IPC DTO and TypeScript types.

**Architecture:** `RepeatUntil` is a plain value struct in `rocket-flow` carried as `Option<RepeatUntil>` on `FlowNodeKind::Request`, serialized with `skip_serializing_if` so old flow files stay byte-identical. Validation adds one rule (V9) to `rocket_flow::validate`. `src-tauri` mirrors it with a camelCase `RepeatUntilDto`, and `src/lib/tauri-api.ts` mirrors the DTO. No execution change in this plan (plan 04 does that).

**Tech Stack:** Rust (serde, serde_yaml), Tauri 2 IPC, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` (§6.1, §6.2, §6.5). Index and locked contract: `docs/superpowers/plans/flow-async/00-index.md`.

## Global Constraints

- `repeat_until` is omitted from saved files when `None` (`#[serde(default, skip_serializing_if = "Option::is_none")]`). A flow without it re-saves byte-identically.
- Defaults: condition `response.status === 200`, `interval_ms` 2000, `max_attempts` 30, `timeout_ms` 60000.
- Limits: `interval_ms >= 100`, `1 <= max_attempts <= 1000`, `timeout_ms <= 3_600_000`, `timeout_ms >= interval_ms`, condition not blank.
- Persistence structs are snake_case; only IPC DTOs get `#[serde(rename_all = "camelCase")]`.
- Cargo commands use `-j4` and target one crate. Never run the full workspace test suite.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill.
- Never write the literal panicking-unwrap call text in any file (a write hook blocks it). Tests use `expect`.

## Review Focus

1. **A flow saved before this feature is loaded and re-saved.** Expected: the file is byte-identical, with no `repeat_until` key. Pinned in Task 1 (`fs_repo_resaves_request_without_repeat_until_byte_identically`).
2. **A flow file with `repeat_until` but missing a field (hand-edited).** Expected: load fails with a clear serde error, not a silent default for one field. Pinned in Task 1 (`repeat_until_missing_a_field_is_rejected`).
3. **A timeout shorter than the interval.** Expected: save is rejected naming the node. Pinned in Task 2 (`repeat_until_timeout_shorter_than_interval_is_rejected`).
4. **An old frontend payload without `repeatUntil`.** Expected: the DTO deserializes with `None`. Pinned in Task 3 (`request_repeat_until_converts_both_ways_and_defaults_to_none`).
5. **`max_attempts: 0`.** Expected: rejected at save, because the loop would never send. Pinned in Task 2 (`repeat_until_zero_attempts_is_rejected`).

---

### Task 1: `RepeatUntil` model and flow file round-trip

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/node.rs` (`FlowNodeKind::Request` at :18-24, new `RepeatUntil` struct after `SwitchCase`, tests module)
- Modify: `crates/rocket-flow/src/lib.rs:9-11` (re-export `RepeatUntil`)
- Modify (add `..` or `repeat_until: None` to every `FlowNodeKind::Request { … }` literal/pattern the compiler flags): `crates/rocket-flow/src/flow.rs`, `crates/rocket-flow/src/validate.rs`, `crates/rocket-app/src/flow_execution_service.rs`, `crates/rocket-infra/src/fs_flow_repo.rs`, `src-tauri/src/commands/flow.rs`
- Test: `crates/rocket-flow/src/node.rs` (tests), `crates/rocket-infra/src/fs_flow_repo.rs` (tests)

**Interfaces:**
- Consumes: nothing new.
- Produces: `rocket_flow::RepeatUntil { condition: String, interval_ms: u64, max_attempts: u32, timeout_ms: u64 }` with the `DEFAULT_*`, `MIN_INTERVAL_MS`, `MAX_MAX_ATTEMPTS`, `MAX_TIMEOUT_MS` associated consts and `impl Default`; `FlowNodeKind::Request { label, source, debug, repeat_until: Option<RepeatUntil> }`.

- [ ] **Step 1: Write the failing model tests**

Append to the `tests` module in `crates/rocket-flow/src/node.rs`:

```rust
    #[test]
    fn repeat_until_default_uses_the_spec_values() {
        let r = RepeatUntil::default();
        assert_eq!(r.condition, "response.status === 200");
        assert_eq!(r.interval_ms, 2000);
        assert_eq!(r.max_attempts, 30);
        assert_eq!(r.timeout_ms, 60_000);
    }

    #[test]
    fn request_without_repeat_until_omits_the_key() {
        let kind = FlowNodeKind::Request {
            label: "Get".to_string(),
            source: RequestSource::Saved {
                request_path: "a.yml".to_string(),
            },
            debug: false,
            repeat_until: None,
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(!yaml.contains("repeat_until"), "got: {yaml}");
    }

    #[test]
    fn request_with_repeat_until_roundtrips_in_snake_case() {
        let kind = FlowNodeKind::Request {
            label: "Poll job".to_string(),
            source: RequestSource::Saved {
                request_path: "jobs/get-job.yml".to_string(),
            },
            debug: false,
            repeat_until: Some(RepeatUntil {
                condition: "response.body.status === \"done\"".to_string(),
                interval_ms: 2000,
                max_attempts: 30,
                timeout_ms: 60_000,
            }),
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(yaml.contains("repeat_until:"), "got: {yaml}");
        assert!(yaml.contains("interval_ms: 2000"), "got: {yaml}");
        assert!(yaml.contains("max_attempts: 30"), "got: {yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back, kind);
    }

    #[test]
    fn repeat_until_missing_a_field_is_rejected() {
        let yaml = "kind: Request\nlabel: P\nsource:\n  type: Saved\n  request_path: a.yml\nrepeat_until:\n  condition: x\n  interval_ms: 100\n  max_attempts: 3\n";
        let err = serde_yaml::from_str::<FlowNodeKind>(yaml).expect_err("timeout_ms is required");
        assert!(err.to_string().contains("timeout_ms"), "got: {err}");
    }
```

Check `crates/rocket-flow/Cargo.toml` has `serde_yaml` under `[dev-dependencies]`. If it does not, add `serde_yaml.workspace = true` there (it is a workspace dependency used by `rocket-infra`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow repeat_until`
Expected: FAIL to compile with "cannot find type `RepeatUntil`" and "struct variant `FlowNodeKind::Request` has no field named `repeat_until`".

- [ ] **Step 3: Implement the model**

In `crates/rocket-flow/src/node.rs`, change the `Request` variant:

```rust
    Request {
        label: String,
        source: RequestSource,
        /// When true, a run reports the request as sent and its response.
        #[serde(default, skip_serializing_if = "is_false")]
        debug: bool,
        /// When set, the request is sent again until `condition` holds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repeat_until: Option<RepeatUntil>,
    },
```

Add after `SwitchCase`:

```rust
/// Polling settings of a Request node. The node sends its request until
/// `condition` is truthy, pausing `interval_ms` between attempts, and gives
/// up after `max_attempts` attempts or `timeout_ms` from the first send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatUntil {
    /// Script condition, evaluated like an If node's condition against `response`.
    pub condition: String,
    pub interval_ms: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
}

impl RepeatUntil {
    pub const DEFAULT_CONDITION: &'static str = "response.status === 200";
    pub const DEFAULT_INTERVAL_MS: u64 = 2000;
    pub const MIN_INTERVAL_MS: u64 = 100;
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 30;
    pub const MAX_MAX_ATTEMPTS: u32 = 1000;
    pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
    pub const MAX_TIMEOUT_MS: u64 = 3_600_000;
}

impl Default for RepeatUntil {
    fn default() -> Self {
        Self {
            condition: Self::DEFAULT_CONDITION.to_string(),
            interval_ms: Self::DEFAULT_INTERVAL_MS,
            max_attempts: Self::DEFAULT_MAX_ATTEMPTS,
            timeout_ms: Self::DEFAULT_TIMEOUT_MS,
        }
    }
}
```

In `crates/rocket-flow/src/lib.rs`, add `RepeatUntil` to the `pub use node::{ … }` list.

- [ ] **Step 4: Fix every `Request { … }` literal and pattern**

Run: `cargo check -j4 -p rocket-flow --tests && cargo check -j4 -p rocket-infra --tests && cargo check -j4 -p rocket-app --tests && cargo check -j4 -p rocket --tests`
For each error:
- Struct literals: add `repeat_until: None,` after `debug`.
- Exhaustive destructures such as `FlowNodeKind::Request { label, source, debug }` in `src-tauri/src/commands/flow.rs:137-147` and `:165-175`: add `repeat_until` and pass it through. The DTO has no field yet, so in this task map it to/from the domain value only where the compiler requires it; Task 3 adds the DTO field. To keep this task compiling, destructure `repeat_until: _` in `From<FlowNodeKind> for FlowNodeKindDto` and set `repeat_until: None` in `From<FlowNodeKindDto> for FlowNodeKind`. Task 3 replaces both.
- Patterns that only read some fields: add `..`.

Expected: all four checks pass.

- [ ] **Step 5: Add the persistence round-trip tests**

Append to the tests module in `crates/rocket-infra/src/fs_flow_repo.rs` (import `rocket_flow::RepeatUntil` in the test `use` list):

```rust
    #[test]
    fn request_node_with_repeat_until_roundtrips() {
        let (_dir, repo) = setup();
        let flow = Flow {
            name: "Poll Flow".to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Request {
                    debug: false,
                    label: "Poll job".to_string(),
                    source: RequestSource::Saved {
                        request_path: "jobs/get-job.yml".to_string(),
                    },
                    repeat_until: Some(RepeatUntil {
                        condition: "response.body.status === \"done\"".to_string(),
                        interval_ms: 500,
                        max_attempts: 10,
                        timeout_ms: 5000,
                    }),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
        };
        repo.save("acme", &flow).expect("save");
        let loaded = repo.get("acme", "Poll Flow").expect("get");
        assert_eq!(loaded, flow);
    }

    #[test]
    fn fs_repo_resaves_request_without_repeat_until_byte_identically() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        let path = flows_dir.join("plain.yml");
        let original = "name: Plain\nnodes:\n- id: n1\n  kind:\n    kind: Request\n    label: Get\n    source:\n      type: Saved\n      request_path: a.yml\n  position:\n    x: 0.0\n    y: 0.0\nedges: []\n";
        fs::write(&path, original).expect("write file");

        let loaded = repo.get("acme", "Plain").expect("load");
        repo.save("acme", &loaded).expect("re-save");

        let resaved = fs::read_to_string(&path).expect("read re-saved file");
        assert_eq!(resaved, original, "a Request without repeat_until must re-save unchanged");
    }
```

If the second test's `original` does not match what `serde_yaml` writes for this shape (for example float formatting), first generate it once by serializing the loaded `Flow` with `serde_yaml::to_string`, then paste the exact string. The assertion must compare against a literal written before the feature.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow repeat_until` then `cargo test -j4 -p rocket-infra fs_flow_repo`
Expected: PASS, including the existing `fs_repo_resaves_phase1_file_byte_identically`.

- [ ] **Step 7: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): add repeat_until setting to Request nodes`.

---

### Task 2: Validate `repeat_until` at save time (rule V9)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/validate.rs` (call site in `validate` after `check_expressions(flow)?;` at :49, new `check_repeat_until` fn after `check_expressions`, tests module)
- Test: `crates/rocket-flow/src/validate.rs` (tests)

**Interfaces:**
- Consumes: `RepeatUntil` and its consts from Task 1.
- Produces: `validate` returns `FlowGraphError::InvalidNode { node_id, reason }` for a bad `repeat_until`, with these exact reasons:
  - `the repeat-until condition is empty`
  - `repeat-until interval must be at least 100 ms`
  - `repeat-until max attempts must be between 1 and 1000`
  - `repeat-until timeout must be at most 3600000 ms`
  - `repeat-until timeout must not be shorter than the interval`

- [ ] **Step 1: Write the failing tests**

In the `validate.rs` tests module, add a helper and tests:

```rust
    fn polling_request(id: &str, repeat: crate::node::RepeatUntil) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                debug: false,
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
                repeat_until: Some(repeat),
            },
        )
    }

    fn invalid_node_reason(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidNode { reason, .. }) => reason,
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    #[test]
    fn a_valid_repeat_until_passes() {
        let f = flow(vec![polling_request("p", crate::node::RepeatUntil::default())], vec![]);
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn repeat_until_blank_condition_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            condition: "  ".to_string(),
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(invalid_node_reason(validate(&f)), "the repeat-until condition is empty");
    }

    #[test]
    fn repeat_until_short_interval_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            interval_ms: 99,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until interval must be at least 100 ms"
        );
    }

    #[test]
    fn repeat_until_zero_attempts_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            max_attempts: 0,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until max attempts must be between 1 and 1000"
        );
    }

    #[test]
    fn repeat_until_too_many_attempts_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            max_attempts: 1001,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until max attempts must be between 1 and 1000"
        );
    }

    #[test]
    fn repeat_until_long_timeout_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            timeout_ms: 3_600_001,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until timeout must be at most 3600000 ms"
        );
    }

    #[test]
    fn repeat_until_timeout_shorter_than_interval_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            interval_ms: 5000,
            timeout_ms: 1000,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat.clone())], vec![]);
        assert_eq!(invalid_node_id(validate(&f)), "p");
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until timeout must not be shorter than the interval"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow repeat_until`
Expected: `a_valid_repeat_until_passes` passes; the six rejection tests FAIL with "expected InvalidNode, got Ok([...])".

- [ ] **Step 3: Implement rule V9**

In `validate`, after `check_expressions(flow)?;` add `check_repeat_until(flow)?;`. Add after `check_expressions`:

```rust
/// V9: a Request node's `repeat_until` settings are within their limits.
fn check_repeat_until(flow: &Flow) -> Result<(), FlowGraphError> {
    use crate::node::RepeatUntil;
    for node in &flow.nodes {
        let FlowNodeKind::Request {
            repeat_until: Some(r),
            ..
        } = &node.kind
        else {
            continue;
        };
        let reason = if r.condition.trim().is_empty() {
            Some("the repeat-until condition is empty".to_string())
        } else if r.interval_ms < RepeatUntil::MIN_INTERVAL_MS {
            Some(format!(
                "repeat-until interval must be at least {} ms",
                RepeatUntil::MIN_INTERVAL_MS
            ))
        } else if r.max_attempts < 1 || r.max_attempts > RepeatUntil::MAX_MAX_ATTEMPTS {
            Some(format!(
                "repeat-until max attempts must be between 1 and {}",
                RepeatUntil::MAX_MAX_ATTEMPTS
            ))
        } else if r.timeout_ms > RepeatUntil::MAX_TIMEOUT_MS {
            Some(format!(
                "repeat-until timeout must be at most {} ms",
                RepeatUntil::MAX_TIMEOUT_MS
            ))
        } else if r.timeout_ms < r.interval_ms {
            Some("repeat-until timeout must not be shorter than the interval".to_string())
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(invalid_node(node, reason));
        }
    }
    Ok(())
}
```

Update the module doc comment's rule range from "V1-V8" to "V1-V9".

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS (all existing validate tests still pass).

- [ ] **Step 5: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): validate repeat_until limits on save`.

---

### Task 3: IPC DTO and TypeScript types

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/flow.rs` (`FlowNodeKindDto::Request` :114-119, `From` impls :136-190, new `RepeatUntilDto`, tests module near :456)
- Modify: `src/lib/tauri-api.ts:1760-1765` (node kind union, new `RepeatUntil` interface)
- Create: `src/lib/flow-repeat.ts`
- Test: `src-tauri/src/commands/flow.rs` (tests), `src/lib/__tests__/flow-repeat.test.ts`

**Interfaces:**
- Consumes: `rocket_flow::RepeatUntil` (Task 1).
- Produces:
  - `RepeatUntilDto { condition: String, interval_ms: u64, max_attempts: u32, timeout_ms: u64 }` (`#[serde(rename_all = "camelCase")]`), `From` both ways with `RepeatUntil`.
  - `FlowNodeKindDto::Request` field `#[serde(default)] repeat_until: Option<RepeatUntilDto>` (wire name `repeatUntil`, via the enum's existing `rename_all_fields = "camelCase"`).
  - TS `export interface RepeatUntil { condition: string; intervalMs: number; maxAttempts: number; timeoutMs: number }`; Request kind `repeatUntil?: RepeatUntil | null`.
  - `src/lib/flow-repeat.ts`: `export const DEFAULT_REPEAT_UNTIL: RepeatUntil` and `export function msToSecondsLabel(ms: number): string`.

- [ ] **Step 1: Write the failing Rust DTO test**

In `src-tauri/src/commands/flow.rs` tests, next to `request_debug_flag_converts_both_ways_and_defaults_to_false`:

```rust
    #[test]
    fn request_repeat_until_converts_both_ways_and_defaults_to_none() {
        let dto = FlowNodeKindDto::Request {
            label: "Poll".to_string(),
            source: RequestSourceDto::Saved {
                request_path: "jobs/get.yml".to_string(),
            },
            debug: false,
            repeat_until: Some(RepeatUntilDto {
                condition: "response.body.done".to_string(),
                interval_ms: 500,
                max_attempts: 4,
                timeout_ms: 3000,
            }),
        };
        let json = serde_json::to_string(&dto).expect("serialize");
        assert!(json.contains("\"repeatUntil\""), "got: {json}");
        assert!(json.contains("\"intervalMs\":500"), "got: {json}");
        assert!(json.contains("\"maxAttempts\":4"), "got: {json}");

        let domain: FlowNodeKind = dto.into();
        let FlowNodeKind::Request {
            repeat_until: Some(r),
            ..
        } = &domain
        else {
            panic!("repeat_until must survive the conversion, got {domain:?}");
        };
        assert_eq!(r.timeout_ms, 3000);
        let back: FlowNodeKindDto = domain.into();
        assert!(matches!(
            back,
            FlowNodeKindDto::Request {
                repeat_until: Some(RepeatUntilDto { max_attempts: 4, .. }),
                ..
            }
        ));

        let old =
            r#"{"kind":"Request","label":"L","source":{"type":"Saved","requestPath":"a.yml"}}"#;
        let parsed: FlowNodeKindDto = serde_json::from_str(old).expect("deserialize");
        assert!(matches!(
            parsed,
            FlowNodeKindDto::Request {
                repeat_until: None,
                ..
            }
        ));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -j4 -p rocket --lib request_repeat_until`
Expected: FAIL to compile, "cannot find struct `RepeatUntilDto`" and "variant has no field named `repeat_until`".

- [ ] **Step 3: Implement the DTO**

In `src-tauri/src/commands/flow.rs`, import `rocket_flow::RepeatUntil`, then add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatUntilDto {
    pub condition: String,
    pub interval_ms: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
}
impl From<RepeatUntil> for RepeatUntilDto {
    fn from(r: RepeatUntil) -> Self {
        Self {
            condition: r.condition,
            interval_ms: r.interval_ms,
            max_attempts: r.max_attempts,
            timeout_ms: r.timeout_ms,
        }
    }
}
impl From<RepeatUntilDto> for RepeatUntil {
    fn from(r: RepeatUntilDto) -> Self {
        Self {
            condition: r.condition,
            interval_ms: r.interval_ms,
            max_attempts: r.max_attempts,
            timeout_ms: r.timeout_ms,
        }
    }
}
```

Change `FlowNodeKindDto::Request` to:

```rust
    Request {
        label: String,
        source: RequestSourceDto,
        #[serde(default)]
        debug: bool,
        #[serde(default)]
        repeat_until: Option<RepeatUntilDto>,
    },
```

Replace the Task 1 placeholders in both `From` impls:

```rust
            FlowNodeKind::Request {
                label,
                source,
                debug,
                repeat_until,
            } => FlowNodeKindDto::Request {
                label,
                source: source.into(),
                debug,
                repeat_until: repeat_until.map(Into::into),
            },
```

```rust
            FlowNodeKindDto::Request {
                label,
                source,
                debug,
                repeat_until,
            } => FlowNodeKind::Request {
                label,
                source: source.into(),
                debug,
                repeat_until: repeat_until.map(Into::into),
            },
```

Add `repeat_until: None,` to the DTO literals the compiler flags in the tests module (`sample_dto` near :402, the debug test near :457).

- [ ] **Step 4: Run the Rust tests to verify they pass**

Run: `cargo test -j4 -p rocket --lib commands::flow`
Expected: PASS.

- [ ] **Step 5: Write the failing TS test**

Create `src/lib/__tests__/flow-repeat.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { DEFAULT_REPEAT_UNTIL, msToSecondsLabel } from '../flow-repeat';

describe('flow-repeat', () => {
  it('uses the spec defaults', () => {
    expect(DEFAULT_REPEAT_UNTIL).toEqual({
      condition: 'response.status === 200',
      intervalMs: 2000,
      maxAttempts: 30,
      timeoutMs: 60000,
    });
  });

  it('labels whole and fractional seconds', () => {
    expect(msToSecondsLabel(2000)).toBe('2s');
    expect(msToSecondsLabel(1500)).toBe('1.5s');
    expect(msToSecondsLabel(14230)).toBe('14.2s');
  });
});
```

Run: `yarn test src/lib/__tests__/flow-repeat.test.ts`
Expected: FAIL, "Failed to resolve import '../flow-repeat'".

- [ ] **Step 6: Implement the TS types and helpers**

In `src/lib/tauri-api.ts`, above `export type FlowNodeKind`:

```ts
/** Polling settings of a Request node. Mirrors the Rust `RepeatUntilDto`. */
export interface RepeatUntil {
  condition: string;
  intervalMs: number;
  maxAttempts: number;
  timeoutMs: number;
}
```

and change the Request member of `FlowNodeKind`:

```ts
  | {
      kind: 'Request';
      label: string;
      source: RequestSource;
      debug?: boolean;
      repeatUntil?: RepeatUntil | null;
    }
```

Create `src/lib/flow-repeat.ts`:

```ts
import type { RepeatUntil } from './tauri-api';

/** Settings a Request node gets when Repeat until is turned on. Matches `RepeatUntil::default()` in rocket-flow. */
export const DEFAULT_REPEAT_UNTIL: RepeatUntil = {
  condition: 'response.status === 200',
  intervalMs: 2000,
  maxAttempts: 30,
  timeoutMs: 60000,
};

/** Formats milliseconds as seconds, e.g. 2000 → "2s", 1500 → "1.5s". */
export function msToSecondsLabel(ms: number): string {
  const seconds = ms / 1000;
  return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)}s`;
}
```

- [ ] **Step 7: Run the checks to verify they pass**

Run: `yarn test src/lib/__tests__/flow-repeat.test.ts && yarn tsc --noEmit && yarn check`
Expected: PASS, no type or lint errors.

- [ ] **Step 8: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): expose repeat_until over IPC and in TS types`.
