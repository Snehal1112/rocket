import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

function defaultCapability(): { windows: string[]; permissions: string[] } {
  const file = resolve(__dirname, '../../../src-tauri/capabilities/default.json');
  return JSON.parse(readFileSync(file, 'utf8'));
}

describe('default Tauri capability', () => {
  it('grants the default capability to the main window only', () => {
    expect(defaultCapability().windows).toEqual(['main']);
  });

  it('lists no wildcard window in the default capability', () => {
    expect(defaultCapability().windows.filter((w) => w.includes('*'))).toEqual([]);
  });
});
