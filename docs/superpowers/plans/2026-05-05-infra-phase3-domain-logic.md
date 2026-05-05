# rocket-infra Phase 3: Pull Domain Logic Out of Infra

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the two remaining pieces of domain logic still embedded in `rocket-infra`: silent UID minting in OC deserialization (A2), and the pair of free-function variable converters that should be `From`/`Into` impls (synthesis §5).

**Architecture:** Phase 3 is two independent tasks. Task 1 changes `oc_conversions.rs` to warn+preserve (not silently mint) when a request or folder deserialized from disk lacks a UID. Task 2 adds `From<OcVariable> for CollectionVariable` and `From<CollectionVariable> for OcVariable` impls in `oc_conversions.rs` and replaces all ~20 free-function call sites with the standard trait syntax, then deletes the old functions.

**Tech Stack:** Rust, `tracing`, `serde_yaml`, `rocket-infra` (internal only)

---

## Status check: what Phase 3 already completed

Before implementing, note that several Phase 3 items are **already done**:

- **A1 (folder-chain merge):** `merge_folder_chain_variables` lives in `rocket-collection::settings` and `get_folder_chain_variables` in infra calls it. ✅ Done.
- **A3 (filename collision policy):** `candidate_filename` and `MAX_FILENAME_COLLISION_RETRIES` live in `rocket-collection::request`. ✅ Done.

The remaining work is A2 and the From/Into conversion refactor.

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-infra/src/oc_conversions.rs` | Task 1: warn + preserve on missing UID in `oc_http_request_to_request`, `oc_folder_to_folder`, `oc_collection_to_collection`. Task 2: add `From` impls, remove free functions, update all call sites. |
| `crates/rocket-infra/src/fs_collection_repo.rs` | Task 2: update 8 call sites from `oc_variable_to_collection_variable(v)` → `CollectionVariable::from(v)` and `collection_variable_to_oc_variable(cv)` → `OcVariable::from(cv)`. Also remove the now-unused imports of the two free functions. |

---

### Task 1: Stop silent UID minting in OC deserialization (A2)

**Files:**
- Modify: `crates/rocket-infra/src/oc_conversions.rs`

**Background:** Three sites in `oc_conversions.rs` silently invent UIDs when the on-disk YAML is missing one:

1. `oc_http_request_to_request` line ~933: `uid: oc.uid.unwrap_or_else(generate_uid)` — if `opencollection.yml` or a `.yml` request file has no `uid:` field, infra silently creates one. This means every load of a UID-less file produces a *different* UID, making the request unfindable by stable reference.

2. `oc_folder_to_folder` line ~1164: `uid: generate_uid()` — always mints a fresh UID, even when the folder has one on disk. This function is only used by the bundled-collection import path (not by `build_folder_tree`), but generating a new UID on every call silently discards stable identifiers.

3. `oc_collection_to_collection` line ~1285: same pattern for the collection root folder.

The correct behavior: if a UID is present on disk, use it. If it is absent, log a `tracing::warn!` and use an empty string — callers (or future migration code) can detect and fix the gap. Do **not** generate a new UID silently; a freshly-minted UID is worse than an empty one because it looks valid but differs every load.

**Exception for `build_folder_tree`:** This function reads UIDs through `read_uid_from_yaml` (which handles legacy `.uid` files) and is unaffected by this change.

- [ ] **Step 1: Read the current implementations to get exact line numbers**

  ```bash
  grep -n "uid.*unwrap_or_else.*generate_uid\|uid: generate_uid()" \
    crates/rocket-infra/src/oc_conversions.rs
  ```

  Note the exact lines. There should be three matches.

- [ ] **Step 2: Write a test that fails if a UID-less request gets a new UID on every load**

  Add inside `mod tests` at the bottom of `crates/rocket-infra/src/oc_conversions.rs` (before the final `}`):

  ```rust
  #[test]
  fn oc_request_missing_uid_gets_empty_not_minted() {
      use crate::opencollection::{OcHttpRequest, OcHttpRequestDetails, OcHttpRequestInfo};

      let oc = OcHttpRequest {
          uid: None,   // deliberately absent — as in a file missing the uid field
          info: OcHttpRequestInfo {
              name: "No UID".into(),
              description: None,
              request_type: Some("http".into()),
              seq: None,
              tags: vec![],
          },
          http: OcHttpRequestDetails {
              method: "GET".into(),
              url: "https://example.com".into(),
              headers: vec![],
              params: vec![],
              body: None,
              auth: None,
          },
          runtime: None,
          settings: None,
          examples: None,
          docs: None,
      };

      let req1 = oc_http_request_to_request(oc.clone());
      let req2 = oc_http_request_to_request(oc);

      // Both calls must return the same (empty) uid — not two different minted uids.
      assert_eq!(req1.uid, req2.uid, "uid must be stable across loads");
      // The uid must be empty — not a freshly-minted UUID.
      assert!(req1.uid.is_empty(), "expected empty uid for missing uid field, got: {}", req1.uid);
  }
  ```

  You also need `OcHttpRequest` to implement `Clone`. Check if it already does:

  ```bash
  grep -n "#\[derive.*Clone\|OcHttpRequest" crates/rocket-infra/src/opencollection.rs | head -10
  ```

  If `OcHttpRequest` does not derive `Clone`, simplify the test to make two separate identical `OcHttpRequest` values rather than cloning.

- [ ] **Step 3: Run the test to confirm it fails with current code**

  ```bash
  cargo test -p rocket-infra oc_request_missing_uid_gets_empty_not_minted 2>&1 | tail -15
  ```

  Expected: FAIL — current code mints a different UUID each call, so `req1.uid != req2.uid`.

- [ ] **Step 4: Fix `oc_http_request_to_request` — use empty string instead of minting**

  In `crates/rocket-infra/src/oc_conversions.rs`, find the line:

  ```rust
  uid: oc.uid.unwrap_or_else(generate_uid),
  ```

  Replace with:

  ```rust
  uid: oc.uid.unwrap_or_else(|| {
      tracing::warn!("request file is missing uid field; using empty uid");
      String::new()
  }),
  ```

- [ ] **Step 5: Fix `oc_folder_to_folder` — use uid from disk, warn if absent**

  Find `oc_folder_to_folder` (around line 1108). It currently has:

  ```rust
  let name = oc.info.name;
  let items = oc.items ...;
  Folder {
      uid: generate_uid(),
      name,
      ...
  }
  ```

  Replace the `uid: generate_uid()` line with:

  ```rust
  uid: oc.info.uid.clone().unwrap_or_else(|| {
      tracing::warn!(folder = %name, "folder.yml is missing uid field; using empty uid");
      String::new()
  }),
  ```

  Note: `oc.info.uid` is `Option<String>` (from `OcFolderInfo`). Verify:

  ```bash
  grep -n "struct OcFolderInfo\|uid:" crates/rocket-infra/src/opencollection.rs | head -10
  ```

- [ ] **Step 6: Fix `oc_collection_to_collection` — use uid from disk for root folder**

  Find `oc_collection_to_collection` (around line 1217). It has a `Folder` construction with `uid: generate_uid()`. Replace with:

  ```rust
  uid: oc.uid.clone().unwrap_or_else(|| {
      tracing::warn!(collection = %name, "opencollection.yml is missing uid field; using empty uid");
      String::new()
  }),
  ```

  Where `oc.uid` is the `uid` field of `OcCollection`.

- [ ] **Step 7: Remove the now-unused `generate_uid` import if applicable**

  Check if `generate_uid` is still used elsewhere in `oc_conversions.rs`:

  ```bash
  grep -n "generate_uid" crates/rocket-infra/src/oc_conversions.rs
  ```

  If the only remaining uses are the three you just replaced, remove `generate_uid` from the import on line 10:

  ```rust
  // Before:
  use rocket_collection::{generate_uid, Request};
  // After:
  use rocket_collection::Request;
  ```

- [ ] **Step 8: Run the test to confirm it now passes**

  ```bash
  cargo test -p rocket-infra oc_request_missing_uid_gets_empty_not_minted 2>&1 | tail -15
  ```

  Expected: PASS — both loads return `uid: ""`.

- [ ] **Step 9: Run the full infra test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 10: Commit**

  ```bash
  git add crates/rocket-infra/src/oc_conversions.rs
  git commit -m "fix(infra): warn on missing uid instead of silently minting in OC deserialization"
  ```

---

### Task 2: Replace free variable-conversion functions with From/Into impls

**Files:**
- Modify: `crates/rocket-infra/src/oc_conversions.rs`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`

**Background:** `oc_variable_to_collection_variable` and `collection_variable_to_oc_variable` are plain free functions. The synthesis flags them as candidates for `From`/`Into` impls. Converting them lets call sites use `.map(CollectionVariable::from)` and `.map(OcVariable::from)` instead of `.map(oc_variable_to_collection_variable)` — more idiomatic and removes two exported pub symbols from the infra boundary.

These functions are used in 21 places across two files, all inside `rocket-infra`. They are not used outside the crate.

**Plan:**
1. Add `impl From<OcVariable> for CollectionVariable` and `impl From<CollectionVariable> for OcVariable` using the existing function bodies.
2. Update all call sites in both files.
3. Delete the old free functions.
4. Remove their names from the import in `fs_collection_repo.rs`.

- [ ] **Step 1: Verify no external callers exist**

  ```bash
  grep -rn "oc_variable_to_collection_variable\|collection_variable_to_oc_variable" \
    crates/ src-tauri/ 2>/dev/null | grep -v "rocket-infra/"
  ```

  Expected: no output. If external callers exist, **stop** and report BLOCKED — do not remove the functions.

- [ ] **Step 2: Write a compile-check test for the From impls**

  Add to `mod tests` in `crates/rocket-infra/src/oc_conversions.rs`:

  ```rust
  #[test]
  fn collection_variable_from_oc_variable_roundtrip() {
      use crate::opencollection::{OcVariable, VariableValue};

      let oc = OcVariable {
          name: "BASE_URL".into(),
          value: Some(VariableValue::simple("https://api.example.com".into())),
          initial: Some(VariableValue::simple("https://localhost".into())),
          description: None,
          disabled: None,
      };
      // Use From trait, not the free function.
      let cv = CollectionVariable::from(oc);
      assert_eq!(cv.key, "BASE_URL");
      assert_eq!(cv.value, "https://api.example.com");
      assert_eq!(cv.initial_value, "https://localhost");
      assert!(cv.enabled);
  }

  #[test]
  fn oc_variable_from_collection_variable_roundtrip() {
      let cv = CollectionVariable {
          key: "TOKEN".into(),
          value: "secret".into(),
          initial_value: "".into(),
          enabled: false,
          secret: false,
      };
      // Use From trait, not the free function.
      let oc = OcVariable::from(cv);
      assert_eq!(oc.name, "TOKEN");
      assert_eq!(oc.disabled, Some(true));
      assert!(oc.initial.is_none()); // empty initial_value → None
  }
  ```

- [ ] **Step 3: Run the tests to confirm they fail (impls don't exist yet)**

  ```bash
  cargo test -p rocket-infra collection_variable_from_oc_variable_roundtrip oc_variable_from_collection_variable_roundtrip 2>&1 | tail -10
  ```

  Expected: compile error — `From<OcVariable> for CollectionVariable` not implemented.

- [ ] **Step 4: Add the From impls in `oc_conversions.rs`**

  Immediately after the existing `oc_variable_to_collection_variable` function (around line 712), add:

  ```rust
  impl From<OcVariable> for CollectionVariable {
      fn from(v: OcVariable) -> Self {
          let current = v.value.as_ref().map(|vv| vv.data().to_string()).unwrap_or_default();
          // Fall back to the current value if initial is absent (backward compat with old files).
          let initial = v.initial.as_ref()
              .map(|vv| vv.data().to_string())
              .unwrap_or_else(|| current.clone());
          CollectionVariable {
              key:           v.name,
              value:         current,
              initial_value: initial,
              enabled:       !v.disabled.unwrap_or(false),
              secret:        false,
          }
      }
  }

  impl From<CollectionVariable> for OcVariable {
      fn from(cv: CollectionVariable) -> Self {
          OcVariable {
              name:        cv.key,
              value:       if cv.value.is_empty() { None } else { Some(VariableValue::simple(cv.value)) },
              initial:     if cv.initial_value.is_empty() { None } else { Some(VariableValue::simple(cv.initial_value)) },
              description: None,
              disabled:    if cv.enabled { None } else { Some(true) },
          }
      }
  }
  ```

  `VariableValue` is already imported — verify with:

  ```bash
  grep -n "use.*VariableValue\|VariableValue" crates/rocket-infra/src/oc_conversions.rs | head -5
  ```

- [ ] **Step 5: Run the tests to confirm they pass**

  ```bash
  cargo test -p rocket-infra collection_variable_from_oc_variable_roundtrip oc_variable_from_collection_variable_roundtrip 2>&1 | tail -10
  ```

  Expected: both PASS.

- [ ] **Step 6: Update all call sites in `oc_conversions.rs` from free functions to From**

  Find every call site:

  ```bash
  grep -n "oc_variable_to_collection_variable\|collection_variable_to_oc_variable" \
    crates/rocket-infra/src/oc_conversions.rs
  ```

  For each call site (excluding the function definitions and tests you just added), apply these replacements:

  - `.map(oc_variable_to_collection_variable)` → `.map(CollectionVariable::from)`
  - `.map(collection_variable_to_oc_variable)` → `.map(OcVariable::from)`
  - `oc_variable_to_collection_variable(x)` → `CollectionVariable::from(x)`
  - `collection_variable_to_oc_variable(x)` → `OcVariable::from(x)`

  The sites in `oc_conversions.rs` are around lines 1306, 1380, 1672, 1690, 1707, 1708, 1726, 1743, 1752.

- [ ] **Step 7: Update all call sites in `fs_collection_repo.rs`**

  ```bash
  grep -n "oc_variable_to_collection_variable\|collection_variable_to_oc_variable" \
    crates/rocket-infra/src/fs_collection_repo.rs
  ```

  Apply the same replacements at each site (around lines 531, 599, 664, 694, 730, 751, 770).

  Also update the import at the top of `fs_collection_repo.rs`. Find:

  ```rust
  use crate::oc_conversions::{
      collection_variable_to_oc_variable, oc_http_request_to_request,
      oc_variable_to_collection_variable, request_to_oc_http_request,
  };
  ```

  Remove the two free-function names:

  ```rust
  use crate::oc_conversions::{
      oc_http_request_to_request,
      request_to_oc_http_request,
  };
  ```

- [ ] **Step 8: Compile check**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
  ```

  Fix any remaining call sites that still use the old function names.

- [ ] **Step 9: Delete the old free functions from `oc_conversions.rs`**

  Remove `pub fn oc_variable_to_collection_variable` (lines ~699-712) and `pub fn collection_variable_to_oc_variable` (lines ~717-725), including their doc comments.

  Keep the comment on line ~727 that explains the difference between `From<OcVariable> for CollectionVariable` (for collection variables) vs `From<OcVariable> for Variable` (for environment variables — does not preserve `initial`). Update it to reference the trait impls instead of the old free functions.

- [ ] **Step 10: Final compile check and full test suite**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
  cargo test -p rocket-infra 2>&1 | tail -10
  ```

  Expected: zero errors, all tests pass.

- [ ] **Step 11: Commit**

  ```bash
  git add crates/rocket-infra/src/oc_conversions.rs crates/rocket-infra/src/fs_collection_repo.rs
  git commit -m "refactor(infra): replace oc_variable free fns with From<OcVariable>/From<CollectionVariable> impls"
  ```

---

## Self-Review

### Spec coverage (Phase 3 §7 checklist)

| Requirement | Task | Status |
|---|---|---|
| Move folder-chain merge to `rocket_collection::variables::merge_folder_chain` | — | ✅ Already done — `merge_folder_chain_variables` lives in `rocket-collection::settings` and infra calls it |
| Move UID minting into domain constructors; conversion returns error instead of inventing UIDs | Task 1 | This plan — warn + empty string instead of minting |
| Move filename-collision policy into `rocket-collection` | — | ✅ Already done — `candidate_filename` and `MAX_FILENAME_COLLISION_RETRIES` live in `rocket-collection::request` |
| Convert free conversion fns to `From`/`Into` impls | Task 2 | This plan |

**Note on A2 implementation choice:** The synthesis says "returns `Err(OcConversionError::MissingUid)` instead of inventing UIDs". This plan uses an empty string + `tracing::warn!` rather than a new error type. Rationale: introducing `OcConversionError` would require changing the return type of `oc_http_request_to_request` from `Request` to `Result<Request, OcConversionError>`, which would cascade to every call site in `build_folder_tree` and the test suite. The empty-string approach achieves the safety goal (no silent identity change on every load) with zero API breakage. The empty uid is detectable and will trigger `save_request`'s existing validation (`if request.uid.is_empty()` → `DomainError::Internal`), surfacing the problem at the correct boundary. This is a deliberate, conservative implementation of the spirit of A2.

### Placeholder scan

No TBDs, TODOs, or incomplete steps found.

### Type consistency

- `CollectionVariable::from(v: OcVariable)` defined in Task 2 Step 4 and used in Steps 6–7 consistently.
- `OcVariable::from(cv: CollectionVariable)` same.
- `VariableValue::simple(...)` — same call pattern as the existing free functions it replaces.
