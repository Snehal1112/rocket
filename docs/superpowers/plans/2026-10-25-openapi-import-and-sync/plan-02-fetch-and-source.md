# OpenAPI Fetch and Source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a pasted spec link (a GitHub link or any `https` link) into a fetch target, download it safely, and record where an import came from in a sidecar file, so Plan 3 can wire the import command and Plans 4 and later can sync.

**Architecture:** `SpecLocation` (a small value type) and the source record live in `rocket-collection`. The URL parser lives in `rocket-import` and is pure. The fetch port `SpecFetcher` lives in `rocket-http` (like `WebSocketClient`), with a reqwest implementation in `rocket-infra`. The sidecar `.rocket/openapi-source.yml` is read and written by a repository trait in `rocket-collection` with a filesystem implementation in `rocket-infra`. Nothing here is wired to a Tauri command yet.

**Tech Stack:** Rust, `reqwest` (already a dependency), `async-trait`, `wiremock` and `tempfile` for tests, `sha2` (already added to `rocket-import`), `serde_yaml`.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`

## Global Constraints

- Only `https` links are fetched. `http`, `file` and any other scheme are rejected. A link with a user name or password in it (`https://user:pass@host/...`) is rejected, because the sidecar is committed to git.
- At most 5 redirects. A redirect to a different origin (scheme, host or port) never carries the `Authorization` header. A redirect to a non-`https` URL or to a private or loopback IP address is refused.
- Timeout 30 seconds. Body cap 25 MB (`DEFAULT_MAX_SPEC_BYTES = 25 * 1024 * 1024`), enforced while reading, with or without `Content-Length`.
- Distinct failure states for the UI: not found, unauthorized, rate limited (with `Retry-After` seconds when sent), too large, not https, too many redirects, blocked redirect, other status, network error.
- The fetcher uses the app proxy settings the same way `ReqwestTokenClientProvider` does (`SharedProxy`).
- A token is never logged, never put in a `Debug` string and never written to the sidecar.
- Persistence structs use snake_case keys and no `rename_all`. Writes use `crate::atomic_write`. Never unwrap errors in production paths. Cargo commands use `-j4 -p <crate>`.
- Sidecar location is `<collection>/.rocket/openapi-source.yml`. A dot folder is hidden from the collection loader and from the Bruno export, so no loader change is needed (the contract feature uses `<collection>/.rocket/contracts/` the same way).

## Review Focus

- A token must not reach a different origin: a redirect from the first server to a second one, and a redirect that downgrades to `http`.
- A body over the cap, both with a large `Content-Length` and without one (chunked), stops reading early and reports `TooLarge`.
- A redirect loop ends with `TooManyRedirects` after 5 hops, not forever.
- Odd links: a `blob` link with `?raw=true` or `#L10`, `refs/heads/` in a raw link, a `tree` (folder) link, an upper-case host, spaces around the text, `http://`, `file:///etc/passwd`, and a link with `user:pass@`.
- The sidecar: a missing file is `None`, a corrupt file or a newer `version` is an error (never silently replaced by defaults), and a collection that contains `.rocket/openapi-source.yml` still loads in strict mode.

---

### Task 1: Spec location and URL parser

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/openapi_source.rs` (the `SpecLocation` type only in this task)
- Modify: `crates/rocket-collection/src/lib.rs` (add `pub mod openapi_source;`)
- Create: `crates/rocket-import/src/openapi/source_url.rs`
- Modify: `crates/rocket-import/src/openapi/mod.rs`, `crates/rocket-import/src/lib.rs` (re-export `parse_spec_url`, `FetchTarget`, `fetch_target`)
- Test: unit tests in `source_url.rs`

**Interfaces:**
- Produces, in `rocket-collection::openapi_source`:
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum SpecLocation { Github { owner: String, repo: String, git_ref: String, path: String }, Https { url: String } }`
  - `impl SpecLocation { pub fn display_url(&self) -> String; pub fn ref_is_immutable(&self) -> bool }` (`display_url` is the canonical `https://github.com/{owner}/{repo}/blob/{git_ref}/{path}` or the `https` URL; `ref_is_immutable` is true for a 40-character hexadecimal ref, false otherwise and for `Https`)
- Produces, in `rocket-import`:
  - `pub struct FetchTarget { pub url: String, pub accept: Option<String>, pub origin: String }`
  - `pub fn parse_spec_url(input: &str) -> ImportResult<SpecLocation>`
  - `pub fn fetch_target(location: &SpecLocation, with_token: bool) -> FetchTarget` (GitHub without a token: `https://raw.githubusercontent.com/{owner}/{repo}/{git_ref}/{path}`; with a token: `https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={git_ref}` and `accept = Some("application/vnd.github.raw+json")`; `Https`: the URL itself, `accept = None`; every path segment and the ref are percent-encoded; `origin` is `scheme://host[:port]` of `url`)
- Link forms `parse_spec_url` accepts, all giving `Github`: `https://github.com/{o}/{r}/blob/{ref}/{path}` (also `/raw/{ref}/{path}`), `https://raw.githubusercontent.com/{o}/{r}/{ref}/{path}` (also `refs/heads/{ref}` and `refs/tags/{ref}`), `https://api.github.com/repos/{o}/{r}/contents/{path}?ref={ref}` (default ref `HEAD` when missing). The ref is the first path segment after `blob`, `raw` or the repo; a ref that contains `/` can only be given through the contents-API form. Query and fragment are dropped from `blob` and raw links. A `tree` link, a link with too few segments, and any other `github.com` path give an error that says to link to the file. Any other `https` URL gives `Https` with the fragment removed. Surrounding whitespace is trimmed and the host is compared case-insensitively.
- Errors use `ImportError::NotAnOpenApiSpec(message)` with a plain message (`only https links are supported`, `links with a user name or password are not supported`, and so on).

- [ ] **Step 1: Write the failing tests** in `source_url.rs`:
  - `blob_raw_and_api_links_give_the_same_location`: the three forms for `acme/orders`, ref `main`, path `docs/openapi.yaml` all parse to `Github { owner: "acme", repo: "orders", git_ref: "main", path: "docs/openapi.yaml" }`.
  - `queries_fragments_refs_heads_and_case_are_handled`: `https://GitHub.com/acme/orders/blob/main/openapi.yaml?raw=true#L10` and `https://raw.githubusercontent.com/acme/orders/refs/heads/main/openapi.yaml` parse to ref `main`; `  https://github.com/acme/orders/blob/v1.2.0/spec.json  ` parses with trimmed input and ref `v1.2.0`; the contents-API form with `?ref=feature%2Fx` gives ref `feature/x`.
  - `other_https_links_stay_as_they_are`: `https://example.com/api/openapi.json#top` gives `Https { url: "https://example.com/api/openapi.json" }`.
  - `rejects_unsafe_or_unusable_links`: `http://...`, `file:///etc/passwd`, `ftp://x/y`, `https://user:pass@example.com/x`, `https://github.com/acme/orders/tree/main/docs`, `https://github.com/acme`, empty text, and `not a url` each return `ImportError::NotAnOpenApiSpec`.
  - `fetch_target_picks_raw_or_contents_api`: without a token the raw URL; with a token the contents-API URL with `ref=` percent-encoded (`feature%2Fx`), `accept` set, and `origin == "https://api.github.com"`; for `Https` the URL, no `accept`, and its own origin.
  - `immutable_ref_is_detected`: a 40-character hex ref gives `ref_is_immutable() == true`; `main`, `v1.2.0` and `Https` give false.
  - `display_url_round_trips_through_the_parser`: `parse_spec_url(&loc.display_url()) == loc` for a `Github` location.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-import source_url` → FAIL (module missing).
- [ ] **Step 3: Implement `SpecLocation` and its two methods** in `rocket-collection/src/openapi_source.rs` (types and pure methods only), then `parse_spec_url`, `fetch_target` and a private `encode_segment` (percent-encode everything except letters, digits, `-`, `.`, `_`, `~`) in `source_url.rs`. Parse by hand with `str` methods; add no URL crate.
- [ ] **Step 4: Run to verify pass:** `cargo test -j4 -p rocket-import source_url` → PASS; `cargo check -j4 -p rocket-collection`.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-collection/src/openapi_source.rs`, `crates/rocket-collection/src/lib.rs`, `crates/rocket-import/src/openapi`, `crates/rocket-import/src/lib.rs`. Message: `feat(import): parse GitHub and https spec links`.

---

### Task 2: Spec fetch port and reqwest implementation

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-http/src/spec_fetch.rs` (port, request, outcome, failure, constants)
- Modify: `crates/rocket-http/src/lib.rs` (re-export)
- Create: `crates/rocket-infra/src/reqwest_spec_fetcher.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (declare and re-export `ReqwestSpecFetcher`)
- Test: unit tests in both files (wiremock in the infra file)

**Interfaces:**
- Produces, in `rocket-http::spec_fetch`:
  - `pub const DEFAULT_MAX_SPEC_BYTES: usize = 25 * 1024 * 1024;`
  - `pub struct SpecFetchRequest { pub url: String, pub accept: Option<String>, pub bearer_token: Option<String>, pub if_none_match: Option<String>, pub if_modified_since: Option<String>, pub max_bytes: usize }` with a manual `Debug` that prints `bearer_token` as `<redacted>` (same idea as `WebSocketConnectRequest`)
  - `pub enum SpecFetchOutcome { Fetched { body: Vec<u8>, etag: Option<String>, last_modified: Option<String>, final_url: String }, NotModified }`
  - `pub enum SpecFetchFailure { NotFound, Unauthorized, RateLimited { retry_after_secs: Option<u64> }, TooLarge, NotHttps, TooManyRedirects, BlockedRedirect, Status(u16), Network(String) }` with `Display` (plain sentences) and `std::error::Error`
  - `#[async_trait] pub trait SpecFetcher: Send + Sync { async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure>; }`
  - `pub fn redirect_target_allowed(from: &str, to: &str) -> bool` (pure: `to` must be `https` and its host must not be a loopback, private, link-local or unspecified IP literal; host names are not resolved)
  - `pub fn same_origin(a: &str, b: &str) -> bool` (scheme, host and port equal, host case-insensitive)
- Produces, in `rocket-infra`: `pub struct ReqwestSpecFetcher` with `pub fn new() -> Self`, `pub fn with_proxy(self, shared: rocket_http::SharedProxy) -> Self`, implementing `SpecFetcher`.
- Status mapping: 200 gives `Fetched` (with `ETag` and `Last-Modified`); 304 gives `NotModified`; 401 gives `Unauthorized`; 404 gives `NotFound`; 429 gives `RateLimited`; 403 gives `RateLimited` when `x-ratelimit-remaining` is `0` or `Retry-After` is present, else `Unauthorized`; any other status gives `Status(code)`. `retry_after_secs` is `Retry-After` when it is a whole number of seconds.
- Behaviour: reject a non-`https` first URL with `NotHttps` before any request; follow redirects by hand up to 5 (then `TooManyRedirects`); for each hop check `redirect_target_allowed` (else `BlockedRedirect`) and send the `Authorization: Bearer` header only while `same_origin` with the first URL; send `If-None-Match` and `If-Modified-Since` and a `User-Agent`; read the body with `chunk()` and stop with `TooLarge` as soon as the total passes `max_bytes` or `Content-Length` already does.
- For tests only, `ReqwestSpecFetcher` has a `#[cfg(test)] fn allow_local_http(self) -> Self` that lets the strict `https` and local-address checks pass so `wiremock` (plain `http` on 127.0.0.1) can be used.

- [ ] **Step 1: Write the failing tests.** In `spec_fetch.rs`: `redirect_target_allowed_rejects_http_and_private_ips` (https public host true; `http://x`, `https://127.0.0.1/`, `https://10.0.0.5/`, `https://192.168.1.2/`, `https://169.254.169.254/`, `https://[::1]/`, `https://0.0.0.0/` false), `same_origin_compares_scheme_host_and_port`, `request_debug_hides_the_token`. In `reqwest_spec_fetcher.rs` with `wiremock`:
  - `fetches_body_and_validators`: 200 with `ETag: "abc"` and `Last-Modified`, body `openapi: 3.0.3` gives `Fetched` with those values.
  - `sends_conditional_headers_and_maps_304`: the mock requires `If-None-Match: "abc"`; a 304 gives `NotModified`.
  - `maps_error_statuses`: 404, 401, 429 with `Retry-After: 120` (gives `RateLimited { retry_after_secs: Some(120) }`), 403 with `x-ratelimit-remaining: 0` (rate limited), 403 plain (unauthorized), 500 gives `Status(500)`.
  - `stops_at_the_size_cap`: a 2 KB body with `max_bytes = 1024` gives `TooLarge`; a chunked response without `Content-Length` over the cap also gives `TooLarge`.
  - `follows_redirects_and_gives_up_after_five`: a redirect chain of 3 ends in `Fetched`; a loop (a redirects to itself) gives `TooManyRedirects`.
  - `token_is_not_sent_to_another_origin`: server A redirects to server B (a different port); server B records its requests and none has an `Authorization` header, while server A saw it.
  - `rejects_http_without_the_test_switch`: a fetcher built with `new()` given `http://127.0.0.1:{port}/x` returns `NotHttps` and the mock receives no request.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-http spec_fetch` and `cargo test -j4 -p rocket-infra reqwest_spec_fetcher` → FAIL.
- [ ] **Step 3: Implement the port and constants in `spec_fetch.rs`, and `ReqwestSpecFetcher::fetch` in `reqwest_spec_fetcher.rs`.** Build the client the way `ReqwestTokenClientProvider` does (`apply_proxy` and `read_proxy` from `reqwest_executor.rs`; make them `pub(crate)` if needed) with `redirect::Policy::none()` and a 30 second timeout. Read the body like `read_capped` in `rocketvault/certificates.rs`. Add no dependencies other than `async-trait` where missing.
- [ ] **Step 4: Run to verify pass:** the two commands above → PASS, then `cargo check -j4`.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-http/src/spec_fetch.rs`, `crates/rocket-http/src/lib.rs`, `crates/rocket-infra/src/reqwest_spec_fetcher.rs`, `crates/rocket-infra/src/lib.rs`, plus any `Cargo.toml` and `Cargo.lock` you touched. Message: `feat(http): fetch OpenAPI specs with safe redirects`.

---

### Task 3: Source record and sidecar repository

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/openapi_source.rs` (add the record types and the repository trait)
- Create: `crates/rocket-infra/src/fs_openapi_source_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (declare and re-export `FsOpenApiSourceRepo`)
- Modify: `crates/rocket-import/src/openapi/source_url.rs` or a new `crates/rocket-import/src/openapi/source_record.rs` (the builder), `crates/rocket-import/src/lib.rs` (re-export)
- Test: unit tests in `openapi_source.rs`, `fs_openapi_source_repo.rs` and the builder file

**Interfaces:**
- Consumes: `SpecLocation` (Task 1), `OpenApiImportOutcome` and `ImportedOperation` from Plan 1.
- Produces, in `rocket-collection::openapi_source` (all `Debug, Clone, PartialEq, Serialize, Deserialize`, snake_case keys, `#[serde(default)]` on every field after `version`):
  - `pub const SOURCE_FILE_VERSION: u32 = 1;`
  - `pub struct OpenApiSource { pub version: u32, pub location: SpecLocation, pub group_by: String, pub spec_sha256: String, pub etag: Option<String>, pub last_modified: Option<String>, pub operations: Vec<SourceOperation> }` (`group_by` is `"tags"` or `"paths"`)
  - `pub struct SourceOperation { pub key: String, pub uid: String, pub request_path: String, pub baseline_hash: String }`
  - `pub trait OpenApiSourceRepository: Send + Sync { fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiSource>>; fn save(&self, collection_root: &Path, source: &OpenApiSource) -> DomainResult<()>; fn delete(&self, collection_root: &Path) -> DomainResult<()>; }`
- Produces, in `rocket-infra`: `pub struct FsOpenApiSourceRepo;` (unit struct) implementing the trait; the file is `<collection_root>/.rocket/openapi-source.yml`, written with `atomic_write`.
- Produces, in `rocket-import`: `pub fn source_record(location: &SpecLocation, group_by: GroupBy, spec_body: &[u8], etag: Option<String>, last_modified: Option<String>, outcome: &OpenApiImportOutcome) -> OpenApiSource` (`spec_sha256` is the hex SHA-256 of `spec_body`; `operations` mirrors `outcome.operations`).

- [ ] **Step 1: Write the failing tests:**
  - In `fs_openapi_source_repo.rs` (tempdir): `missing_file_is_none`, `save_then_load_round_trips` (all fields including both `SpecLocation` kinds), `file_has_snake_case_keys_and_no_secret_fields` (the written text contains `spec_sha256:` and `git_ref:` and not `token`), `corrupt_file_is_an_error`, `newer_version_is_an_error` (`version: 99` gives `Err`, not defaults), `delete_removes_the_file_and_ignores_a_missing_one`.
  - In `crates/rocket-infra/src/fs_collection/tests.rs` or the new file: `collection_with_source_file_loads_in_strict_mode`: create a collection with `FsCollectionRepo`, save a request, write the sidecar, then `get_strict` returns the collection with only that one request and no error.
  - In the builder file: `record_mirrors_the_import_outcome`: import `orders-3.0.json` with `ImportService`, build the record with body bytes `b"abc"`, and assert `spec_sha256` equals the known SHA-256 of `abc` (`ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`), `group_by == "tags"`, and `operations.len() == 3` with keys, uids and hashes equal to the outcome.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-infra fs_openapi_source_repo` and `cargo test -j4 -p rocket-import source_record` → FAIL.
- [ ] **Step 3: Implement the types, trait, `FsOpenApiSourceRepo` and `source_record`.** Model the repo on `FsProxySettingsRepo` and `ContractRepository`. `load` maps a missing file to `Ok(None)`, and a parse failure or `version` above `SOURCE_FILE_VERSION` to `DomainError::Internal` with a message that names the file.
- [ ] **Step 4: Run to verify pass:** the commands above → PASS, then `cargo check -j4` and `cargo clippy -j4 -p rocket-collection -p rocket-http -p rocket-infra -p rocket-import --all-targets` with no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-collection/src/openapi_source.rs`, `crates/rocket-infra/src/fs_openapi_source_repo.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-import/src`. Message: `feat(import): record where an OpenAPI import came from`.
- [ ] **Step 6: Update the docs.** Add a short "Fetching and the source file" part to `crates/rocket-import/CLAUDE.md` (link forms, failure states, the `.rocket/openapi-source.yml` location and fields, no token on disk) and mark P2 done in `00-plan-index.md`. Commit with the same skill: `docs(import): describe spec fetch and the source file`.
