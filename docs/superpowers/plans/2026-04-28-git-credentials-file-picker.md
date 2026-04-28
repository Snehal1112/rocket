# Git Credentials File Picker & OS Keychain Persistence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the plain-text SSH key path input with a native file picker, auto-detect the default SSH key on startup, and persist credentials securely in the OS keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service).

**Architecture:** Three new Tauri commands in `src-tauri/src/commands/git.rs` (`get_default_ssh_key_path`, `save_git_credentials`, `load_git_credentials`) backed by the `keyring` crate (v4). The frontend gains three new API wrappers in `tauri-api.ts`, a rewritten `GitCredentialsDialog.tsx` (clickable field, WCAG 2.1 AAA), and auto-load logic in `git-store.ts`.

**Tech Stack:** Rust `keyring` crate v4, `@tauri-apps/plugin-dialog` (already installed), `dirs` crate (already installed), React/TypeScript, Zustand, shadcn/ui, Vitest.

---

## File Map

| File | Action | Change |
|---|---|---|
| `src-tauri/Cargo.toml` | Modify | Add `keyring` dependency |
| `src-tauri/src/commands/git.rs` | Modify | Add 3 new commands + `GitCredentialsPayload` struct |
| `src-tauri/src/lib.rs` | Modify | Register 3 new commands in `generate_handler!` |
| `src/lib/tauri-api.ts` | Modify | Add 3 new exported functions |
| `src/components/git/GitCredentialsDialog.tsx` | Modify | Replace Input with file-picker button; add keychain load/save; WCAG AAA |
| `src/stores/git-store.ts` | Modify | Auto-load credentials in `setCollection` |
| `src/stores/__tests__/git-store.test.ts` | Modify | Add tests for credential auto-load |

---

## Task 1: Add `keyring` crate dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the dependency**

Open `src-tauri/Cargo.toml`. In the `[dependencies]` section, add after the existing `dirs` line:

```toml
keyring = { version = "4", features = ["apple-native", "windows-native", "sync-secret-service"] }
```

- [ ] **Step 2: Verify it resolves**

```bash
cargo check -p src-tauri 2>&1 | tail -5
```

Expected: `Finished` or only existing warnings — no errors about `keyring`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml Cargo.lock
git commit -m "chore(deps): add keyring crate for OS keychain credential storage"
```

---

## Task 2: Add Rust commands — `get_default_ssh_key_path`, `save_git_credentials`, `load_git_credentials`

**Files:**
- Modify: `src-tauri/src/commands/git.rs`

- [ ] **Step 1: Add the `GitCredentialsPayload` struct and new commands**

At the top of `src-tauri/src/commands/git.rs`, add the import:

```rust
use serde::{Deserialize, Serialize};
```

Then at the **bottom** of the file, after the last existing command, add:

```rust
const KEYRING_SERVICE: &str = "rocket-api";
const KEYRING_ACCOUNT: &str = "git-credentials";

/// Serialisable mirror of GitCredentials — used only for keychain persistence.
/// Kept separate from the domain type so the wire format is stable even if
/// the domain enum changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitCredentialsPayload {
    #[serde(rename_all = "camelCase")]
    SshKey {
        private_key_path: String,
        passphrase: Option<String>,
    },
    SshAgent,
    #[serde(rename_all = "camelCase")]
    UserPass {
        username: String,
        password: String,
    },
    Token {
        token: String,
    },
}

/// Return the absolute path of the first default SSH private key found in
/// `~/.ssh/`, checking id_ed25519 → id_rsa → id_ecdsa → id_dsa in order.
/// Returns None if the home directory cannot be determined or no key exists.
#[tauri::command]
pub fn get_default_ssh_key_path() -> Option<String> {
    let home = dirs::home_dir()?;
    let ssh_dir = home.join(".ssh");
    for name in &["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"] {
        let path = ssh_dir.join(name);
        if path.exists() {
            return path.to_str().map(str::to_owned);
        }
    }
    None
}

/// Persist git credentials to the OS keychain (macOS Keychain, Windows
/// Credential Manager, Linux Secret Service). The passphrase, if present,
/// is stored inside the encrypted keychain entry — never written to disk.
#[tauri::command]
pub fn save_git_credentials(creds: GitCredentialsPayload) -> Result<(), String> {
    let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| e.to_string())?;
    entry.set_password(&json).map_err(|e| e.to_string())
}

/// Load previously saved git credentials from the OS keychain.
/// Returns None if no entry exists yet (first run). Errors if the keychain
/// is unavailable (e.g. locked) — callers should treat this as no-credentials.
#[tauri::command]
pub fn load_git_credentials() -> Result<Option<GitCredentialsPayload>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(json) => {
            let creds: GitCredentialsPayload =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            Ok(Some(creds))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p src-tauri 2>&1 | tail -5
```

Expected: `Finished` — no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/git.rs
git commit -m "feat(git): add get_default_ssh_key_path, save/load_git_credentials commands"
```

---

## Task 3: Register new commands in Tauri handler

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Register the three new commands**

In `src-tauri/src/lib.rs`, find the `generate_handler![` block. Locate the line:

```rust
            commands::git::git_set_remote_url,
```

Add the three new commands immediately after it:

```rust
            commands::git::git_set_remote_url,
            commands::git::get_default_ssh_key_path,
            commands::git::save_git_credentials,
            commands::git::load_git_credentials,
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p src-tauri 2>&1 | tail -5
```

Expected: `Finished`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(git): register get_default_ssh_key_path, save/load_git_credentials in Tauri handler"
```

---

## Task 4: Add frontend API wrappers

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add the three new exported functions**

Open `src/lib/tauri-api.ts`. Find where `GitCredentials` is defined or imported. Add the three exports near the existing git API functions (search for `gitFetch` and add after the block):

```typescript
export const getDefaultSshKeyPath = (): Promise<string | null> =>
  invoke<string | null>('get_default_ssh_key_path');

export const saveGitCredentials = (creds: GitCredentials): Promise<void> =>
  invoke<void>('save_git_credentials', { creds });

export const loadGitCredentials = (): Promise<GitCredentials | null> =>
  invoke<GitCredentials | null>('load_git_credentials');
```

- [ ] **Step 2: Verify TypeScript**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(git): add getDefaultSshKeyPath, saveGitCredentials, loadGitCredentials API wrappers"
```

---

## Task 5: Auto-load credentials in `git-store.ts`

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/stores/__tests__/git-store.test.ts`

- [ ] **Step 1: Write the failing test**

Open `src/stores/__tests__/git-store.test.ts`. Add `loadGitCredentials` to the mock at the top:

```typescript
// inside the vi.mock('@/lib/tauri-api', ...) factory, add:
loadGitCredentials: vi.fn().mockResolvedValue(null),
```

Then add a new `describe` block at the bottom of the file:

```typescript
describe('git-store credential auto-load', () => {
  const { loadGitCredentials } = await import('@/lib/tauri-api');

  beforeEach(() => {
    useGitStore.setState({
      isRepo: false,
      collectionPath: null,
      credentials: null,
      status: null,
      branches: null,
      remotes: [],
      stashes: [],
      commitLog: [],
      conflicts: [],
      loading: false,
      error: null,
      showCredentialsDialog: false,
      pendingNetworkOp: null,
    });
    vi.clearAllMocks();
  });

  it('auto-loads saved credentials from keychain when collection is a repo', async () => {
    const savedCreds = { type: 'sshKey', privateKeyPath: '/home/user/.ssh/id_ed25519', passphrase: undefined };
    vi.mocked(loadGitCredentials).mockResolvedValue(savedCreds as any);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await useGitStore.getState().setCollection('/some/collection');

    expect(useGitStore.getState().credentials).toEqual(savedCreds);
  });

  it('leaves credentials null when keychain returns null', async () => {
    vi.mocked(loadGitCredentials).mockResolvedValue(null);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await useGitStore.getState().setCollection('/some/collection');

    expect(useGitStore.getState().credentials).toBeNull();
  });

  it('does not overwrite already-set credentials when keychain returns null', async () => {
    const existing = { type: 'token', token: 'mytoken' };
    useGitStore.setState({ credentials: existing as any });
    vi.mocked(loadGitCredentials).mockResolvedValue(null);
    vi.mocked(gitIsRepo).mockResolvedValue(true);

    await useGitStore.getState().setCollection('/some/collection');

    expect(useGitStore.getState().credentials).toEqual(existing);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
yarn test git-store 2>&1 | tail -20
```

Expected: FAIL — `loadGitCredentials` not imported in store yet.

- [ ] **Step 3: Import and wire `loadGitCredentials` in the store**

Open `src/stores/git-store.ts`. Add `loadGitCredentials` to the import from `@/lib/tauri-api`:

```typescript
import {
  // ... existing imports ...
  loadGitCredentials,
} from '@/lib/tauri-api';
```

Then in the `setCollection` action, after `set({ isRepo })` and inside the `if (isRepo)` block, add the credential auto-load **before** the `Promise.all`:

```typescript
setCollection: async (path: string) => {
  set({ collectionPath: path, loading: true, error: null });
  try {
    const isRepo = await gitIsRepo(path);
    set({ isRepo });
    if (isRepo) {
      // Auto-load persisted credentials if none are set in memory.
      if (!get().credentials) {
        try {
          const saved = await loadGitCredentials();
          if (saved) set({ credentials: saved });
        } catch {
          // Keychain unavailable — proceed without credentials.
        }
      }
      const [status] = await Promise.all([
        gitStatus(path),
        get().refreshStashes(),
        get().refreshBranches(),
        get().refreshRemotes(),
      ]);
      set({ status, loading: false });
    } else {
      set({ status: null, loading: false });
    }
  } catch (e) {
    set({ error: String(e), loading: false });
  }
},
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
yarn test git-store 2>&1 | tail -20
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
git commit -m "feat(git): auto-load persisted credentials from OS keychain on collection open"
```

---

## Task 6: Rewrite `GitCredentialsDialog.tsx`

**Files:**
- Modify: `src/components/git/GitCredentialsDialog.tsx`

- [ ] **Step 1: Replace the entire file**

Replace the full contents of `src/components/git/GitCredentialsDialog.tsx` with:

```typescript
import { useEffect, useId, useState } from 'react';
import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { FolderOpen } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { GitCredentials } from '@/lib/tauri-api';
import {
  getDefaultSshKeyPath,
  loadGitCredentials,
  saveGitCredentials,
} from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';

type AuthType = 'sshKey' | 'sshAgent' | 'userPass' | 'token';

export function GitCredentialsDialog() {
  const { showCredentialsDialog, setShowCredentialsDialog, setCredentials } = useGitStore();
  const [authType, setAuthType] = useState<AuthType>('sshKey');
  const [privateKeyPath, setPrivateKeyPath] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [token, setToken] = useState('');
  const [saveError, setSaveError] = useState<string | null>(null);

  const keyPathHintId = useId();
  const saveErrorId = useId();

  // On open: load persisted credentials first; fall back to SSH key auto-detection.
  useEffect(() => {
    if (!showCredentialsDialog) return;
    setSaveError(null);

    (async () => {
      try {
        const saved = await loadGitCredentials();
        if (saved) {
          if (saved.type === 'sshKey') {
            setAuthType('sshKey');
            setPrivateKeyPath((saved as any).privateKeyPath ?? '');
            setPassphrase((saved as any).passphrase ?? '');
          } else if (saved.type === 'sshAgent') {
            setAuthType('sshAgent');
          } else if (saved.type === 'userPass') {
            setAuthType('userPass');
            setUsername((saved as any).username ?? '');
            setPassword((saved as any).password ?? '');
          } else if (saved.type === 'token') {
            setAuthType('token');
            setToken((saved as any).token ?? '');
          }
          return;
        }
      } catch {
        // Keychain unavailable — fall through to defaults.
      }

      // No saved credentials: auto-detect SSH key.
      try {
        const detected = await getDefaultSshKeyPath();
        if (detected) setPrivateKeyPath(detected);
      } catch {
        // Auto-detection failed — leave field empty (placeholder shown).
      }
    })();
  }, [showCredentialsDialog]);

  const handleBrowseKey = async () => {
    // Always open the picker in ~/.ssh/ — use getDefaultSshKeyPath to resolve
    // the home directory cross-platform, then strip the filename.
    let sshDir: string | undefined;
    try {
      const detected = await getDefaultSshKeyPath();
      if (detected) {
        sshDir = detected.replace(/[\\/][^\\/]+$/, '');
      }
    } catch {
      sshDir = undefined;
    }
    const selected = await openFilePicker({ multiple: false, defaultPath: sshDir });
    if (typeof selected === 'string' && selected) {
      setPrivateKeyPath(selected);
    }
  };

  const handleConnect = async () => {
    let creds: GitCredentials;
    switch (authType) {
      case 'sshKey':
        creds = { type: 'sshKey', privateKeyPath, passphrase: passphrase || undefined };
        break;
      case 'sshAgent':
        creds = { type: 'sshAgent' };
        break;
      case 'userPass':
        creds = { type: 'userPass', username, password };
        break;
      case 'token':
        creds = { type: 'token', token };
        break;
    }

    // Persist to OS keychain; surface error inline but never block the connect.
    try {
      await saveGitCredentials(creds);
    } catch (e) {
      setSaveError(`Could not save credentials to keychain: ${String(e)}`);
    }

    setCredentials(creds);
  };

  return (
    <Dialog open={showCredentialsDialog} onOpenChange={setShowCredentialsDialog}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle>Git Credentials</DialogTitle>
        </DialogHeader>

        <div className='space-y-3'>
          {/* Save error alert */}
          {saveError && (
            <p
              id={saveErrorId}
              role='alert'
              className='text-xs text-destructive'
            >
              {saveError}
            </p>
          )}

          {/* Authentication type */}
          <div>
            <Label htmlFor='auth-type-select' className='text-sm'>
              Authentication Type
            </Label>
            <Select value={authType} onValueChange={(v) => setAuthType(v as AuthType)}>
              <SelectTrigger id='auth-type-select' className='h-8 text-sm'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='sshAgent'>SSH Agent</SelectItem>
                <SelectItem value='sshKey'>SSH Key</SelectItem>
                <SelectItem value='userPass'>Username / Password</SelectItem>
                <SelectItem value='token'>Token</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* SSH Key fields */}
          {authType === 'sshKey' && (
            <>
              <div>
                <Label htmlFor='ssh-key-picker' className='text-sm'>
                  Private Key Path
                </Label>
                <button
                  id='ssh-key-picker'
                  type='button'
                  onClick={handleBrowseKey}
                  aria-label='Select SSH private key file'
                  aria-describedby={keyPathHintId}
                  className={[
                    'flex h-8 w-full items-center justify-between rounded-md px-3 text-sm transition-colors',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
                    'border bg-background hover:bg-accent hover:text-accent-foreground',
                    privateKeyPath
                      ? 'border-input text-foreground'
                      : 'border-dashed border-muted-foreground/50 text-muted-foreground italic',
                  ].join(' ')}
                >
                  <span className='truncate font-mono'>
                    {privateKeyPath || 'Click to select a key file…'}
                  </span>
                  <FolderOpen
                    className='ml-2 h-3.5 w-3.5 shrink-0 text-muted-foreground'
                    aria-hidden='true'
                  />
                </button>
                <p id={keyPathHintId} className='mt-1 text-xs text-muted-foreground'>
                  Click to choose a file — opens in ~/.ssh/
                </p>
              </div>

              <div>
                <Label htmlFor='passphrase-input' className='text-sm'>
                  Passphrase <span className='text-muted-foreground'>(optional)</span>
                </Label>
                <Input
                  id='passphrase-input'
                  type='password'
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='current-password'
                />
              </div>
            </>
          )}

          {/* Username / Password fields */}
          {authType === 'userPass' && (
            <>
              <div>
                <Label htmlFor='username-input' className='text-sm'>
                  Username
                </Label>
                <Input
                  id='username-input'
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='username'
                />
              </div>
              <div>
                <Label htmlFor='password-input' className='text-sm'>
                  Password
                </Label>
                <Input
                  id='password-input'
                  type='password'
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className='h-8 text-sm'
                  autoComplete='current-password'
                />
              </div>
            </>
          )}

          {/* Token field */}
          {authType === 'token' && (
            <div>
              <Label htmlFor='token-input' className='text-sm'>
                Token
              </Label>
              <Input
                id='token-input'
                type='password'
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className='h-8 text-sm'
                autoComplete='current-password'
              />
            </div>
          )}

          <Button onClick={handleConnect} className='w-full' size='sm'>
            Connect
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify TypeScript**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Verify lint**

```bash
yarn check 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/git/GitCredentialsDialog.tsx
git commit -m "feat(git): replace SSH key path input with WCAG-AAA file picker and add keychain save/load"
```

---

## Task 7: Manual smoke test

> No automated test exists for the full dialog interaction (requires a running Tauri app). Do this manually before marking complete.

- [ ] **Step 1: Start the dev app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Open a collection that is a git repo with an SSH remote**

Open any workspace collection that has a git remote using SSH.

- [ ] **Step 3: Trigger the credentials dialog**

Click **Fetch** in the Git panel. The Git Credentials dialog should appear.

- [ ] **Step 4: Verify SSH Key auth type**

Confirm:
- Auth type defaults to "SSH Key"
- Private Key Path field is a clickable button (not a text input)
- If `~/.ssh/id_ed25519` (or `id_rsa` etc.) exists on the machine, it is pre-filled
- If no key exists, the italic placeholder "Click to select a key file…" is shown
- The hint text "Click to choose a file — opens in ~/.ssh/" appears below

- [ ] **Step 5: Test the file picker**

Click the Private Key Path button. Confirm:
- The native OS file picker opens
- It starts in the `~/.ssh/` directory
- Cancelling leaves the field unchanged
- Selecting a file updates the field with the full path

- [ ] **Step 6: Test keyboard accessibility**

Tab to the Private Key Path button. Confirm a visible focus ring appears. Press Enter — the file picker should open.

- [ ] **Step 7: Connect and verify persistence**

Fill in the path and click Connect. Close the app completely and reopen it. Open the same collection and click Fetch again. The credentials dialog should pre-fill with the previously saved values (loaded from OS keychain).

- [ ] **Step 8: Test error display**

If you can simulate a keychain failure (e.g. deny keychain access on macOS), confirm the dialog still opens (with defaults) and Connect still works — the `role="alert"` error appears but doesn't block the operation.

---

## Task 8: Final checks and cleanup

- [ ] **Step 1: Run the full test suite**

```bash
yarn test 2>&1 | tail -30
```

Expected: all tests pass, no regressions.

- [ ] **Step 2: Run TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Run lint**

```bash
yarn check 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Run Rust check**

```bash
cargo check -p src-tauri 2>&1 | tail -5
```

Expected: `Finished`.

- [ ] **Step 5: Commit any remaining changes**

```bash
git status
# If clean, nothing to do. If not:
git add -p
git commit -m "chore(git): final cleanup after file picker implementation"
```
