import { describe, expect, it } from 'vitest';
import { buildSettingsForSave } from '@/lib/collection-settings-save';
import type { CollectionSettings } from '@/lib/tauri-api';

describe('buildSettingsForSave', () => {
  const current: CollectionSettings = {
    headers: [],
    variables: [],
    sandboxMode: 'developer',
    scriptContextRoots: ['../shared'],
    scriptFlow: 'sequential',
  };

  it('keeps the sandbox mode and script context roots from the fresh settings', () => {
    const payload = buildSettingsForSave(current, {
      headers: [{ key: 'X-A', value: '1', enabled: true }],
      variables: [],
      docs: 'hello',
    });

    expect(payload.sandboxMode).toBe('developer');
    expect(payload.scriptContextRoots).toEqual(['../shared']);
    expect(payload.docs).toBe('hello');
    expect(payload.headers).toHaveLength(1);
  });

  it('keeps the script flow from the fresh settings', () => {
    const payload = buildSettingsForSave(current, {
      headers: [],
      variables: [],
    });

    expect(payload.scriptFlow).toBe('sequential');
  });

  it('leaves the script flow out when the backend did not send one', () => {
    const withoutFlow: CollectionSettings = {
      headers: [],
      variables: [],
      sandboxMode: 'safe',
    };
    const payload = buildSettingsForSave(withoutFlow, { headers: [], variables: [] });

    expect(payload.scriptFlow).toBeUndefined();
    expect(JSON.parse(JSON.stringify(payload))).not.toHaveProperty('scriptFlow');
  });
});
