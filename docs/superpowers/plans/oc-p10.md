# OC-P10: Conversions — Full Request Files (HTTP, GraphQL, gRPC, WS, Folder)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete request-level conversions: OcHttpRequest ↔ Request, OcGraphQLRequest ↔ domain, OcGrpcRequest ↔ domain, OcWebSocketRequest ↔ domain, OcFolder ↔ Folder, OcCollection ↔ Collection.

**Tech Stack:** Rust

**Prerequisite:** OC-P09 complete.

---

## Task 1: OcHttpRequest ↔ Request (full conversion)

- [ ] **Step 1: Implement oc_http_request_to_domain() and domain_to_oc_http_request()**

Convert all fields: info (name, description, type, seq, tags), http (method, url, headers, params, body), runtime (scripts, assertions, actions, variables, auth), settings, examples, docs.

Extract scripts from runtime array by type (before-request, after-response, tests, hooks). Map actions to ActionSetVariable domain type.

Tests: parse a full YAML request → domain Request → back to YAML → verify round-trip preserves all fields.

- [ ] **Step 2: Run tests + commit**

---

## Task 2: GraphQL + gRPC + WebSocket request conversions

- [ ] **Step 1: Implement conversions for each protocol**

For now, these protocol types may map to a generic `Request` with `type` field distinguishing them, or to separate domain types if they exist. The key is that we can **read and write** these YAML files without data loss.

If domain types don't exist yet for gRPC/WebSocket, store the protocol-specific data as `serde_yaml::Value` in an extension field on Request, so we preserve it during read/write without needing full domain support.

Tests: parse each protocol YAML → convert to domain → back to YAML → verify no data loss.

- [ ] **Step 2: Run tests + commit**

---

## Task 3: Folder + Collection conversions

- [ ] **Step 1: Implement OcFolder ↔ Folder, OcCollection ↔ Collection**

OcFolder: convert info (name, description, seq, tags), items (recursive — each item dispatched to HTTP/GraphQL/gRPC/WS/Folder/ScriptFile converter), request defaults (headers, auth, variables, scripts, settings), docs.

OcCollection (opencollection.yml): convert info (name, summary, version, authors), config (environments, proxy, protobuf, clientCertificates), request defaults, items, docs, bundled flag.

Tests: parse a collection YAML with nested folders and items, verify structure.

- [ ] **Step 2: Run tests + commit**

---

## Milestone Checklist — OC-P10

- [ ] OcHttpRequest ↔ Request — complete with examples + actions
- [ ] GraphQL/gRPC/WebSocket request conversions (lossless read/write)
- [ ] OcFolder ↔ Folder (recursive items)
- [ ] OcCollection ↔ Collection (opencollection.yml)
- [ ] All conversions preserve data through roundtrip

---

---
