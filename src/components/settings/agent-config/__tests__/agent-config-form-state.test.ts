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

  it('preserves a stored reasoning level that differs from the default', () => {
    const form = formFromConfig({ ...stored, tuning: { reasoning: 'medium' } });
    expect(form.reasoning).toBe('medium');
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
    expect(
      missingRequiredFields({ ...env, label: '', command: '', credentialEnvVar: '' }, ''),
    ).toEqual(['label', 'command', 'key name']);
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
    expect(
      isAgentFormValid({ ...env, credentialEnvVar: 'GEMINI_CLI_HOME', kind: 'gemini' }, ''),
    ).toBe(false);
  });
});
