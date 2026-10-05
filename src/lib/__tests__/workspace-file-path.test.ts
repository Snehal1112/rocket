import { describe, expect, it } from 'vitest';
import { toWorkspaceRelativePath } from '../workspace-file-path';

describe('toWorkspaceRelativePath', () => {
  it('returns a forward-slash path relative to the workspace', () => {
    expect(toWorkspaceRelativePath('/home/me/ws/files/a.png', '/home/me/ws')).toBe('files/a.png');
  });

  it('ignores a trailing slash on the root', () => {
    expect(toWorkspaceRelativePath('/home/me/ws/a.png', '/home/me/ws/')).toBe('a.png');
  });

  it('normalizes Windows-style backslashes and drive letter case', () => {
    expect(toWorkspaceRelativePath('C:\\Users\\me\\ws\\files\\a.png', 'c:\\Users\\me\\ws')).toBe(
      'files/a.png',
    );
  });

  it('returns null for a file outside the workspace', () => {
    expect(toWorkspaceRelativePath('/home/me/other/a.png', '/home/me/ws')).toBeNull();
  });

  it('does not treat a sibling folder with the same prefix as inside', () => {
    expect(toWorkspaceRelativePath('/home/me/ws-other/a.png', '/home/me/ws')).toBeNull();
  });

  it('returns null when the path climbs out with ..', () => {
    expect(toWorkspaceRelativePath('/home/me/ws/../x/a.png', '/home/me/ws')).toBeNull();
  });
});
