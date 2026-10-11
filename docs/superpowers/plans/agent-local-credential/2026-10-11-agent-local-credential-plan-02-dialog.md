# Agent Local Credential Implementation Plan 02: Dialog

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A user can choose where an agent's credential comes from (RocketVault, a Rocket environment variable, or the OS keychain) in the AI Agents dialog, and can save or remove a keychain credential without it ever being shown again.

**Architecture:** The TS `AgentConfig` type and three IPC wrappers come first. Then `AgentConfigsDialog` gets a source choice (`RadioGroup`) and a form that no longer depends on RocketVault connections. Last, a keychain field flow saves the credential through `set_agent_credential` after the config is saved. Plan 01 provides the backend.

**Tech Stack:** React + TypeScript, shadcn/ui (`RadioGroup`, `Input`, `Button`, `Label`), lucide-react, TanStack Query (existing mutations), Vitest + Testing Library.

**Spec:** [../../specs/2026-10-11-agent-local-credential-design.md](../../specs/2026-10-11-agent-local-credential-design.md). Index and rulings: [00-index.md](00-index.md). Plan 01 must be complete.

## Global Constraints

- shadcn/ui primitives and lucide-react icons only. No raw `<button>`, `<input>`, `<select>`. Zustand/React Query usage as in the neighboring code. Biome style.
- A typed credential value is held only in component state, sent only to `setAgentCredential`, cleared after the call, on cancel, on dialog close and when the source changes. It is never put in a toast, console message, query cache, error text or DOM attribute other than the password input's value. Its input has `type='password'` and `autoComplete='off'`.
- `hasAgentCredential` returns only a boolean. The dialog never asks for the value.
- The dialog form is no longer blocked when there are no RocketVault connections. Only the vault block shows that notice.
- Comments are short full sentences ending with a period. Yarn only.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths.

## Review Focus

- A user with no RocketVault connection can add an `env` or `keychain` agent (T2).
- A token-looking text in the environment variable name field is flagged, the text is not repeated in the message, and Save is blocked (T2).
- A failed `setAgentCredential` leaves the dialog open with an error that does not contain the value (T3).
- A blank keychain field on an existing agent keeps the saved entry; a new keychain agent with a blank field is blocked (T3).
- Switching an agent from `keychain` to another source clears the typed value and does not call `setAgentCredential` (T3).

---

### Task 1: TS types and wrappers

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/__tests__/tauri-api.test.ts`

**Interfaces:**
- Consumes: the IPC commands from Plan 01 T3 (`set_agent_credential`, `clear_agent_credential`, `has_agent_credential`, argument `configId`) and the DTO fields `credentialSource`, `sourceEnvVar`.
- Produces: `AgentCredentialSource`, optional `AgentConfig.credentialSource` and `sourceEnvVar`, `setAgentCredential`, `clearAgentCredential`, `hasAgentCredential`.

- [ ] **Step 1: Write the failing test.** In `tauri-api.test.ts`, find how an existing wrapper such as `testAgentConfig` or `deleteAgentConfig` is tested (the `invoke` mock and its assertion style) and add the same for the three new wrappers:

```ts
  it('calls the agent credential commands with a configId and returns only a boolean', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    await api.setAgentCredential('agent-1', 'fake-value');
    expect(invokeMock).toHaveBeenLastCalledWith('set_agent_credential', {
      configId: 'agent-1',
      value: 'fake-value',
    });
    invokeMock.mockResolvedValueOnce(undefined);
    await api.clearAgentCredential('agent-1');
    expect(invokeMock).toHaveBeenLastCalledWith('clear_agent_credential', { configId: 'agent-1' });
    invokeMock.mockResolvedValueOnce(true);
    await expect(api.hasAgentCredential('agent-1')).resolves.toBe(true);
    expect(invokeMock).toHaveBeenLastCalledWith('has_agent_credential', { configId: 'agent-1' });
  });
```

(Use the file's real name for the mocked `invoke`.)

- [ ] **Step 2: Run to verify it fails**

Run: `yarn vitest run src/lib/__tests__/tauri-api.test.ts`
Expected: FAIL (wrappers not defined).

- [ ] **Step 3: Implement** in `tauri-api.ts`. Next to the `AgentConfig` interface add:

```ts
export type AgentCredentialSource = 'vault' | 'env' | 'keychain';
```

and inside `AgentConfig`:

```ts
  // Where the credential comes from. A missing value means 'vault'.
  credentialSource?: AgentCredentialSource;
  // For the 'env' source: the name of the variable Rocket reads from its own environment.
  sourceEnvVar?: string;
```

After `testAgentConfig`:

```ts
// The value goes in and never comes back. `hasAgentCredential` returns a boolean only.
export const setAgentCredential = (configId: string, value: string) =>
  invoke<void>('set_agent_credential', { configId, value });

export const clearAgentCredential = (configId: string) =>
  invoke<void>('clear_agent_credential', { configId });

export const hasAgentCredential = (configId: string) =>
  invoke<boolean>('has_agent_credential', { configId });
```

- [ ] **Step 4: Run to verify it passes**

Run: `yarn vitest run src/lib/__tests__/tauri-api.test.ts`
Expected: PASS.

- [ ] **Step 5: Checks.** `yarn tsc --noEmit` and `yarn check`.

- [ ] **Step 6: Commit** via the commit skill. Paths: the two files. Message: `feat(assistant): add agent credential wrappers`.

---

### Task 2: Source choice, environment variable field and payload

**Files:**
- Modify: `src/components/settings/AgentConfigsDialog.tsx`
- Modify: `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`

**Interfaces:**
- Consumes: T1's `AgentCredentialSource`, `AgentConfig.credentialSource`, `AgentConfig.sourceEnvVar`; the existing `validateEnvName`, `looksLikeSecret`.
- Produces: form fields `credentialSource` and `sourceEnvVar` in the edit state; a payload with `credentialSource` and `sourceEnvVar`; the source `RadioGroup` (labels "RocketVault secret", "Rocket environment variable", "Keychain").

- [ ] **Step 1: Write the failing tests.** In `AgentConfigsDialog.test.tsx` add the three new API wrappers to the `vi.mock` factory (`setAgentCredential`, `clearAgentCredential`, `hasAgentCredential`: `vi.fn()`), reset them in `beforeEach` (`hasAgentCredential` resolves `false`). Find how the existing tests open the add form, fill Label, Command and Credential Env Var, and press Save, and reuse those helpers or steps. Add:

```ts
  it('lets a user with no vault connection add an environment variable agent', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Label'), 'Claude');
    await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
    await user.type(screen.getByLabelText('Credential Env Var'), 'CLAUDE_CODE_OAUTH_TOKEN');
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'MY_CLAUDE_TOKEN');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      credentialSource: 'env',
      sourceEnvVar: 'MY_CLAUDE_TOKEN',
      credentialEnvVar: 'CLAUDE_CODE_OAUTH_TOKEN',
      vaultConnectionId: '',
      vaultName: '',
      vaultSecretId: '',
    });
  });

  it('flags a token-looking environment variable name without repeating it', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Label'), 'Claude');
    await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
    await user.type(screen.getByLabelText('Credential Env Var'), 'CLAUDE_CODE_OAUTH_TOKEN');
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'sk-ant-oat01-FAKE');

    expect(await screen.findByText(/looks like a secret value/i)).toBeInTheDocument();
    expect(screen.getByText(/looks like a secret value/i).textContent).not.toContain('sk-ant');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('keeps the vault fields and sends credentialSource vault for a vault agent', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    expect(await screen.findByLabelText('Vault Name')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /rocketvault secret/i })).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
        credentialSource: 'vault',
        vaultConnectionId: 'conn-1',
      }),
    );
  });
```

Update the existing tests that assert the exact payload with `toHaveBeenCalledWith({...})` (for example "saves added variables with the exact payload") so the expected object gains `credentialSource: 'vault'` (leave `sourceEnvVar` out; the payload omits it for other sources).

- [ ] **Step 2: Run to verify it fails**

Run: `yarn vitest run src/components/settings/__tests__/AgentConfigsDialog.test.tsx`
Expected: FAIL (no radios, no env field).

- [ ] **Step 3: Implement** in `AgentConfigsDialog.tsx`.

Imports: `RadioGroup, RadioGroupItem` from `@/components/ui/radio-group`; `type AgentCredentialSource` from `@/lib/tauri-api`.

Add the validator and message next to `validateCredentialEnvVar`:

```ts
const SOURCE_ENV_SECRET_MESSAGE =
  'This looks like a secret value. This field takes only the name of the environment variable that holds it, for example CLAUDE_CODE_OAUTH_TOKEN. If this is a real key, treat it as exposed and revoke it.';

// An empty value is not flagged here. The save handler reports it as required.
// The message never repeats the entered text.
function validateSourceEnvVar(value: string): string | null {
  const text = value.trim();
  if (text.length === 0) return null;
  if (looksLikeSecret(text)) return SOURCE_ENV_SECRET_MESSAGE;
  return validateEnvName(text);
}
```

Form state: add to the edit-form type, `emptyForm` and `startEdit`: `credentialSource: AgentCredentialSource` (`'vault'` by default; `c.credentialSource ?? 'vault'`) and `sourceEnvVar: string` (`''`; `c.sourceEnvVar ?? ''`).

Remove the blocking branch for `connectionsLoading` / `connectionsError` / `connections.length === 0` around the whole form. The form always renders. Move that notice into the vault block: when `editing.credentialSource === 'vault'` show, in this order, the loading line, the error/empty notice (same texts as today, without the Cancel button), or the existing Vault Connection / Vault Name / Fetch / Secret controls. Place the `RadioGroup` directly above that block:

```tsx
<div>
  <Label className='text-sm'>Credential source</Label>
  <RadioGroup
    value={editing.credentialSource}
    onValueChange={(value) =>
      setEditing({ ...editing, credentialSource: value as AgentCredentialSource })
    }
    className='mt-1 space-y-1'
  >
    <div className='flex items-center gap-2'>
      <RadioGroupItem value='vault' id='ac-source-vault' />
      <Label htmlFor='ac-source-vault' className='text-sm font-normal'>RocketVault secret</Label>
    </div>
    <div className='flex items-center gap-2'>
      <RadioGroupItem value='env' id='ac-source-env' />
      <Label htmlFor='ac-source-env' className='text-sm font-normal'>Rocket environment variable</Label>
    </div>
    <div className='flex items-center gap-2'>
      <RadioGroupItem value='keychain' id='ac-source-keychain' />
      <Label htmlFor='ac-source-keychain' className='text-sm font-normal'>Keychain</Label>
    </div>
  </RadioGroup>
</div>
```

The `env` block:

```tsx
{editing.credentialSource === 'env' && (
  <div>
    <Label htmlFor='ac-source-env-var' className='text-sm'>Environment variable name</Label>
    <Input
      id='ac-source-env-var'
      value={editing.sourceEnvVar}
      onChange={(e) => setEditing({ ...editing, sourceEnvVar: e.target.value })}
      placeholder='CLAUDE_CODE_OAUTH_TOKEN'
      className='h-8 text-sm'
    />
    {sourceEnvError ? (
      <p className='text-xs text-destructive'>{sourceEnvError}</p>
    ) : (
      <p className='text-xs text-muted-foreground'>
        Rocket reads this variable when a session starts, so it must be exported in the shell Rocket
        was started from. An app started from a desktop icon may not see it.
      </p>
    )}
  </div>
)}
```

with `const sourceEnvError = editing && editing.credentialSource === 'env' ? validateSourceEnvVar(editing.sourceEnvVar) : null;` and `envInvalid` extended with `|| sourceEnvError !== null`. The Save button is disabled when `envInvalid` (check how it is disabled today and keep that).

Update the help text under Credential Env Var that says to put the secret in the RocketVault secret: say "the secret itself comes from the credential source below".

`handleSave`: the required-field check depends on the source:

```ts
    const source = editing.credentialSource;
    const missingVault =
      source === 'vault' &&
      (!editing.vaultConnectionId || !editing.vaultName.trim() || !editing.vaultSecretId);
    const missingEnv = source === 'env' && !editing.sourceEnvVar.trim();
    if (!editing.label.trim() || !editing.command.trim() || !editing.credentialEnvVar.trim() || missingVault || missingEnv) {
      toast.error(
        source === 'vault'
          ? 'Label, command, credential env var, vault connection, vault name, and secret are required.'
          : source === 'env'
            ? 'Label, command, credential env var, and the environment variable name are required.'
            : 'Label, command, and credential env var are required.',
      );
      return;
    }
```

The payload gets `credentialSource: source`, `sourceEnvVar: source === 'env' ? editing.sourceEnvVar.trim() : undefined`, and for a non-vault source `vaultConnectionId: ''`, `vaultName: ''`, `vaultSecretId: ''`, `vaultSecretName: ''` (vault source keeps the current values).

- [ ] **Step 4: Run to verify it passes**

Run: `yarn vitest run src/components/settings/__tests__/AgentConfigsDialog.test.tsx`
Expected: PASS (existing tests with updated payloads, and the 3 new ones).

- [ ] **Step 5: Checks.** `yarn tsc --noEmit` and `yarn check`.

- [ ] **Step 6: Commit** via the commit skill. Paths: the two files. Message: `feat(assistant): choose the agent credential source`.

---

### Task 3: Keychain field flow, notice and docs

**Files:**
- Modify: `src/components/settings/AgentConfigsDialog.tsx`
- Modify: `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`
- Modify: `src/components/assistant/AssistantHistoryView.tsx` (notice text) and the test that pins it
- Modify: `.claude/assistant-conversations.md` (the "not scrubbed" limit)

**Interfaces:**
- Consumes: T1's `setAgentCredential`, `clearAgentCredential`, `hasAgentCredential`; T2's form state and `handleSave`.
- Produces: form state `localCredential: string`, `hasSavedCredential: boolean`, `removeSavedCredential: boolean`; the keychain block; the post-save credential calls.

- [ ] **Step 1: Write the failing tests** in `AgentConfigsDialog.test.tsx`. Define `const FAKE = 'sk-ant-oat01-FAKE-VALUE-FOR-TESTS';` and a helper that opens the add form, fills Label, Command, Credential Env Var and picks the Keychain radio (`screen.getByRole('radio', { name: /^keychain$/i })`). Add:

```ts
  it('saves the config first and then the typed keychain credential, and clears the field', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    vi.mocked(api.setAgentCredential).mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderDialog();
    await openKeychainForm(user);
    await user.type(screen.getByLabelText('Keychain credential'), FAKE);
    expect(screen.getByLabelText('Keychain credential')).toHaveAttribute('type', 'password');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(api.setAgentCredential).toHaveBeenCalledTimes(1));
    const savedConfig = vi.mocked(api.saveAgentConfig).mock.calls[0][0];
    expect(savedConfig.credentialSource).toBe('keychain');
    expect(vi.mocked(api.setAgentCredential).mock.calls[0]).toEqual([savedConfig.id, FAKE]);
    expect(
      vi.mocked(api.saveAgentConfig).mock.invocationCallOrder[0],
    ).toBeLessThan(vi.mocked(api.setAgentCredential).mock.invocationCallOrder[0]);
    expect(JSON.stringify(vi.mocked(api.saveAgentConfig).mock.calls)).not.toContain(FAKE);
  });

  it('keeps the dialog open and hides the value when the credential cannot be saved', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    vi.mocked(api.setAgentCredential).mockRejectedValue(new Error('the keychain could not be read'));
    const user = userEvent.setup();
    renderDialog();
    await openKeychainForm(user);
    await user.type(screen.getByLabelText('Keychain credential'), FAKE);
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(JSON.stringify(vi.mocked(toast.error).mock.calls)).not.toContain(FAKE);
    expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
  });

  it('blocks a new keychain agent with an empty credential', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await openKeychainForm(user);
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(toast.error).toHaveBeenCalledWith(expect.stringMatching(/enter the credential/i));
    expect(api.saveAgentConfig).not.toHaveBeenCalled();
  });

  it('shows a saved credential as saved, keeps it on a blank field, and can remove it', async () => {
    const keychainAgent: api.AgentConfig = {
      ...claude,
      credentialSource: 'keychain',
      vaultConnectionId: '',
      vaultName: '',
      vaultSecretId: '',
      vaultSecretName: '',
    };
    vi.mocked(api.listAgentConfigs).mockResolvedValue([keychainAgent]);
    vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
    vi.mocked(api.clearAgentCredential).mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    expect(await screen.findByText(/saved in the keychain/i)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.setAgentCredential).not.toHaveBeenCalled();
    expect(api.clearAgentCredential).not.toHaveBeenCalled();
  });

  it('removes the saved credential when Remove is used and the agent is saved', async () => {
    const keychainAgent: api.AgentConfig = { ...claude, credentialSource: 'keychain' };
    vi.mocked(api.listAgentConfigs).mockResolvedValue([keychainAgent]);
    vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
    vi.mocked(api.clearAgentCredential).mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await user.click(await screen.findByRole('button', { name: /remove saved credential/i }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.clearAgentCredential).toHaveBeenCalledWith('agent-1'));
  });

  it('does not save a typed credential after the source is switched away from the keychain', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await openKeychainForm(user);
    await user.type(screen.getByLabelText('Keychain credential'), FAKE);
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'MY_CLAUDE_TOKEN');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(api.setAgentCredential).not.toHaveBeenCalled();
  });
```

For the History notice, find the test that pins the notice text (`grep -rn "Vault values and collections" src`) and update its expectation to the new text in Step 3.

- [ ] **Step 2: Run to verify it fails**

Run: `yarn vitest run src/components/settings/__tests__/AgentConfigsDialog.test.tsx`
Expected: FAIL (no keychain block).

- [ ] **Step 3: Implement** in `AgentConfigsDialog.tsx`.

Add to the edit-form state, `emptyForm` and `startEdit`: `localCredential: ''`, `removeSavedCredential: false`, and a separate `const [hasSavedCredential, setHasSavedCredential] = useState(false);`. In `startEdit`, when `c.credentialSource === 'keychain'` call `hasAgentCredential(c.id)` and set the state (catch: `setHasSavedCredential(false)`), guarded against a stale answer the same way `fetchGeneration` guards the secrets fetch (reuse its pattern: compare the id that is being edited). In `startAdd`, `cancelEdit` and the `!open` effect, reset `localCredential` and `hasSavedCredential`.

When the radio changes away from `keychain`, also reset `localCredential: ''` and `removeSavedCredential: false` in the same `setEditing` call.

The keychain block:

```tsx
{editing.credentialSource === 'keychain' && (
  <div>
    <Label htmlFor='ac-local-credential' className='text-sm'>Keychain credential</Label>
    <div className='flex items-center gap-2'>
      <Input
        id='ac-local-credential'
        type='password'
        autoComplete='off'
        value={editing.localCredential}
        onChange={(e) => setEditing({ ...editing, localCredential: e.target.value })}
        placeholder={hasSavedCredential && !editing.removeSavedCredential ? 'Saved in the keychain. Type to replace.' : 'Paste the token'}
        className='h-8 text-sm'
      />
      {hasSavedCredential && (
        <Button
          type='button'
          variant='outline'
          size='sm'
          aria-label='Remove saved credential'
          aria-pressed={editing.removeSavedCredential}
          onClick={() => setEditing({ ...editing, removeSavedCredential: !editing.removeSavedCredential, localCredential: '' })}
        >
          <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      )}
    </div>
    <p className='text-xs text-muted-foreground'>
      {hasSavedCredential && !editing.removeSavedCredential
        ? 'Saved in the keychain. Leave the field empty to keep it.'
        : 'Stored in the OS keychain on this computer. It is never written to a file and never shown again.'}
    </p>
  </div>
)}
```

(`Trash2` is already imported; use `type='button'` only if the Button component accepts it, as the neighbors do.)

`handleSave`: after the checks, for a new keychain agent require a typed value:

```ts
    if (source === 'keychain' && editing.isNew && !editing.localCredential.trim()) {
      toast.error('Enter the credential to save in the keychain.');
      return;
    }
```

After `await saveMutation.mutateAsync(config)` succeeds and before `cancelEdit()`:

```ts
      if (source === 'keychain') {
        const typed = editing.localCredential.trim();
        try {
          if (typed) await setAgentCredential(config.id, typed);
          else if (editing.removeSavedCredential) await clearAgentCredential(config.id);
        } catch (e) {
          // The agent is saved. Stay in the form so the user can try again.
          setEditing((current) => (current ? { ...current, isNew: false, localCredential: '' } : current));
          toast.error(`The agent was saved, but its credential was not: ${saveErrorMessage(e)}`);
          return;
        }
      }
```

so the value is not in the message (`saveErrorMessage` shows the backend's text, which never holds the value). Confirm `saveErrorMessage` does not append the input. Make sure `localCredential` is cleared on every path that leaves the form.

History notice in `AssistantHistoryView.tsx` line 33: change "Vault values and collections linked from outside the workspace are not scrubbed." to "Vault values, agent credentials and collections linked from outside the workspace are not scrubbed." and update the test that pins the old sentence.

Docs: in `.claude/assistant-conversations.md` find the sentence about vault values not being scrubbed and add "and agent credentials from the env or keychain sources".

- [ ] **Step 4: Run to verify it passes**

Run: `yarn vitest run src/components/settings/__tests__/AgentConfigsDialog.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx`
Expected: PASS.

- [ ] **Step 5: Checks.** `yarn tsc --noEmit` and `yarn check`.

- [ ] **Step 6: Commit** via the commit skill. Paths: the files above that changed. Message: `feat(assistant): save an agent credential in the keychain`.
