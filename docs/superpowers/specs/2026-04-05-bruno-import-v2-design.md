# Bruno Import v2 — Design Spec

**Date:** 2026-04-05
**Status:** Approved

## Problem

The current Bruno import feature (Plans 01–05) has three gaps:

1. **ZIP-only exports.** Bruno's UI exports collections and workspaces as ZIP files. The importer only accepts directories, so users must manually unzip before importing.
2. **Wrong detection marker.** Bruno 3.0+ uses `workspace.yml` (workspace root) and `opencollection.yml` (collection root) instead of `bruno.json`. The importer gates every entry point on `bruno.json`, silently failing for all modern Bruno exports.
3. **Unnecessary user choice.** The dialog asks the user to pick "collection" or "workspace" before selecting a source — information the backend can derive from the content itself.

---

## Goals

- Accept both directory and ZIP inputs.
- Detect workspace vs collection automatically from file content.
- Support Bruno 3.0+ (OpenCollection format) and legacy Bruno 2.x side-by-side.
- For Bruno 3.0+ sources, copy files directly (no parse/convert pipeline) since the format is already OpenCollection-compatible.
- Improve the import dialog UI: drop zone with inline browse links, auto-detected type badge, no manual type selector.

---

## Detection Logic

Two helper functions replace all three hardcoded `bruno.json` checks in `importer.rs`:

```rust
enum BrunoFormat { Modern, Legacy }

fn detect_workspace(path: &Path) -> Option<BrunoFormat> {
    if path.join("workspace.yml").exists()      { Some(BrunoFormat::Modern) }
    else if path.join("bruno.json").exists()    { Some(BrunoFormat::Legacy) }
    else                                        { None }
}

fn detect_collection(path: &Path) -> Option<BrunoFormat> {
    if path.join("opencollection.yml").exists() { Some(BrunoFormat::Modern) }
    else if path.join("bruno.json").exists()    { Some(BrunoFormat::Legacy) }
    else                                        { None }
}
```

`import_auto` tries `detect_workspace` first, then `detect_collection`. If neither matches, it returns `NotABrunoDirectory`.

Within `import_workspace`, each subdirectory is probed with `detect_collection` so a workspace may contain a mix of modern and legacy collections — each handled independently.

---

## ZIP Extraction

A new `bru/zip_extractor.rs` module exposes one function:

```rust
pub(crate) fn extract_to_temp(zip_path: &Path) -> ImportResult<(TempDir, PathBuf)>
```

- Opens the ZIP and extracts to a `TempDir`.
- Bruno ZIPs always contain a single top-level directory (e.g. `my-workspace/`). The function finds that directory and returns it alongside the `TempDir` owner.
- The `TempDir` is returned to the caller to control lifetime — it is dropped (and the temp files deleted) when the caller's scope ends.
- Adds the `zip` crate to `rocket-import` dependencies.

New error variants in `ImportError`:

```rust
ZipExtractionFailed(String),  // corrupt or unreadable ZIP
EmptyZip,                     // ZIP extracted but no top-level folder found
```

---

## Import Paths

### Auto-detect entry point

```rust
// ImportService
pub fn import_auto(&self, path: &Path, workspace_id: &str) -> ImportResult<ImportReport>
pub fn import_auto_from_zip(&self, zip_path: &Path, workspace_id: &str) -> ImportResult<ImportReport>
```

`import_auto_from_zip` calls `extract_to_temp`, then delegates to `import_auto` with the extracted inner path. The `TempDir` is held in a local binding for the duration of the call.

`import_auto` routes to `import_workspace` or `import_collection` based on detection result.

### Modern path (Bruno 3.0+)

When `detect_collection` returns `BrunoFormat::Modern`:

1. Resolve collection name (existing conflict-resolution logic, unchanged).
2. `repo.create(&resolved_name)` — creates `collections/<name>/opencollection.yml` in Rocket's workspace.
3. Recursively copy all `.yml` files from the source into `collections/<name>/`, preserving subfolder structure. Skip `opencollection.yml` at the root (already written by `create`) and any `workspace.yml`.
4. Copy `environments/` subdirectory verbatim if present.
5. Count copied request files for `report.imported`.

No `.bru` parsing, no AST conversion.

### Legacy path (Bruno 2.x)

Identical to the existing implementation — walk files, parse `.bru`/`.yml` via the lexer/parser pipeline, convert via `req_converter`, write via `repo.save_request`. No changes.

---

## Report Changes

`ImportReport` gains one field:

```rust
pub detected_type: String,  // "collection" or "workspace"
```

Serialises as `detectedType` (camelCase). The frontend uses this to display the correct label in the done state.

---

## Tauri Commands

The existing two commands are replaced by two new ones with a unified auto-detect signature:

```rust
// src-tauri/src/commands/import.rs
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String>

#[tauri::command]
pub async fn import_bruno_zip(
    zip_path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String>
```

Both read the workspace path from Tauri managed state (same pattern as the existing commands post-review-fix).

The old commands (`import_bruno_collection`, `import_bruno_workspace`) are removed. The frontend and command registration in `lib.rs` are updated accordingly.

---

## TypeScript API

```typescript
// src/lib/tauri-api.ts

// Updated ImportReport — adds detectedType
export interface ImportReport {
  totalFiles: number;
  imported: number;
  skipped: SkippedItem[];
  createdWorkspace: string | null;
  createdCollections: string[];
  detectedType: 'collection' | 'workspace';
}

// Two new commands replace the four existing ones
export const importBruno = (path: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_bruno', { path, targetWorkspaceId });

export const importBrunoZip = (zipPath: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_bruno_zip', { zipPath, targetWorkspaceId });
```

The old `importBrunoCollection` and `importBrunoWorkspace` exports are removed.

---

## Dialog UI

### Layout — `ImportBrunoDialog.tsx`

Three states as before (picking / importing / done), with the picking state redesigned:

**Picking state:**

```
┌─────────────────────────────────────┐
│ Import from Bruno                   │
│ Supports Bruno 2.x and 3.x formats. │
│ Collection or workspace detected    │
│ automatically.                      │
├─────────────────────────────────────┤
│ SOURCE                              │
│                                     │
│  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐  │
│  │  ↑                            │  │
│  │  Drop a folder or ZIP here    │  │
│  │  Bruno export or directory    │  │
│  │                               │  │
│  │  or browse: [choose folder]   │  │
│  │             · [choose ZIP]    │  │
│  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘  │
├─────────────────────────────────────┤
│                  [Cancel] [Import]  │
└─────────────────────────────────────┘
```

"choose folder" and "choose ZIP" are inline underlined text links, not buttons.

**After selection (drop zone filled):**

- Drop zone border becomes solid purple, background tinted.
- Icon updates to folder (📁) or zip (🗜️) based on input type.
- Filename / directory name shown in bold.
- Path shown in a small monospace row below the drop zone with a clear (✕) control.
- Import button activates.

Note: collection vs workspace detection requires running the import — the type badge is shown only in the done state, not in the picking state.

**Done state:**

- Shows `detectedType` ("Imported as collection" / "Imported as workspace").
- Existing created-collections badges and skipped-items collapsible unchanged.

### Drag-and-drop

The drop zone handles `onDragOver` / `onDrop` events. On drop, check if `dataTransfer.files[0]` is a `.zip` (call `importBrunoZip`) or a directory (call `importBruno`). Tauri's webview supports file drops via the `@tauri-apps/plugin-drag-drop` event — wire this if available; otherwise the two inline links cover the selection flow.

---

## Testing

### Rust — Unit

- `detect_workspace`: returns `Modern` for dir with `workspace.yml`, `Legacy` for `bruno.json`, `None` for neither.
- `detect_collection`: same pattern for `opencollection.yml` / `bruno.json`.
- `extract_to_temp`: extracts a test ZIP, returns correct inner path.
- `extract_to_temp`: returns `EmptyZip` for a ZIP with no directories.

### Rust — Integration

- Modern collection import (directory with `opencollection.yml`): files copied, no conversion.
- Modern workspace import (directory with `workspace.yml` + `collections/`): all sub-collections copied.
- Legacy collection import: existing fixture tests continue to pass unchanged.
- ZIP import: create a ZIP of the existing `my-api` fixture, import via `import_auto_from_zip`, assert same output as directory import.
- Mixed workspace: workspace containing one modern and one legacy collection — both imported correctly.
- `import_auto` on a directory with neither marker → `NotABrunoDirectory`.

### Frontend

- TypeScript compiles with updated `ImportReport` type and two new API functions.
- Biome lint and format pass.

---

## Migration Notes

- Remove old Tauri command registrations: `import_bruno_collection`, `import_bruno_workspace`; register `import_bruno` and `import_bruno_zip` in `lib.rs`.
- Remove `importBrunoCollection` and `importBrunoWorkspace` from `tauri-api.ts`; add `importBruno` and `importBrunoZip`.
- Update `ImportBrunoDialog.tsx` — remove `mode` state, RadioGroup, and the `importBrunoCollection`/`importBrunoWorkspace` call sites entirely.
- Add `.superpowers/` to `.gitignore` if not already present (brainstorm session artifacts).
