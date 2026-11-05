# DDD Phase 5 audit: domain crates back to pure

Date: 2026-11-04. Scope: roadmap items D-50 to D-55 in `docs/roadmaps/ddd-roadmap.md`, read on branch `main`.
Method: six read-only sub-audits (three on Opus at high effort, one on Sonnet, two on Haiku). The orchestrator then re-checked the key claims against the code. "Verified" below means the orchestrator read the code. "Reported" means the claim comes from a sub-audit only.

## Verdict

**Phase 5 is mostly done, but it is not complete, and two real security gaps sit in the import path.**

- The structural goal holds. No domain crate depends on `rocket-infra`, `rocket-app` or `tauri` in `[dependencies]`.
- `rocket-http`, `rocket-collection` and `rocket-shared` have no file or network I/O in production code.
- `cargo check`, `clippy -D warnings` and tests pass for the seven touched crates (3204 passed, 0 failed, about 10 ignored that need a network, keychain or OpenSSL). Reported by the Haiku run, totals re-counted from its log.
- The roadmap overstates D-51. A direct `canonicalize` and `is_file` call remains in `rocket-import`, and the `is_dir()` predicates left behind keep a symlink-following walk.

| Item | Result | Note |
|---|---|---|
| D-50 token POST to infra | PASS-WITH-GAPS | Done as claimed. No timeout on the token client (older than D-50). |
| D-51 import file port | PASS-WITH-GAPS | Port exists and is wired. One undocumented leak, two real symlink gaps. |
| D-52 consent root | PASS-WITH-GAPS | Disk store behaves as before. In-memory store no longer canonicalizes (theoretical). |
| D-53 home dir | PASS | `dirs` is gone from `rocket-shared`. No trust decision uses `expand_tilde`. |
| D-54 duplicate trait | PASS | One definition remains (verified). |
| D-55 git module | PASS | Already true before the commit. One `git2` use in `src-tauri`. |

## Security gaps

### Real

| ID | Severity | Where | Gap | Status |
|---|---|---|---|---|
| S-1 | Medium | `rocket-import/src/importer.rs:1149`, `rocket-infra/src/fs_import_files.rs:44` | A symlinked `.yml` file in a Bruno 3 folder is copied with `std::fs::copy`, which follows symlinks. A cloned repo holding `leak.yml -> /etc/passwd` or `-> ~/.config/gh/hosts.yml` puts that file into `<workspace>/collections/<name>/`, where it can be committed or pushed. Only `.js` and `.proto` get a symlink check. | Verified |
| S-2 | Medium | `importer.rs:339` and `:223` | The legacy Bruno walk uses `p.is_dir()`, which follows symlinks. A folder `up -> ../..` is walked outside the collection root, and request, script and proto files found there are imported. This is the `is_dir()` predicate the roadmap left in place. | Verified |
| S-3 | Medium | `rocket-infra/src/reqwest_executor.rs:783-864` | The OAuth token client has no timeout. A stalled token endpoint hangs "Get token", refresh and Send. Only Flow auth is bounded (`flow_auth.rs:261`). The behavior is older than D-50. | Verified (no `.timeout(` in `post_form` or `build_client`) |
| S-4 | Medium | `rocket-import/src/bru/zip_extractor.rs:23-41` | No limit on zip entry count, total size or compression ratio. A downloaded zip bomb can fill `$TMPDIR`. | Reported |
| S-5 | Low-medium | `bru/mod.rs:17,33`, `postman/parser.rs:13`, `importer.rs:1205` | Imported files are read with no size cap. A `.bru -> /dev/zero` symlink grows memory until the app aborts. The WSDL and OpenAPI readers do have caps. | Reported |
| S-6 | Low | `reqwest_executor.rs:804,810` | The token response body is unbounded, and a non-2xx body is copied whole into the error string. | Reported |
| S-7 | Low-medium | `importer.rs:49-50,1071,1079` | Importing a Postman environment with an existing name silently replaces it. Outside D-51. | Reported |

### Theoretical (no demonstrated path)

- T-1: the token POST follows up to 10 redirects with no client certificate, and a 307 or 308 re-sends `client_secret`, `refresh_token` and `code_verifier` to the new host. It needs a token endpoint that redirects across hosts. Reported.
- T-2: `reqwest` error text includes the URL. A secret in a query-type token parameter could show up in "Token request failed". Only `.env` values are masked. Reported.
- T-3: `OAuthToken` and the OAuth request structs derive `Debug` with secrets inside. No `{:?}` logging exists today. Reported.
- T-4: `InMemoryConsentStore` no longer canonicalizes the root. Only reachable if the home directory is unknown, which setup blocks, and the result fails closed. Verified in the code, reachability reported.
- T-5: a time-of-check gap between the kept `exists()` and `is_dir()` checks and the later reads. It needs a local writer in the source folder. Reported.
- T-6: an untrusted WSDL can make Rocket read local XML files of 5 MiB or less outside its folder (it warns and does not refuse). By design. Reported.

### Checked and clean

- Zip slip: blocked by `enclosed_name`. Zip symlink entries are written as plain files. Reported.
- Names taken from imported files (collection, Postman, WSDL, OpenAPI, environment) are sanitized. Reported.
- The vault-certificate trust gate is still applied before the token POST. Imported collections get no trust record, so their scripts start gated. Reported.
- No `unwrap()` added to production paths in the new code. Reported.
- The OpenAPI consent store never decided which directory may be read. It only keys approvals, and a key mismatch fails closed. Verified.

## Completeness gaps (not security)

| ID | Where | Gap | Status |
|---|---|---|---|
| C-1 | `importer.rs:1281-1290` (`rebase_proto_path`) | Direct `canonicalize()` on the root and each candidate, then `is_file()`. The port already has `canonicalize`. Not covered by the documented "predicates left" exception. `crates/rocket-import/CLAUDE.md` is wrong about it. | Verified |
| C-2 | `rocket-http/Cargo.toml:9,13`, `spec_fetch.rs:155` | `reqwest` (used only for `reqwest::Url`) and `tokio` with the workspace `full` features are normal dependencies. `url::Url` is already available. `rocket-http/CLAUDE.md` says no leaks remain. | Verified for the `reqwest` and `tokio` lines |
| C-3 | `rocket-import/Cargo.toml` | `tempfile` is in `[dependencies]` but used only in tests. | Verified |
| C-4 | `src-tauri/src/lib.rs:187-188`, `src-tauri/Cargo.toml:57` | `src-tauri` sets global libgit2 timeouts through `git2` directly, so "`GitService` is the only way in" has one exception. | Verified |
| C-5 | `importer.rs:1105` vs old `is_dir()` | Commit `68f11358` says "behavior unchanged". Symlinked folders in a Bruno 3 import are now dropped silently, and a `*.yml` symlink to a folder aborts the import. Safer, but unreported and untested. | Reported |
| C-6 | `importer.rs:2569-2630` | `FakeFiles::list_dir` returns nothing, so no importer runs end to end without a disk. `detect_collection` and `detect_workspace` bypass the port. | Reported |
| C-7 | `reqwest_executor.rs:1399-1471` | `fetch_direct_grant_token` is a second token POST outside the port, with different errors and parsing, and no timeout. | Reported |
| C-8 | `crates/rocket-import/CLAUDE.md:72,272`, `crates/rocket-collection/CLAUDE.md:7` | Stale lines: all disk I/O through the repos (false), `rocket-infra` listed as a normal dependency, a `canonicalize` call that no longer exists. | Reported |
| C-9 | `rocket-app/Cargo.toml:41`, `rocket-http/src/oauth2.rs` | Unused `reqwest` dev-dependency, an unused exported `OAuthConfig`, and two blank lines that `cargo fmt --check` may flag. | Reported |
| C-10 | `rocket-infra/src/paths.rs:21-37` and the consent repo tests | Tests wrapped in `if let Some(home)` do nothing without a home. No test that a symlink to the root shares a consent record with the real path. | Reported |

## Recommended follow-ups

Put these in the roadmap as new items. Keep plans to 3 tasks each.

1. **Import symlink hardening (S-1, S-2, C-5).** Replace `p.is_dir()` and the `.yml` copy with `entry.kind` checks, skip and report symlinks, and add tests. Needs no new port method. Also close the predicates (`exists()` becomes `kind().is_ok()`) and route `rebase_proto_path` through the port (C-1). This finishes D-51.
2. **Import limits (S-4, S-5).** Cap zip entries and total bytes, and use `regular_file_len` plus `read_up_to` for `.bru`, Postman and YAML reads.
3. **Token client hardening (S-3, S-6, T-1, T-2).** Add a connect and total timeout and a response byte cap, use `Policy::none()` for token POSTs, and strip the URL from error text. Then route `fetch_direct_grant_token` through the port (C-7).
4. **Dependency cleanup (C-2, C-3, C-4).** Move `tempfile` to dev-dependencies, switch `rocket-http` from `reqwest::Url` to `url::Url`, and add a `rocket-git` function for the libgit2 timeouts.
5. **Docs (C-8, C-9) and tests (C-10).** Fix the stale `CLAUDE.md` lines and correct the D-51 and D-55 roadmap rows.

Items 3 to 5 are small. Item 1 is the most valuable, because S-1 and S-2 are reachable by importing a cloned repo.

## Method notes

- Sub-audits read `main` through `git show`. The checkout changed branches twice during the run, and the working tree held uncommitted D-63 work. The D-63 edits did not affect the build or test results.
- Haiku agents did the sweep and the test run. Their strongest claims were re-checked. Opus findings marked "Reported" were not re-read line by line.
- The Haiku test run wrote its logs to `/tmp/claude-1000/clippy.log` and `/tmp/claude-1000/test.log`, not to the scratchpad.
