import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const src = (path: string) => readFileSync(resolve(process.cwd(), `src/${path}`), 'utf8');

describe('sunken surfaces', () => {
  it.each([
    ['components/settings/SettingsPage.tsx'],
    ['components/settings/sections/AiAgentsSection.tsx'],
    ['components/layout/ConsolePanel.tsx'],
  ])('%s uses the sunken token', (path) => {
    expect(src(path)).toContain('bg-elevation-sunken');
  });

  it('console output well is no longer a translucent background', () => {
    expect(src('components/layout/ConsolePanel.tsx')).not.toContain('bg-background/60');
  });

  it('settings no longer names the editor token directly', () => {
    expect(src('components/settings/SettingsPage.tsx')).not.toContain('bg-editor-bg');
    expect(src('components/settings/sections/AiAgentsSection.tsx')).not.toContain('bg-editor-bg');
  });
});
