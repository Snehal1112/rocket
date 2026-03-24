# SP2 Plan 0: SP1 Bug Fixes

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 6 known SP1 issues: blank sidebar, history panel not wired, environment CRUD broken, file watcher not emitting events, broken production build, and Go backend not archived.

**Architecture:** These are frontend wiring issues (api.ts ↔ tauri-api.ts ↔ Rust commands) plus a Tauri config issue. No domain/infra changes needed.

**Tech Stack:** React, TypeScript, Tauri 2.0, shadcn/ui

---

## Chunk 1: Diagnose and fix frontend wiring

### Task 1: Diagnose the root cause

**Files:**
- Inspect: `frontend/src/lib/api.ts`
- Inspect: `frontend/src/lib/tauri-api.ts`
- Inspect: browser console (F12 in Tauri dev window)
- Inspect: terminal output from `cargo tauri dev`

- [ ] **Step 1: Run the app in dev mode and capture errors**

```bash
cargo tauri dev
```

Open browser console (F12 → Console tab). Copy all red error lines.
Check terminal for Rust panics or errors.

- [ ] **Step 2: Verify the Tauri API bridge is being imported**

```bash
cd frontend && grep -rn "from.*tauri-api\|from.*@tauri-apps" src/lib/api.ts src/components/ src/hooks/ src/features/ --include="*.ts" --include="*.tsx" | head -20
```

Expected: `api.ts` should re-export from `tauri-api.ts`. Components should import from `@/lib/api`.

- [ ] **Step 3: Verify invoke command names match Rust**

Check that every `invoke('command_name')` in `tauri-api.ts` matches a `#[tauri::command]` function registered in `src-tauri/src/lib.rs` `generate_handler![]`.

```bash
# List all invoke calls in frontend
grep -rn "invoke(" frontend/src/lib/tauri-api.ts | sed "s/.*invoke('\([^']*\)'.*/\1/" | sort

# List all registered commands in Rust
grep -rn "commands::" src-tauri/src/lib.rs | grep "::" | sed 's/.*commands::\([^,]*\).*/\1/' | sort
```

Compare the two lists — mismatches are the likely cause.

- [ ] **Step 4: Verify argument names match**

Tauri's `invoke` requires argument names to match the Rust function parameter names exactly (camelCase in TS → snake_case in Rust is handled by serde, but the top-level invoke args must match).

Example mismatch:
```typescript
// Frontend: invoke('create_collection', { name: 'test' })
// Rust: fn create_collection(collection_name: String)  ← WRONG, param is "collection_name" not "name"
```

- [ ] **Step 5: Document all found issues**

Create a checklist of specific fixes needed before proceeding.

- [ ] **Step 6: Commit diagnostic notes**

```bash
git add -A
git commit -m "chore: diagnose SP1 frontend wiring issues"
```

---

### Task 2: Fix sidebar (collections not loading)

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts` (if invoke names wrong)
- Modify: `frontend/src/lib/api.ts` (if re-exports missing)
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx` (if import path wrong)

- [ ] **Step 1: Fix the specific issue found in Task 1**

The sidebar is blank, meaning one of:
a) `listCollections()` invoke fails silently → add `.catch(console.error)` to debug
b) The response shape doesn't match what the component expects (e.g., `requestCount` vs `request_count`)
c) The component isn't calling the API at all (import path issue)

Fix whichever issue was identified in Task 1.

- [ ] **Step 2: Verify fix**

```bash
cargo tauri dev
```
Expected: sidebar shows collections from `~/.rocket-api/collections/`. Create a test collection folder manually if none exist:
```bash
mkdir -p ~/.rocket-api/collections/test-api
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/
git commit -m "fix: sidebar collections loading via Tauri invoke"
```

---

### Task 3: Fix environment CRUD

**Files:**
- Modify: relevant frontend components + tauri-api.ts as needed

- [ ] **Step 1: Apply same diagnostic pattern as sidebar**

Check invoke name, argument names, response shape.

- [ ] **Step 2: Verify fix**

Create environment, add variables, save, reload app → environment persists.

- [ ] **Step 3: Commit**

```bash
git commit -am "fix: environment CRUD wiring"
```

---

### Task 4: Wire history panel

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts` (ensure history functions exported)
- Modify: `frontend/src/lib/api.ts` (ensure re-exports)
- Modify: history panel component (connect to API)

- [ ] **Step 1: Identify the history panel component**

```bash
grep -rn "history\|History" frontend/src/components/ --include="*.tsx" -l
```

- [ ] **Step 2: Wire the component to use Tauri API**

Ensure it calls `listHistory()` from `@/lib/api` and renders the results.

- [ ] **Step 3: Verify fix**

Execute a request → check history panel shows the entry.

- [ ] **Step 4: Commit**

```bash
git commit -am "fix: history panel wiring to Tauri commands"
```

---

### Task 5: Fix file watcher events

**Files:**
- Modify: `src-tauri/src/lib.rs` (ensure `watch_collections` called on setup)
- Modify: `frontend/src/features/realtime/hooks/useRealtimeSync.ts`

- [ ] **Step 1: Ensure file watcher starts on app launch**

In `src-tauri/src/lib.rs` `.setup()`, verify that `NotifyFileWatcher::start()` is called with the correct collections directory and a working `EventPublisher`.

The issue is likely that the watcher is using `NullEventPublisher` instead of `TauriEventBus`. Fix:
```rust
// In setup(), create a TauriEventBus for the watcher
let watcher_bus = Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone()));
let _ = watcher.start(collections_dir, watcher_bus);
```

- [ ] **Step 2: Verify frontend listener is registered**

Check that `useRealtimeSync` calls `listen('file-change')` and that the callback refreshes the sidebar.

- [ ] **Step 3: Test**

With app running, create a file in `~/.rocket-api/collections/` from terminal → sidebar should refresh.

- [ ] **Step 4: Commit**

```bash
git commit -am "fix: file watcher using TauriEventBus instead of NullPublisher"
```

---

### Task 6: Fix production build

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Create: icon files if missing

- [ ] **Step 1: Identify the build error**

```bash
cargo tauri build 2>&1 | tail -30
```

Common issues:
a) Missing icon files → generate with `cargo tauri icon`
b) Wrong `frontendDist` path → should be `../frontend/dist`
c) `beforeBuildCommand` fails → should be `cd ../frontend && npm run build`

- [ ] **Step 2: Fix the specific issue**

If icons missing:
```bash
# Create a placeholder 1024x1024 PNG, then:
cargo tauri icon path/to/icon.png
```

If path issues, update `tauri.conf.json`.

- [ ] **Step 3: Verify**

```bash
cargo tauri build
```
Expected: produces installer in `src-tauri/target/release/bundle/`.

- [ ] **Step 4: Commit**

```bash
git commit -am "fix: production build configuration"
```

---

### Task 7: Archive Go backend

- [ ] **Step 1: Move backend directory**

```bash
mv backend backend-legacy
```

- [ ] **Step 2: Update any references**

```bash
grep -rn "backend/" . --include="*.json" --include="*.toml" --include="*.md" | grep -v "backend-legacy" | grep -v node_modules | grep -v target
```

Fix any remaining references.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: archive Go backend to backend-legacy"
```

---

## Milestone Checklist — Plan 0

- [ ] Collections sidebar loads and displays collections
- [ ] Environment CRUD works (create, edit variables, save, delete)
- [ ] History panel shows executed requests
- [ ] File watcher detects external changes and refreshes UI
- [ ] `cargo tauri build` produces working installer
- [ ] Go backend archived to `backend-legacy/`
- [ ] All previously passing tests still pass: `cargo test --workspace`
