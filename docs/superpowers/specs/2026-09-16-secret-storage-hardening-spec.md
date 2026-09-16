# Spec: Secret Storage Hardening

**Status:** Draft
**Severity:** High
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 1
**Related:** item 4 ([secret-aware-variable-context-spec.md](2026-09-16-secret-aware-variable-context-spec.md)) depends on this spec's `Variable.secret` round-trip; item 3 ([env-var-write-audit-spec.md](2026-09-16-env-var-write-audit-spec.md)) depends on this spec's fixed `Environment::set_variable`-preserves-secret behavior.

> Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

Environment secrets are persisted to disk **in plaintext**, and the `secret` flag is silently
discarded on every save, so the app itself forgets a variable was ever marked secret.

Confirmed end to end by direct source inspection:

- `rocket_environment::Variable` (`crates/rocket-environment/src/variable.rs:7-20`) has a real
  `secret: bool` field.
- The environment write path drops it entirely. `From<Environment> for OcEnvironment`
  (`crates/rocket-infra/src/conversions/environment.rs:19-31`) maps every variable through
  `OcVariable::from` (`crates/rocket-infra/src/conversions/variables.rs:51-63`), which has **no
  branch for secrets** — it always emits `value: Some(VariableValue::simple(v.value))` in plaintext
  and the target struct, `OcVariable` (`crates/rocket-infra/src/oc/variables.rs:12-22`), has no
  `secret` field to carry the flag even if the conversion wanted to.
- `OcEnvironment.variables` (`crates/rocket-infra/src/oc/environment.rs:19`) is typed
  `Vec<OcVariable>` — there is structurally no room in the container for an `OcSecretVariable`
  entry. `OcSecretVariable` (`crates/rocket-infra/src/oc/variables.rs:25-35`) exists and correctly
  models the spec's value-less `SecretVariable` (§4, §2.9 of the OpenCollection spec reference),
  and its *read* conversion (`impl From<OcSecretVariable> for Variable`,
  `conversions/variables.rs:65-78`) is correct — but nothing ever *writes* an `OcSecretVariable`,
  so that read path is dead code today.
- On load, `From<OcVariable> for Variable` (`conversions/variables.rs:37-49`) hardcodes
  `secret: false` unconditionally. So even the plaintext value that did get written comes back
  with the secret flag erased — round-tripping through save/load silently un-marks every secret
  variable.
- There is no secure secret backend for environment variables. `keyring` is already a dependency
  (currently of `src-tauri` only, `src-tauri/Cargo.toml:52`) and already used correctly for git
  credentials (`src-tauri/src/commands/git.rs:317-342`, `save_git_credentials`/
  `load_git_credentials`) — the pattern exists in the codebase, it's just never applied to
  environment secrets.
- `FsEnvironmentRepo::save`/`get` (`crates/rocket-infra/src/fs_environment_repo.rs:63-69,50-61`)
  round-trip through `OcEnvironment` with no involvement of any secret store.

**Impact:** any user who marks an API key/token as "secret" in the environment editor gets no
actual protection — the value sits in plaintext in `<workspace>/environments/<name>.yml` (or
`<collection>/environments/<name>.yml`), which the app's own git integration can commit and push
to a remote with zero warning, and the "secret" marking itself doesn't survive a reload.

## 2. Goals

- A variable marked `secret: true` never has its **value** written to any `.yml` file on disk.
- The `secret` flag (and `secret_type`) round-trips correctly through save → load, forever.
- The real secret value lives only in the OS-native secret store (macOS Keychain / Windows
  Credential Manager / Linux Secret Service), via the same `keyring` crate already vetted for git
  credentials in this codebase.
- Non-secret variables are completely unaffected — same plaintext YAML behavior as today.
- No IPC/DTO contract change visible to the frontend: `EnvironmentService::get`/`list` must still
  return `Environment` values with the real (decrypted) value populated in `Variable.value`, exactly
  as before, so the existing editor UI and `secret-mask.ts` display-masking keep working unmodified.

## 3. Non-goals

- Not building a general-purpose secrets-manager UI (rotation, sharing, expiry). Out of scope.
- Not changing how `CollectionVariable` (`opencollection.yml`, git-tracked by design) handles
  secrets — collection variables are meant to be shared/committed; marking one "secret" there is a
  separate, lower-priority concern tracked as a follow-up note in this spec (§7) but not
  implemented here.
- Not migrating already-committed plaintext secrets out of users' git history — that is
  fundamentally not something the app can undo; this spec only stops the bleeding going forward
  (see §6 Migration).
- Not implementing the `.gitignore` scaffolding for `environments/` as a hard requirement — once
  this spec ships, `environments/*.yml` no longer contains secret *values* (only variable names,
  descriptions, and non-secret values), which removes the critical exposure. Scaffolding a
  `.gitignore` entry is listed as an optional defense-in-depth task in §7, not blocking.

## 4. Design

### 4.1 New domain trait: `SecretStore` (in `rocket-environment`)

Pure trait, no I/O, following the existing `EnvironmentRepository` pattern
(`crates/rocket-environment/src/repository.rs`):

```rust
// crates/rocket-environment/src/secret_store.rs
use rocket_shared::error::DomainResult;

/// Backend for the real value of a `secret: true` Variable. Never touches YAML.
pub trait SecretStore: Send + Sync {
    /// `scope_id` uniquely identifies the environment file this secret belongs to
    /// (see 4.3 for how it's derived) — distinct environments never collide.
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>>;
    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()>;
    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()>;
}

/// No-op implementation for tests and contexts with no keychain (e.g. headless CI).
pub struct NullSecretStore;
impl SecretStore for NullSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> { Ok(None) }
    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> { Ok(()) }
    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> { Ok(()) }
}
```

### 4.2 New infra implementation: `KeyringSecretStore` (in `rocket-infra`)

Add `keyring` (same version/features as `src-tauri/Cargo.toml:52`) as a dependency of
`rocket-infra` — it currently only depends on it via `src-tauri`, which is the wrong crate for
this per the DDD rule "concrete I/O only in `rocket-infra`" (`.claude/rules/rust-ddd-boundaries.md`).

```rust
// crates/rocket-infra/src/secret_store.rs
use rocket_environment::secret_store::SecretStore;
use rocket_shared::error::{DomainError, DomainResult};

const KEYRING_SERVICE: &str = "com.rocketapi.env-secrets"; // distinct namespace from git creds

pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // Locked/unavailable keychain must not hard-fail environment load — see 4.4.
            Err(e) => { tracing::warn!(error = %e, "keychain unavailable, secret unreadable"); Ok(None) }
        }
    }

    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        entry.set_password(value).map_err(|e| DomainError::Internal(e.to_string()))
    }

    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::Internal(e.to_string())),
        }
    }
}

fn account(scope_id: &str, key: &str) -> String {
    format!("{scope_id}:{key}")
}
```

### 4.3 `scope_id` derivation

`FsEnvironmentRepo` only knows a directory (`dir: PathBuf` — either
`<workspace>/environments/` for global envs or `<collection>/environments/` for per-collection
envs, per the file layout in the OpenCollection spec reference §5). That directory is already a
stable, unique identifier per environment *file family* (global vs. per-collection, per
workspace/collection). Derive `scope_id` from it directly — no need to plumb workspace/collection
IDs through the constructor:

```rust
fn scope_id(dir: &Path, env_name: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()).hash(&mut h);
    format!("{:016x}:{}", h.finish(), env_name)
}
```

(A `DefaultHasher` collision here only risks two *different* environments sharing a keychain
account, which is a correctness bug, not a confidentiality one — every access is still local and
authenticated by the OS keychain. Acceptable for v1; if stronger guarantees are wanted later this
can move to a stored UUID, out of scope for this spec.)

### 4.4 `FsEnvironmentRepo` wiring

`FsEnvironmentRepo` gains an optional `secret_store: Arc<dyn SecretStore>` (defaulting to
`NullSecretStore` via a new constructor, so every existing test/caller keeps compiling —
`FsEnvironmentRepo::new(dir)` stays, add `FsEnvironmentRepo::with_secret_store(dir, store)`).

**`save()`** (`fs_environment_repo.rs:63-69`): before serializing to YAML, split each `Variable`:
- non-secret → unchanged, goes to YAML with its real value (today's behavior).
- secret → the *value* goes to `secret_store.set(scope_id, key, value)` first; only after that
  `Ok`, the YAML gets an `OcSecretVariable` entry (name/description/disabled/secret_type only, no
  value — see §4.5). If `secret_store.set` errors (e.g. keychain locked), **abort the whole save**
  with that error — never silently write a YAML claiming `secret: true` whose value didn't
  actually make it to secure storage. If the variable *used to be* secret and no longer is (or was
  removed), call `secret_store.delete(scope_id, key)` — best-effort, log on failure, don't abort
  (a stale keychain entry is not a confidentiality problem, it just leaks nothing new).

**`get()`/`list()`** (`fs_environment_repo.rs:50-61,26-48`): after building the domain
`Environment` from YAML, for every variable with `secret: true`, call
`secret_store.get(scope_id, key)` and set `variable.value` to the result (empty string if
`None` — e.g. first read right after migration, before the user has re-saved it once). A keychain
read failure must **not** fail environment load (would brick app startup) — log a warning and
treat as `None`/empty, matching `KeyringSecretStore::get`'s own soft-fail above.

### 4.5 `OcEnvironment.variables` becomes a real sum type

Add an untagged enum mirroring the existing `OcAuth`/`OcItem` untagged pattern documented in
`crates/rocket-infra/CLAUDE.md` ("more specific types must come before less specific ones"):

```rust
// crates/rocket-infra/src/oc/variables.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcEnvVariableEntry {
    Secret(OcSecretVariable), // has a required `secret: true` field — tried first
    Plain(OcVariable),
}
```

Change `OcEnvironment.variables: Vec<OcVariable>` → `Vec<OcEnvVariableEntry>`
(`crates/rocket-infra/src/oc/environment.rs:19`).

Update the conversions (`crates/rocket-infra/src/conversions/environment.rs:5-31`):

```rust
impl From<OcEnvironment> for Environment {
    fn from(oc: OcEnvironment) -> Self {
        Environment {
            // ...
            variables: oc.variables.into_iter().map(|e| match e {
                OcEnvVariableEntry::Secret(s) => Variable::from(s), // existing impl, secret: true
                OcEnvVariableEntry::Plain(v) => Variable::from(v),  // existing impl, secret: false
            }).collect(),
            // ...
        }
    }
}

impl From<Environment> for OcEnvironment {
    fn from(env: Environment) -> Self {
        OcEnvironment {
            // ...
            variables: env.variables.into_iter().map(|v| {
                if v.secret {
                    OcEnvVariableEntry::Secret(OcSecretVariable {
                        secret: true,
                        name: v.key,
                        description: v.description,
                        disabled: if v.enabled { None } else { Some(true) },
                        secret_type: v.secret_type,
                    })
                } else {
                    OcEnvVariableEntry::Plain(OcVariable::from(v))
                }
            }).collect(),
            // ...
        }
    }
}
```

Note this conversion no longer has access to the raw secret *value* (correct — it must never
reach the YAML struct at all). The value routing happens one layer up, in
`FsEnvironmentRepo::save`/`get` (§4.4), which calls the secret store directly and only hands
`OcEnvironment::from(Environment)` a domain `Variable` whose `.value` is irrelevant for secret
entries (the conversion above never reads `v.value` in the `Secret` branch).

### 4.6 `EnvironmentService` — no change needed

`EnvironmentService::save` (`crates/rocket-app/src/environment_service.rs:41-74`) already emits
`AuditEventKind::SecretVariableWritten` on secret-value change by diffing against
`self.repo.get(&env.name)` — this keeps working unmodified once `repo.get()` correctly returns
real decrypted values to diff against (today it can't, because `secret: false` is hardcoded on
load, so the existing diff logic technically already "works" but only because it operates on
values that happen to be plaintext-and-unflagged; after this fix it operates on real values and
still stays correct).

## 5. Interfaces (for the implementation plan)

- `rocket_environment::secret_store::SecretStore` — new trait, `get`/`set`/`delete`, `Send + Sync`.
- `rocket_environment::secret_store::NullSecretStore` — new no-op impl.
- `rocket_infra::secret_store::KeyringSecretStore` — new impl backed by the `keyring` crate.
- `FsEnvironmentRepo::with_secret_store(dir: PathBuf, store: Arc<dyn SecretStore>) -> Self` — new
  constructor. `FsEnvironmentRepo::new(dir)` keeps working, defaults to `NullSecretStore`.
- `OcEnvVariableEntry` — new untagged enum, replaces `Vec<OcVariable>` with
  `Vec<OcEnvVariableEntry>` on `OcEnvironment.variables`.
- `src-tauri/src/lib.rs` wiring: construct one `Arc<KeyringSecretStore>` at startup (mirrors how
  `ReqwestExecutor::with_allowed_base` is wired) and pass it into every `FsEnvironmentRepo`
  construction site (global environments repo + per-collection environments repo, if there are
  two separate construction sites — verify at implementation time).

## 6. Migration

There is **no existing on-disk secret marker to migrate** — because the write path has always
silently dropped `secret: false` on load (`conversions/variables.rs:42` hardcodes it), no
environment `.yml` file in the wild today has ever had `secret: true` actually persisted. What
exists on disk today is indistinguishable plaintext for both "secret" and "non-secret" variables.

Consequence: after this fix ships, nothing changes automatically for existing files. A variable
the user *intended* as secret keeps its plaintext value sitting in the YAML, unprotected, until
the user opens that environment, (re-)confirms the secret toggle, and saves — at which point the
new save path strips it from YAML into the keychain for the first time.

This is an acceptable v1 scope (the fix stops new/edited secrets from leaking; it cannot
retroactively scrub git history or force a re-save the app has no way to know is needed). Flag it
to the user via a one-time changelog/release-note entry recommending affected users re-save their
environments; a proactive in-app "detect plaintext-looking values on secret-flagged variables and
prompt to re-save" nudge is a reasonable follow-up but out of scope here (§7).

## 7. Follow-ups (not blocking this spec)

- `.gitignore` scaffolding for `environments/` in newly git-init'd workspaces/collections, as
  defense in depth (secret values are already safe after this fix; this would additionally hide
  variable *names*/descriptions from git history for the extra-cautious).
- `CollectionVariable.secret` (`rocket-collection`) has the identical "always `false`, never
  persisted" gap for variables in `opencollection.yml` — deliberately out of scope here because
  collection variables are meant to be committed/shared by design; whether "secret" should even be
  a supported concept there is a product question, not purely a bug fix.
- A "migrate plaintext secrets now" in-app action, per §6.

## 8. Acceptance criteria

1. Saving an `Environment` with a `Variable { secret: true, value: "sk-live-123", .. }` produces a
   `.yml` file on disk that does **not** contain the substring `sk-live-123` anywhere.
2. The same `.yml` file, inspected directly, contains `secret: true` for that variable's entry
   and no `value:` field on it.
3. `FsEnvironmentRepo::get` on that environment returns a `Variable` with `secret: true` and
   `value == "sk-live-123"` (round-trips correctly).
4. A non-secret variable's on-disk representation and round-trip behavior are byte-for-byte
   unchanged from before this change (existing `fs_environment_repo.rs` tests
   `save_writes_spec_field_names`, `save_then_load_roundtrip_via_oc_format`,
   `load_old_format_with_key_field_still_works` continue to pass unmodified).
5. `cargo test -p rocket-environment -p rocket-infra` passes, including new tests for
   `KeyringSecretStore` (using a mock/in-memory `SecretStore` for the round-trip assertions above,
   not a real OS keychain in CI — see plan for how to structure this test).
6. `yarn tsc --noEmit` passes (no frontend contract change expected, but verify).
