# Plan 03 — Domain events

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add five workspace domain events to `DomainEvent` in `rocket-shared`.

**Architecture:** `rocket-shared/src/events.rs` already defines `DomainEvent` as a serde-tagged enum. We add five new variants following the same pattern as existing collection/environment events.

**Tech Stack:** Rust, serde

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-02-fs-workspace-repo.md`
**Next plan:** `plan-04-workspace-service.md`

---

### Task 1: Add workspace events to DomainEvent

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

- [ ] **Step 1: Open `crates/rocket-shared/src/events.rs` and locate the `DomainEvent` enum**

The existing variants look like:
```rust
CollectionCreated { name: String },
CollectionDeleted { name: String },
// ...
EnvironmentSaved { name: String },
```

- [ ] **Step 2: Add five workspace variants after the environment events block**

```rust
// Workspace events
WorkspaceCreated  { id: String, name: String, path: String },
WorkspaceSwitched { id: String, name: String, path: String },
WorkspaceRenamed  { id: String, old_name: String, new_name: String },
WorkspaceClosed   { id: String },
WorkspaceDeleted  { id: String },
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build -p rocket-shared
```

Expected: compiles with no errors or warnings about the new variants.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(workspace): add WorkspaceCreated/Switched/Renamed/Closed/Deleted domain events"
```

---

### Task 2: Test event serialization

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (add tests)

- [ ] **Step 1: Add serialization tests for workspace events in the existing `#[cfg(test)]` block**

```rust
#[test]
fn workspace_created_serializes() {
    let event = DomainEvent::WorkspaceCreated {
        id: "abc-123".into(),
        name: "My API".into(),
        path: "/home/user/my-api".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("workspaceCreated") || json.contains("WorkspaceCreated"));
    assert!(json.contains("abc-123"));
    assert!(json.contains("My API"));
}

#[test]
fn workspace_switched_serializes() {
    let event = DomainEvent::WorkspaceSwitched {
        id: "def-456".into(),
        name: "Staging".into(),
        path: "/home/user/staging".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("def-456"));
}

#[test]
fn workspace_renamed_serializes() {
    let event = DomainEvent::WorkspaceRenamed {
        id: "abc-123".into(),
        old_name: "Old".into(),
        new_name: "New".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("Old"));
    assert!(json.contains("New"));
}

#[test]
fn workspace_closed_serializes() {
    let event = DomainEvent::WorkspaceClosed { id: "abc-123".into() };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("abc-123"));
}

#[test]
fn workspace_deleted_serializes() {
    let event = DomainEvent::WorkspaceDeleted { id: "abc-123".into() };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("abc-123"));
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-shared
```

Expected: all tests pass including the 5 new ones.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "test(workspace): add serialization tests for workspace domain events"
```

---

### Task 3: Verify no downstream breakage

**Files:** None (compile check only)

After adding new enum variants, verify that all crates that match on `DomainEvent` still compile. Rust will warn (or error if exhaustive match) about unhandled variants.

- [ ] **Step 1: Build the entire workspace**

```bash
cargo build --workspace
```

- [ ] **Step 2: Fix any exhaustive match errors**

If any crate has a `match event { ... }` without a wildcard `_` arm, add the new workspace variants or a wildcard. Example fix:

```rust
match event {
    DomainEvent::CollectionCreated { name } => { /* existing */ }
    // ... other existing arms ...
    DomainEvent::WorkspaceCreated { .. }
    | DomainEvent::WorkspaceSwitched { .. }
    | DomainEvent::WorkspaceRenamed { .. }
    | DomainEvent::WorkspaceClosed { .. }
    | DomainEvent::WorkspaceDeleted { .. } => { /* handled by frontend via Tauri events */ }
}
```

- [ ] **Step 3: Run full test suite**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(workspace): handle new workspace event variants in existing match arms"
```
