# Contract OpenAPI Export — Design Spec

**Date:** 2026-05-11  
**Status:** Approved  
**Scope:** `rocket-app/src/contract_service.rs` — `export_as_openapi_yaml` only. No frontend changes required.

---

## Problem

The current `export_as_openapi_yaml` implementation produces a minimal stub that omits most contract and endpoint data:

- Only `title`, `version`, and a `method + path + 200 OK` stub per endpoint
- URL parsing bug: full URLs like `https://api.example.com/users` are used verbatim as path keys instead of being split into a `servers` entry and a `/users` path
- No contract metadata (parties, policy, dates, status, enforcement mode, audit fields)
- No per-endpoint parameters, request body, auth, or source tracing

---

## Approach

Replace the string-templating implementation with **typed `serde_yaml` structs** (Option B). `serde_yaml` 0.9.34 is already a workspace dependency in `rocket-app`. No new dependencies required.

`x-*` extension fields are declared as **explicit named struct fields** with `#[serde(rename = "x-...")]` rather than using `#[serde(flatten)]` with a `HashMap`, avoiding a known `serde_yaml` 0.9 bug with flattened maps.

All work is confined to `rocket-app/src/contract_service.rs`. The Tauri command and frontend are unchanged.

---

## Output Structure

### Full example output

```yaml
openapi: '3.0.3'
info:
  title: 'My API Contract'
  version: '1.0.0'
  description: |
    # My API Contract v1.0.0

    **Provider:** Team A (team)
    **Consumers:** Team B (service)
    **Status:** active  |  **Enforcement:** informational
    **Effective:** 2025-01-01  |  **Expires:** 2026-01-01
    **Scope:** collection
    **Drift:** 2 changes  |  **Breach:** 0 breaking

    ## Policy
    - Breaking change policy: lenient
    - Notice period: 30 days
    - Uptime SLA: 99.9%

    ## Attachments
    - .rocket/contracts/attachments/01J.../spec.pdf

    *Exported from Rocket API — created by alice on 2025-01-01T10:00:00Z*
  contact:
    name: 'Team A'
  x-contract-id: '01JXXXXX'
  x-contract-status: 'active'
  x-contract-enforcement-mode: 'informational'
  x-contract-provider:
    id: 'team-a'
    name: 'Team A'
    kind: 'team'
  x-contract-consumers:
    - id: 'team-b'
      name: 'Team B'
      kind: 'service'
  x-contract-effective-date: '2025-01-01'
  x-contract-expiry-date: '2026-01-01'
  x-contract-policy:
    breakingChangePolicy: 'lenient'
    noticeDays: 30
    uptimeSla: 99.9
  x-contract-scope: 'collection'
  x-contract-drift-count: 2
  x-contract-breach-count: 0
  x-contract-endpoint-count: 3
  x-contract-document-paths:
    - '.rocket/contracts/attachments/01J.../spec.pdf'
  x-contract-created-by: 'alice'
  x-contract-created-at: '2025-01-01T10:00:00Z'
  x-contract-updated-at: '2025-03-15T14:22:00Z'

servers:
  - url: 'https://api.example.com'

paths:
  /users:
    get:
      operationId: 'GET /users'
      summary: 'GET /users'
      tags:
        - 'users'
      parameters:
        - name: 'page'
          in: query
          schema:
            type: string
          example: '1'
        - name: 'Authorization'
          in: header
          schema:
            type: string
      security:
        - BearerAuth: []
      responses:
        '200':
          description: OK
        '401':
          description: Unauthorized
      x-source-path: 'users/get-users.yml'
      x-captured-at: '2025-03-15T14:22:00Z'
      x-auth-detail: 'supersec…'
    post:
      operationId: 'POST /users'
      summary: 'POST /users'
      tags:
        - 'users'
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
            example:
              name: Ada
              email: a@b.com
      responses:
        '200':
          description: OK
        '422':
          description: Unprocessable Entity
      x-source-path: 'users/post-users.yml'
      x-captured-at: '2025-03-15T14:22:00Z'

components:
  securitySchemes:
    BearerAuth:
      type: http
      scheme: bearer
    BasicAuth:
      type: http
      scheme: basic
    ApiKeyAuth:
      type: apiKey
      in: header
      name: X-Api-Key
    OAuth2Auth:
      type: oauth2
      flows: {}
    AwsSigV4Auth:
      type: http
      scheme: aws-sig-v4
```

---

## Struct Layout

All structs are private to the `export_as_openapi_yaml` function module. They derive `Serialize` only — they are never deserialized.

```
OpenApiDoc
├── openapi: &'static str                  // "3.0.3"
├── info: InfoObject
│   ├── title, version, description
│   ├── contact: ContactObject { name }
│   └── x-contract-* fields (explicit named, not flatten)
├── servers: Vec<ServerObject>             // { url }
├── paths: IndexMap<String, PathItemObject>
│   └── PathItemObject: IndexMap<String, OperationObject>  // method → op
│       └── OperationObject
│           ├── operation_id, summary
│           ├── tags: Vec<String>
│           ├── parameters: Vec<ParameterObject>
│           │   └── { name, in, schema: SchemaObject, example }
│           ├── request_body: Option<RequestBodyObject>
│           │   └── content: IndexMap<String, MediaTypeObject>
│           │       └── { schema, example }
│           ├── security: Option<Vec<IndexMap<String, Vec<String>>>>
│           ├── responses: IndexMap<String, ResponseObject>
│           ├── x-source-path: Option<String>
│           ├── x-captured-at: Option<String>
│           └── x-auth-detail: Option<String>
└── components: Option<ComponentsObject>
    └── security_schemes: IndexMap<String, SecuritySchemeObject>
```

`IndexMap` (already a workspace dep via `serde_yaml`) is used for `paths` and `security_schemes` to preserve insertion order in the YAML output.

---

## Key Logic

### URL extraction (fixing the existing bug)

```
url_pattern = "https://api.example.com/users?page=1"
→ servers[0].url = "https://api.example.com"
→ path key        = "/users"

url_pattern = "/users"           → no server extracted; path key = "/users"
url_pattern = "{{baseUrl}}/users" → server = "{{baseUrl}}"; path key = "/users"
url_pattern = "http://localhost:3000/api/v1/users"
→ server = "http://localhost:3000"; path key = "/api/v1/users"
```

Extraction logic: find the third `/` after `://` (or first `/` if no `://`). Everything before is the server URL, everything from that `/` onward (stripping query string) is the path key. Deduplicate server URLs — only unique base URLs appear in the `servers` array.

### Path grouping

Entries are grouped by path key using an `IndexMap<String, IndexMap<String, OperationObject>>` (path → method → operation). This ensures `GET /users` and `POST /users` appear under the same `/users` path item.

### Tags

Derived from the first directory segment of `request_path`. Examples:
- `users/get-users.yml` → tag `users`
- `auth/login.yml` → tag `auth`
- `root-request.yml` (no folder) → no tag

### Body content type inference

`body_content` is a raw string. Content type is inferred:
- Attempt `serde_json::from_str` → if valid JSON: `application/json`, parse into `serde_yaml::Value` for the example
- Otherwise: `text/plain`, use raw string as example

Form fields (`form_fields` non-empty) → `application/x-www-form-urlencoded` with key/value schema properties.

### Auth → securitySchemes mapping

| `auth_type` | Scheme name | Type | Details |
|---|---|---|---|
| `bearer` | `BearerAuth` | `http` | `scheme: bearer` |
| `basic` | `BasicAuth` | `http` | `scheme: basic` |
| `api-key` | `ApiKeyAuth` | `apiKey` | placement from `auth_detail` (`in: header` or `in: query`), name from key portion |
| `oauth2` | `OAuth2Auth` | `oauth2` | `flows: {}` stub (full flow config not stored in snapshot) |
| `aws-sig-v4` | `AwsSigV4Auth` | `http` | `scheme: aws-sig-v4` |
| `wsse` | `WsseAuth` | `http` | `scheme: wsse` |
| `digest` | `DigestAuth` | `http` | `scheme: digest` |
| `ntlm` | `NtlmAuth` | `http` | `scheme: ntlm` |
| `none` / `inherit` | — | — | No scheme emitted, no security requirement on operation |

**`ApiKeyAuth` placement parsing:** `auth_detail` for API key is stored as `"key=value… (header)"` or `"key=value… (query)"`. Extract `in` from the parenthesised suffix and `name` from the key portion before `=`. If parsing fails, default to `in: header, name: X-Api-Key`.

Only schemes actually used by at least one endpoint are emitted in `components.securitySchemes`.

### Additional response codes

Added automatically based on endpoint characteristics:
- Auth type is non-none → add `401: Unauthorized`
- `requestBody` is present → add `422: Unprocessable Entity`

### Fallback (no snapshot)

If the contract has no signed snapshot (e.g. `Draft` status), emit a single placeholder path:

```yaml
paths:
  /example:
    get:
      summary: No snapshot available — contract has not been signed
      responses:
        '200':
          description: OK
```

---

## Fields Intentionally Omitted

| Field | Reason |
|---|---|
| `contract.project` | Deprecated — superseded by `ContractParty` identities, always empty in current data |
| `ContractParty.avatar_seed` / `avatar_color` | UI-only rendering hints, not meaningful outside the app |
| Legacy snapshot fields (`query_param_keys`, `header_keys`, `body_field_keys`) | Superseded by the full key+value fields; old files already migrated |

---

## Files Changed

| File | Change |
|---|---|
| `crates/rocket-app/src/contract_service.rs` | Replace `export_as_openapi_yaml` body; add private structs |

No other files change. The Tauri command signature, frontend API wrapper, and `saveContractAsOpenApi` helper are all unchanged.

---

## Testing

Add unit tests in `contract_service.rs` `#[cfg(test)]` block covering:

1. **Full export with snapshot** — contract with provider, 2 consumers, policy, dates, 2 endpoints (one GET with query params + bearer auth, one POST with JSON body) → assert YAML parses correctly and key fields are present
2. **URL extraction** — full URL, path-only URL, URL with `{{variable}}`
3. **Auth scheme mapping** — each `auth_type` value maps to correct scheme name and type
4. **Body content type inference** — valid JSON body → `application/json`; non-JSON → `text/plain`; form fields → `application/x-www-form-urlencoded`
5. **Path grouping** — two entries with same URL path but different methods collapse into one path item with two operations
6. **Fallback** — contract with no snapshot emits the placeholder path
7. **Tag derivation** — `auth/login.yml` → tag `auth`; root-level file → no tag
