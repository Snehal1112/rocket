# AI Agents Dialog Redesign: Plan 3, Form Restructure

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the agent validators into a pure module, add the Gemini reserved names check, build the Advanced section and the Agent form out of the Plan 1 and Plan 2 components, and turn `AgentConfigsDialog.tsx` into a thin shell plus list view.

**Architecture:** `agent-config-validation.ts` is pure (no React). `agent-config-form-state.ts` is pure too and holds the form state type, the mapping from a stored `AgentConfig`, the kind switch rule and the validity rules. `AgentAdvancedSection.tsx` and `AgentConfigForm.tsx` are controlled components (value in, `onChange` out). `AgentConfigsDialog.tsx` owns the edit state, the keychain saved-credential check, the save mapping and the list view. The Credential section is Plan 2's `AgentCredentialSection`, used through its contract only.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, Biome, Yarn, shadcn/ui primitives, lucide-react.

**Spec:** `docs/superpowers/specs/2026-10-14-ai-agents-dialog-redesign-design.md` (index: `docs/superpowers/plans/2026-10-14-ai-agents-dialog-redesign/00-plan-index.md`).

**Depends on:** Plan 1 (`FormField`, `CollapsibleSection`, `KeyValueRows`, `StateBlock`, `DialogBody`) and Plan 2 (`form-types.ts`, `AgentCredentialSection.tsx`, `vaultNameForSave`).

---

## Global Constraints

- shadcn/ui primitives only: no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Icons: `lucide-react` only, no inline SVGs.
- Tokens only, no hard-coded colours. Follow the Rocket DSM (`docs/dsm/index.html`). This plan adds no new documented pattern, so the DSM page is not edited.
- Single-line variable-aware fields use `SingleLineEditor`; multi-line editors use Monaco. (No such field is added here.)
- Zustand: never fully destructure store state at component top level. (This plan reads no store.)
- Persisted data is unchanged: `CredentialSource` stays `vault | env | keychain` and the `vault_*` fields stay. Do not change `AgentConfig` in `src/lib/tauri-api.ts`.
- Commits: conventional commits, written through the `dev-workflow-skills:1-git-commit` skill. Stage the exact files with `git add <paths>` and commit with a pathspec (`git commit --only -- <paths>`). Never `git add -A`, `--all` or `.`. The working tree already holds unrelated staged and modified files, so a pathspec commit is mandatory.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. Never run the whole workspace Rust tests (no Rust is touched here).
- Comments: short full sentences ending in a punctuation mark.
- A repo hook blocks file writes whose content contains the literal Rust panicking call. Do not type it anywhere.
- Contract names (fixed by the index): `validateEnvName`, `validateCredentialEnvVar(name, kind)`, `validateSourceEnvVar`, `validateIdleSeconds(raw)`, `validateModelName(raw)`, `validateExtraEnv(rows, credentialEnvVar, kind)`, `KIND_PRESETS`, `FormField`, `CollapsibleSection`, `KeyValueRows`, `StateBlock`, `DialogBody`, `AgentCredentialSection`, `CredentialFormState`, `CredentialErrors`, `AgentKindValue`, `vaultNameForSave(connection: SecretManagerConnection | undefined, typedScope: string): string` (imported from `./AgentCredentialSection`).

## Review Focus

Each of these is pinned by a test in the task that owns it.

1. **An agent saved with old comma-joined args.** Old data stores `args` as `string[]`, and a hand-edited element may contain a comma. It must load as one row and save back as one element, never re-split. Pinned in Task 2 (`agent-config-form-state.test.ts`, `AgentConfigForm.test.tsx`) and Task 3 (dialog test "keeps an argument that contains a comma as one argument").
2. **An agent with a stored `workingDir`.** The field is not rendered, but the value must survive an edit and a save. Pinned in Task 2 (form test "does not render the working directory and carries it through") and Task 3 (every edit-and-save payload test uses a fixture with `workingDir: '/work'`).
3. **An old Azure agent whose `vault_name` is arbitrary.** Saving must replace it with the connection label through `vaultNameForSave`, and a non-Azure agent must keep what was typed. Pinned in Task 3 ("saves the connection label as the vault name for an Azure agent").
4. **Switching kind after typing a custom command.** A new agent keeps a command the user typed and replaces only values still equal to the previous kind's preset. A stored agent changes nothing but `kind`. Pinned in Task 2 (`withKind` tests) and Task 3 (dialog tests).
5. **A Gemini reserved env name.** `GEMINI_CLI_HOME`, `GEMINI_CLI_TRUST_WORKSPACE` and `GEMINI_CLI_IDE_WORKSPACE_PATH` must be refused case-insensitively as the credential name and as an extra variable, for Gemini only. Pinned in Task 1 (validation tests), Task 2 (form test) and Task 3 (dialog tests).

---

### Task 1: Validation module

**Files:**
- Create: `src/components/settings/agent-config/agent-config-validation.ts`
- Create: `src/components/settings/agent-config/__tests__/agent-config-validation.test.ts`
- Read only (rules being moved): `src/components/settings/AgentConfigsDialog.tsx` lines 46-198; `crates/rocket-acp/src/agent_config.rs` lines 58-97 and 191-235.

**Interfaces:**
- Consumes: `KeyValueRow` from `@/components/ui/key-value-rows` (`{ key: string; name: string; value: string }`); `AgentKindValue` from `./form-types` (`'claude' | 'gemini' | 'generic'`).
- Produces (exact):

```ts
export function isBlankRow(row: KeyValueRow): boolean; // empty name and empty value
export function validateEnvName(name: string): string | null;
export function validateCredentialEnvVar(name: string, kind: AgentKindValue): string | null;
export function validateSourceEnvVar(name: string): string | null;
export function validateIdleSeconds(raw: string): string | null;
export function validateModelName(raw: string): string | null;
export function validateExtraEnv(
  rows: KeyValueRow[],
  credentialEnvVar: string,
  kind: AgentKindValue,
): (string | null)[];
export const KIND_PRESETS: Record<
  AgentKindValue,
  { command: string; args: string[]; credentialEnvVar: string; hint: string }
>;
// Extra exports used by Task 2 and 3:
export const MIN_IDLE_SECONDS: number; // 30
export const MAX_IDLE_SECONDS: number; // 1800
export function reservedEnvVarsFor(kind: AgentKindValue): readonly string[];
```

- [ ] **Step 1: Check the Plan 1 and Plan 2 files exist**

Run:

```bash
ls src/components/ui/key-value-rows.tsx src/components/settings/agent-config/form-types.ts
```

Expected: both paths listed. If either is missing, stop: Plan 1 or 2 has not run.

- [ ] **Step 2: Write the failing tests**

Create `src/components/settings/agent-config/__tests__/agent-config-validation.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { KeyValueRow } from '@/components/ui/key-value-rows';
import {
  KIND_PRESETS,
  MAX_IDLE_SECONDS,
  MIN_IDLE_SECONDS,
  isBlankRow,
  reservedEnvVarsFor,
  validateCredentialEnvVar,
  validateEnvName,
  validateExtraEnv,
  validateIdleSeconds,
  validateModelName,
  validateSourceEnvVar,
} from '../agent-config-validation';

const GEMINI_RESERVED = [
  'GEMINI_CLI_HOME',
  'GEMINI_CLI_TRUST_WORKSPACE',
  'GEMINI_CLI_IDE_WORKSPACE_PATH',
];

function rows(...names: string[]): KeyValueRow[] {
  return names.map((name, i) => ({ key: `k${i}`, name, value: '' }));
}

describe('validateEnvName', () => {
  it('accepts a plain name', () => {
    expect(validateEnvName('ANTHROPIC_API_KEY')).toBeNull();
    expect(validateEnvName('_x1')).toBeNull();
  });

  it('requires a name', () => {
    expect(validateEnvName('')).toBe('Name is required.');
  });

  it('refuses more than 128 characters', () => {
    expect(validateEnvName('A'.repeat(129))).toMatch(/at most 128/);
    expect(validateEnvName('A'.repeat(128))).toBeNull();
  });

  it('refuses a leading digit and bad characters', () => {
    expect(validateEnvName('1BAD')).toMatch(/do not start with a digit/);
    expect(validateEnvName('BAD NAME')).toMatch(/letters, digits and underscore/);
    expect(validateEnvName('A=B')).toMatch(/letters, digits and underscore/);
  });

  it('refuses CLAUDE_CONFIG_DIR in any case', () => {
    expect(validateEnvName('CLAUDE_CONFIG_DIR')).toMatch(/reserved/);
    expect(validateEnvName('claude_config_dir')).toMatch(/reserved/);
  });
});

describe('validateCredentialEnvVar', () => {
  it('does not flag an empty value, the form reports it as required', () => {
    expect(validateCredentialEnvVar('', 'claude')).toBeNull();
    expect(validateCredentialEnvVar('   ', 'claude')).toBeNull();
  });

  it('accepts a normal key name for every kind', () => {
    expect(validateCredentialEnvVar('ANTHROPIC_API_KEY', 'claude')).toBeNull();
    expect(validateCredentialEnvVar('GEMINI_API_KEY', 'gemini')).toBeNull();
    expect(validateCredentialEnvVar('MY_AGENT_TOKEN', 'generic')).toBeNull();
  });

  it.each([
    'sk-ant-oat01-FAKE',
    'ghp_abcdef',
    'xoxb1234',
    'AKIA1234',
    'eyJhbGciOi',
    'a/b',
    'a.b',
    'A'.repeat(65),
  ])('flags the secret-looking value %s without repeating it', (text) => {
    const message = validateCredentialEnvVar(text, 'claude');
    expect(message).toMatch(/looks like a secret value/);
    expect(message).toMatch(/treat it as exposed and revoke it/);
    expect(message).not.toContain(text);
  });

  it('does not tell the user to use a RocketVault secret for every source', () => {
    const message = validateCredentialEnvVar('sk-ant-oat01-FAKE', 'claude') ?? '';
    expect(message).not.toContain('RocketVault');
    expect(message).toMatch(/credential source/);
  });

  it('flags a digit start and the Claude reserved name', () => {
    expect(validateCredentialEnvVar('1KEY', 'claude')).toMatch(/do not start with a digit/);
    expect(validateCredentialEnvVar('claude_config_dir', 'generic')).toMatch(/reserved/);
  });

  it.each(GEMINI_RESERVED)('refuses %s for a Gemini agent in any case', (name) => {
    expect(validateCredentialEnvVar(name, 'gemini')).toMatch(/reserved/);
    expect(validateCredentialEnvVar(name.toLowerCase(), 'gemini')).toMatch(/reserved/);
  });

  it.each(GEMINI_RESERVED)('allows %s for Claude and Generic agents', (name) => {
    expect(validateCredentialEnvVar(name, 'claude')).toBeNull();
    expect(validateCredentialEnvVar(name, 'generic')).toBeNull();
  });
});

describe('validateSourceEnvVar', () => {
  it('does not flag an empty value', () => {
    expect(validateSourceEnvVar('')).toBeNull();
  });

  it('accepts a name', () => {
    expect(validateSourceEnvVar('CLAUDE_CODE_OAUTH_TOKEN')).toBeNull();
  });

  it('flags a secret-looking value without repeating it', () => {
    const message = validateSourceEnvVar('sk-ant-oat01-FAKE');
    expect(message).toMatch(/looks like a secret value/);
    expect(message).not.toContain('sk-ant');
  });

  it('flags a bad name', () => {
    expect(validateSourceEnvVar('1KEY')).toMatch(/do not start with a digit/);
  });
});

describe('validateIdleSeconds', () => {
  it('treats empty as the default', () => {
    expect(validateIdleSeconds('')).toBeNull();
    expect(validateIdleSeconds('  ')).toBeNull();
  });

  it('accepts the bounds', () => {
    expect(validateIdleSeconds(String(MIN_IDLE_SECONDS))).toBeNull();
    expect(validateIdleSeconds(String(MAX_IDLE_SECONDS))).toBeNull();
    expect(validateIdleSeconds('300')).toBeNull();
  });

  it.each(['29', '1801', '5', '12.5', 'abc', '-40'])('refuses %s', (raw) => {
    expect(validateIdleSeconds(raw)).toBe('Use a whole number between 30 and 1800.');
  });
});

describe('validateModelName', () => {
  it('treats empty as no pinned model', () => {
    expect(validateModelName('')).toBeNull();
  });

  it('accepts typical model names', () => {
    expect(validateModelName('gemini-2.5-pro')).toBeNull();
    expect(validateModelName('models/gemini:pro_1')).toBeNull();
  });

  it('refuses spaces, other characters and more than 128 characters', () => {
    expect(validateModelName('gemini 2.5')).toMatch(/letters, digits/);
    expect(validateModelName('a$b')).toMatch(/letters, digits/);
    expect(validateModelName('a'.repeat(129))).toMatch(/at most 128/);
  });
});

describe('validateExtraEnv', () => {
  it('returns one entry per row', () => {
    expect(validateExtraEnv(rows('A', 'B'), 'KEY', 'claude')).toEqual([null, null]);
    expect(validateExtraEnv([], 'KEY', 'claude')).toEqual([]);
  });

  it('flags an empty name when the row has a value', () => {
    const row = { key: 'k0', name: '', value: 'x' };
    expect(validateExtraEnv([row], 'KEY', 'claude')).toEqual(['Name is required.']);
  });

  it('ignores a blank row (empty name and empty value)', () => {
    expect(validateExtraEnv(rows(''), 'KEY', 'claude')).toEqual([null]);
    expect(validateExtraEnv(rows('', 'Foo', ''), 'KEY', 'claude')).toEqual([null, null, null]);
    const spaces = { key: 'k0', name: '  ', value: ' ' };
    expect(validateExtraEnv([spaces], 'KEY', 'claude')).toEqual([null]);
  });

  it('flags a name equal to the credential name, case-insensitively', () => {
    const [error] = validateExtraEnv(rows('anthropic_api_key'), 'ANTHROPIC_API_KEY', 'claude');
    expect(error).toMatch(/differ from the credential/);
  });

  it('flags duplicates case-insensitively on the later row only', () => {
    expect(validateExtraEnv(rows('Foo', 'FOO'), 'KEY', 'claude')).toEqual([
      null,
      'Duplicate variable name.',
    ]);
  });

  it('flags CLAUDE_CONFIG_DIR for every kind', () => {
    for (const kind of ['claude', 'gemini', 'generic'] as const) {
      expect(validateExtraEnv(rows('claude_config_dir'), 'KEY', kind)[0]).toMatch(/reserved/);
    }
  });

  it.each(GEMINI_RESERVED)('flags %s as an extra variable for Gemini only', (name) => {
    expect(validateExtraEnv(rows(name), 'KEY', 'gemini')[0]).toMatch(/reserved/);
    expect(validateExtraEnv(rows(name.toLowerCase()), 'KEY', 'gemini')[0]).toMatch(/reserved/);
    expect(validateExtraEnv(rows(name), 'KEY', 'claude')).toEqual([null]);
    expect(validateExtraEnv(rows(name), 'KEY', 'generic')).toEqual([null]);
  });
});

describe('isBlankRow', () => {
  it('is true only when both the name and the value are empty', () => {
    expect(isBlankRow({ key: 'a', name: '', value: '' })).toBe(true);
    expect(isBlankRow({ key: 'a', name: ' ', value: ' ' })).toBe(true);
    expect(isBlankRow({ key: 'a', name: '', value: 'x' })).toBe(false);
    expect(isBlankRow({ key: 'a', name: 'A', value: '' })).toBe(false);
  });
});

describe('reservedEnvVarsFor', () => {
  it('lists the Gemini names and none for the other kinds', () => {
    expect([...reservedEnvVarsFor('gemini')]).toEqual(GEMINI_RESERVED);
    expect(reservedEnvVarsFor('claude')).toEqual([]);
    expect(reservedEnvVarsFor('generic')).toEqual([]);
  });
});

describe('KIND_PRESETS', () => {
  it('defines command, args, key name and hint for every kind', () => {
    expect(KIND_PRESETS.claude).toMatchObject({
      command: 'claude-agent-acp',
      args: [],
      credentialEnvVar: 'ANTHROPIC_API_KEY',
    });
    expect(KIND_PRESETS.gemini).toMatchObject({
      command: 'gemini',
      args: ['--acp'],
      credentialEnvVar: 'GEMINI_API_KEY',
    });
    expect(KIND_PRESETS.generic).toMatchObject({ command: '', args: [], credentialEnvVar: '' });
    for (const kind of ['claude', 'gemini', 'generic'] as const) {
      expect(KIND_PRESETS[kind].hint.length).toBeGreaterThan(20);
    }
  });

  it('keeps the existing Gemini and Generic notes', () => {
    expect(KIND_PRESETS.gemini.hint).toMatch(/private home with only the Rocket tools/);
    expect(KIND_PRESETS.gemini.hint).toMatch(/Google login is not available/);
    expect(KIND_PRESETS.generic.hint).toMatch(/Rocket sets no isolation for a generic agent/);
  });

  it('only uses valid key names', () => {
    for (const kind of ['claude', 'gemini'] as const) {
      expect(validateCredentialEnvVar(KIND_PRESETS[kind].credentialEnvVar, kind)).toBeNull();
    }
  });
});
```

- [ ] **Step 3: Run the tests, expect FAIL**

```bash
yarn test src/components/settings/agent-config/__tests__/agent-config-validation.test.ts
```

Expected: FAIL with `Failed to resolve import "../agent-config-validation"`.

- [ ] **Step 4: Write the implementation**

Create `src/components/settings/agent-config/agent-config-validation.ts`:

```ts
import type { KeyValueRow } from '@/components/ui/key-value-rows';
import type { AgentKindValue } from './form-types';

const RESERVED_ENV_VAR = 'CLAUDE_CONFIG_DIR';
const ENV_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

// Names Rocket sets itself for a Gemini agent. Mirrors `reserved_env_vars` in rocket-acp.
const GEMINI_RESERVED_ENV_VARS = [
  'GEMINI_CLI_HOME',
  'GEMINI_CLI_TRUST_WORKSPACE',
  'GEMINI_CLI_IDE_WORKSPACE_PATH',
] as const;

export function reservedEnvVarsFor(kind: AgentKindValue): readonly string[] {
  return kind === 'gemini' ? GEMINI_RESERVED_ENV_VARS : [];
}

// Returns the canonical reserved name that matches, ignoring case.
function findReserved(name: string, kind: AgentKindValue): string | null {
  const lower = name.toLowerCase();
  return reservedEnvVarsFor(kind).find((reserved) => reserved.toLowerCase() === lower) ?? null;
}

// Mirrors the backend rules for fast feedback. The backend stays the authority.
export function validateEnvName(name: string): string | null {
  if (name.length === 0) return 'Name is required.';
  if (name.length > 128) return 'Name must be at most 128 characters.';
  if (!ENV_NAME_PATTERN.test(name)) {
    return 'Use letters, digits and underscore only, and do not start with a digit.';
  }
  if (name.toLowerCase() === RESERVED_ENV_VAR.toLowerCase()) {
    return `${RESERVED_ENV_VAR} is reserved.`;
  }
  return null;
}

const SECRET_VALUE_MESSAGE =
  'This looks like a secret value. This field takes only the name of an environment variable, for example ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN. Put the secret itself in the credential source. If this is a real key, treat it as exposed and revoke it.';

// Heuristic only. The message never repeats the entered text.
function looksLikeSecret(text: string): boolean {
  return (
    /[-./]/.test(text) ||
    text.length > 64 ||
    ['sk-', 'ghp_', 'xox', 'AKIA', 'eyJ'].some((prefix) => text.startsWith(prefix))
  );
}

// An empty value is not flagged here. The form reports it as required.
export function validateCredentialEnvVar(name: string, kind: AgentKindValue): string | null {
  const text = name.trim();
  if (text.length === 0) return null;
  if (looksLikeSecret(text)) return SECRET_VALUE_MESSAGE;
  const nameError = validateEnvName(text);
  if (nameError) return nameError;
  const reserved = findReserved(text, kind);
  if (reserved) return `${reserved} is reserved by Rocket for Gemini agents.`;
  return null;
}

const SOURCE_ENV_SECRET_MESSAGE =
  'This looks like a secret value. This field takes only the name of the environment variable that holds it, for example CLAUDE_CODE_OAUTH_TOKEN. If this is a real key, treat it as exposed and revoke it.';

// An empty value is not flagged here. The form reports it as required.
// The message never repeats the entered text.
export function validateSourceEnvVar(name: string): string | null {
  const text = name.trim();
  if (text.length === 0) return null;
  if (looksLikeSecret(text)) return SOURCE_ENV_SECRET_MESSAGE;
  return validateEnvName(text);
}

// A row with an empty name and an empty value. Validation ignores it and save drops it.
export function isBlankRow(row: KeyValueRow): boolean {
  return row.name.trim() === '' && row.value.trim() === '';
}

// One entry per row, in row order.
export function validateExtraEnv(
  rows: KeyValueRow[],
  credentialEnvVar: string,
  kind: AgentKindValue,
): (string | null)[] {
  const credential = credentialEnvVar.trim().toLowerCase();
  return rows.map((row, index) => {
    // A blank row is a row the user just added. It is dropped on save, so it is not an error.
    if (isBlankRow(row)) return null;
    const lower = row.name.toLowerCase();
    const nameError = validateEnvName(row.name);
    if (nameError) return nameError;
    const reserved = findReserved(row.name, kind);
    if (reserved) return `${reserved} is reserved by Rocket for Gemini agents.`;
    if (credential && lower === credential) {
      return 'Must differ from the credential env var.';
    }
    if (rows.findIndex((r) => r.name.toLowerCase() === lower) !== index) {
      return 'Duplicate variable name.';
    }
    return null;
  });
}

export const MIN_IDLE_SECONDS = 30;
export const MAX_IDLE_SECONDS = 1800;

// An empty field means the default. The backend stays the authority.
export function validateIdleSeconds(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value < MIN_IDLE_SECONDS || value > MAX_IDLE_SECONDS) {
    return `Use a whole number between ${MIN_IDLE_SECONDS} and ${MAX_IDLE_SECONDS}.`;
  }
  return null;
}

const MODEL_NAME_PATTERN = /^[A-Za-z0-9._:/-]{1,128}$/;

// An empty field means no pinned model. The backend stays the authority.
export function validateModelName(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  if (trimmed.length > 128) return 'Use at most 128 characters.';
  return MODEL_NAME_PATTERN.test(trimmed)
    ? null
    : 'Use letters, digits and . _ - : / only, at most 128 characters.';
}

// Defaults for a new agent of a kind. They fill only fields the user left unset.
export const KIND_PRESETS: Record<
  AgentKindValue,
  { command: string; args: string[]; credentialEnvVar: string; hint: string }
> = {
  claude: {
    command: 'claude-agent-acp',
    args: [],
    credentialEnvVar: 'ANTHROPIC_API_KEY',
    hint: 'Claude Code through the claude-agent-acp adapter. It needs an Anthropic API key or an OAuth token.',
  },
  gemini: {
    command: 'gemini',
    args: ['--acp'],
    credentialEnvVar: 'GEMINI_API_KEY',
    hint: 'Rocket runs Gemini in a private home with only the Rocket tools. Use a Gemini API key: a Google login is not available inside a Rocket session.',
  },
  generic: {
    command: '',
    args: [],
    credentialEnvVar: '',
    hint: 'Rocket sets no isolation for a generic agent. It keeps its own tools and settings, so configure it to use only the Rocket tools.',
  },
};
```

Note: the model-length branch is new but small. The old pattern already refused over 128 characters with the generic message, so a 129-character name of valid characters now gets the more specific one. The test above expects `/letters, digits/` for `'a$b'` and `/at most 128/` for the long name, and both messages contain "at most 128", so the test for `'a$b'` still matches the pattern message.

- [ ] **Step 5: Run the tests, expect PASS**

```bash
yarn test src/components/settings/agent-config/__tests__/agent-config-validation.test.ts
```

Expected: all tests PASS.

- [ ] **Step 6: Type check and lint**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors. Fix formatting with `yarn format` if Biome reports only format issues in the two new files.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/components/settings/agent-config/agent-config-validation.ts src/components/settings/agent-config/__tests__/agent-config-validation.test.ts
git commit --only -m "feat(settings): add agent config validation module" -- src/components/settings/agent-config/agent-config-validation.ts src/components/settings/agent-config/__tests__/agent-config-validation.test.ts
```

---

### Task 2: Advanced section and agent form

**Files:**
- Create: `src/components/settings/agent-config/agent-config-form-state.ts`
- Create: `src/components/settings/agent-config/AgentAdvancedSection.tsx`
- Create: `src/components/settings/agent-config/AgentConfigForm.tsx`
- Create: `src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts`
- Create: `src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx`
- Read only: `src/components/ui/number-input.tsx`; `src/components/settings/AgentConfigsDialog.tsx` lines 163-189 (reasoning levels, empty form) and 446-760 (form markup being replaced); `src/components/ui/form-field.tsx`, `key-value-rows.tsx`, `collapsible-section.tsx` (Plan 1).

**Interfaces:**
- Consumes: `FormField`, `FieldA11yProps` (`@/components/ui/form-field`); `CollapsibleSection` (`@/components/ui/collapsible-section`); `KeyValueRows`, `KeyValueRow` (`@/components/ui/key-value-rows`); `NumberInput` (`@/components/ui/number-input`); `Input`, `RadioGroup`, `RadioGroupItem`, `Label`; `AgentCredentialSection`, `CredentialFormState`, `CredentialErrors`, `AgentKindValue` (Plan 2); Task 1 exports; `AgentConfig`, `ReasoningLevel` (`@/lib/tauri-api`).
- Produces (exact):

```ts
// agent-config-form-state.ts
export interface AgentFormState extends CredentialFormState {
  id: string;
  isNew: boolean;
  label: string;
  kind: AgentKindValue;
  command: string;
  args: KeyValueRow[];          // one row per argument, only `value` is used
  workingDir: string;           // carried through untouched, never rendered
  idleTimeoutSecs: string;      // '' means the default
  reasoning: ReasoningLevel;
  model: string;
  extraEnv: KeyValueRow[];
}
export type AdvancedFields = Pick<
  AgentFormState,
  'idleTimeoutSecs' | 'reasoning' | 'model' | 'extraEnv'
>;
export interface AgentFormErrors {
  credentialEnvVar: string | null;
  sourceEnvVar: string | null;
  idle: string | null;
  model: string | null;
  extraEnv: (string | null)[];
}
export function newRow(name?: string, value?: string): KeyValueRow;
export function emptyAgentForm(id: string): AgentFormState;
export function formFromConfig(config: AgentConfig): AgentFormState;
export function withKind(form: AgentFormState, kind: AgentKindValue): AgentFormState;
export function validateAgentForm(form: AgentFormState): AgentFormErrors;
export function hasAgentFormErrors(errors: AgentFormErrors): boolean;
export function missingRequiredFields(form: AgentFormState, resolvedVaultName: string): string[];
export function formatMissingRequired(missing: string[]): string; // 'Required: a, b and c.' or ''
export function isAgentFormComplete(form: AgentFormState, resolvedVaultName: string): boolean;
export function isAgentFormValid(form: AgentFormState, resolvedVaultName: string): boolean;

// AgentAdvancedSection.tsx
export function AgentAdvancedSection(props: {
  kind: AgentKindValue;
  value: AdvancedFields;
  errors: Pick<AgentFormErrors, 'idle' | 'model' | 'extraEnv'>;
  onChange: (patch: Partial<AdvancedFields>) => void;
}): JSX.Element;

// AgentConfigForm.tsx
export function AgentConfigForm(props: {
  value: AgentFormState;
  onChange: (next: AgentFormState) => void;
  hasSavedCredential: boolean;
}): JSX.Element;
```

**Spec note:** a new agent starts with the Claude preset (command `claude-agent-acp`, key name `ANTHROPIC_API_KEY`, no arguments). `withKind` replaces a field (command, arguments, key name) only if it is empty or still equals the previous kind's preset, so a value the user typed is kept. A stored agent changes only its `kind`. This rule is in the spec and in the index decisions.

**State API note:** `AgentConfigForm` keeps the `value` and `onChange(next)` props. Because two patches can fire in the same event (the credential section's prefill effect and a user edit, or two fields set together), `patch` must not close over a stale `value`. The form keeps the latest state in a ref that `patch` updates synchronously, and builds `patch` and `patchCredential` with `useCallback` over refs only. The reference passed to `AgentCredentialSection` is therefore stable, as its contract requires.

- [ ] **Step 1: Confirm the accessible names of the Plan 1 and Plan 2 components**

The tests below assume these names. Read the Plan 1 files and align the constants if they differ (the assumptions are the only thing to adjust, never the form's design):

```bash
sed -n 1,200p src/components/ui/key-value-rows.tsx
sed -n 1,120p src/components/ui/form-field.tsx
sed -n 1,80p src/components/ui/collapsible-section.tsx
grep -n "export" src/components/settings/agent-config/AgentCredentialSection.tsx src/components/settings/agent-config/form-types.ts
```

Assumptions used below:
- `KeyValueRows` names each input `<valueLabel> <n>` and `<nameLabel> <n>` (1-based): `Argument 1` in `single` mode, `Variable name 1` and `Variable value 1` in `pair` mode. Its remove buttons are `Remove <nameLabel lowercase> <n>` (`Remove variable name 1`) and in single mode `Remove <valueLabel lowercase> <n>` (`Remove argument 1`). Its add button text is the `addLabel`.
- `FormField` renders the label as a `<label>` tied to the control `id`, the `help` text always, and the `error` text inside an element with `role='alert'` and id `<id>-error`. With `group` it renders `role='group'` named by the label (a span, not a `<label>`), so use `group` for the radio sets and the row lists.
- `CollapsibleSection` renders a button whose accessible name starts with the title, and does not mount its children while closed. `defaultOpen` applies on first mount only. It is controlled when `open` is given (with `onOpenChange`).

If `AgentCredentialSection` or `vaultNameForSave` is not exported, stop: Plan 2 is incomplete.

- [ ] **Step 2: Write the failing form-state tests**

Create `src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { AgentConfig } from '@/lib/tauri-api';
import {
  emptyAgentForm,
  formatMissingRequired,
  formFromConfig,
  hasAgentFormErrors,
  isAgentFormComplete,
  isAgentFormValid,
  missingRequiredFields,
  newRow,
  validateAgentForm,
  withKind,
} from '../agent-config-form-state';

const stored: AgentConfig = {
  id: 'agent-1',
  label: 'Claude',
  command: 'claude-agent-acp',
  args: ['--stdio', '--flag=a,b'],
  workingDir: '/work',
  credentialEnvVar: 'ANTHROPIC_API_KEY',
  extraEnv: [{ name: 'HTTPS_PROXY', value: 'http://p:8080' }],
  vaultConnectionId: 'conn-1',
  vaultName: 'prod-vault',
  vaultSecretId: 'sec-1',
  vaultSecretName: 'anthropic',
};

const argValues = (rows: { value: string }[]) => rows.map((r) => r.value);

describe('emptyAgentForm', () => {
  it('starts as a new Claude agent with the Claude preset', () => {
    const form = emptyAgentForm('new-1');
    expect(form).toMatchObject({
      id: 'new-1',
      isNew: true,
      kind: 'claude',
      command: 'claude-agent-acp',
      credentialEnvVar: 'ANTHROPIC_API_KEY',
      credentialSource: 'vault',
      reasoning: 'high',
      workingDir: '',
    });
    expect(form.args).toEqual([]);
  });
});

describe('formFromConfig', () => {
  it('keeps every argument as its own row, including one that contains a comma', () => {
    const form = formFromConfig(stored);
    expect(argValues(form.args)).toEqual(['--stdio', '--flag=a,b']);
  });

  it('gives every row a distinct stable key', () => {
    const form = formFromConfig(stored);
    const keys = [...form.args, ...form.extraEnv].map((r) => r.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('carries the working directory through', () => {
    expect(formFromConfig(stored).workingDir).toBe('/work');
    expect(formFromConfig({ ...stored, workingDir: undefined }).workingDir).toBe('');
  });

  it('maps tuning, kind and source with their defaults', () => {
    expect(formFromConfig(stored)).toMatchObject({
      isNew: false,
      kind: 'claude',
      reasoning: 'high',
      idleTimeoutSecs: '',
      model: '',
      credentialSource: 'vault',
      sourceEnvVar: '',
      vaultConnectionId: 'conn-1',
      vaultName: 'prod-vault',
      vaultSecretId: 'sec-1',
      vaultSecretName: 'anthropic',
      localCredential: '',
      removeSavedCredential: false,
    });
    expect(
      formFromConfig({
        ...stored,
        kind: 'gemini',
        credentialSource: 'env',
        sourceEnvVar: 'MY_KEY',
        tuning: { idleTimeoutSecs: 600, reasoning: 'low', model: 'gemini-2.5-pro' },
      }),
    ).toMatchObject({
      kind: 'gemini',
      credentialSource: 'env',
      sourceEnvVar: 'MY_KEY',
      idleTimeoutSecs: '600',
      reasoning: 'low',
      model: 'gemini-2.5-pro',
    });
  });

  it('loads an old config without extraEnv as empty', () => {
    const { extraEnv: _omit, ...old } = stored;
    expect(formFromConfig(old as AgentConfig).extraEnv).toEqual([]);
  });

  it('maps extra env pairs to rows', () => {
    const [row] = formFromConfig(stored).extraEnv;
    expect(row).toMatchObject({ name: 'HTTPS_PROXY', value: 'http://p:8080' });
  });
});

describe('withKind', () => {
  it('fills an untouched new agent with the Gemini preset', () => {
    const next = withKind(emptyAgentForm('n'), 'gemini');
    expect(next.kind).toBe('gemini');
    expect(next.command).toBe('gemini');
    expect(argValues(next.args)).toEqual(['--acp']);
    expect(next.credentialEnvVar).toBe('GEMINI_API_KEY');
  });

  it('keeps a command the user typed and still fills the unset arguments', () => {
    const typed = { ...emptyAgentForm('n'), command: '/opt/gemini' };
    const next = withKind(typed, 'gemini');
    expect(next.command).toBe('/opt/gemini');
    expect(argValues(next.args)).toEqual(['--acp']);
  });

  it('keeps arguments and a key name the user typed', () => {
    const typed = {
      ...emptyAgentForm('n'),
      args: [newRow('', '--custom')],
      credentialEnvVar: 'MY_KEY',
    };
    const next = withKind(typed, 'gemini');
    expect(argValues(next.args)).toEqual(['--custom']);
    expect(next.credentialEnvVar).toBe('MY_KEY');
  });

  it('replaces values that still equal the previous preset when switching back', () => {
    const gemini = withKind(emptyAgentForm('n'), 'gemini');
    const claude = withKind(gemini, 'claude');
    expect(claude.command).toBe('claude-agent-acp');
    expect(claude.args).toEqual([]);
    expect(claude.credentialEnvVar).toBe('ANTHROPIC_API_KEY');
    const generic = withKind(gemini, 'generic');
    expect(generic.command).toBe('');
    expect(generic.credentialEnvVar).toBe('');
  });

  it('changes only the kind of a stored agent', () => {
    const form = formFromConfig(stored);
    const next = withKind(form, 'gemini');
    expect(next).toEqual({ ...form, kind: 'gemini' });
  });

  it('returns an equal form when the kind does not change', () => {
    const form = emptyAgentForm('n');
    expect(withKind(form, 'claude')).toEqual(form);
  });
});

describe('validateAgentForm', () => {
  it('has no errors for a fresh form', () => {
    expect(hasAgentFormErrors(validateAgentForm(emptyAgentForm('n')))).toBe(false);
  });

  it('flags a Gemini reserved key name and extra variable', () => {
    const form = {
      ...withKind(emptyAgentForm('n'), 'gemini'),
      credentialEnvVar: 'gemini_cli_home',
      extraEnv: [newRow('GEMINI_CLI_TRUST_WORKSPACE', '1')],
    };
    const errors = validateAgentForm(form);
    expect(errors.credentialEnvVar).toMatch(/reserved/);
    expect(errors.extraEnv[0]).toMatch(/reserved/);
    expect(hasAgentFormErrors(errors)).toBe(true);
  });

  it('checks the source variable only for the env source', () => {
    const form = { ...emptyAgentForm('n'), sourceEnvVar: 'sk-ant-FAKE' };
    expect(validateAgentForm(form).sourceEnvVar).toBeNull();
    expect(validateAgentForm({ ...form, credentialSource: 'env' }).sourceEnvVar).toMatch(
      /looks like a secret value/,
    );
  });

  it('checks the model only for Gemini', () => {
    const form = { ...emptyAgentForm('n'), model: 'a b' };
    expect(validateAgentForm(form).model).toBeNull();
    expect(validateAgentForm({ ...form, kind: 'gemini' }).model).toMatch(/letters, digits/);
  });

  it('does not flag a blank extra variable row but flags a row with only a value', () => {
    const blank = { ...emptyAgentForm('n'), extraEnv: [newRow('', '')] };
    expect(validateAgentForm(blank).extraEnv).toEqual([null]);
    expect(hasAgentFormErrors(validateAgentForm(blank))).toBe(false);
    const valueOnly = { ...emptyAgentForm('n'), extraEnv: [newRow('', 'x')] };
    expect(validateAgentForm(valueOnly).extraEnv).toEqual(['Name is required.']);
  });

  it('checks the idle timeout', () => {
    expect(validateAgentForm({ ...emptyAgentForm('n'), idleTimeoutSecs: '5' }).idle).toMatch(
      /between 30 and 1800/,
    );
  });
});

describe('missingRequiredFields', () => {
  const env = {
    ...emptyAgentForm('n'),
    label: 'Claude',
    credentialSource: 'env' as const,
    sourceEnvVar: 'MY_TOKEN',
  };

  it('lists nothing for a complete form', () => {
    expect(missingRequiredFields(env, '')).toEqual([]);
  });

  it('lists only what is missing, in form order', () => {
    expect(missingRequiredFields({ ...env, label: '', command: '', credentialEnvVar: '' }, '')).toEqual([
      'label',
      'command',
      'key name',
    ]);
    expect(missingRequiredFields({ ...env, command: ' ' }, '')).toEqual(['command']);
  });

  it('lists the missing parts of each credential source', () => {
    expect(missingRequiredFields({ ...env, sourceEnvVar: '' }, '')).toEqual(['source variable']);
    const vault = { ...emptyAgentForm('n'), label: 'Claude' };
    expect(missingRequiredFields(vault, '')).toEqual(['connection', 'secret']);
    expect(missingRequiredFields({ ...vault, vaultConnectionId: 'c1' }, ' ')).toEqual([
      'vault name',
      'secret',
    ]);
    const keychain = { ...env, credentialSource: 'keychain' as const };
    expect(missingRequiredFields(keychain, '')).toEqual(['keychain credential']);
    expect(missingRequiredFields({ ...keychain, isNew: false }, '')).toEqual([]);
  });
});

describe('formatMissingRequired', () => {
  it('joins one, two and three names', () => {
    expect(formatMissingRequired([])).toBe('');
    expect(formatMissingRequired(['label'])).toBe('Required: label.');
    expect(formatMissingRequired(['label', 'command'])).toBe('Required: label and command.');
    expect(formatMissingRequired(['label', 'command', 'key name'])).toBe(
      'Required: label, command and key name.',
    );
  });
});

describe('isAgentFormValid', () => {
  const env = {
    ...emptyAgentForm('n'),
    label: 'Claude',
    credentialSource: 'env' as const,
    sourceEnvVar: 'MY_TOKEN',
  };

  it('is false for a new form with no label', () => {
    expect(isAgentFormComplete(emptyAgentForm('n'), '')).toBe(false);
    expect(isAgentFormValid(emptyAgentForm('n'), '')).toBe(false);
  });

  it('is true for a complete env agent', () => {
    expect(isAgentFormValid(env, '')).toBe(true);
  });

  it('requires label, command and key name', () => {
    expect(isAgentFormComplete({ ...env, label: '  ' }, '')).toBe(false);
    expect(isAgentFormComplete({ ...env, command: '' }, '')).toBe(false);
    expect(isAgentFormComplete({ ...env, credentialEnvVar: '' }, '')).toBe(false);
  });

  it('requires the source variable for the env source', () => {
    expect(isAgentFormComplete({ ...env, sourceEnvVar: '' }, '')).toBe(false);
  });

  it('requires connection, secret and a resolved vault name for the secret manager source', () => {
    const vault = {
      ...emptyAgentForm('n'),
      label: 'Claude',
      vaultConnectionId: 'c1',
      vaultSecretId: 's1',
    };
    expect(isAgentFormComplete(vault, 'prod')).toBe(true);
    expect(isAgentFormComplete(vault, '  ')).toBe(false);
    expect(isAgentFormComplete({ ...vault, vaultConnectionId: '' }, 'prod')).toBe(false);
    expect(isAgentFormComplete({ ...vault, vaultSecretId: '' }, 'prod')).toBe(false);
  });

  it('requires a typed credential only for a new keychain agent', () => {
    const keychain = { ...env, credentialSource: 'keychain' as const };
    expect(isAgentFormComplete(keychain, '')).toBe(false);
    expect(isAgentFormComplete({ ...keychain, localCredential: 'tok' }, '')).toBe(true);
    expect(isAgentFormComplete({ ...keychain, isNew: false }, '')).toBe(true);
  });

  it('is false when any field has an error', () => {
    expect(isAgentFormValid({ ...env, idleTimeoutSecs: '5' }, '')).toBe(false);
    expect(isAgentFormValid({ ...env, credentialEnvVar: 'GEMINI_CLI_HOME', kind: 'gemini' }, '')).toBe(
      false,
    );
  });
});
```

- [ ] **Step 3: Run, expect FAIL**

```bash
yarn test src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts
```

Expected: FAIL with `Failed to resolve import "../agent-config-form-state"`.

- [ ] **Step 4: Write `agent-config-form-state.ts`**

```ts
import type { KeyValueRow } from '@/components/ui/key-value-rows';
import type { AgentConfig, ReasoningLevel } from '@/lib/tauri-api';
import {
  KIND_PRESETS,
  validateCredentialEnvVar,
  validateExtraEnv,
  validateIdleSeconds,
  validateModelName,
  validateSourceEnvVar,
} from './agent-config-validation';
import type { AgentKindValue, CredentialFormState } from './form-types';

export interface AgentFormState extends CredentialFormState {
  id: string;
  isNew: boolean;
  label: string;
  kind: AgentKindValue;
  command: string;
  // One row per argument. Only `value` is used.
  args: KeyValueRow[];
  // Carried through untouched. The form never renders it.
  workingDir: string;
  // An empty string means the default.
  idleTimeoutSecs: string;
  reasoning: ReasoningLevel;
  model: string;
  extraEnv: KeyValueRow[];
}

export type AdvancedFields = Pick<
  AgentFormState,
  'idleTimeoutSecs' | 'reasoning' | 'model' | 'extraEnv'
>;

export interface AgentFormErrors {
  credentialEnvVar: string | null;
  sourceEnvVar: string | null;
  idle: string | null;
  model: string | null;
  extraEnv: (string | null)[];
}

export function newRow(name = '', value = ''): KeyValueRow {
  return { key: crypto.randomUUID(), name, value };
}

function presetArgs(kind: AgentKindValue): KeyValueRow[] {
  return KIND_PRESETS[kind].args.map((arg) => newRow('', arg));
}

export function emptyAgentForm(id: string): AgentFormState {
  const preset = KIND_PRESETS.claude;
  return {
    id,
    isNew: true,
    label: '',
    kind: 'claude',
    command: preset.command,
    args: presetArgs('claude'),
    workingDir: '',
    idleTimeoutSecs: '',
    reasoning: 'high',
    model: '',
    extraEnv: [],
    credentialEnvVar: preset.credentialEnvVar,
    credentialSource: 'vault',
    sourceEnvVar: '',
    localCredential: '',
    removeSavedCredential: false,
    vaultConnectionId: '',
    vaultName: '',
    vaultSecretId: '',
    vaultSecretName: '',
  };
}

export function formFromConfig(config: AgentConfig): AgentFormState {
  return {
    id: config.id,
    isNew: false,
    label: config.label,
    kind: config.kind ?? 'claude',
    command: config.command,
    args: config.args.map((arg) => newRow('', arg)),
    workingDir: config.workingDir ?? '',
    idleTimeoutSecs: config.tuning?.idleTimeoutSecs ? String(config.tuning.idleTimeoutSecs) : '',
    reasoning: config.tuning?.reasoning ?? 'high',
    model: config.tuning?.model ?? '',
    extraEnv: (config.extraEnv ?? []).map((pair) => newRow(pair.name, pair.value)),
    credentialEnvVar: config.credentialEnvVar,
    credentialSource: config.credentialSource ?? 'vault',
    sourceEnvVar: config.sourceEnvVar ?? '',
    localCredential: '',
    removeSavedCredential: false,
    vaultConnectionId: config.vaultConnectionId,
    vaultName: config.vaultName,
    vaultSecretId: config.vaultSecretId,
    vaultSecretName: config.vaultSecretName,
  };
}

// For a new agent, a field is replaced when it is empty or still holds the previous
// kind's preset. A value the user typed is kept. A stored agent changes only its kind.
export function withKind(form: AgentFormState, kind: AgentKindValue): AgentFormState {
  if (!form.isNew || form.kind === kind) return { ...form, kind };
  const previous = KIND_PRESETS[form.kind];
  const next = KIND_PRESETS[kind];
  const isUnset = (current: string, preset: string) =>
    current.trim() === '' || current === preset;
  const argValues = form.args.map((row) => row.value);
  const argsUnset =
    argValues.every((value) => value.trim() === '') ||
    argValues.join('\n') === previous.args.join('\n');
  return {
    ...form,
    kind,
    command: isUnset(form.command, previous.command) ? next.command : form.command,
    args: argsUnset ? presetArgs(kind) : form.args,
    credentialEnvVar: isUnset(form.credentialEnvVar, previous.credentialEnvVar)
      ? next.credentialEnvVar
      : form.credentialEnvVar,
  };
}

export function validateAgentForm(form: AgentFormState): AgentFormErrors {
  return {
    credentialEnvVar: validateCredentialEnvVar(form.credentialEnvVar, form.kind),
    sourceEnvVar:
      form.credentialSource === 'env' ? validateSourceEnvVar(form.sourceEnvVar) : null,
    idle: validateIdleSeconds(form.idleTimeoutSecs),
    model: form.kind === 'gemini' ? validateModelName(form.model) : null,
    extraEnv: validateExtraEnv(form.extraEnv, form.credentialEnvVar, form.kind),
  };
}

export function hasAgentFormErrors(errors: AgentFormErrors): boolean {
  return (
    errors.credentialEnvVar !== null ||
    errors.sourceEnvVar !== null ||
    errors.idle !== null ||
    errors.model !== null ||
    errors.extraEnv.some((error) => error !== null)
  );
}

// Names of the required fields that are still empty, in form order.
// `resolvedVaultName` is what `vaultNameForSave` returns for the chosen connection.
export function missingRequiredFields(form: AgentFormState, resolvedVaultName: string): string[] {
  const missing: string[] = [];
  if (!form.label.trim()) missing.push('label');
  if (!form.command.trim()) missing.push('command');
  if (!form.credentialEnvVar.trim()) missing.push('key name');
  if (form.credentialSource === 'vault') {
    if (form.vaultConnectionId === '') missing.push('connection');
    else if (resolvedVaultName.trim() === '') missing.push('vault name');
    if (form.vaultSecretId === '') missing.push('secret');
  } else if (form.credentialSource === 'env') {
    if (form.sourceEnvVar.trim() === '') missing.push('source variable');
  } else if (form.isNew && form.localCredential.trim() === '') {
    // Keychain: a new agent needs the credential. A stored one may keep what it has.
    missing.push('keychain credential');
  }
  return missing;
}

// 'Required: a, b and c.' for the footer help, or an empty string when nothing is missing.
export function formatMissingRequired(missing: string[]): string {
  if (missing.length === 0) return '';
  const head = missing.slice(0, -1).join(', ');
  const list = head ? `${head} and ${missing[missing.length - 1]}` : missing[0];
  return `Required: ${list}.`;
}

export function isAgentFormComplete(form: AgentFormState, resolvedVaultName: string): boolean {
  return missingRequiredFields(form, resolvedVaultName).length === 0;
}

export function isAgentFormValid(form: AgentFormState, resolvedVaultName: string): boolean {
  return (
    isAgentFormComplete(form, resolvedVaultName) && !hasAgentFormErrors(validateAgentForm(form))
  );
}
```

- [ ] **Step 5: Run the form-state tests, expect PASS**

```bash
yarn test src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts
```

Expected: all PASS.

- [ ] **Step 6: Write the failing form tests**

Create `src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx`. The Credential section is mocked so this test does not depend on Plan 2 internals.

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { AgentConfigForm } from '@/components/settings/agent-config/AgentConfigForm';
import {
  type AgentFormState,
  emptyAgentForm,
  newRow,
} from '@/components/settings/agent-config/agent-config-form-state';
import { KIND_PRESETS } from '@/components/settings/agent-config/agent-config-validation';

const seenOnChange: unknown[] = [];

vi.mock('@/components/settings/agent-config/AgentCredentialSection', () => ({
  AgentCredentialSection: (props: {
    kind: string;
    errors: { credentialEnvVar?: string | null };
    onChange: (patch: Record<string, string>) => void;
  }) => {
    seenOnChange.push(props.onChange);
    return (
      <div data-testid='credential-section' data-kind={props.kind}>
        {props.errors.credentialEnvVar ?? ''}
        <button
          type='button'
          onClick={() => {
            props.onChange({ localCredential: 'tok' });
            props.onChange({ credentialEnvVar: 'MY_KEY' });
          }}
        >
          Patch twice
        </button>
      </div>
    );
  },
}));

function Harness({
  initial,
  onState,
}: {
  initial: AgentFormState;
  onState: (state: AgentFormState) => void;
}) {
  const [value, setValue] = useState(initial);
  const change = (next: AgentFormState) => {
    setValue(next);
    onState(next);
  };
  return <AgentConfigForm value={value} onChange={change} hasSavedCredential={false} />;
}

function setup(over: Partial<AgentFormState> = {}) {
  const states: AgentFormState[] = [];
  const initial = { ...emptyAgentForm('a1'), ...over };
  render(<Harness initial={initial} onState={(state) => states.push(state)} />);
  return { last: () => states[states.length - 1] };
}

const openAdvanced = (user: ReturnType<typeof userEvent.setup>) =>
  user.click(screen.getByRole('button', { name: /^advanced/i }));

describe('AgentConfigForm', () => {
  it('applies two credential patches fired in the same event', async () => {
    const user = userEvent.setup();
    const { last } = setup();
    await user.click(screen.getByRole('button', { name: 'Patch twice' }));
    expect(last()).toMatchObject({ localCredential: 'tok', credentialEnvVar: 'MY_KEY' });
  });

  it('passes the credential section the same onChange reference on every render', async () => {
    const user = userEvent.setup();
    seenOnChange.length = 0;
    setup();
    await user.type(screen.getByLabelText('Label'), 'ab');
    expect(seenOnChange.length).toBeGreaterThan(1);
    expect(new Set(seenOnChange).size).toBe(1);
  });

  it('labels the radio groups and row lists as groups', async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByRole('group', { name: /^agent type/i })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: /^arguments/i })).toBeInTheDocument();
    await openAdvanced(user);
    expect(screen.getByRole('group', { name: /^reasoning/i })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: /^environment variables/i })).toBeInTheDocument();
  });

  it('renders the Agent, Credential and Advanced sections in order', () => {
    setup();
    const agent = screen.getByRole('heading', { name: 'Agent' });
    const credentialHeading = screen.getByRole('heading', { name: 'Credential' });
    const section = screen.getByTestId('credential-section');
    const advanced = screen.getByRole('button', { name: /^advanced/i });
    const follows = (a: Node, b: Node) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(follows(agent, credentialHeading)).toBe(true);
    expect(follows(credentialHeading, section)).toBe(true);
    expect(follows(section, advanced)).toBe(true);
  });

  it('shows the hint of the chosen kind and updates it when the kind changes', async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByText(KIND_PRESETS.claude.hint)).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByText(KIND_PRESETS.gemini.hint)).toBeInTheDocument();
    expect(screen.queryByText(KIND_PRESETS.claude.hint)).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^generic/i }));
    expect(screen.getByText(KIND_PRESETS.generic.hint)).toBeInTheDocument();
    expect(screen.getByTestId('credential-section')).toHaveAttribute('data-kind', 'generic');
  });

  it('explains the label and the command', () => {
    setup();
    expect(screen.getByLabelText('Label')).toBeInTheDocument();
    expect(screen.getByLabelText('Command')).toHaveValue('claude-agent-acp');
    expect(screen.getByText(/The program Rocket starts/)).toBeInTheDocument();
  });

  it('shows one argument per row and never splits on a comma', async () => {
    const user = userEvent.setup();
    const { last } = setup({ args: [newRow('', '--a'), newRow('', '--flag=a,b')] });
    expect(screen.getByLabelText('Argument 1')).toHaveValue('--a');
    expect(screen.getByLabelText('Argument 2')).toHaveValue('--flag=a,b');
    await user.click(screen.getByRole('button', { name: 'Add argument' }));
    await user.type(screen.getByLabelText('Argument 3'), 'x,y');
    expect(last().args.map((row) => row.value)).toEqual(['--a', '--flag=a,b', 'x,y']);
  });

  it('does not render the working directory and carries it through', async () => {
    const user = userEvent.setup();
    const { last } = setup({ workingDir: '/work' });
    expect(screen.queryByLabelText(/working director/i)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Label'), 'Claude');
    expect(last().workingDir).toBe('/work');
    expect(last().label).toBe('Claude');
  });

  it('keeps a typed command when the kind changes on a new agent', async () => {
    const user = userEvent.setup();
    const { last } = setup({ command: '/opt/gemini' });
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByLabelText('Command')).toHaveValue('/opt/gemini');
    expect(last().args.map((row) => row.value)).toEqual(['--acp']);
    expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
  });

  it('shows the idle timeout summary on the collapsed Advanced section', () => {
    setup();
    expect(screen.getByRole('button', { name: /idle timeout 120 s, reasoning high/i })).toBeVisible();
  });

  it('shows the stored idle timeout and reasoning in the summary', () => {
    setup({ idleTimeoutSecs: '600', reasoning: 'low' });
    expect(screen.getByRole('button', { name: /idle timeout 600 s, reasoning low/i })).toBeVisible();
  });

  it('omits the reasoning from the summary for a generic agent', () => {
    setup({ kind: 'generic' });
    const trigger = screen.getByRole('button', { name: /^advanced/i });
    expect(trigger).toHaveTextContent(/idle timeout 120 s/i);
    expect(trigger).not.toHaveTextContent(/reasoning/i);
  });

  it('offers reasoning for Claude, reasoning and a model for Gemini, neither for Generic', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    expect(screen.getByLabelText(/idle timeout/i)).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /^high/i })).toBeChecked();
    expect(screen.queryByLabelText(/gemini model/i)).not.toBeInTheDocument();

    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByRole('radio', { name: /^high/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/gemini model/i)).toBeInTheDocument();

    await user.click(screen.getByRole('radio', { name: /^generic/i }));
    expect(screen.queryByRole('radio', { name: /^high/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/gemini model/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/idle timeout/i)).toBeInTheDocument();
  });

  it('writes the idle timeout, reasoning and model into the state', async () => {
    const user = userEvent.setup();
    const { last } = setup({ kind: 'gemini' });
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/idle timeout/i), '300');
    await user.click(screen.getByRole('radio', { name: /^medium/i }));
    await user.type(screen.getByLabelText(/gemini model/i), 'gemini-2.5-pro');
    expect(last()).toMatchObject({
      idleTimeoutSecs: '300',
      reasoning: 'medium',
      model: 'gemini-2.5-pro',
    });
  });

  it('refuses an idle timeout outside 30 to 1800 with an inline alert', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/idle timeout/i), '5');
    expect(screen.getByRole('alert')).toHaveTextContent(/between 30 and 1800/i);
  });

  it('refuses a Gemini model name with spaces', async () => {
    const user = userEvent.setup();
    setup({ kind: 'gemini' });
    await openAdvanced(user);
    await user.type(screen.getByLabelText(/gemini model/i), 'gemini 2.5');
    expect(screen.getByRole('alert')).toHaveTextContent(/letters, digits/i);
  });

  it('adds extra variables as name and value rows', async () => {
    const user = userEvent.setup();
    const { last } = setup();
    await openAdvanced(user);
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    await user.type(screen.getByLabelText('Variable name 1'), 'HTTPS_PROXY');
    await user.type(screen.getByLabelText('Variable value 1'), 'http://p:8080');
    expect(last().extraEnv).toMatchObject([{ name: 'HTTPS_PROXY', value: 'http://p:8080' }]);
  });

  it('shows the base URL warning only when a variable name ends in _URL', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    expect(screen.queryByText(/can send the credential to another host/i)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    await user.type(screen.getByLabelText('Variable name 1'), 'ANTHROPIC_BASE_URL');
    expect(screen.getByText(/can send the credential to another host/i)).toBeInTheDocument();
  });

  it('opens the Advanced section when it holds an error and names a Gemini reserved variable', () => {
    setup({ kind: 'gemini', extraEnv: [newRow('GEMINI_CLI_HOME', 'x')] });
    expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
    expect(screen.getByRole('button', { name: /^advanced/i })).toHaveTextContent(/needs attention/i);
  });

  it('opens a closed Advanced section when a kind switch creates an error inside it', async () => {
    const user = userEvent.setup();
    setup({ extraEnv: [newRow('GEMINI_CLI_HOME', 'x')] });
    const trigger = screen.getByRole('button', { name: /^advanced/i });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(trigger).not.toHaveTextContent(/needs attention/i);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(trigger).toHaveTextContent(/needs attention/i);
    expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
  });

  it('does not show an error for a blank variable row that was just added', async () => {
    const user = userEvent.setup();
    setup();
    await openAdvanced(user);
    await user.click(screen.getByRole('button', { name: 'Add variable' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Variable value 1'), 'x');
    expect(screen.getByRole('alert')).toHaveTextContent('Name is required.');
  });

  it('removes an argument row with its named remove button', async () => {
    const user = userEvent.setup();
    const { last } = setup({ args: [newRow('', '--a'), newRow('', '--b')] });
    await user.click(screen.getByRole('button', { name: 'Remove argument 1' }));
    expect(last().args.map((row) => row.value)).toEqual(['--b']);
  });

  it('passes a Gemini reserved key name to the credential section as an error', () => {
    setup({ kind: 'gemini', credentialEnvVar: 'GEMINI_CLI_HOME' });
    expect(screen.getByTestId('credential-section')).toHaveTextContent(/reserved/i);
  });
});
```

- [ ] **Step 7: Run, expect FAIL**

```bash
yarn test src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx
```

Expected: FAIL with `Failed to resolve import "@/components/settings/agent-config/AgentConfigForm"`.

- [ ] **Step 8: Write `AgentAdvancedSection.tsx`**

```tsx
import { useEffect, useState } from 'react';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { KeyValueRows } from '@/components/ui/key-value-rows';
import { Label } from '@/components/ui/label';
import { NumberInput } from '@/components/ui/number-input';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import type { ReasoningLevel } from '@/lib/tauri-api';
import type { AdvancedFields, AgentFormErrors } from './agent-config-form-state';
import { MAX_IDLE_SECONDS, MIN_IDLE_SECONDS } from './agent-config-validation';
import type { AgentKindValue } from './form-types';

const DEFAULT_IDLE_SECONDS = 120;

const REASONING_LEVELS: Array<{ value: ReasoningLevel; label: string; hint: string }> = [
  { value: 'low', label: 'Low', hint: 'Fast answers, little deliberation.' },
  { value: 'medium', label: 'Medium', hint: 'Balanced.' },
  { value: 'high', label: 'High', hint: 'Deepest reasoning. Best for guiding and correcting.' },
];

interface AgentAdvancedSectionProps {
  kind: AgentKindValue;
  value: AdvancedFields;
  errors: Pick<AgentFormErrors, 'idle' | 'model' | 'extraEnv'>;
  onChange: (patch: Partial<AdvancedFields>) => void;
}

export function AgentAdvancedSection({ kind, value, errors, onChange }: AgentAdvancedSectionProps) {
  const hasReasoning = kind === 'claude' || kind === 'gemini';
  const hasError =
    errors.idle !== null || errors.model !== null || errors.extraEnv.some((e) => e !== null);
  const idleText = value.idleTimeoutSecs.trim() || String(DEFAULT_IDLE_SECONDS);
  const baseSummary = `Idle timeout ${idleText} s${hasReasoning ? `, reasoning ${value.reasoning}` : ''}`;
  const summary = hasError ? `${baseSummary} (needs attention)` : baseSummary;
  // Controlled so an error inside the section can never stay hidden.
  const [open, setOpen] = useState(hasError);
  useEffect(() => {
    if (hasError) setOpen(true);
  }, [hasError]);
  const hasUrlVariable = value.extraEnv.some((row) => row.name.trim().toUpperCase().endsWith('_URL'));

  return (
    <CollapsibleSection title='Advanced' summary={summary} open={open} onOpenChange={setOpen}>
      <div className='space-y-4'>
        <FormField
          id='ac-idle-timeout'
          label='Idle timeout'
          optionalHint='seconds, optional'
          help={`How long a turn may stay silent before the session ends. Leave it empty for ${DEFAULT_IDLE_SECONDS} seconds, or raise it (up to ${MAX_IDLE_SECONDS}) for deep reasoning on a slow model.`}
          error={errors.idle}
        >
          {(a11y) => (
            <NumberInput
              {...a11y}
              min={MIN_IDLE_SECONDS}
              max={MAX_IDLE_SECONDS}
              value={value.idleTimeoutSecs}
              onChange={(e) => onChange({ idleTimeoutSecs: e.target.value })}
              placeholder={String(DEFAULT_IDLE_SECONDS)}
              className='h-8 text-sm'
              containerClassName='w-full'
            />
          )}
        </FormField>

        {hasReasoning && (
          <FormField
            id='ac-reasoning'
            label='Reasoning'
            group
            help="Claude uses its effort setting, Gemini its thinking budget. You can still change Claude's effort in the chat toolbar."
          >
            {() => (
              <RadioGroup
                aria-label='Reasoning'
                value={value.reasoning}
                onValueChange={(next) => onChange({ reasoning: next as ReasoningLevel })}
                className='gap-2'
              >
                {REASONING_LEVELS.map((level) => (
                  <div key={level.value} className='flex items-center gap-2'>
                    <RadioGroupItem value={level.value} id={`ac-reasoning-${level.value}`} />
                    <Label htmlFor={`ac-reasoning-${level.value}`} className='text-sm font-normal'>
                      {level.label} <span className='text-muted-foreground'>({level.hint})</span>
                    </Label>
                  </div>
                ))}
              </RadioGroup>
            )}
          </FormField>
        )}

        {kind === 'gemini' && (
          <FormField
            id='ac-gemini-model'
            label='Gemini model'
            optionalHint='optional'
            help='Leave empty to let Gemini choose. Example: gemini-2.5-pro. A name Gemini does not know makes the session fail to start.'
            error={errors.model}
          >
            {(a11y) => (
              <Input
                {...a11y}
                value={value.model}
                onChange={(e) => onChange({ model: e.target.value })}
                placeholder='auto'
                className='h-8 text-sm'
                autoComplete='off'
                spellCheck={false}
              />
            )}
          </FormField>
        )}

        <FormField
          id='ac-extra-env'
          label='Environment variables'
          optionalHint='optional'
          group
          help='Extra variables the agent process receives. Values are stored as plain text, so put secrets in the credential source, not here.'
        >
          {() => (
            <div className='space-y-2'>
              <KeyValueRows
                rows={value.extraEnv}
                onChange={(rows) => onChange({ extraEnv: rows })}
                columns='pair'
                nameLabel='Variable name'
                valueLabel='Variable value'
                addLabel='Add variable'
                rowErrors={errors.extraEnv}
              />
              {hasUrlVariable && (
                <p role='note' className='text-xs text-muted-foreground'>
                  A variable such as ANTHROPIC_BASE_URL can send the credential to another host.
                  Only set it to a server you trust.
                </p>
              )}
            </div>
          )}
        </FormField>
      </div>
    </CollapsibleSection>
  );
}
```

- [ ] **Step 9: Write `AgentConfigForm.tsx`**

```tsx
import { useCallback, useEffect, useRef } from 'react';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { KeyValueRows } from '@/components/ui/key-value-rows';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { AgentAdvancedSection } from './AgentAdvancedSection';
import { AgentCredentialSection } from './AgentCredentialSection';
import { type AgentFormState, validateAgentForm, withKind } from './agent-config-form-state';
import { KIND_PRESETS } from './agent-config-validation';
import type { AgentKindValue, CredentialFormState } from './form-types';

const KIND_OPTIONS: Array<{ value: AgentKindValue; label: string }> = [
  { value: 'claude', label: 'Claude Code' },
  { value: 'gemini', label: 'Gemini CLI' },
  { value: 'generic', label: 'Generic ACP agent' },
];

interface AgentConfigFormProps {
  value: AgentFormState;
  onChange: (next: AgentFormState) => void;
  hasSavedCredential: boolean;
}

export function AgentConfigForm({ value, onChange, hasSavedCredential }: AgentConfigFormProps) {
  const errors = validateAgentForm(value);

  // Latest state and callback live in refs, so `patch` is stable and never reads a stale
  // `value`. `patch` also updates the ref at once, so two patches in one event both apply.
  const latest = useRef(value);
  const onChangeRef = useRef(onChange);
  useEffect(() => {
    latest.current = value;
    onChangeRef.current = onChange;
  });

  const commit = useCallback((next: AgentFormState) => {
    latest.current = next;
    onChangeRef.current(next);
  }, []);
  const patch = useCallback(
    (changes: Partial<AgentFormState>) => commit({ ...latest.current, ...changes }),
    [commit],
  );
  const patchCredential = useCallback(
    (changes: Partial<CredentialFormState>) => patch(changes),
    [patch],
  );

  return (
    <div className='space-y-6'>
      <section aria-labelledby='ac-section-agent' className='space-y-4'>
        <h3 id='ac-section-agent' className='text-sm font-semibold'>
          Agent
        </h3>
        <FormField
          id='ac-label'
          label='Label'
          help='The name shown in the agent picker. Example: Claude Code (work).'
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.label}
              onChange={(e) => patch({ label: e.target.value })}
              className='h-8 text-sm'
              autoComplete='off'
            />
          )}
        </FormField>

        <FormField id='ac-kind' label='Agent type' group help={KIND_PRESETS[value.kind].hint}>
          {() => (
            <RadioGroup
              aria-label='Agent type'
              value={value.kind}
              onValueChange={(next) => commit(withKind(latest.current, next as AgentKindValue))}
              className='gap-2'
            >
              {KIND_OPTIONS.map((option) => (
                <div key={option.value} className='flex items-center gap-2'>
                  <RadioGroupItem value={option.value} id={`ac-kind-${option.value}`} />
                  <Label htmlFor={`ac-kind-${option.value}`} className='text-sm font-normal'>
                    {option.label}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          )}
        </FormField>

        <FormField
          id='ac-command'
          label='Command'
          help='The program Rocket starts for this agent, found on the PATH or given as a full path. Example: claude-agent-acp. If it is wrong, the Test button cannot start the agent.'
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.command}
              onChange={(e) => patch({ command: e.target.value })}
              placeholder={KIND_PRESETS[value.kind].command || 'my-agent-acp'}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>

        <FormField
          id='ac-args'
          label='Arguments'
          optionalHint='optional'
          group
          help='One argument per row, passed to the command in order. Example: --acp. A value with a comma stays one argument.'
        >
          {() => (
            <KeyValueRows
              rows={value.args}
              onChange={(rows) => patch({ args: rows })}
              columns='single'
              valueLabel='Argument'
              addLabel='Add argument'
            />
          )}
        </FormField>
      </section>

      <section aria-labelledby='ac-section-credential' className='space-y-4'>
        <h3 id='ac-section-credential' className='text-sm font-semibold'>
          Credential
        </h3>
        <AgentCredentialSection
          kind={value.kind}
          value={value}
          onChange={patchCredential}
          errors={{
            credentialEnvVar: errors.credentialEnvVar,
            sourceEnvVar: errors.sourceEnvVar,
          }}
          hasSavedCredential={hasSavedCredential}
        />
      </section>

      <AgentAdvancedSection
        kind={value.kind}
        value={value}
        errors={{ idle: errors.idle, model: errors.model, extraEnv: errors.extraEnv }}
        onChange={patch}
      />
    </div>
  );
}
```

- [ ] **Step 10: Run the form tests, expect PASS**

```bash
yarn test src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts
```

Expected: all PASS. If an accessible-name assertion fails because Plan 1 named a control differently, adjust the test constants to the real names found in Step 1 (not the component design).

- [ ] **Step 11: Type check and lint**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors. `yarn tsc` also type-checks the still-old `AgentConfigsDialog.tsx`, which this task does not touch.

- [ ] **Step 12: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/components/settings/agent-config/agent-config-form-state.ts src/components/settings/agent-config/AgentAdvancedSection.tsx src/components/settings/agent-config/AgentConfigForm.tsx src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx
git commit --only -m "feat(settings): add agent config form and advanced section" -- src/components/settings/agent-config/agent-config-form-state.ts src/components/settings/agent-config/AgentAdvancedSection.tsx src/components/settings/agent-config/AgentConfigForm.tsx src/components/settings/agent-config/__tests__/agent-config-form-state.test.ts src/components/settings/agent-config/__tests__/AgentConfigForm.test.tsx
```

---

### Task 3: Dialog shell and list view

**Files:**
- Modify (full rewrite): `src/components/settings/AgentConfigsDialog.tsx` (currently lines 1-1034)
- Modify (full rewrite): `src/components/settings/__tests__/AgentConfigsDialog.test.tsx` (currently lines 1-677)
- Read only: `src/components/ui/dialog.tsx` (confirm `DialogBody`, `DialogDescription`, `DialogFooter` are exported, the default `DialogContent` width is `max-w-lg` and `DialogContent` is a CSS grid); `src/components/ui/state-block.tsx`; `src/components/settings/agent-config/AgentCredentialSection.tsx` and its tests `AgentCredentialSection.vault.test.tsx` and `AgentCredentialSection.local.test.tsx` (Plan 2).

**Interfaces:**
- Consumes: Task 2 exports (`AgentConfigForm`, `AgentFormState`, `emptyAgentForm`, `formFromConfig`, `isAgentFormValid`, `missingRequiredFields`, `formatMissingRequired`) and `isBlankRow` from Task 1; `vaultNameForSave(connection: SecretManagerConnection | undefined, typedScope: string): string` from `@/components/settings/agent-config/AgentCredentialSection`; `useAgentConfigs`, `useSaveAgentConfig`, `useDeleteAgentConfig`, `useTestAgentConfig`; `useSecretManagerConnections`; `hasAgentCredential`, `setAgentCredential`, `clearAgentCredential`; `StateBlock`; `Dialog`, `DialogBody`, `DialogContent`, `DialogDescription`, `DialogFooter`, `DialogHeader`, `DialogTitle`.
- Produces: `export function AgentConfigsDialog(props: { open: boolean; onOpenChange: (open: boolean) => void }): JSX.Element` (unchanged signature, the only importer-facing export).

- [ ] **Step 1: Confirm the primitives and align the credential labels**

```bash
grep -n "max-w-lg" src/components/ui/dialog.tsx
grep -n "DialogBody\|DialogDescription\|DialogFooter" src/components/ui/dialog.tsx | head
sed -n 1,80p src/components/ui/state-block.tsx
grep -n "label=\|aria-label\|name:" src/components/settings/agent-config/AgentCredentialSection.tsx | head -40
grep -n "getBy\|findBy" src/components/settings/agent-config/__tests__/AgentCredentialSection.vault.test.tsx src/components/settings/agent-config/__tests__/AgentCredentialSection.local.test.tsx | head -60
grep -rn "AgentConfigsDialog" src --include=*.tsx -l
```

Expected: `max-w-lg` appears in `DialogContent` defaults, so the shell passes no width override. `DialogContent` is a CSS grid, so the shell adds `flex max-h-[85vh] flex-col` (see Step 4), otherwise the body never scrolls and the footer is not pinned. The test file below keeps every Plan 2 label in one `L` constant at its top. Make `L` match what the Plan 2 test queries (the source radio names, the two environment variable field labels, the keychain input and the saved-pill Remove button). The values below are the spec's wording.

- [ ] **Step 2: Rewrite the dialog test file (failing against the old dialog)**

Replace the whole content of `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
import * as api from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  listAgentConfigs: vi.fn(),
  saveAgentConfig: vi.fn(),
  deleteAgentConfig: vi.fn(),
  testAgentConfig: vi.fn(),
  fetchExternalSecretNames: vi.fn(),
  listSecretManagerConnections: vi.fn(),
  setAgentCredential: vi.fn(),
  clearAgentCredential: vi.fn(),
  hasAgentCredential: vi.fn(),
}));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}

// Labels owned by the Plan 2 credential section. Align them with its test.
const L = {
  agentReadsAs: 'Agent reads it as',
  rocketReadsFrom: 'Rocket reads it from',
  keychainCredential: /keychain credential/i,
  // The pill text. It appears once, and the saved-state help text must not contain it.
  savedInKeychain: 'Saved in the keychain',
  savedHelp:
    'A credential is stored in the OS keychain on this computer. Use Replace to change it or Remove to delete it.',
  removeSaved: /^remove$/i,
  sourceSecretManager: /^secret manager$/i,
  sourceEnvironment: /^environment$/i,
  sourceKeychain: /^keychain$/i,
};

const claude: api.AgentConfig = {
  id: 'agent-1',
  label: 'Claude',
  command: 'claude-agent-acp',
  args: ['--stdio', '--verbose'],
  workingDir: '/work',
  credentialEnvVar: 'ANTHROPIC_API_KEY',
  extraEnv: [],
  vaultConnectionId: 'conn-1',
  vaultName: 'prod-vault',
  vaultSecretId: 'sec-1',
  vaultSecretName: 'anthropic',
};

function renderDialog() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AgentConfigsDialog open onOpenChange={vi.fn()} />
    </QueryClientProvider>,
  );
}

type User = ReturnType<typeof userEvent.setup>;

async function openEdit(user: User = userEvent.setup()) {
  renderDialog();
  await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
  await screen.findByLabelText('Label');
  return user;
}

async function openAdd(user: User = userEvent.setup()) {
  renderDialog();
  await user.click(await screen.findByRole('button', { name: /add agent/i }));
  await screen.findByLabelText('Label');
  return user;
}

async function setField(user: User, label: string | RegExp, text: string) {
  const field = await screen.findByLabelText(label);
  await user.clear(field);
  await user.type(field, text);
}

const openAdvanced = (user: User) => user.click(screen.getByRole('button', { name: /^advanced/i }));
const clickSave = (user: User) => user.click(screen.getByRole('button', { name: 'Save' }));
const savedConfig = () => vi.mocked(api.saveAgentConfig).mock.calls[0][0];

async function addVariable(user: User, name: string, value = '') {
  await user.click(screen.getByRole('button', { name: 'Add variable' }));
  const names = screen.getAllByLabelText(/^Variable name \d+$/);
  const values = screen.getAllByLabelText(/^Variable value \d+$/);
  await user.type(names[names.length - 1], name);
  if (value) await user.type(values[values.length - 1], value);
}

describe('AgentConfigsDialog', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.listAgentConfigs).mockResolvedValue([claude]);
    vi.mocked(api.saveAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.deleteAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.testAgentConfig).mockResolvedValue(undefined);
    vi.mocked(api.hasAgentCredential).mockResolvedValue(false);
    vi.mocked(api.fetchExternalSecretNames).mockResolvedValue([
      { secretId: 'sec-1', name: 'anthropic' },
    ]);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
      { id: 'conn-1', label: 'Vault one' } as api.SecretManagerConnection,
    ]);
  });

  describe('list view', () => {
    it('lists the configured agents', async () => {
      renderDialog();
      expect(await screen.findByText('Claude')).toBeInTheDocument();
      expect(screen.getByText('claude-agent-acp')).toBeInTheDocument();
    });

    it('has a title and a description', async () => {
      renderDialog();
      expect(await screen.findByRole('dialog', { name: 'AI Agents' })).toBeInTheDocument();
      expect(screen.getByRole('dialog')).toHaveAccessibleDescription();
    });

    it('lays the dialog out as a column so the body scrolls and the footer is pinned', async () => {
      renderDialog();
      const dialog = await screen.findByRole('dialog', { name: 'AI Agents' });
      expect(dialog).toHaveClass('flex', 'max-h-[85vh]', 'flex-col');
    });

    it('shows an empty state', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      renderDialog();
      expect(await screen.findByText('No agents configured.')).toBeInTheDocument();
    });

    it('shows a retry when the agents cannot be loaded', async () => {
      vi.mocked(api.listAgentConfigs).mockRejectedValueOnce('boom');
      vi.mocked(api.listAgentConfigs).mockResolvedValue([claude]);
      const user = userEvent.setup();
      renderDialog();
      expect(await screen.findByText(/Failed to load agents/)).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Retry' }));
      expect(await screen.findByText('Claude')).toBeInTheDocument();
    });

    it('runs the test command and toasts success', async () => {
      renderDialog();
      await userEvent.click(await screen.findByRole('button', { name: 'Test' }));
      await waitFor(() => expect(api.testAgentConfig).toHaveBeenCalledWith('agent-1'));
      await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Agent test succeeded.'));
    });

    it('toasts the error when the test fails', async () => {
      vi.mocked(api.testAgentConfig).mockRejectedValue('spawn failed');
      renderDialog();
      await userEvent.click(await screen.findByRole('button', { name: 'Test' }));
      await waitFor(() =>
        expect(toast.error).toHaveBeenCalledWith('Agent test failed: spawn failed'),
      );
    });

    it('asks to confirm and then deletes', async () => {
      renderDialog();
      await userEvent.click(await screen.findByRole('button', { name: 'Delete agent' }));
      expect(api.deleteAgentConfig).not.toHaveBeenCalled();
      await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
      await waitFor(() => expect(api.deleteAgentConfig).toHaveBeenCalledWith('agent-1'));
    });

    it('keeps the agent when the removal is cancelled', async () => {
      renderDialog();
      await userEvent.click(await screen.findByRole('button', { name: 'Delete agent' }));
      await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(api.deleteAgentConfig).not.toHaveBeenCalled();
      expect(screen.getByText('Claude')).toBeInTheDocument();
    });
  });

  describe('edit form', () => {
    it('edits a config and saves the trimmed values, keeping the working directory', async () => {
      const user = await openEdit();
      expect(screen.getByLabelText('Label')).toHaveValue('Claude');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--stdio');
      expect(screen.getByLabelText('Argument 2')).toHaveValue('--verbose');
      await setField(user, 'Label', '  Claude Code  ');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        label: 'Claude Code',
      });
      expect(savedConfig().workingDir).toBe('/work');
    });

    it('does not render the working directory', async () => {
      await openEdit();
      expect(screen.queryByLabelText(/working director/i)).not.toBeInTheDocument();
    });

    it('keeps an argument that contains a comma as one argument', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, args: ['--flag=a,b'] }]);
      const user = await openEdit();
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--flag=a,b');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().args).toEqual(['--flag=a,b']);
    });

    it('saves added and removed arguments, one per row, dropping empty rows', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await user.type(screen.getByLabelText('Argument 3'), '--extra');
      await user.click(screen.getByRole('button', { name: 'Add argument' }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().args).toEqual(['--stdio', '--verbose', '--extra']);
    });

    it('disables Save for an empty command without a toast', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText('Command'));
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows no required help while Save is enabled', async () => {
      await openEdit();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
      expect(screen.queryByText(/^Required:/)).not.toBeInTheDocument();
    });

    it('lists only the missing fields under the footer while Save is disabled', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText('Command'));
      expect(screen.getByText('Required: command.')).toBeInTheDocument();
      await user.clear(screen.getByLabelText('Label'));
      expect(screen.getByText('Required: label and command.')).toBeInTheDocument();
      await user.clear(screen.getByLabelText(L.agentReadsAs));
      expect(screen.getByText('Required: label, command and key name.')).toBeInTheDocument();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await user.type(screen.getByLabelText('Command'), 'claude-agent-acp');
      await user.type(screen.getByLabelText(L.agentReadsAs), 'ANTHROPIC_API_KEY');
      expect(screen.queryByText(/^Required:/)).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    });

    it('lists the missing credential parts of a new agent', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      expect(screen.getByText('Required: label, connection and secret.')).toBeInTheDocument();
      await user.type(screen.getByLabelText('Label'), 'New');
      expect(screen.getByText('Required: connection and secret.')).toBeInTheDocument();
    });

    it('disables Save for an empty key name without a toast', async () => {
      const user = await openEdit();
      await user.clear(screen.getByLabelText(L.agentReadsAs));
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('disables Save for a new agent with required fields missing and shows no toast', async () => {
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'New');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(toast.error).not.toHaveBeenCalled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('puts Cancel before Save in the footer and returns to the list on Cancel', async () => {
      const user = await openEdit();
      const buttons = screen.getAllByRole('button').map((b) => b.textContent);
      expect(buttons.indexOf('Cancel')).toBeGreaterThan(-1);
      expect(buttons.indexOf('Cancel')).toBeLessThan(buttons.indexOf('Save'));
      await user.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(await screen.findByText('claude-agent-acp')).toBeInTheDocument();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });
  });

  describe('advanced settings', () => {
    it('saves an idle timeout in seconds under tuning', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Idle timeout/), '300');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ tuning: { idleTimeoutSecs: 300 } });
    });

    it('sends no tuning when everything is left at the default', async () => {
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().tuning).toBeUndefined();
    });

    it('refuses an idle timeout outside 30 to 1800 seconds', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Idle timeout/), '5');
      expect(screen.getByRole('alert')).toHaveTextContent(/between 30 and 1800/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('shows a stored idle timeout', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { idleTimeoutSecs: 600 } },
      ]);
      const user = await openEdit();
      expect(screen.getByRole('button', { name: /idle timeout 600 s/i })).toBeInTheDocument();
      await openAdvanced(user);
      expect(screen.getByLabelText(/Idle timeout/)).toHaveValue(600);
    });

    it('shows High as the reasoning level of an agent that has none stored', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.getByRole('radio', { name: /^high/i })).toBeChecked();
    });

    it('saves a lower reasoning level under tuning and omits the default', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('radio', { name: /^medium/i }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ tuning: { reasoning: 'medium' } });
    });

    it('shows a stored reasoning level', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { reasoning: 'low' } },
      ]);
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.getByRole('radio', { name: /^low/i })).toBeChecked();
    });

    it('hides the reasoning for a generic agent but keeps its stored value on save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, kind: 'generic', tuning: { reasoning: 'low' } },
      ]);
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.queryByRole('radio', { name: /^low/i })).not.toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'generic', tuning: { reasoning: 'low' } });
    });

    it('offers a model field for Gemini agents only and saves it under tuning', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await openAdvanced(user);
      await user.type(screen.getByLabelText(/Gemini model/), 'gemini-2.5-pro');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'gemini', tuning: { model: 'gemini-2.5-pro' } });
    });

    it('hides the model field for a Claude agent and refuses a Gemini model name with spaces', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      expect(screen.queryByLabelText(/Gemini model/)).not.toBeInTheDocument();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      await user.type(screen.getByLabelText(/Gemini model/), 'gemini 2.5');
      expect(screen.getByRole('alert')).toHaveTextContent(/letters, digits/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('drops a stored model for a non-Gemini agent on save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, tuning: { model: 'gemini-2.5-pro' } },
      ]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().tuning).toBeUndefined();
    });
  });

  describe('extra environment variables', () => {
    it('saves added variables with the exact payload', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, 'ANTHROPIC_BASE_URL', 'https://proxy.example');
      await addVariable(user, 'HTTPS_PROXY', 'http://p:8080');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        extraEnv: [
          { name: 'ANTHROPIC_BASE_URL', value: 'https://proxy.example' },
          { name: 'HTTPS_PROXY', value: 'http://p:8080' },
        ],
      });
    });

    it('removes a variable row', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        {
          ...claude,
          extraEnv: [
            { name: 'A', value: '1' },
            { name: 'B', value: '2' },
          ],
        },
      ]);
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Remove variable name 1' }));
      expect(screen.getAllByLabelText(/^Variable name \d+$/)).toHaveLength(1);
      expect(screen.getByLabelText('Variable name 1')).toHaveValue('B');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().extraEnv).toEqual([{ name: 'B', value: '2' }]);
    });

    it.each([
      ['1BAD', /do not start with a digit/],
      ['BAD NAME', /letters, digits and underscore/],
      ['A=B', /letters, digits and underscore/],
      ['CLAUDE_CONFIG_DIR', /reserved/],
      ['claude_config_dir', /reserved/],
      ['anthropic_api_key', /differ from the credential/],
      ['A'.repeat(129), /at most 128/],
    ])('flags %s and blocks Save', async (name, message) => {
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, name);
      expect(screen.getByRole('alert')).toHaveTextContent(message);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows no error for a freshly added blank row and drops it on save', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Add variable' }));
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().extraEnv).toEqual([]);
    });

    it('flags a row that has a value but no name, and duplicates case-insensitively', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await user.click(screen.getByRole('button', { name: 'Add variable' }));
      await user.type(screen.getByLabelText('Variable value 1'), 'x');
      expect(screen.getByRole('alert')).toHaveTextContent('Name is required.');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      await user.type(screen.getByLabelText('Variable name 1'), 'Foo');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await addVariable(user, 'FOO');
      expect(screen.getByRole('alert')).toHaveTextContent('Duplicate variable name.');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('loads an old config without extraEnv as empty', async () => {
      const { extraEnv: _omit, ...old } = claude;
      vi.mocked(api.listAgentConfigs).mockResolvedValue([old as api.AgentConfig]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        extraEnv: [],
      });
    });

    it.each([
      'GEMINI_CLI_HOME',
      'gemini_cli_trust_workspace',
      'GEMINI_CLI_IDE_WORKSPACE_PATH',
    ])('refuses the Gemini reserved variable %s for a Gemini agent', async (name) => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, name);
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('opens the closed Advanced section and disables Save when a kind switch makes a variable reserved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, extraEnv: [{ name: 'GEMINI_CLI_HOME', value: 'x' }] },
      ]);
      const user = await openEdit();
      expect(screen.getByRole('button', { name: /^advanced/i })).toHaveAttribute(
        'aria-expanded',
        'false',
      );
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByRole('button', { name: /^advanced/i })).toHaveAttribute(
        'aria-expanded',
        'true',
      );
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/i);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('allows a Gemini reserved name as an extra variable for a Claude agent', async () => {
      const user = await openEdit();
      await openAdvanced(user);
      await addVariable(user, 'GEMINI_CLI_HOME');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    });
  });

  describe('key name validation', () => {
    const fake = 'sk-test-not-a-real-key-123';

    it('saves a valid name', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.saveAgentConfig).toHaveBeenCalledWith({
        ...claude,
        credentialSource: 'vault',
        credentialEnvVar: 'CLAUDE_CODE_OAUTH_TOKEN',
      });
    });

    it('flags a token-like value without echoing it and blocks Save', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, fake);
      const alert = screen.getByRole('alert');
      expect(alert).toHaveTextContent(/looks like a secret value/);
      expect(alert).toHaveTextContent(/treat it as exposed and revoke it/);
      expect(alert.textContent).not.toContain(fake);
      expect(alert.textContent).not.toContain('RocketVault');
      expect(document.body.textContent).not.toContain(fake);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags a name that starts with a digit', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, '1KEY');
      expect(screen.getByRole('alert')).toHaveTextContent(/do not start with a digit/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    });

    it('flags the Claude reserved name', async () => {
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'claude_config_dir');
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/);
    });

    it('flags a Gemini reserved key name for a Gemini agent only', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
      const user = await openEdit();
      await setField(user, L.agentReadsAs, 'GEMINI_CLI_HOME');
      expect(screen.getByRole('alert')).toHaveTextContent(/reserved/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      await user.click(screen.getByRole('radio', { name: /^claude code/i }));
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('shows the message on open for an invalid stored config and does not save', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, credentialEnvVar: fake }]);
      await openEdit();
      expect(screen.getByRole('alert')).toHaveTextContent(/looks like a secret value/);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('maps the backend rule error to a friendly toast', async () => {
      vi.mocked(api.saveAgentConfig).mockRejectedValue(
        'Invalid input: agent config credential_env_var may contain only letters, digits and underscores',
      );
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() =>
        expect(toast.error).toHaveBeenCalledWith(
          'The credential variable name is not valid. Use only letters, digits and underscores.',
        ),
      );
    });
  });

  describe('credential sources', () => {
    it('lets a user with no connection add an environment variable agent', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'MY_CLAUDE_TOKEN');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
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
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'sk-ant-oat01-FAKE');
      const message = await screen.findByText(/looks like a secret value/i);
      expect(message.textContent).not.toContain('sk-ant');
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('keeps the secret manager fields and sends credentialSource vault for a vault agent', async () => {
      const user = await openEdit();
      expect(screen.getByRole('radio', { name: L.sourceSecretManager })).toBeChecked();
      await clickSave(user);
      await waitFor(() =>
        expect(savedConfig()).toMatchObject({
          credentialSource: 'vault',
          vaultConnectionId: 'conn-1',
          vaultName: 'prod-vault',
          vaultSecretId: 'sec-1',
          vaultSecretName: 'anthropic',
        }),
      );
    });

    it('saves the connection label as the vault name for an Azure agent with an arbitrary stored name', async () => {
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
        { id: 'conn-1', label: 'Prod KV', provider: 'azure' } as api.SecretManagerConnection,
      ]);
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, vaultName: 'whatever-old' }]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
        credentialSource: 'vault',
        vaultConnectionId: 'conn-1',
        vaultName: 'Prod KV',
      });
    });

    it('keeps the typed vault name for a RocketVault connection', async () => {
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([
        { id: 'conn-1', label: 'Vault one', provider: 'rocketvault' } as api.SecretManagerConnection,
      ]);
      const user = await openEdit();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().vaultName).toBe('prod-vault');
    });

    const FAKE = 'sk-ant-oat01-FAKE-VALUE-FOR-TESTS';

    async function openKeychainForm(user: User) {
      await user.type(screen.getByLabelText('Label'), 'Claude');
      await setField(user, L.agentReadsAs, 'CLAUDE_CODE_OAUTH_TOKEN');
      await user.click(screen.getByRole('radio', { name: L.sourceKeychain }));
    }

    it('saves the config first and then the typed keychain credential', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.setAgentCredential).mockResolvedValue(undefined);
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      expect(screen.getByLabelText(L.keychainCredential)).toHaveAttribute('type', 'password');
      await clickSave(user);
      await waitFor(() => expect(api.setAgentCredential).toHaveBeenCalledTimes(1));
      expect(savedConfig().credentialSource).toBe('keychain');
      expect(vi.mocked(api.setAgentCredential).mock.calls[0]).toEqual([savedConfig().id, FAKE]);
      expect(vi.mocked(api.saveAgentConfig).mock.invocationCallOrder[0]).toBeLessThan(
        vi.mocked(api.setAgentCredential).mock.invocationCallOrder[0],
      );
      expect(JSON.stringify(vi.mocked(api.saveAgentConfig).mock.calls)).not.toContain(FAKE);
    });

    it('keeps the dialog open and hides the value when the credential cannot be saved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.setAgentCredential).mockRejectedValue(
        new Error('the keychain could not be read'),
      );
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      await clickSave(user);
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(JSON.stringify(vi.mocked(toast.error).mock.calls)).not.toContain(FAKE);
      expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
      expect(screen.getByLabelText(L.keychainCredential)).toHaveValue('');
    });

    it('blocks a new keychain agent with an empty credential', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await openKeychainForm(user);
      expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
      expect(api.saveAgentConfig).not.toHaveBeenCalled();
    });

    it('shows a saved credential as saved and keeps it on a blank field', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        {
          ...claude,
          credentialSource: 'keychain',
          vaultConnectionId: '',
          vaultName: '',
          vaultSecretId: '',
          vaultSecretName: '',
        },
      ]);
      vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
      const user = await openEdit();
      expect(await screen.findByText(L.savedInKeychain)).toBeInTheDocument();
      expect(screen.getByText(L.savedHelp)).toBeInTheDocument();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.setAgentCredential).not.toHaveBeenCalled();
      expect(api.clearAgentCredential).not.toHaveBeenCalled();
    });

    it('removes the saved credential when Remove is used and the agent is saved', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([
        { ...claude, credentialSource: 'keychain' },
      ]);
      vi.mocked(api.hasAgentCredential).mockResolvedValue(true);
      vi.mocked(api.clearAgentCredential).mockResolvedValue(undefined);
      const user = await openEdit();
      await user.click(await screen.findByRole('button', { name: L.removeSaved }));
      await clickSave(user);
      await waitFor(() => expect(api.clearAgentCredential).toHaveBeenCalledWith('agent-1'));
    });

    it('does not save a typed credential after the source is switched away from the keychain', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await openKeychainForm(user);
      await user.type(screen.getByLabelText(L.keychainCredential), FAKE);
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'MY_CLAUDE_TOKEN');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(api.setAgentCredential).not.toHaveBeenCalled();
    });
  });

  describe('agent kind', () => {
    it('shows a stored Claude agent as Claude Code and saves no kind override', async () => {
      const user = await openEdit();
      expect(screen.getByRole('radio', { name: /^claude code/i })).toBeChecked();
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig().kind).toBeUndefined();
    });

    it('saves the chosen agent type', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('radio', { name: /^generic/i }));
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({ kind: 'generic' });
    });

    it('opens a stored generic agent as generic', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'generic' }]);
      await openEdit();
      expect(screen.getByRole('radio', { name: /^generic/i })).toBeChecked();
    });

    it('prefills a new Gemini agent with the Gemini command, argument and key name', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('gemini');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
      expect(screen.getByLabelText(L.agentReadsAs)).toHaveValue('GEMINI_API_KEY');
    });

    it('keeps a command the user already typed when Gemini is chosen', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      const user = await openAdd();
      await setField(user, 'Command', '/opt/gemini');
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('/opt/gemini');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--acp');
    });

    it('does not change the fields of a stored agent when its type is switched', async () => {
      const user = await openEdit();
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      expect(screen.getByLabelText('Command')).toHaveValue('claude-agent-acp');
      expect(screen.getByLabelText(L.agentReadsAs)).toHaveValue('ANTHROPIC_API_KEY');
      expect(screen.getByLabelText('Argument 1')).toHaveValue('--stdio');
    });

    it('saves a Gemini agent with kind gemini', async () => {
      vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
      vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
      const user = await openAdd();
      await user.type(screen.getByLabelText('Label'), 'Gemini');
      await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
      await user.click(screen.getByRole('radio', { name: L.sourceEnvironment }));
      await setField(user, L.rocketReadsFrom, 'GEMINI_API_KEY');
      await clickSave(user);
      await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
      expect(savedConfig()).toMatchObject({
        kind: 'gemini',
        command: 'gemini',
        args: ['--acp'],
        credentialEnvVar: 'GEMINI_API_KEY',
        credentialSource: 'env',
        sourceEnvVar: 'GEMINI_API_KEY',
      });
    });
  });
});
```

- [ ] **Step 3: Run, expect FAIL**

```bash
yarn test src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

Expected: FAIL. The old dialog has no `Argument 1`, no `Agent reads it as` and no Advanced section, so most of the edit-form tests fail while the list-view tests pass.

- [ ] **Step 4: Rewrite the dialog**

Replace the whole content of `src/components/settings/AgentConfigsDialog.tsx`:

```tsx
import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { AgentConfigForm } from '@/components/settings/agent-config/AgentConfigForm';
import { vaultNameForSave } from '@/components/settings/agent-config/AgentCredentialSection';
import {
  type AgentFormState,
  emptyAgentForm,
  formatMissingRequired,
  formFromConfig,
  isAgentFormValid,
  missingRequiredFields,
} from '@/components/settings/agent-config/agent-config-form-state';
import { isBlankRow } from '@/components/settings/agent-config/agent-config-validation';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { StateBlock } from '@/components/ui/state-block';
import {
  useAgentConfigs,
  useDeleteAgentConfig,
  useSaveAgentConfig,
  useTestAgentConfig,
} from '@/lib/queries/agent-config-queries';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import {
  type AgentConfig,
  clearAgentCredential,
  hasAgentCredential,
  setAgentCredential,
} from '@/lib/tauri-api';

interface AgentConfigsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// Maps the known backend rule violation to a friendlier text without echoing input.
function saveErrorMessage(e: unknown): string {
  const text = String(e);
  if (text.includes('credential_env_var may contain only')) {
    return 'The credential variable name is not valid. Use only letters, digits and underscores.';
  }
  return `Could not save agent: ${text}`;
}

// Maps the form to the persisted shape. The shape itself is unchanged.
function toAgentConfig(form: AgentFormState, vaultName: string): AgentConfig {
  const source = form.credentialSource;
  const idleText = form.idleTimeoutSecs.trim();
  const idleTimeoutSecs = idleText === '' ? undefined : Number(idleText);
  const reasoning = form.reasoning === 'high' ? undefined : form.reasoning;
  const model =
    form.kind === 'gemini' && form.model.trim() !== '' ? form.model.trim() : undefined;
  const tuning =
    idleTimeoutSecs === undefined && reasoning === undefined && model === undefined
      ? undefined
      : { idleTimeoutSecs, reasoning, model };
  return {
    id: form.id,
    label: form.label.trim(),
    kind: form.kind === 'claude' ? undefined : form.kind,
    tuning,
    command: form.command.trim(),
    args: form.args.map((row) => row.value.trim()).filter((arg) => arg.length > 0),
    workingDir: form.workingDir.trim() || undefined,
    credentialEnvVar: form.credentialEnvVar.trim(),
    // Blank rows (empty name and empty value) are dropped.
    extraEnv: form.extraEnv.filter((row) => !isBlankRow(row)).map(({ name, value }) => ({ name, value })),
    credentialSource: source,
    sourceEnvVar: source === 'env' ? form.sourceEnvVar.trim() : undefined,
    vaultConnectionId: source === 'vault' ? form.vaultConnectionId : '',
    vaultName: source === 'vault' ? vaultName.trim() : '',
    vaultSecretId: source === 'vault' ? form.vaultSecretId : '',
    vaultSecretName: source === 'vault' ? form.vaultSecretName : '',
  };
}

export function AgentConfigsDialog({ open, onOpenChange }: AgentConfigsDialogProps) {
  const { data: configs = [], isLoading, isError, error, refetch } = useAgentConfigs();
  const { data: connections = [] } = useSecretManagerConnections();
  const saveMutation = useSaveAgentConfig();
  const deleteMutation = useDeleteAgentConfig();
  const testMutation = useTestAgentConfig();

  const [editing, setEditing] = useState<AgentFormState | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [hasSavedCredential, setHasSavedCredential] = useState(false);
  // Id of the agent whose saved-credential check is current. A late answer for another id is dropped.
  const credentialCheckId = useRef<string | null>(null);

  useEffect(() => {
    if (!open) {
      setEditing(null);
      setDeletingId(null);
      credentialCheckId.current = null;
      setHasSavedCredential(false);
    }
  }, [open]);

  const startAdd = () => {
    credentialCheckId.current = null;
    setHasSavedCredential(false);
    setEditing(emptyAgentForm(crypto.randomUUID()));
  };

  const startEdit = (c: AgentConfig) => {
    credentialCheckId.current = c.id;
    setHasSavedCredential(false);
    if (c.credentialSource === 'keychain') {
      hasAgentCredential(c.id).then(
        (saved) => {
          if (credentialCheckId.current === c.id) setHasSavedCredential(saved);
        },
        () => {
          if (credentialCheckId.current === c.id) setHasSavedCredential(false);
        },
      );
    }
    setEditing(formFromConfig(c));
  };

  const cancelEdit = () => {
    credentialCheckId.current = null;
    setHasSavedCredential(false);
    setEditing(null);
  };

  const resolveVaultName = (form: AgentFormState) =>
    vaultNameForSave(
      connections.find((c) => c.id === form.vaultConnectionId),
      form.vaultName,
    );

  const canSave = editing !== null && isAgentFormValid(editing, resolveVaultName(editing));
  // Shown only while Save is disabled, and lists only what is actually missing.
  const requiredHelp = editing
    ? formatMissingRequired(missingRequiredFields(editing, resolveVaultName(editing)))
    : '';

  const handleSave = async () => {
    if (!editing || !canSave) return;
    const source = editing.credentialSource;
    const config = toAgentConfig(editing, resolveVaultName(editing));
    try {
      await saveMutation.mutateAsync(config);
      if (source === 'keychain') {
        const typed = editing.localCredential.trim();
        try {
          if (typed) await setAgentCredential(config.id, typed);
          else if (editing.removeSavedCredential) await clearAgentCredential(config.id);
        } catch (e) {
          // The agent is saved. Stay in the form so the user can try again.
          setEditing((current) =>
            current ? { ...current, isNew: false, localCredential: '' } : current,
          );
          toast.error(`The agent was saved, but its credential was not: ${String(e)}`);
          return;
        }
      }
      cancelEdit();
    } catch (e) {
      toast.error(saveErrorMessage(e));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id);
    } catch (e) {
      toast.error(`Could not delete agent: ${String(e)}`);
    } finally {
      setDeletingId(null);
    }
  };

  const handleTest = async (id: string) => {
    try {
      await testMutation.mutateAsync(id);
      toast.success('Agent test succeeded.');
    } catch (e) {
      toast.error(`Agent test failed: ${String(e)}`);
    }
  };

  const renderList = () => {
    if (isLoading) return <StateBlock tone='loading'>Loading agents…</StateBlock>;
    if (isError) {
      return (
        <StateBlock tone='error' action={{ label: 'Retry', onClick: () => void refetch() }}>
          {`Failed to load agents. ${String(error)}`}
        </StateBlock>
      );
    }
    return (
      <div className='space-y-3'>
        {configs.length === 0 && <StateBlock tone='empty'>No agents configured.</StateBlock>}
        {configs.map((c) => {
          if (c.id === deletingId) {
            return (
              <div
                key={c.id}
                className='flex items-center gap-2 rounded-md bg-destructive/10 px-2 py-1.5 text-sm'
              >
                <span className='flex-1'>
                  Remove <span className='font-semibold'>{c.label}</span>?
                </span>
                <Button
                  size='sm'
                  variant='destructive'
                  className='h-7 text-xs'
                  disabled={deleteMutation.isPending}
                  onClick={() => void handleDelete(c.id)}
                >
                  Remove
                </Button>
                <Button
                  size='sm'
                  variant='ghost'
                  className='h-7 text-xs'
                  onClick={() => setDeletingId(null)}
                >
                  Cancel
                </Button>
              </div>
            );
          }
          return (
            <div key={c.id} className='flex items-center justify-between gap-2 text-sm'>
              <div className='min-w-0'>
                <div className='font-medium'>{c.label}</div>
                <div className='truncate text-xs text-muted-foreground'>{c.command}</div>
              </div>
              <div className='flex items-center gap-1'>
                <Button
                  variant='outline'
                  size='sm'
                  onClick={() => void handleTest(c.id)}
                  disabled={testMutation.isPending}
                >
                  Test
                </Button>
                <Button
                  variant='ghost'
                  size='icon'
                  aria-label='Edit agent'
                  onClick={() => startEdit(c)}
                >
                  <Pencil className='h-3.5 w-3.5' aria-hidden='true' />
                </Button>
                <Button
                  variant='ghost'
                  size='icon'
                  aria-label='Delete agent'
                  onClick={() => setDeletingId(c.id)}
                >
                  <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
                </Button>
              </div>
            </div>
          );
        })}
        <Button size='sm' onClick={startAdd}>
          <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
          Add Agent
        </Button>
      </div>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='flex max-h-[85vh] flex-col'>
        <DialogHeader>
          <DialogTitle>AI Agents</DialogTitle>
          <DialogDescription>
            {editing
              ? 'Set how Rocket starts this agent and where its credential comes from.'
              : 'The agents Rocket can start for the workspace assistant.'}
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          {editing ? (
            <AgentConfigForm
              value={editing}
              onChange={setEditing}
              hasSavedCredential={hasSavedCredential}
            />
          ) : (
            renderList()
          )}
        </DialogBody>
        {editing && (
          <DialogFooter>
            {requiredHelp && (
              <p className='mr-auto text-xs text-muted-foreground'>{requiredHelp}</p>
            )}
            <Button variant='outline' size='sm' onClick={cancelEdit}>
              Cancel
            </Button>
            <Button
              size='sm'
              onClick={() => void handleSave()}
              disabled={saveMutation.isPending || !canSave}
              aria-busy={saveMutation.isPending}
            >
              {saveMutation.isPending && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
              Save
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 5: Run the dialog tests, expect PASS**

```bash
yarn test src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

Expected: all PASS. If a Plan 2 label (the `L` constant) or a Plan 1 accessible name differs, fix the constant or the query to the real name. Do not weaken an assertion about payloads, disabled Save or the absence of a toast.

- [ ] **Step 6: Run the full affected test set**

```bash
yarn test src/components/settings src/components/ui src/lib/queries
```

Expected: PASS. If some other file imports a symbol that used to live in the old dialog, `yarn tsc --noEmit` in the next step shows it. Only `AgentConfigsDialog` is exported from the dialog file, so none is expected.

- [ ] **Step 7: Type check and lint**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors. Run `yarn lint` first if Biome reports only auto-fixable formatting and re-run `yarn check`.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add src/components/settings/AgentConfigsDialog.tsx src/components/settings/__tests__/AgentConfigsDialog.test.tsx
git commit --only -m "refactor(settings): rebuild the AI Agents dialog as a shell around the new form" -- src/components/settings/AgentConfigsDialog.tsx src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

- [ ] **Step 9: Manual check in the real app**

Run `yarn tauri dev`, open Settings, then AI Agents. Check each item and note any difference from the spec:

1. Layout: the dialog is `max-w-lg`, the header has a title and a description, the body scrolls when long while the header and footer stay pinned, Cancel then Save sit right aligned in the footer, and no toast appears for missing fields (Save is just disabled). While Save is disabled for missing fields, a muted line on the footer's left lists them (for example "Required: label, command and key name.").
2. Azure connection: add an agent, choose Secret manager, pick the Azure connection. The scope field is hidden, secrets load by themselves, the refresh icon reloads them. Save, reopen, and confirm in `~/.rocket-api/` that `vault_name` equals the connection label. Edit an older Azure agent with a different stored `vault_name` and confirm it is replaced by the label on save.
3. RocketVault connection: choose it, enter a vault name, confirm secrets auto-load, pick one, save, reopen, and confirm the typed vault name is kept.
4. Disabled providers: AWS, HashiCorp and GCP connections show as "not available yet" and cannot be picked.
5. Each kind: Claude Code (reasoning shown, no model), Gemini CLI (reasoning and model shown, `gemini` and `--acp` prefilled on a new agent), Generic ACP agent (no reasoning, no model). Switch kind on a new agent after typing a custom command and confirm the command stays. Switch kind on a stored agent and confirm nothing else changes.
6. Each source: Environment (both names shown, the mapping sentence updates, the shell-export note shows only here), Keychain (type a credential, save, reopen and see the saved state, Remove then Save clears it), Secret manager.
7. Arguments: an agent with several arguments shows one per row. Add, edit and remove rows. An old agent keeps its arguments exactly after a save.
8. Working directory: not shown. Edit an agent that has one stored, save, and confirm the value is still in the saved file.
9. Advanced: collapsed with the summary "Idle timeout N s, reasoning X" (no reasoning for Generic). Open it, set an idle timeout of 5 and see the inline error and a disabled Save. Add `ANTHROPIC_BASE_URL` and see the redirect warning appear. Click "Add variable" and confirm no error shows on the empty row, and that saving drops it. For a Gemini agent add `GEMINI_CLI_HOME` as a variable and as the key name and see the reserved error for each. With a Claude agent that has `GEMINI_CLI_HOME` as a variable and Advanced closed, switch to Gemini CLI and confirm Advanced opens, its summary says "(needs attention)" and Save is disabled.
10. List view: Test, Edit and Delete (inline Remove confirmation) still work, and a failed load shows the error with Retry.
