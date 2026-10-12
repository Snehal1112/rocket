# Flow Tier 2 Corrupt Flows and Step Guard Implementation Plan (Plan 5 of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a flow file that cannot be parsed in the sidebar as a corrupt entry with its parse message instead of hiding it (F-29), and give a flow run a hard upper bound on the number of nodes it will execute (F-30).

**Architecture:** Task 1 adds `FlowListEntry` and a defaulted `FlowRepository::list_entries` to `rocket-flow`, overrides it in `FsFlowRepo` and `SharedPathFlowRepo`, exposes it through `FlowService::list_entries`, and changes the `list_flows` IPC command to return entries. `FlowRepository::list` stays as it is, so every existing implementor and fake keeps compiling. Task 2 updates the TypeScript binding, the sidebar flow row and the Flow tab picker. Task 3 adds a `MAX_RUN_STEPS` node-count check to `FlowExecutionService::load_ordered_nodes`.

**Tech Stack:** Rust (serde_yaml, tempfile for tests), Tauri IPC, React, TypeScript, TanStack Query, Vitest, Testing Library, Biome. Run with `cargo -j4` and `yarn`.

**Spec:** `.claude/flow-roadmap.md` rows F-29 and F-30 (Tier 2). Phase 1 spec `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md` section 7 ("a `MAX_RUN_STEPS`-equivalent guard"). Task numbers restart at 1 in this plan.

## Global Constraints

- Rust gates: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never run `cargo fmt` over a crate; check only your own files with `rustfmt --check --edition 2021 <file>`.
- No unwrap or expect calls in production code. Test code may use `expect("message")`.
- Serde `camelCase` only on IPC DTOs, never on persistence structs. `FlowListEntry` is a domain type and has no serde derive. Only `FlowListEntryDto` in `src-tauri` serializes.
- Frontend gates after every frontend task: `yarn tsc --noEmit`, `yarn check`, and `yarn test <pattern> --run --maxWorkers=2`.
- shadcn/ui primitives and `lucide-react` icons only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`. Narrow Zustand selectors.
- Comments are short full sentences ending in a punctuation mark. No emojis.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A`.
- A parse message shown to the user never contains file contents or an absolute path (Task 1 enforces this for the YAML parser text).

## Review Focus

- A file that cannot be parsed is listed with its file stem as the name and a message, and a good flow next to it is still listed unchanged (Task 1).
- The parse message never carries a quoted value from the file or the flows directory path (Task 1).
- `FlowRepository::list` keeps its old behavior, and a file whose stem does not match its flow name is still skipped (Task 1).
- A corrupt flow can be deleted, cannot be renamed (the rename needs the parsed name), and open and run fail with the parse message instead of panicking (Task 1 and Task 2).
- Clicking a corrupt row never opens a tab, shows the message in an error toast, and keeps Delete available (Task 2).
- A flow with more than `MAX_RUN_STEPS` nodes is refused before the run starts, and one with exactly that many is accepted (Task 3).

---

### Task 1: Corrupt entries in the flow list (F-29, backend)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/flow.rs` (new `FlowListEntry` after `Flow` ~54, `list_entries` default method in the trait ~59-81)
- Modify: `crates/rocket-flow/src/lib.rs` (re-export, line 9)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (imports ~5, `read_flow` ~39-44, new helpers ~97-121, `list_entries` in the trait impl ~123-152, tests at the end of `mod tests`)
- Modify: `crates/rocket-infra/src/shared_path_flow_repo.rs` (delegate ~38-60, one test)
- Modify: `crates/rocket-app/src/flow_service.rs` (`list_entries` after `list` ~14-16, one test next to `list_returns_flow_names_for_collection` ~170)
- Modify: `src-tauri/src/commands/flow.rs` (DTO and `list_flows` ~418-425, test in `mod tests` ~635)
- Modify: `crates/rocket-flow/CLAUDE.md` (one line about `list_entries`)

**Interfaces:**
- Produces in `rocket-flow`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct FlowListEntry {
      /// The flow name. For a corrupt file this is the file stem, because the name inside the file cannot be read.
      pub name: String,
      /// The parse or read message when the file is corrupt. `None` for a healthy flow.
      pub error: Option<String>,
  }
  impl FlowListEntry {
      pub fn healthy(name: impl Into<String>) -> Self;
      pub fn corrupt(name: impl Into<String>, error: impl Into<String>) -> Self;
  }
  // On FlowRepository, with a default body that wraps `list`:
  fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>>;
  ```
- Produces in `rocket-app`: `FlowService::list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>>`. `FlowService::list` stays.
- Produces in `src-tauri`: `list_flows` returns `Vec<FlowListEntryDto>` where `FlowListEntryDto { name: String, error: Option<String> }` serializes with `rename_all = "camelCase"` and `skip_serializing_if = "Option::is_none"` on `error`. The TypeScript side reads `error?: string`; a missing key means healthy.
- Consumes: `FsFlowRepo::read_flow`, `slugify`, `file_path`.

Behavior, stated once:

| Operation on a corrupt file `broken.yml` | Result |
|---|---|
| `list` | Skipped, as before. |
| `list_entries` | `FlowListEntry { name: "broken", error: Some("Failed to parse flow YAML: ...") }`. |
| `get("broken")` and so open and run | `InvalidInput("Failed to parse flow YAML: ...")`. No panic. |
| `delete("broken")` | Works. `delete` already tolerates an unreadable file. |
| `rename("broken", ..)` | `InvalidInput("Failed to parse flow YAML: ...")`, file untouched. A rename rewrites the name inside the file, which needs a parse. |
| `save` of a new flow named "broken" | Overwrites the corrupt file, as today. The UI guards this (Task 2). |

A file whose stem is not a slug (for example `My Broken.yml`, made by hand) cannot be targeted by `delete`, because the repo maps a name to `slugify(name).yml`. Such a file stays hidden, like a file whose stem does not match its flow name. Recorded in Open decisions.

The parse message is built from the serde_yaml error text with every double-quoted segment replaced by `"..."`. serde_yaml writes offending values in double quotes (`invalid type: string "secret", expected a sequence`), so this removes file content and keeps field names, the expected type and the line and column.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-infra/src/fs_flow_repo.rs`, append inside `mod tests` (after the last test):

```rust
    fn write_flow_file(dir: &TempDir, file: &str, content: &str) {
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(flows_dir.join(file), content).expect("write flow file");
    }

    #[test]
    fn list_entries_shows_unparseable_files_as_corrupt_with_a_message() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Good Flow")).expect("save valid flow");
        write_flow_file(&dir, "broken.yml", "not: valid: yaml: [");
        write_flow_file(&dir, "empty.yml", "");

        let entries = repo.list_entries("acme").expect("list_entries");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Good Flow", "broken", "empty"]);
        assert_eq!(entries[0], FlowListEntry::healthy("Good Flow"));
        for corrupt in &entries[1..] {
            let message = corrupt
                .error
                .as_deref()
                .expect("a corrupt entry carries a message");
            assert!(
                message.starts_with("Failed to parse flow YAML"),
                "got: {message}"
            );
        }
    }

    #[test]
    fn list_still_skips_unparseable_files() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Good Flow")).expect("save valid flow");
        write_flow_file(&dir, "broken.yml", "not: valid: yaml: [");

        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["Good Flow".to_string()]
        );
    }

    #[test]
    fn corrupt_message_has_no_file_contents_and_no_path() {
        let (dir, repo) = setup();
        write_flow_file(
            &dir,
            "leaky.yml",
            "name: Leaky\nnodes: sk-secret-value-123\nedges: []\n",
        );

        let entries = repo.list_entries("acme").expect("list_entries");

        assert_eq!(entries.len(), 1);
        let message = entries[0].error.as_deref().expect("message");
        assert!(!message.is_empty());
        assert!(
            !message.contains("sk-secret-value-123"),
            "file content leaked: {message}"
        );
        let dir_text = dir.path().to_string_lossy().into_owned();
        assert!(!message.contains(&dir_text), "path leaked: {message}");
    }

    #[test]
    fn redact_quoted_replaces_quoted_values_and_handles_escapes() {
        assert_eq!(
            redact_quoted("invalid type: string \"a\\\"b\", expected a sequence at line 2"),
            "invalid type: string \"...\", expected a sequence at line 2"
        );
        assert_eq!(redact_quoted("missing field `nodes`"), "missing field `nodes`");
        assert_eq!(redact_quoted("cut \"off"), "cut \"...");
    }

    #[test]
    fn list_entries_skips_corrupt_files_whose_stem_is_not_a_slug() {
        let (dir, repo) = setup();
        write_flow_file(&dir, "My Broken.yml", "not: valid: yaml: [");

        assert!(repo.list_entries("acme").expect("list_entries").is_empty());
    }

    #[test]
    fn list_entries_still_skips_a_valid_file_whose_name_does_not_match_its_filename() {
        let (dir, repo) = setup();
        write_flow_file(&dir, "renamed.yml", "name: Other\nnodes: []\nedges: []\n");
        repo.save("acme", &sample("Good Flow")).expect("save valid flow");

        assert_eq!(
            repo.list_entries("acme").expect("list_entries"),
            vec![FlowListEntry::healthy("Good Flow")]
        );
    }

    #[test]
    fn list_entries_on_a_collection_with_no_flows_dir_is_empty() {
        let (_dir, repo) = setup();
        assert!(repo.list_entries("acme").expect("list_entries").is_empty());
    }

    #[test]
    fn a_corrupt_flow_can_be_deleted() {
        let (dir, repo) = setup();
        write_flow_file(&dir, "broken.yml", "not: valid: yaml: [");

        repo.delete("acme", "broken").expect("delete a corrupt flow");

        assert!(flow_files(&dir).is_empty());
        assert!(repo.list_entries("acme").expect("list_entries").is_empty());
    }

    #[test]
    fn a_corrupt_flow_cannot_be_renamed_and_the_file_is_left_alone() {
        let (dir, repo) = setup();
        write_flow_file(&dir, "broken.yml", "not: valid: yaml: [");

        let err = repo
            .rename("acme", "broken", "Fixed")
            .expect_err("rename needs a parsable file");

        match err {
            DomainError::InvalidInput(message) => {
                assert!(message.starts_with("Failed to parse flow YAML"), "{message}")
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
        assert_eq!(flow_files(&dir), vec!["broken.yml".to_string()]);
    }

    #[test]
    fn opening_a_corrupt_flow_fails_with_the_parse_message() {
        let (dir, repo) = setup();
        write_flow_file(&dir, "broken.yml", "not: valid: yaml: [");

        let err = repo.get("acme", "broken").expect_err("must not load");

        match err {
            DomainError::InvalidInput(message) => {
                assert!(message.starts_with("Failed to parse flow YAML"), "{message}")
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
```

In `crates/rocket-infra/src/shared_path_flow_repo.rs` add `use rocket_flow::FlowListEntry;` is not needed in the test if the full path is used. Append inside `mod tests`:

```rust
    #[test]
    fn list_entries_goes_through_the_fs_override_so_corrupt_files_show() {
        let dir = TempDir::new().expect("temp dir");
        let flows = dir.path().join("collections").join("acme").join("flows");
        std::fs::create_dir_all(&flows).expect("create flows dir");
        std::fs::write(flows.join("broken.yml"), b"not: valid: yaml: [").expect("write");
        let repo = SharedPathFlowRepo::new(Arc::new(Mutex::new(dir.path().to_path_buf())));

        let entries = repo.list_entries("acme").expect("list_entries");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "broken");
        assert!(entries[0].error.is_some());
    }
```

In `crates/rocket-app/src/flow_service.rs`, after `list_returns_flow_names_for_collection`:

```rust
    #[test]
    fn list_entries_defaults_to_healthy_entries_for_a_repo_without_an_override() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        assert_eq!(
            svc.list_entries("demo").expect("list_entries"),
            vec![rocket_flow::FlowListEntry::healthy("Login Then Fetch")]
        );
        assert!(svc.list_entries("other").expect("other").is_empty());
    }
```

In `src-tauri/src/commands/flow.rs`, inside `mod tests`:

```rust
    #[test]
    fn list_entry_dto_omits_error_for_a_healthy_flow_and_carries_it_for_a_corrupt_one() {
        let healthy = FlowListEntryDto::from(rocket_flow::FlowListEntry::healthy("Login"));
        assert_eq!(
            serde_json::to_value(&healthy).expect("serialize"),
            serde_json::json!({ "name": "Login" })
        );
        let corrupt = FlowListEntryDto::from(rocket_flow::FlowListEntry::corrupt(
            "broken",
            "Failed to parse flow YAML: EOF",
        ));
        assert_eq!(
            serde_json::to_value(&corrupt).expect("serialize"),
            serde_json::json!({ "name": "broken", "error": "Failed to parse flow YAML: EOF" })
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra fs_flow_repo`
Expected: the build FAILS with "no method named `list_entries`", "cannot find type `FlowListEntry`" and "cannot find function `redact_quoted`". After Step 3 the same command must pass. (`list_still_skips_unparseable_files` and the delete test only fail because the module does not compile.)

- [ ] **Step 3: Write the implementation**

`crates/rocket-flow/src/flow.rs`: add after the `Flow` struct (before the `FlowRepository` doc comment):

```rust
/// One row of a collection's flow list. A file that cannot be parsed is listed
/// too, with `error` set, so the user can see and delete it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowListEntry {
    /// The flow name. For a corrupt file this is the file stem, because the
    /// name inside the file cannot be read.
    pub name: String,
    /// The parse or read message when the file is corrupt. `None` when healthy.
    pub error: Option<String>,
}

impl FlowListEntry {
    pub fn healthy(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            error: None,
        }
    }

    pub fn corrupt(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            error: Some(error.into()),
        }
    }
}
```

and in the trait, right after `fn list`:

```rust
    /// Like `list`, but a flow that exists and cannot be read is returned as a
    /// corrupt entry instead of being left out. The default wraps `list`, which
    /// has no corrupt flows to report. File-backed stores override it.
    fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>> {
        Ok(self
            .list(collection)?
            .into_iter()
            .map(FlowListEntry::healthy)
            .collect())
    }
```

`crates/rocket-flow/src/lib.rs`: `pub use flow::{Flow, FlowEdge, FlowListEntry, FlowNode, FlowRepository};`

`crates/rocket-infra/src/fs_flow_repo.rs`: change the import to `use rocket_flow::{Flow, FlowListEntry, FlowRepository};`, replace `read_flow`:

```rust
    fn read_flow(path: &Path) -> DomainResult<Flow> {
        let content = fs::read_to_string(path)
            .map_err(|e| DomainError::Io(format!("Failed to read flow file: {e}")))?;
        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!(
                "Failed to parse flow YAML: {}",
                redact_quoted(&e.to_string())
            ))
        })
    }
```

Add after `not_found`:

```rust
/// Replaces every double-quoted segment with `"..."`. serde_yaml puts the
/// offending file value in double quotes, and that value must not reach the UI.
fn redact_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut in_quote = false;
    let mut escaped = false;
    for ch in message.chars() {
        if in_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quote = false;
                out.push('"');
            }
        } else {
            out.push(ch);
            if ch == '"' {
                in_quote = true;
                out.push_str("...");
            }
        }
    }
    out
}

/// The text of a read or parse failure, without the `Display` prefix of the error type.
fn failure_message(err: DomainError) -> String {
    match err {
        DomainError::InvalidInput(message) | DomainError::Io(message) => message,
        other => other.to_string(),
    }
}
```

and in `impl FlowRepository for FsFlowRepo`, after `list`:

```rust
    fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>> {
        let dir = self.flows_dir(collection)?;
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|e| DomainError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| DomainError::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "yml") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or_default();
            match Self::read_flow(&path) {
                Ok(flow) => {
                    // Only list names that `get` can resolve back to this same file.
                    if slugify(&flow.name) == stem {
                        entries.push(FlowListEntry::healthy(flow.name));
                    }
                }
                Err(err) => {
                    // Delete maps a name to `slugify(name).yml`, so a stem that is
                    // not a slug could not be targeted. Such a file stays hidden.
                    if !stem.is_empty() && slugify(&stem) == stem {
                        entries.push(FlowListEntry::corrupt(stem, failure_message(err)));
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
```

`crates/rocket-infra/src/shared_path_flow_repo.rs`: change the import to `use rocket_flow::{Flow, FlowListEntry, FlowRepository};` and add to the impl, after `list`:

```rust
    fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>> {
        self.repo().list_entries(collection)
    }
```

`crates/rocket-app/src/flow_service.rs`: change the first import to `use rocket_flow::{validate, Flow, FlowGraphError, FlowListEntry, FlowRepository};` and add after `list`:

```rust
    /// Like `list`, but a flow file that cannot be read is included as a corrupt entry.
    pub fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>> {
        self.flow_repo.list_entries(collection)
    }
```

(Change `rocket_flow::FlowListEntry::healthy` in the test to `FlowListEntry::healthy` if the file already imports it through `use super::*`.)

`src-tauri/src/commands/flow.rs`: replace `list_flows` and add the DTO above it:

```rust
/// One row of the flow list. `error` is present only for a flow file that cannot be parsed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowListEntryDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl From<rocket_flow::FlowListEntry> for FlowListEntryDto {
    fn from(entry: rocket_flow::FlowListEntry) -> Self {
        Self {
            name: entry.name,
            error: entry.error,
        }
    }
}

#[tauri::command]
pub fn list_flows(
    collection: String,
    svc: State<'_, FlowService>,
) -> Result<Vec<FlowListEntryDto>, DomainError> {
    svc.list_entries(&collection)
        .map(|entries| entries.into_iter().map(FlowListEntryDto::from).collect())
}
```

`crates/rocket-flow/CLAUDE.md`: next to the `FlowRepository` line add one sentence: "`list_entries` returns corrupt files as entries with an `error`; its default wraps `list`."

- [ ] **Step 4: Run the tests to verify they pass**

Run, in order:
- `cargo test -j4 -p rocket-infra fs_flow_repo`
- `cargo test -j4 -p rocket-infra shared_path_flow_repo`
- `cargo test -j4 -p rocket-app flow_service`
- `cargo check -j4 -p rocket-flow`
- `cargo check -j4 -p rocket`
- `cargo test -j4 -p rocket list_entry_dto`
- `rustfmt --check --edition 2021 crates/rocket-flow/src/flow.rs crates/rocket-infra/src/shared_path_flow_repo.rs` (the other edited files predate rustfmt cleanliness; compare with `git stash`-free means: run `rustfmt --check` and read only the hunks in lines you touched)

Expected: all PASS. If `corrupt_message_has_no_file_contents_and_no_path` fails because serde_yaml 0.9 words the error differently, read the printed message and extend `redact_quoted` or the test's fixture, but keep the rule that no value from the file survives.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/flow.rs crates/rocket-flow/src/lib.rs crates/rocket-flow/CLAUDE.md crates/rocket-infra/src/fs_flow_repo.rs crates/rocket-infra/src/shared_path_flow_repo.rs crates/rocket-app/src/flow_service.rs src-tauri/src/commands/flow.rs && git commit --only -m "feat(flow): list unparseable flow files as corrupt entries" -- <the same paths>`

The IPC type changes here and the TypeScript side changes in Task 2. Land both before running the app.

---

### Task 2: Corrupt flows in the sidebar and the picker (F-29, frontend)

**Files:**
- Modify: `src/lib/tauri-api.ts` (`listFlows` ~2260)
- Modify: `src/lib/queries/flow-queries.ts` (doc comment only)
- Modify: `src/components/collections/FlowListItem.tsx` (props ~17-21, `open` ~48-55, row ~118-145, rename menu item ~172)
- Modify: `src/components/collections/CollectionNode.tsx` (`EMPTY_NAMES` ~66, `useFlows` ~113, `filteredFlows` ~351-353, render ~641-655, type import ~44)
- Modify: `src/components/flow/FlowPicker.tsx` (`NO_FLOWS` ~19, `useFlows` ~32, `handleCreate` ~49-55, flow `Select` ~100-113)
- Test: `src/components/collections/__tests__/FlowListItem.test.tsx`
- Test: `src/components/collections/__tests__/CollectionNode.flows.test.tsx`
- Test: `src/components/flow/__tests__/FlowPane.test.tsx` (picker tests, `listFlows` mock values ~133)
- Test: `src/components/layout/__tests__/CollectionsSidebar.test.tsx` (line ~177 mock value)
- Test: `src/lib/queries/__tests__/flow-queries.test.tsx`
- Test: `src/lib/queries/__tests__/flow-api.test.ts`

**Interfaces:**
- Produces in `src/lib/tauri-api.ts`:
  ```ts
  /** One row of a collection's flow list. `error` is set when the flow file cannot be parsed. */
  export interface FlowListEntry {
    name: string;
    error?: string;
  }
  export const listFlows = (collection: string) =>
    invoke<FlowListEntry[]>('list_flows', { collection });
  ```
- `useFlows` now returns `FlowListEntry[]` as `data`. Its two callers (`CollectionNode`, `FlowPicker`) are the only consumers.
- `FlowListItem` gains an optional prop `error?: string`. When set: the row shows a `TriangleAlert` icon (`data-testid='flow-corrupt-icon'`), the name gets a tooltip with the message, the row `aria-label` is `Corrupt flow <name>: <message>`, a click shows `toast.error` and opens nothing, Rename is disabled, Delete works as before.
- Consumes: `Tooltip`, `TooltipContent`, `TooltipProvider`, `TooltipTrigger` from `@/components/ui/tooltip`; `TriangleAlert` from `lucide-react` (already used in the repo).

How the open path works today: `FlowListItem.open` awaits `usePaneStore.getState().openFlowTab(collectionName, name)` and shows `toast.error("Could not open ...")` on failure. For a corrupt file `openFlowTab` would call `getFlow`, which now fails with the parse message, so even without a guard no tab with a stale graph opens. The guard avoids the round trip and the half-open tab, and shows the message that the list already holds.

- [ ] **Step 1: Write the failing tests**

`flow-api.test.ts`, replace the first test:

```ts
  it('listFlows invokes list_flows and returns entries, with an error for a corrupt file', async () => {
    const entries = [{ name: 'My Flow' }, { name: 'broken', error: 'Failed to parse flow YAML: EOF' }];
    vi.mocked(invoke).mockResolvedValue(entries);
    const { listFlows } = await import('@/lib/tauri-api');
    const result = await listFlows('my-collection');
    expect(invoke).toHaveBeenCalledWith('list_flows', { collection: 'my-collection' });
    expect(result).toEqual(entries);
  });
```

`flow-queries.test.tsx`: change the three string arrays to entries. Test 1:

```tsx
    vi.mocked(listFlows).mockResolvedValue([{ name: 'Login' }, { name: 'Sync' }]);
    ...
    await waitFor(() => expect(result.current.data).toEqual([{ name: 'Login' }, { name: 'Sync' }]));
    expect(listFlows).toHaveBeenCalledWith('demo');
    expect(client.getQueryData(flowKeys.collection('demo'))).toEqual([{ name: 'Login' }, { name: 'Sync' }]);
```

Test 3: `mockResolvedValueOnce([{ name: 'Login' }]).mockResolvedValue([{ name: 'Login' }, { name: 'New' }])` and the two `toEqual` lines to match. Add:

```tsx
  it('passes a corrupt entry through with its message', async () => {
    vi.mocked(listFlows).mockResolvedValue([{ name: 'broken', error: 'Failed to parse flow YAML: EOF' }]);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useFlows('demo'), { wrapper: wrapperFor(client) });
    await waitFor(() =>
      expect(result.current.data).toEqual([{ name: 'broken', error: 'Failed to parse flow YAML: EOF' }]),
    );
  });
```

`CollectionsSidebar.test.tsx` line ~177: `mockResolvedValue([{ name: 'Login' }])`.

`CollectionNode.flows.test.tsx`: line 56 becomes `mockResolvedValue([{ name: 'Login' }, { name: 'Sync' }])`. Add:

```tsx
  it('lists a corrupt flow in the Flows group with its message', async () => {
    vi.mocked(tauriApi.listFlows).mockResolvedValue([
      { name: 'Login' },
      { name: 'broken', error: 'Failed to parse flow YAML: EOF' },
    ]);
    renderNode();
    expect(await screen.findByLabelText('Open flow Login')).toBeInTheDocument();
    expect(
      screen.getByLabelText('Corrupt flow broken: Failed to parse flow YAML: EOF'),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText('Open flow broken')).not.toBeInTheDocument();
  });

  it('applies the sidebar filter to corrupt flow names too', async () => {
    vi.mocked(tauriApi.listFlows).mockResolvedValue([
      { name: 'Login' },
      { name: 'broken', error: 'Failed to parse flow YAML: EOF' },
    ]);
    renderNode('log');
    expect(await screen.findByLabelText('Open flow Login')).toBeInTheDocument();
    expect(screen.queryByLabelText(/Corrupt flow broken/)).not.toBeInTheDocument();
  });
```

`FlowListItem.test.tsx`: change `renderItem` to accept the error and pass it:

```tsx
function renderItem(onDelete = vi.fn(), error?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Tree aria-label='tree'>
        <FlowListItem name='Login' collectionName='col' onDelete={onDelete} error={error} />
      </Tree>
    </QueryClientProvider>,
  );
  return { onDelete, client };
}
```

and add a describe at the end of the file:

```tsx
describe('FlowListItem corrupt flow', () => {
  const message = 'Failed to parse flow YAML: EOF while parsing a value';

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset().mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
    vi.mocked(toast.error).mockReset();
    vi.mocked(renameFlow).mockReset();
  });

  it('shows a warning icon and no icon for a healthy flow', () => {
    renderItem(vi.fn(), message);
    expect(screen.getByTestId('flow-corrupt-icon')).toBeInTheDocument();
  });

  it('shows no warning icon without an error', () => {
    renderItem();
    expect(screen.queryByTestId('flow-corrupt-icon')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Open flow Login')).toBeInTheDocument();
  });

  it('names the problem in the row label', () => {
    renderItem(vi.fn(), message);
    expect(screen.getByLabelText(`Corrupt flow Login: ${message}`)).toBeInTheDocument();
  });

  it('shows the message in an error toast on click and opens no tab', async () => {
    renderItem(vi.fn(), message);
    fireEvent.click(screen.getByText('Login'));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(expect.stringContaining(message)),
    );
    expect(getFlow).not.toHaveBeenCalled();
    expect(collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)).toHaveLength(0);
  });

  it('keeps Delete available', async () => {
    const { onDelete } = renderItem(vi.fn(), message);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({ type: 'flow', collection: 'col', name: 'Login' });
  });

  it('disables Rename, because the name inside the file cannot be read', async () => {
    renderItem(vi.fn(), message);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    const item = (await screen.findByText('Rename')).closest('[role="menuitem"]');
    expect(item).toHaveAttribute('data-disabled');
    await userEvent.click(screen.getByText('Rename'));
    expect(renameFlow).not.toHaveBeenCalled();
  });
});
```

`FlowPane.test.tsx`: change line ~133 to `vi.mocked(listFlows).mockResolvedValueOnce([]).mockResolvedValue([{ name: 'Alpha' }]);` and add next to the other picker tests:

```tsx
  it('refuses to create a flow over a corrupt file with the same name', async () => {
    vi.mocked(listFlows).mockResolvedValue([
      { name: 'broken', error: 'Failed to parse flow YAML: EOF' },
    ]);
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    usePaneStore.setState({ openFlowTab });
    renderPicker();
    // Wait for the list to load, so the corrupt entry is known.
    await waitFor(() => expect(listFlows).toHaveBeenCalled());
    await screen.findByText('Select flow');

    await userEvent.type(screen.getByLabelText('New flow name'), 'broken');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/corrupt/i);
    expect(saveFlow).not.toHaveBeenCalled();
    expect(openFlowTab).not.toHaveBeenCalled();
  });
```

(`renderPicker`, `saveFlow`, `listFlows`, `userEvent`, `waitFor` are already imported in that file; confirm with `grep -n "^import\|function renderPicker" src/components/flow/__tests__/FlowPane.test.tsx` and add any missing import.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test flow-api flow-queries FlowListItem CollectionNode.flows FlowPane.test CollectionsSidebar --run --maxWorkers=2`
Expected: `yarn tsc --noEmit` also reports the type errors (`{ name: string }` is not assignable to `string`). The new runtime tests FAIL: no `flow-corrupt-icon`, no `Corrupt flow` label, Rename not disabled, picker creates over the corrupt name. The old tests that now pass objects through `listFlows` FAIL until the components read `.name`.

- [ ] **Step 3: Write the implementation**

`src/lib/tauri-api.ts`:

```ts
/** One row of a collection's flow list. `error` is set when the flow file cannot be parsed. */
export interface FlowListEntry {
  name: string;
  error?: string;
}

export const listFlows = (collection: string) =>
  invoke<FlowListEntry[]>('list_flows', { collection });
```

`src/lib/queries/flow-queries.ts`: change the doc comment to "The flows of one collection as entries. A flow file that cannot be parsed has an `error`." Code unchanged.

`FlowListItem.tsx`: add `TriangleAlert` to the lucide import, `import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';`, extend the props:

```tsx
interface FlowListItemProps {
  name: string;
  collectionName: string;
  onDelete: (target: DeleteTarget) => void;
  /** Set when the flow file cannot be parsed. The row then cannot be opened or renamed. */
  error?: string;
}
```

signature `FlowListItem({ name, collectionName, onDelete, error }: FlowListItemProps)`. At the top of `open`:

```tsx
    if (isRenaming) return;
    if (error) {
      toast.error(`"${name}" cannot be opened: ${error}`);
      return;
    }
```

The row content (replace the `TreeItemContent` opening props and the icon and name):

```tsx
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={error ? `Corrupt flow ${name}: ${error}` : `Open flow ${name}`}
        >
          {error ? (
            <TriangleAlert
              aria-hidden='true'
              data-testid='flow-corrupt-icon'
              className='h-3.5 w-3.5 shrink-0 text-destructive'
            />
          ) : (
            <Workflow aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )}
```

and the non-renaming name branch:

```tsx
          ) : error ? (
            <TooltipProvider delayDuration={200}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className='truncate text-destructive'>{name}</span>
                </TooltipTrigger>
                <TooltipContent className='max-w-xs break-words'>{error}</TooltipContent>
              </Tooltip>
            </TooltipProvider>
          ) : (
            <span className='truncate text-foreground'>{name}</span>
          )}
```

The Rename item: `<DropdownMenuItem onSelect={startRename} disabled={!!error}>`.

`CollectionNode.tsx`: add `FlowListEntry` to the `import type { Collection, CollectionSummary, Contract } from '@/lib/tauri-api';` line; replace `const EMPTY_NAMES: string[] = [];` with `const EMPTY_FLOWS: FlowListEntry[] = [];`; `const { data: flows = EMPTY_FLOWS } = useFlows(summary.name, open);`; and

```tsx
  const filteredFlows = filter
    ? flows.filter((f) => f.name.toLowerCase().includes(filter.toLowerCase()))
    : flows;
```

and the render:

```tsx
              {filteredFlows.map((flow) => (
                <FlowListItem
                  key={`flow-${flow.name}`}
                  name={flow.name}
                  error={flow.error}
                  collectionName={summary.name}
                  onDelete={onDelete}
                />
              ))}
```

If `grep -n "flowNames\|EMPTY_NAMES" src/components/collections/CollectionNode.tsx` still finds uses after this, rename them the same way.

`FlowPicker.tsx`: import `type FlowListEntry` with the existing tauri-api import, then

```tsx
// Stable empty list, so the query default does not change identity on each render.
const NO_FLOWS: FlowListEntry[] = [];
...
  const { data: flows = NO_FLOWS } = useFlows(selectedCollection || null);
```

`handleCreate` start:

```tsx
    const existing = flows.find((f) => f.name === name);
    if (existing?.error) {
      setCreateError(
        `A corrupt flow file named "${name}" already exists. Delete it from the sidebar first.`,
      );
      return;
    }
    if (existing) {
      openFlow(name);
      return;
    }
```

(replacing the `flowNames.includes(name)` block), and the flow select:

```tsx
      <Select disabled={!selectedCollection || flows.length === 0} onValueChange={openFlow}>
        <SelectTrigger className='w-64' aria-label='Flow'>
          <SelectValue placeholder={flows.length === 0 ? 'No flows yet' : 'Select flow'} />
        </SelectTrigger>
        <SelectContent>
          {flows.map((flow) => (
            <SelectItem key={flow.name} value={flow.name} disabled={!!flow.error}>
              {flow.error ? `${flow.name} (corrupt)` : flow.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
```

Search for other readers with `grep -rn "useFlows\|listFlows" src --include=*.ts --include=*.tsx | grep -v __tests__` and fix any that still treat the data as strings. There should be none besides the three files above.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test flow-api flow-queries FlowListItem CollectionNode CollectionsSidebar FlowPane --run --maxWorkers=2`, then `yarn tsc --noEmit` and `yarn check`.
Expected: all PASS. If the Radix tooltip needs a `ResizeObserver` that jsdom lacks and a test errors on it, look at how `CollectionNode` tests with its lock-pin tooltip and reuse that setup. The tests above do not open the tooltip, they assert the row label, so only the render must not throw.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then commit with paths: `src/lib/tauri-api.ts`, `src/lib/queries/flow-queries.ts`, `src/components/collections/FlowListItem.tsx`, `src/components/collections/CollectionNode.tsx`, `src/components/flow/FlowPicker.tsx`, and the six test files listed under Files. Message: `feat(flow): show corrupt flow files in the sidebar and picker`.

---

### Task 3: Run step guard (F-30)

**Decision, with evidence.** Phase 1 spec section 7 promised "a `MAX_RUN_STEPS`-equivalent guard". `grep -rn MAX_RUN_STEPS crates` finds nothing, so it was never built. The collection runner needed it because `setNextRequest` can loop. The flow run loop cannot: `load_ordered_nodes` calls `rocket_flow::validate`, which rejects cycles and returns each node id once in `order`, and the loop in `run_with_auth` (`for node_id in run_order`, ~line 1189) visits each id once. A `repeat_until` node retries inside its own node and is capped by `RepeatUntil::MAX_MAX_ATTEMPTS` (1000, enforced in `validate.rs` ~286). So the number of executed nodes already equals the node count, and a counter inside the loop could never trip. The one real unbounded input is the node count itself, from a hand-edited or generated file. The guard is therefore a node-count limit applied before the run starts, which is exactly "steps executed at most" because each node runs at most once. It is cheap and keeps the spec promise honest. Recorded in the roadmap in this task.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (constant and helper before `FlowExecutionService`, ~868; `load_ordered_nodes` ~942-951; tests next to `load_ordered_nodes_returns_dependency_order_for_a_valid_flow` ~3258)
- Modify: `.claude/flow-roadmap.md` (rows F-29 and F-30 status)

**Interfaces:**
- Produces: `pub const MAX_RUN_STEPS: usize = 10_000;` in `flow_execution_service.rs`, and a private `fn check_step_budget(flow_name: &str, node_count: usize, limit: usize) -> DomainResult<()>`. The limit is a parameter so a unit test can use a small one. The constant is also tested directly with a flow of `MAX_RUN_STEPS + 1` Input nodes, which is cheap (no edges, no I/O, refused before validation).
- Consumes: `FlowExecutionService::load_ordered_nodes`, `service_with_flow`, `input_node_with`, `run_input` from the existing test module.
- The refusal is `DomainError::InvalidInput` with the text `flow '<name>' has <n> nodes, more than the limit of <limit> a run may execute`. It happens after the run id is reserved and before any event, secret fetch or node, and `RunRegistration` is dropped on the error path like every other early return.

- [ ] **Step 1: Write the failing tests**

In `mod tests` of `flow_execution_service.rs`, after `load_ordered_nodes_returns_dependency_order_for_a_valid_flow`:

```rust
    #[test]
    fn step_budget_accepts_the_limit_and_refuses_one_more() {
        assert!(check_step_budget("f", 3, 3).is_ok());
        let err = check_step_budget("f", 4, 3).expect_err("one over the limit");
        match err {
            DomainError::InvalidInput(message) => {
                assert!(message.contains("flow 'f' has 4 nodes"), "{message}");
                assert!(message.contains("limit of 3"), "{message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    fn flow_of_input_nodes(count: usize) -> Flow {
        Flow {
            name: "wide".to_string(),
            nodes: (0..count)
                .map(|i| input_node_with(&format!("n{i}"), "x"))
                .collect(),
            edges: Vec::new(),
            callback_host: None,
        }
    }

    #[test]
    fn a_flow_with_more_than_max_run_steps_nodes_is_refused_before_it_is_ordered() {
        let service = service_with_flow(flow_of_input_nodes(MAX_RUN_STEPS + 1));
        let err = service
            .load_ordered_nodes("my-api", "wide")
            .expect_err("over the limit");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_flow_with_exactly_max_run_steps_nodes_is_still_accepted() {
        let service = service_with_flow(flow_of_input_nodes(MAX_RUN_STEPS));
        let (_flow, order) = service
            .load_ordered_nodes("my-api", "wide")
            .expect("exactly the limit loads");
        assert_eq!(order.len(), MAX_RUN_STEPS);
    }

    #[tokio::test]
    async fn a_run_over_the_limit_fails_with_a_readable_error_and_publishes_nothing() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow_of_input_nodes(MAX_RUN_STEPS + 1), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let err = service
            .run(&exec, run_input("wide"))
            .await
            .expect_err("over the limit");

        assert!(err.to_string().contains("more than the limit"), "{err}");
        assert!(publisher.events().is_empty());
    }
```

`service_with_publisher`, `RecordingPublisher`, `RecordingExecutor`, `recording_exec`, `fixed_wire` and `run_input` already exist in this module (used by `empty_flow_publishes_started_then_finished_with_zero_counts`). `service_with_publisher` builds the flow under collection `"my-api"`; confirm with `sed -n 4364,4380p crates/rocket-app/src/flow_execution_service.rs`, and if `run_input` uses another collection name than the helper stores the flow under, take both from the existing `empty_flow_publishes_started_then_finished_with_zero_counts` test, which pairs them.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests::step_budget`
Expected: the build FAILS with "cannot find function `check_step_budget`" and "cannot find value `MAX_RUN_STEPS`". After Step 3, `a_flow_with_more_than_max_run_steps_nodes_is_refused_before_it_is_ordered` and the run test must pass, and `a_flow_with_exactly_max_run_steps_nodes_is_still_accepted` must pass (it proves the check is not off by one).

- [ ] **Step 3: Write the implementation**

Before the doc comment of `FlowExecutionService` (the one that starts "Orchestrates one Flow run"), add:

```rust
/// The most nodes one run may execute. A run visits each node at most once,
/// because the graph is acyclic and `repeat_until` retries stay inside their
/// node, so this caps the node count. Real flows have tens of nodes.
pub const MAX_RUN_STEPS: usize = 10_000;

/// Refuses a flow with more nodes than a run may execute.
fn check_step_budget(flow_name: &str, node_count: usize, limit: usize) -> DomainResult<()> {
    if node_count > limit {
        return Err(DomainError::InvalidInput(format!(
            "flow '{flow_name}' has {node_count} nodes, more than the limit of {limit} a run may execute"
        )));
    }
    Ok(())
}
```

and in `load_ordered_nodes`, between the `get` and `validate`:

```rust
        let flow = self.flow_repo.get(collection, flow_name)?;
        // Checked before the graph work, so an oversized file costs nothing more.
        check_step_budget(flow_name, flow.nodes.len(), MAX_RUN_STEPS)?;
        let order = rocket_flow::validate(&flow).map_err(|e| {
```

Add one sentence to the `load_ordered_nodes` doc comment: "A flow with more than `MAX_RUN_STEPS` nodes is rejected too."

`.claude/flow-roadmap.md`: set F-29 and F-30 to `done` with the date and commit hash placeholder-free wording: for F-30 write in the notes column "Node-count guard `MAX_RUN_STEPS` = 10000 in `load_ordered_nodes`. A per-step counter was not added: the loop visits each node once, so it could not trip." For F-29 write "list_entries, corrupt rows in sidebar and picker. Rename of a corrupt flow is disabled by design." If `.claude/flow-roadmap.md` is still untracked, add it with `git add` like any other path.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests::step_budget`, `cargo test -j4 -p rocket-app flow_execution_service::tests::a_flow_with`, `cargo test -j4 -p rocket-app flow_execution_service::tests::a_run_over_the_limit`, `cargo test -j4 -p rocket-app flow_execution_service::tests::load_ordered_nodes`, then `cargo check -j4 -p rocket-app`.
Expected: all PASS. The 10_001-node tests build a vector of small nodes and finish in well under a second. If `validate` is slow for the 10_000-node accept test on this machine, lower the accept test to `check_step_budget(.., MAX_RUN_STEPS, MAX_RUN_STEPS)` and keep the over-limit tests as they are.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-app/src/flow_execution_service.rs .claude/flow-roadmap.md && git commit --only -m "feat(flow): refuse runs over a node count limit" -- crates/rocket-app/src/flow_execution_service.rs .claude/flow-roadmap.md`

---

## Open decisions

1. **Rename of a corrupt flow.** Recommended: disabled in the UI and `InvalidInput` in the repo, because the new name is written inside the file and the file cannot be parsed. The roadmap row said "rename and delete stay available"; delete does, rename cannot without a lossy rewrite. Alternative: rename the file only and leave the broken content, which would leave a name inside the file that disagrees with the stem.
2. **Corrupt files whose stem is not a slug (hand-made `My Broken.yml`).** Recommended: stay hidden, like a valid file whose name does not match its stem. They cannot be targeted by `delete`. Alternative: list them and add a repo method that deletes by file stem.
3. **Where the "corrupt" signal lives.** Recommended: a single optional `error` string; no separate `corrupt` boolean, since the two could disagree. The TypeScript side treats a missing `error` as healthy.
4. **Message redaction.** Recommended: replace every double-quoted segment of the serde_yaml text with `"..."`, which also applies to `get` (open and run). Alternative: show only "invalid flow file at line L column C", losing the field name and expected type.
5. **Saving over a corrupt file.** Recommended: leave `save` as it is (it overwrites) and guard in the picker only. Alternative: make `FsFlowRepo::save` refuse with `Conflict` when the existing file is unparseable, which would also block a deliberate "replace this broken file" save.
6. **Limit value and placement.** Recommended: `MAX_RUN_STEPS = 10_000` nodes, checked at run start only. Alternative: also reject at `FlowService::save`, so a user learns when the flow is made rather than when it is run.
7. **Per-step counter.** Recommended: not added, because the loop visits each node once and it could never trip. Revisit if a loop-forming node type is ever added; then the counter belongs inside the loop and `MAX_RUN_STEPS` should become a step count.

## Self-review

- Spec coverage: F-29 is Task 1 (backend) and Task 2 (sidebar, picker, open, delete, rename behavior). F-30 is Task 3, with the evidence for the node-count form stated in the task.
- Placeholders: none. All new Rust and TypeScript is shown. The few "confirm with" notes point at helpers I read in the repo (`renderPicker`, `service_with_publisher`) and name the fallback.
- Name consistency: `FlowListEntry` (Rust domain and TypeScript), `FlowListEntryDto` (IPC only), `list_entries` (trait, `FlowService`, both repos), `redact_quoted`, `failure_message`, `MAX_RUN_STEPS`, `check_step_budget`, test id `flow-corrupt-icon`, row label `Corrupt flow <name>: <message>`.
- Compile impact: `list_entries` has a default body, so `SharedFlowRepo` (partial-run tests), `FakeFlowRepo`, `PanicsOnSaveRepo`, `UntouchedRepo` and `FakeFlowRepository` need no edit. Only `list_flows` callers change type; the sole production caller is `useFlows`.
- Review Focus: each line maps to a named test above.

This is the last Tier 2 plan.
