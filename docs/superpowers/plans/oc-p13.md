# OC-P13: Migration — JSON → OpenCollection YAML

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-migrate legacy JSON collections to OpenCollection YAML.

**Prerequisite:** OC-P12 complete.

---

## Task 1: Detection logic (OpenCollection vs LegacyJson vs Empty)

- [ ] **Step 1: Create `migration.rs`. Implement detect_format() that checks for opencollection.yml or .json files.**

Tests: detect each format type.

- [ ] **Step 2: Run tests + commit**

---

## Task 2: Migration engine (convert .json → .yml recursively)

- [ ] **Step 1: Implement migrate_collection() — reads each .json, converts via domain types to OcHttpRequest, writes as .yml, deletes .json, creates opencollection.yml + folder.yml for subdirs. Idempotent.**

Tests: migrate collection with nested folders, verify .yml created + .json deleted, verify idempotent.

- [ ] **Step 2: Run tests + commit**

---

## Task 3: Wire into FsCollectionRepo

- [ ] **Step 1: Call migrate_collection() from get() and list() when legacy format detected.**

Tests: access legacy collection → auto-migrated.

- [ ] **Step 2: Run `cargo test --workspace` + commit**

---

---
