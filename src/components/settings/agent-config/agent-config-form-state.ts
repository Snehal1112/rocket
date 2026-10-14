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
  const isUnset = (current: string, preset: string) => current.trim() === '' || current === preset;
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
    sourceEnvVar: form.credentialSource === 'env' ? validateSourceEnvVar(form.sourceEnvVar) : null,
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
