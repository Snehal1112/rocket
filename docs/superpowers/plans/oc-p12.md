# OC-P12: Remaining Repos + File Watcher

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update FsEnvironmentRepo (using OcEnvironment format), FsHistoryRepo, FsTemplateRepo, FsCookieRepo to .yml, and update file watcher.

**Prerequisite:** OC-P11 complete.

---

## Task 1: FsEnvironmentRepo → .yml with OcEnvironment format

- [ ] **Step 1: Rewrite save/get/list to use serde_yaml + OcEnvironment conversions. Update file extension to .yml.**

Tests: save environment with all OC fields (color, extends, secret vars), load back, verify.

- [ ] **Step 2: Run tests + commit**

---

## Task 2: FsHistoryRepo + FsTemplateRepo + FsCookieRepo → .yml

- [ ] **Step 1: Replace serde_json with serde_yaml in all three repos. Update file extensions to .yml. These are internal files (not OC spec), so domain types serialize directly via serde_yaml.**

- [ ] **Step 2: Update tests + commit**

---

## Task 3: File watcher → .yml

- [ ] **Step 1: Update file watcher to monitor .yml instead of .json. Run full workspace test.**

- [ ] **Step 2: `cargo test --workspace` + `cargo clippy --workspace` + commit**

---

---
