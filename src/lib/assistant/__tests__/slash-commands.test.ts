import { describe, expect, it } from 'vitest';
import { filterSlashCommands, SLASH_COMMANDS } from '@/lib/assistant/slash-commands';

describe('slash commands', () => {
  it('offers the seven Rocket templates in order', () => {
    expect(filterSlashCommands('').map((c) => c.name)).toEqual([
      'explain',
      'tests',
      'fix',
      'scaffold',
      'doc',
      'review',
      'howto',
    ]);
  });

  it('filters by name prefix, ignoring case', () => {
    expect(filterSlashCommands('te').map((c) => c.name)).toEqual(['tests']);
    expect(filterSlashCommands('EX').map((c) => c.name)).toEqual(['explain']);
    expect(filterSlashCommands('re').map((c) => c.name)).toEqual(['review']);
    expect(filterSlashCommands('ho').map((c) => c.name)).toEqual(['howto']);
    expect(filterSlashCommands('zzz')).toEqual([]);
  });

  it('asks for documentation with the standard sections and only what the request shows', () => {
    const doc = SLASH_COMMANDS.find((c) => c.name === 'doc');
    expect(doc?.template).toContain('Purpose');
    expect(doc?.template).toContain('Responses and errors');
    expect(doc?.template).toMatch(/only what/i);
  });

  it('gives every command a description and a template', () => {
    for (const command of SLASH_COMMANDS) {
      expect(command.description.length).toBeGreaterThan(0);
      expect(command.template.trim().length).toBeGreaterThan(0);
    }
  });
});
