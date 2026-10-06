import { describe, expect, it } from 'vitest';
import { buildSettingsForSave } from '@/lib/collection-settings-save';
import type { CollectionSettings } from '@/lib/tauri-api';

describe('buildSettingsForSave', () => {
  const current: CollectionSettings = {
    headers: [],
    variables: [],
    sandboxMode: 'developer',
    scriptContextRoots: ['../shared'],
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
});
