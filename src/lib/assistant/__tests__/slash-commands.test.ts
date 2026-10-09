import { describe, expect, it } from 'vitest';
import { filterSlashCommands, SLASH_COMMANDS } from '@/lib/assistant/slash-commands';

describe('slash commands', () => {
  it('offers the five Rocket templates in order', () => {
    expect(filterSlashCommands('').map((c) => c.name)).toEqual([
      'explain',
      'tests',
      'fix',
      'scaffold',
      'doc',
    ]);
  });

  it('filters by name prefix, ignoring case', () => {
    expect(filterSlashCommands('te').map((c) => c.name)).toEqual(['tests']);
    expect(filterSlashCommands('EX').map((c) => c.name)).toEqual(['explain']);
    expect(filterSlashCommands('zzz')).toEqual([]);
  });

  it('gives every command a description and a template', () => {
    for (const command of SLASH_COMMANDS) {
      expect(command.description.length).toBeGreaterThan(0);
      expect(command.template.trim().length).toBeGreaterThan(0);
    }
  });
});
