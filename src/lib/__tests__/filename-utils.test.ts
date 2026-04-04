import { describe, expect, it } from 'vitest';
import { sanitizeFilename } from '../filename-utils';

describe('sanitizeFilename', () => {
  it('returns plain names unchanged (with .yml appended)', () => {
    expect(sanitizeFilename('Get Users')).toBe('Get Users.yml');
  });

  it('replaces forward slash with dash', () => {
    expect(sanitizeFilename('users/create')).toBe('users-create.yml');
  });

  it('replaces backslash with dash', () => {
    expect(sanitizeFilename('users\\create')).toBe('users-create.yml');
  });

  it('replaces square brackets', () => {
    expect(sanitizeFilename('items[0]')).toBe('items-0-.yml');
  });

  it('replaces asterisk', () => {
    expect(sanitizeFilename('search*')).toBe('search-.yml');
  });

  it('replaces colon', () => {
    expect(sanitizeFilename('GET: users')).toBe('GET- users.yml');
  });

  it('replaces all unsafe chars in a realistic request name', () => {
    expect(sanitizeFilename('GET /users/:id [v2]*')).toBe('GET -users--id -v2--.yml');
  });

  it('trims leading and trailing whitespace before sanitizing', () => {
    expect(sanitizeFilename('  hello  ')).toBe('hello.yml');
  });

  it('collapses consecutive dashes into one', () => {
    expect(sanitizeFilename('a//b')).toBe('a-b.yml');
  });

  it('falls back to "request" when result would be empty', () => {
    expect(sanitizeFilename('///')).toBe('request.yml');
  });

  it('falls back to "request" for blank input', () => {
    expect(sanitizeFilename('   ')).toBe('request.yml');
  });
});
