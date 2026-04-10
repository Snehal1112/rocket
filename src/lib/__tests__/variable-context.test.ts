import { describe, expect, it } from 'vitest';
import { buildVariableContext, resolveWithContext } from '../variable-context';

const cv = (key: string, value: string, initialValue?: string) => ({
  key,
  value,
  initialValue: initialValue ?? '',
  enabled: true,
  secret: false,
});

describe('buildVariableContext', () => {
  it('env beats collection', () =>
    expect(
      buildVariableContext({ collectionVars: [cv('k', 'col')], envVars: { k: 'env' } })['k'],
    ).toBe('env'));
  it('folder beats env', () =>
    expect(
      buildVariableContext({ folderVars: [cv('k', 'folder')], envVars: { k: 'env' } })['k'],
    ).toBe('folder'));
  it('request beats folder', () =>
    expect(
      buildVariableContext({ requestVars: [cv('k', 'req')], folderVars: [cv('k', 'folder')] })['k'],
    ).toBe('req'));
  it('runtime beats request', () =>
    expect(
      buildVariableContext({ runtimeVars: { k: 'rt' }, requestVars: [cv('k', 'req')] })['k'],
    ).toBe('rt'));
  it('collection beats global', () =>
    expect(
      buildVariableContext({ collectionVars: [cv('k', 'col')], globalVars: { k: 'global' } })['k'],
    ).toBe('col'));
  it('env beats global', () =>
    expect(buildVariableContext({ envVars: { k: 'env' }, globalVars: { k: 'global' } })['k']).toBe(
      'env',
    ));
  it('process.env uses dotted key', () => {
    const ctx = buildVariableContext({ processEnvVars: { API: 'secret' } });
    expect(ctx['API']).toBeUndefined();
    expect(ctx['process.env.API']).toBe('secret');
  });
  it('initialValue fallback when value empty', () =>
    expect(buildVariableContext({ collectionVars: [cv('k', '', 'default')] })['k']).toBe(
      'default',
    ));
  it('disabled vars excluded', () => {
    const ctx = buildVariableContext({
      collectionVars: [{ key: 'k', value: 'v', initialValue: '', enabled: false, secret: false }],
    });
    expect(ctx['k']).toBeUndefined();
  });
});

describe('resolveWithContext', () => {
  it('resolves vars', () =>
    expect(resolveWithContext('{{a}}/{{b}}', { a: 'x', b: 'y' })).toBe('x/y'));
  it('leaves unknown as-is', () => expect(resolveWithContext('{{miss}}', {})).toBe('{{miss}}'));
  it('handles whitespace in braces', () =>
    expect(resolveWithContext('{{ key }}', { key: 'val' })).toBe('val'));
  it('resolves process.env.KEY', () =>
    expect(resolveWithContext('{{process.env.K}}', { 'process.env.K': 'v' })).toBe('v'));
  it('resolves hyphenated variable names', () =>
    expect(resolveWithContext('{{oidc-baseurl}}/api', { 'oidc-baseurl': 'https://auth.local' })).toBe(
      'https://auth.local/api',
    ));
});
