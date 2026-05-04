# Postman Import — Plan 03: Tauri Commands + Frontend

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire two new Tauri commands (`import_postman_collection`, `import_postman_environment`) and extend `ImportDialog` with a Bruno / Postman source-type toggle, Postman-specific file picker, and optional environment file picker.

**Architecture:** Backend: two new `#[tauri::command]` functions in `import_commands.rs`, registered alongside the existing Bruno commands. Frontend: `ImportDialog.tsx` gets a source state variable, conditional file picker, and updated `handleImport`. Shared `ImportReport` type — no new TypeScript types needed.

**Tech Stack:** Rust, Tauri v2, React 18, TypeScript, shadcn/ui, Lucide React

**Spec:** `docs/superpowers/specs/2026-05-02-postman-import-design.md`

**Prerequisite:** Plan 02 complete.

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-app/src/import_commands.rs` | Modify |
| `crates/rocket-app/src/lib.rs` (or `main.rs`) | Modify — register 2 new commands |
| `src/lib/tauri-api.ts` | Modify |
| `src/components/imports/ImportDialog.tsx` | Modify |

---

## Task 1: Tauri backend commands

**Files:**
- Modify: `crates/rocket-app/src/import_commands.rs`
- Modify: `crates/rocket-app/src/lib.rs` or `src-tauri/src/main.rs`

- [ ] **Step 1: Read `import_commands.rs` in full**

```bash
cat crates/rocket-app/src/import_commands.rs
```

Identify:
- The exact state type used (e.g. `tauri::State<'_, AppState>` or similar)
- How `ImportService` is constructed from state (what workspace path is extracted)
- The exact `repo` and `env_factory` construction pattern
- The return type convention (`Result<ImportReport, String>`)

- [ ] **Step 2: Read how commands are registered**

```bash
grep -rn "generate_handler" crates/rocket-app/src/ src-tauri/src/
```

Find the exact file and the `tauri::generate_handler![...]` call.

- [ ] **Step 3: Add two commands to `import_commands.rs`**

Following the exact pattern of the existing Bruno commands:

```rust
/// Import a Postman Collection JSON file (v2.0 or v2.1).
#[tauri::command]
pub async fn import_postman_collection(
    path: String,
    target_workspace_id: String,
    // use whatever state parameter the existing commands use
) -> Result<ImportReport, String> {
    // construct ImportService exactly as import_bruno_collection does
    let service = /* same construction as existing command */;
    service
        .import_postman_collection(
            &std::path::PathBuf::from(&path),
            &target_workspace_id,
        )
        .map_err(|e| e.to_string())
}

/// Import a Postman environment JSON file into an existing collection.
#[tauri::command]
pub async fn import_postman_environment(
    json_path: String,
    collection_name: String,
    target_workspace_id: String,
    // use whatever state parameter the existing commands use
) -> Result<ImportReport, String> {
    let service = /* same construction */;
    service
        .import_postman_environment(
            &std::path::PathBuf::from(&json_path),
            &collection_name,
            &target_workspace_id,
        )
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Register both commands**

In the file found in step 2, add to `tauri::generate_handler![...]`:

```rust
import_commands::import_postman_collection,
import_commands::import_postman_environment,
```

- [ ] **Step 5: Verify backend compiles**

```bash
cargo check -p rocket-app
```

Expected: compiles cleanly.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/
git commit -m "feat(app): register import_postman_collection and import_postman_environment commands"
```

---

## Task 2: TypeScript API bindings

**Files:**
- Modify: `src/lib/tauri-api.ts`

Before starting, read `/mnt/skills/public/frontend-design/SKILL.md`.

- [ ] **Step 1: Read `tauri-api.ts` in full**

```bash
cat src/lib/tauri-api.ts
```

Find `importBrunoCollection`. Use the same `invoke<ImportReport>` pattern and the same camelCase parameter naming that Tauri expects.

- [ ] **Step 2: Find the `ImportReport` type**

```bash
grep -rn "ImportReport" src/types/ src/lib/
```

Confirm the type and its fields (e.g. `imported`, `skipped`, `createdCollections`, `detectedType`).

- [ ] **Step 3: Add two new bindings**

Following the exact `importBrunoCollection` pattern:

```typescript
export async function importPostmanCollection(
  path: string,
  targetWorkspaceId: string
): Promise<ImportReport> {
  return invoke<ImportReport>('import_postman_collection', {
    path,
    targetWorkspaceId,
  });
}

export async function importPostmanEnvironment(
  jsonPath: string,
  collectionName: string,
  targetWorkspaceId: string
): Promise<ImportReport> {
  return invoke<ImportReport>('import_postman_environment', {
    jsonPath,
    collectionName,
    targetWorkspaceId,
  });
}
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(api): importPostmanCollection + importPostmanEnvironment bindings"
```

---

## Task 3: ImportDialog — source toggle + Postman pickers

**Files:**
- Modify: `src/components/imports/ImportDialog.tsx`

Before starting, read `/mnt/skills/public/frontend-design/SKILL.md`.

- [ ] **Step 1: Read `ImportDialog.tsx` in full**

```bash
cat src/components/imports/ImportDialog.tsx
```

Identify:
- All `useState` calls and their types
- How `source` (file path) state is managed and cleared
- How `handleImport` calls `importBrunoCollection`
- Where the file picker `open()` call is (Bruno uses `directory: true`)
- How `handleClose` resets state
- The dialog state machine: `idle` → `importing` → `done`
- Which lucide icons are already imported

- [ ] **Step 2: Add two new state variables**

Near the existing `useState` calls in the component body, add:

```typescript
const [importSource, setImportSource] = useState<'bruno' | 'postman'>('bruno');
const [envFilePath, setEnvFilePath] = useState<string | null>(null);
```

- [ ] **Step 3: Add the source toggle UI**

In the `idle` dialog state, directly before the file picker section, insert a toggle using `shadcn/ui` `Button` components (do not use raw HTML — use `Button` from `@/components/ui/button`):

```tsx
{/* Source type toggle */}
<div className="flex gap-1 rounded-md border border-border p-0.5 w-fit">
  <Button
    variant={importSource === 'bruno' ? 'secondary' : 'ghost'}
    size="sm"
    className="h-7 text-xs px-3"
    onClick={() => { setImportSource('bruno'); setSource(null); setEnvFilePath(null); }}
  >
    Bruno
  </Button>
  <Button
    variant={importSource === 'postman' ? 'secondary' : 'ghost'}
    size="sm"
    className="h-7 text-xs px-3"
    onClick={() => { setImportSource('postman'); setSource(null); setEnvFilePath(null); }}
  >
    Postman
  </Button>
</div>
```

Note: `setSource` may have a different name in the existing component — use whatever the existing state setter is called.

- [ ] **Step 4: Update the file picker description text**

Find the description paragraph near the file picker button. Make it conditional:

```tsx
<p className="text-xs text-muted-foreground">
  {importSource === 'bruno'
    ? 'Select a Bruno collection folder or workspace directory'
    : 'Select a Postman Collection JSON file (.json)'}
</p>
```

- [ ] **Step 5: Make the file picker call conditional on source type**

Find the `open()` call. The existing Bruno call uses `{ directory: true, multiple: false }`. Make it conditional:

```typescript
const selected = await open(
  importSource === 'bruno'
    ? { directory: true, multiple: false }
    : { directory: false, multiple: false, filters: [{ name: 'Postman Collection', extensions: ['json'] }] }
);
```

- [ ] **Step 6: Add the optional environment file picker (Postman only)**

Directly below the main source file picker display, add:

```tsx
{importSource === 'postman' && (
  <div className="space-y-1 mt-2">
    <p className="text-[11px] text-muted-foreground font-medium">
      Additional Environment JSON <span className="font-normal opacity-60">(optional — embedded environments are imported automatically)</span>
    </p>
    {envFilePath ? (
      <div className="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5">
        <FileJson className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <span className="flex-1 truncate font-mono text-[10px] text-muted-foreground">
          {envFilePath}
        </span>
        <button
          type="button"
          className="shrink-0 text-muted-foreground hover:text-foreground transition-colors"
          onClick={() => setEnvFilePath(null)}
          aria-label="Clear environment file"
        >
          ✕
        </button>
      </div>
    ) : (
      <Button
        variant="outline"
        size="sm"
        className="h-7 text-xs w-full"
        onClick={async () => {
          const selected = await open({
            directory: false,
            multiple: false,
            filters: [{ name: 'Postman Environment', extensions: ['json'] }],
          });
          if (typeof selected === 'string') setEnvFilePath(selected);
        }}
      >
        <Plus className="h-3 w-3 mr-1" />
        Select environment file
      </Button>
    )}
  </div>
)}
```

Add `FileJson` and `Plus` to the `lucide-react` import at the top of the file if they are not already imported.

- [ ] **Step 7: Update `handleImport` to branch on source type**

Find `handleImport`. Replace the `importBrunoCollection` call with a conditional:

```typescript
const handleImport = async () => {
  if (!source) return;
  setDialogState('importing');
  try {
    let result: ImportReport;

    if (importSource === 'bruno') {
      result = await importBrunoCollection(source, activeWorkspaceId);
    } else {
      result = await importPostmanCollection(source, activeWorkspaceId);

      // If an env file was selected and a collection was created, import the env too
      if (envFilePath && result.createdCollections.length > 0) {
        await importPostmanEnvironment(
          envFilePath,
          result.createdCollections[0],
          activeWorkspaceId
        );
      }
    }

    setReport(result);
    setDialogState('done');
  } catch (err) {
    setError(err instanceof Error ? err.message : String(err));
    setDialogState('idle');
  }
};
```

Add `importPostmanCollection` and `importPostmanEnvironment` to the import from `@/lib/tauri-api`. Do not remove `importBrunoCollection`.

- [ ] **Step 8: Update `handleClose` to reset new state**

In `handleClose` (or whatever the close/reset function is called), add:

```typescript
setImportSource('bruno');
setEnvFilePath(null);
```

- [ ] **Step 9: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. Fix any type errors before proceeding.

- [ ] **Step 10: Lint**

```bash
yarn lint
```

Expected: no new errors.

- [ ] **Step 11: Commit**

```bash
git add src/components/imports/ImportDialog.tsx
git commit -m "feat(ui): ImportDialog source toggle — Bruno / Postman with env file picker"
```

---

## Smoke Test (manual, after all three plans)

1. Run `yarn tauri dev`
2. Open File → Import (or Workspace Overview import button)
3. Click **Postman** in the source toggle
4. Verify the file picker label changes to "Postman Collection JSON file"
5. Select `crates/rocket-import/tests/fixtures/postman/full-collection.json`
6. Also select `environment.json` in the env picker
7. Click **Import**
8. Verify:
   - Import report shows ≥ 4 imported, and ≥ 1 skipped (formdata file entry)
   - Collections sidebar shows "Full API" with "Users" and "Auth" subfolders
   - "Users" folder: List Users (GET, query params page/limit), Create User (POST, JSON body), Get User by ID (GET, path var)
   - "Auth" folder: Login (POST, Basic auth, urlencoded body)
   - "Upload File" (POST, formdata body with text entry)
   - Collection variables `baseUrl` and `authToken` appear in collection settings
   - Environment "Local" appears in the environment switcher
9. Import the same file again → collection is auto-renamed "Full API-1"
10. Switch source back to **Bruno** → file picker reverts to folder picker
