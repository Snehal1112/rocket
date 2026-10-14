import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '../console-store';

const base = {
  method: 'GET',
  url: 'https://x.test/a',
  status: 200,
  statusText: 'OK',
  durationMs: 1,
  sizeBytes: 1,
  requestHeaders: [],
  requestBody: 'secret-body',
  responseHeaders: [],
  responseBody: '',
};

describe('console store masked copy', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('keeps the masked copy and the uncertain flag on an http entry', () => {
    const masked = { ...base, requestBody: '••••••' };
    useConsoleStore.getState().addHttpEntry({ ...base, masked, uncertain: true });
    const entry = useConsoleStore.getState().entries[0];
    expect(entry.kind).toBe('http');
    if (entry.kind === 'http') {
      expect(entry.masked?.requestBody).toBe('••••••');
      expect(entry.uncertain).toBe(true);
      expect(entry.requestBody).toBe('secret-body');
    }
  });

  it('leaves both fields absent for an entry that was not masked', () => {
    useConsoleStore.getState().addHttpEntry(base);
    const entry = useConsoleStore.getState().entries[0];
    if (entry.kind === 'http') {
      expect(entry.masked).toBeUndefined();
      expect(entry.uncertain).toBeUndefined();
    }
  });
});
