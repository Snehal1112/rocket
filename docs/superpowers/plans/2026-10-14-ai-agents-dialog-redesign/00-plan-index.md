# AI Agents dialog redesign: plan index

**Spec:** `docs/superpowers/specs/2026-10-14-ai-agents-dialog-redesign-design.md` (read it first, plus the Rocket DSM at `docs/dsm/index.html`).

**Mockup (approved):** https://claude.ai/code/artifact/46f71c02-57ed-4e94-a8f7-39782c8333a2 (private to the user; the layout is also described in the spec).

## Plans

Run one plan at a time, in this order. Each plan has at most three tasks.

| # | File | Delivers | Depends on |
|---|---|---|---|
| 1 | `plan-01-dsm-components.md` | Seven new `ui/` components, their DSM entries and tests | none |
| 2 | `plan-02-credential-section.md` | Form types, secret listing hook, credential section | 1 |
| 3 | `plan-03-form-restructure.md` | Validation module, advanced section, form, dialog shell | 1, 2 |
| 4 | `plan-04-rust-wording.md` | Provider-neutral Rust error text and test updates | none |

Plans 1 and 4 are independent and can run in either order. Run 2 after 1, and 3 after 2.

## Rules for every plan

- Hard rules from `CLAUDE.md`: shadcn primitives only (no raw `button`, `input`, `select`, `dialog`, `form`), `lucide-react` icons only, narrow Zustand selectors, and in production Rust no panicking unwrap calls (map errors explicitly).
- Follow the Rocket DSM. Tokens only, no hard-coded colours. Every new component gets a DSM entry in the plan that builds it.
- Commits: conventional commit format, through the `dev-workflow-skills:1-git-commit` skill, with a pathspec (`git add <paths>` and `git commit --only -- <paths>`). Never `git add -A`, `--all` or `.`.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. Rust: `cargo check -j4` and `cargo test -j4 -p <crate> <name>`. Never `cargo test --workspace`.
- No OpenCollection pre-read is needed: no plan touches collection, environment or request data. Plan 4 changes error text in `rocket-app` only.
- Persisted data is unchanged: `CredentialSource` stays `vault | env | keychain` and the `vault_*` fields stay.
- A repo hook blocks writes whose content contains the literal Rust panic call named in the first rule. Do not type that literal in any plan or doc.

## Interface contract

All names below are fixed. Later plans rely on them, so do not rename.

### Plan 1 produces (all in `src/components/ui/`, each with a test in `src/components/ui/__tests__/`)

```ts
// form-field.tsx
export interface FieldA11yProps {
  id: string;
  'aria-invalid': boolean;
  'aria-describedby': string | undefined;
}
export function FormField(props: {
  id: string;
  label: ReactNode;
  optionalHint?: string;          // e.g. "optional", "seconds, optional"
  group?: boolean;                // true: label names a group (radio set, row list, saved secret):
                                  // renders role="group" + aria-labelledby, label is a span not a <label>
  help: ReactNode;                // required: every field explains itself
  error?: string | null;
  children: (a11y: FieldA11yProps) => ReactNode;
}): JSX.Element;

// segmented-control.tsx (built on the RadioGroup primitive, arrow keys work)
export function SegmentedControl<T extends string>(props: {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: string }[];
  'aria-label': string;
}): JSX.Element;

// collapsible-section.tsx (built on ui/collapsible.tsx)
export function CollapsibleSection(props: {
  title: string;
  summary?: string;               // shown beside the title
  defaultOpen?: boolean;          // default false (uncontrolled)
  open?: boolean;                 // when given, the section is controlled
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
}): JSX.Element;

// state-block.tsx
export function StateBlock(props: {
  tone: 'loading' | 'empty' | 'error';
  children: ReactNode;
  action?: { label: string; onClick: () => void };   // e.g. Retry
}): JSX.Element;

// secret-field.tsx
export function SecretField(props: {
  id: string;
  value: string;
  onValueChange: (value: string) => void;
  hasSaved: boolean;              // a credential is already stored
  markedForRemoval: boolean;
  onToggleRemove: () => void;
  placeholder?: string;
  'aria-invalid'?: boolean;
  'aria-describedby'?: string;
}): JSX.Element;

// key-value-rows.tsx
export interface KeyValueRow { key: string; name: string; value: string }  // key = stable React key
export function KeyValueRows(props: {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  columns: 'pair' | 'single';     // 'single' edits only `value` (used for arguments)
  nameLabel?: string;             // pair mode
  valueLabel: string;
  addLabel: string;               // e.g. "Add argument"
  rowErrors?: (string | null)[];  // by index
}): JSX.Element;

// saved-pill.tsx: optional `persistent?: boolean` (no 3 s auto hide) and `children?: ReactNode`
// (default text 'All changes saved'). Backward compatible, no other importers today.

// dialog.tsx additions
export function DialogBody(props: HTMLAttributes<HTMLDivElement>): JSX.Element; // scrolling region
// DialogContent keeps the DSM width max-w-lg. DialogFooter stays right aligned.
```

DSM page sections added by plan 1 (ids in `docs/dsm/index.html`): `form-field`, `segmented-control`, `collapsible-section`, `state-block`, `secret-field`, `key-value-rows`, `dialog-body`.

### Plan 2 produces (in `src/components/settings/agent-config/`)

```ts
// form-types.ts
export type AgentKindValue = 'claude' | 'gemini' | 'generic';
export interface CredentialFormState {
  credentialSource: 'vault' | 'env' | 'keychain';
  credentialEnvVar: string;       // UI label: "Agent reads it as"
  sourceEnvVar: string;           // UI label: "Rocket reads it from"
  localCredential: string;
  removeSavedCredential: boolean;
  vaultConnectionId: string;
  vaultName: string;              // for Azure, saved as the connection label
  vaultSecretId: string;
  vaultSecretName: string;
}
export type CredentialErrors = Partial<Record<keyof CredentialFormState, string | null>>;

// use-secret-listing.ts
export function useSecretListing(args: {
  connectionId: string;
  scope: string;                  // '' when the provider needs none or none entered
  enabled: boolean;
}): {
  status: 'idle' | 'loading' | 'ready' | 'error';
  secrets: { id: string; name: string }[];
  error: string | null;
  refresh: () => void;
};

// AgentCredentialSection.tsx
export function AgentCredentialSection(props: {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Partial<CredentialFormState>) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}): JSX.Element;               // fetches connections itself via useSecretManagerConnections()
// onChange MUST be a stable reference (useCallback): the section has a prefill effect that depends on it.

// Also exported by plan 2 (plan 3 consumes the first one):
export function vaultNameForSave(connection: SecretManagerConnection | undefined, typedScope: string): string;
// Azure returns the trimmed connection label, every other provider the trimmed typed scope.
export const EMPTY_CREDENTIAL_FORM: CredentialFormState;
export const SCOPE_DEBOUNCE_MS: number;
```
`useSecretManagerConnections` lives at `src/lib/queries/secret-manager-queries.ts`.

### Plan 3 produces (in `src/components/settings/agent-config/`)

```ts
// agent-config-validation.ts  (pure, no React)
export function validateEnvName(name: string): string | null;
export function validateCredentialEnvVar(name: string, kind: AgentKindValue): string | null;
export function validateSourceEnvVar(name: string): string | null;
export function validateIdleSeconds(raw: string): string | null;
export function validateModelName(raw: string): string | null;
export function validateExtraEnv(rows: KeyValueRow[], credentialEnvVar: string, kind: AgentKindValue): (string | null)[];
export const KIND_PRESETS: Record<AgentKindValue, { command: string; args: string[]; credentialEnvVar: string; hint: string }>;
```

Also exported by plan 3: `MIN_IDLE_SECONDS`, `MAX_IDLE_SECONDS`, `reservedEnvVarsFor(kind)`.

Other plan 3 files: `agent-config-form-state.ts` (`AgentFormState`, `formFromConfig`, `withKind`, validity rules), `AgentAdvancedSection.tsx`, `AgentConfigForm.tsx`; `AgentConfigsDialog.tsx` stays at its current path and becomes the shell plus list view.

### Plan 4 produces

Provider-neutral error strings in `crates/rocket-app/src/agent_config_service.rs` and `crates/rocket-app/src/secret_manager_service.rs`, with the asserting tests updated.

## Cross-plan rules (from the plan review)

- Dialog layout: plan 3 renders `<DialogContent className="flex max-h-[85vh] flex-col">` around `DialogHeader`, `DialogBody`, `DialogFooter` (`DialogContent` is a CSS grid, so without this the body never scrolls).
- Accessible names are fixed. `KeyValueRows` inputs: `<valueLabel> <n>` and `<nameLabel> <n>`; remove buttons `Remove <nameLabel lowercase> <n>` (single mode: `Remove <valueLabel lowercase> <n>`); add button is `addLabel`. `FormField` errors are `role="alert"` with id `<id>-error`. Plan 2 keychain pill text is exactly `Saved in the keychain` and appears once; the saved-state help text must not contain that phrase.
- A `CollapsibleSection` that holds an error is opened by its parent through `open` and `onOpenChange`, and its summary gains "needs attention".
- Blank rows (empty name and empty value) in `KeyValueRows` are not validated and are dropped at save.

## Decisions already made (do not reopen)

- Key name stays required for every kind.
- Working Directory is hidden from the form. A stored value is preserved on save by carrying it through the form state untouched.
- Arguments sit directly under Command, one per row.
- Secrets auto-load. No Fetch Secrets button. A refresh icon button sits beside the Secret select.
- Azure hides the scope field and saves the connection label as `vault_name`.
- AWS, HashiCorp and GCP stay listed but disabled with "not available yet".
- A new agent starts with the Claude preset. Changing the agent type replaces a field only when it is empty or still equals the previous type's preset.
- "Rocket reads it from" follows "Agent reads it as" until edited, but only while the source variable name is empty (a new agent), never for a stored agent.
- Help text is always visible and required (`FormField` `help` prop). Conditional notes appear only when relevant (see the spec's Help text section).
