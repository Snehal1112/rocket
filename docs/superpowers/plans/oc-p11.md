# OC-P11: FsCollectionRepo — Read/Write .yml + opencollection.yml + folder.yml

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite FsCollectionRepo to read/write OpenCollection YAML `.yml` files.

**Tech Stack:** Rust, serde_yaml

**Prerequisite:** OC-P10 complete.

---

## Task 1: Create collection writes opencollection.yml + detect by presence

- [ ] **Step 1: Modify create() to write opencollection.yml, list() to detect by opencollection.yml presence, update is_request_file() for .yml + exclude reserved names**

Tests: create collection → opencollection.yml exists, list only dirs with opencollection.yml, reserved file exclusion.

- [ ] **Step 2: Run tests + commit**

---

## Task 2: Save/load requests as .yml using OcHttpRequest

- [ ] **Step 1: Modify save_request() to convert domain→OcHttpRequest→serde_yaml, get_request() to parse .yml→OcHttpRequest→domain. Update build_folder_tree() to scan .yml. Update ALL existing tests to .yml extension.**

Tests: save request as .yml, load back, verify all fields. Subfolder requests.

- [ ] **Step 2: Run tests + commit**

---

## Task 3: Folder.yml read/write + workspace integration

- [ ] **Step 1: create_folder() writes folder.yml with OcFolderInfo. build_folder_tree() reads folder.yml for folder config. Verify full workspace tests pass.**

Tests: create folder → folder.yml exists, folder.yml not treated as request.

- [ ] **Step 2: Run `cargo test --workspace` + commit**

---

## Milestone Checklist — OC-P11

- [ ] opencollection.yml created on collection create
- [ ] .yml files read/written via OcHttpRequest conversions
- [ ] folder.yml created on folder create
- [ ] Reserved files excluded from request listing
- [ ] All existing tests updated to .yml
- [ ] `cargo test --workspace` passes

---

---
