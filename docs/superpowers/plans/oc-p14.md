# OC-P14: Frontend — .yml Handling + End-to-End

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update frontend to use .yml file extensions and verify everything end-to-end.

**Prerequisite:** OC-P13 complete.

---

## Task 1: Tauri API bridge — .yml extensions

- [ ] **Step 1: Search and replace all .json file extension references in frontend API calls. Update to .yml.**

- [ ] **Step 2: Commit**

---

## Task 2: Sidebar — .yml display + reserved file filtering

- [ ] **Step 1: Update sidebar tree to strip .yml from display names. Filter out opencollection.yml and folder.yml from tree.**

- [ ] **Step 2: Commit**

---

## Task 3: End-to-end verification

- [ ] **Step 1: Run full test suite: `cargo test --workspace` + `npx vitest run`**

- [ ] **Step 2: Manual smoke test in `cargo tauri dev`**

Verify:
- [ ] Create new collection → opencollection.yml on disk
- [ ] Create new request → .yml file
- [ ] Edit + save → .yml updated
- [ ] Open legacy JSON collection → auto-migrated to .yml
- [ ] Environment files as .yml
- [ ] Git diff shows readable YAML
- [ ] All 4 protocol types (HTTP, GraphQL, gRPC, WebSocket) can be created/saved/loaded

- [ ] **Step 3: Final commit**

```bash
git add .
git commit -m "feat: OpenCollection full spec support — complete"
```

---

## Milestone Checklist — OC-P14 (Final)

- [ ] Frontend uses .yml everywhere
- [ ] Sidebar handles .yml correctly
- [ ] Full test suite passes
- [ ] Manual smoke test passes
- [ ] **89/89 schema types supported**
- [ ] **OpenCollection spec v1.0.0 fully implemented**
