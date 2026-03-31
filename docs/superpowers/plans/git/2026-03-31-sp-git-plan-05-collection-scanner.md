# Plan 5: Clone Dialog — Collection Scanner Backend & Wiring

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `scan_collections_in_path` Tauri command that discovers `.yml` collection files in a given directory, plus the frontend API export.

**Architecture:** The collection discovery logic should already exist in `rocket-collection` (used when listing collections). This plan wires it as a standalone Tauri command that takes any path and returns discovered collections. If the logic doesn't exist as a reusable function, we create a minimal scanner.

**Tech Stack:** Rust (Tauri v2), TypeScript

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 3

**Depends on:** Plans 1–3 (backend remote CRUD must compile)

**Constraints:** Only scan for `.yml` collection files. No `.json` files.

---

## Chunk 1: Collection Scanner & Tauri Command

### Task 1: Investigate existing collection discovery logic

**Files:**
- Read: `crates/rocket-collection/src/` — browse to understand how collections are discovered
- Read: `crates/rocket-infra/src/` — the filesystem implementation that lists collections

- [ ] **Step 1: Find existing collection listing logic**

Browse `crates/rocket-collection/` and `crates/rocket-infra/` to find how collections are currently discovered on disk. Look for:
- A repository trait method like `list` or `list_collections`
- The filesystem implementation that scans directories for collection `.yml` files
- What file name or pattern identifies a collection (e.g., `collection.yml`, `bruno.yml`, or some manifest file)

Note the exact function signatures, file paths, and collection identification pattern.

- [ ] **Step 2: Determine if the existing logic can be reused**

Check if the existing collection scanner:
- Takes an arbitrary path (good — can reuse)
- Is hardcoded to a specific workspace/app data path (needs wrapping)

Document your findings before proceeding.

- [ ] **Step 3: Commit findings as a comment in the plan**

No code changes. Just ensure you understand the pattern before Task 2.

### Task 2: Create or wire the `scan_collections_in_path` Tauri command

**Files:**
- Possibly modify: `crates/rocket-collection/src/` (if a reusable scanner function needs extracting)
- Modify: the Tauri commands file where collection commands live (find by browsing `src-tauri/src/`)
- Modify: `src-tauri/src/lib.rs` (register the new command)

- [ ] **Step 1: Create the Tauri command**

The command should:
- Take `path: String` (directory to scan)
- Return `Vec<CollectionScanResult>` where:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionScanResult {
    pub name: String,
    pub path: String,
}
```

- Scan the given directory for collection `.yml` manifest files
- Return the name and path for each discovered collection

If the existing `rocket-collection` crate has a function that can do this, call it. If not, implement a minimal scanner:
- Walk immediate subdirectories of the given path
- Look for the collection identifier file (whatever pattern the codebase uses — e.g., `collection.yml` or a specific YAML structure)
- Return name (from the YAML or directory name) and absolute path

- [ ] **Step 2: Register the command in `src-tauri/src/lib.rs`**

Add `scan_collections_in_path` to the `.invoke_handler(tauri::generate_handler![...])` list.

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/ crates/
git commit -m "feat(tauri): add scan_collections_in_path command for clone dialog"
```

### Task 3: Add frontend API export and type

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `CollectionScanResult` interface**

Add in the `// Git types` section (or a new `// Clone types` section):

```typescript
export interface CollectionScanResult {
  name: string;
  path: string;
}
```

- [ ] **Step 2: Add `scanCollectionsInPath` API function**

Add after the git remote API functions:

```typescript
export const scanCollectionsInPath = (path: string) =>
  invoke<CollectionScanResult[]>("scan_collections_in_path", { path });
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add scanCollectionsInPath API export"
```
