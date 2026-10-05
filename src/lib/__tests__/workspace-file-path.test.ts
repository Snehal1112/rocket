import { describe, expect, it } from 'vitest';
import { toUploadFilePath, toWorkspaceRelativePath } from '../workspace-file-path';

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

describe('toUploadFilePath', () => {
  const ws = '/home/me/ws';

  it('is relative to the collection folder for a file inside it', () => {
    expect(toUploadFilePath('/home/me/ws/collections/api/files/a.txt', ws, 'api')).toBe(
      'files/a.txt',
    );
  });

  it('normalizes Windows-style paths inside the collection folder', () => {
    expect(toUploadFilePath('C:\\ws\\collections\\api\\a.txt', 'C:\\ws', 'api')).toBe('a.txt');
  });

  it('keeps the absolute path for a file in the workspace but outside the collection', () => {
    expect(toUploadFilePath('/home/me/ws/collections/other/a.txt', ws, 'api')).toBe(
      '/home/me/ws/collections/other/a.txt',
    );
  });

  it('does not treat a sibling collection with the same prefix as inside', () => {
    expect(toUploadFilePath('/home/me/ws/collections/api-v2/a.txt', ws, 'api')).toBe(
      '/home/me/ws/collections/api-v2/a.txt',
    );
  });

  it('keeps the absolute path when the request is not in a collection', () => {
    expect(toUploadFilePath('/home/me/ws/files/a.txt', ws)).toBe('/home/me/ws/files/a.txt');
  });

  it('returns null for a file outside the workspace', () => {
    expect(toUploadFilePath('/home/me/other/a.txt', ws, 'api')).toBeNull();
    expect(toUploadFilePath('/home/me/other/a.txt', ws)).toBeNull();
  });

  it('returns null when the path climbs out with ..', () => {
    expect(toUploadFilePath('/home/me/ws/collections/api/../../x.txt', ws, 'api')).toBeNull();
  });
});
