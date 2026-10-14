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
