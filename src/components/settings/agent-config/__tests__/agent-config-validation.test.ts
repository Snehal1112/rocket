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

  it('accepts exactly 128 characters', () => {
    expect(validateModelName('a'.repeat(128))).toBeNull();
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
