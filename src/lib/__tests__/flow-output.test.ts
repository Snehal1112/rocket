import { describe, expect, it } from 'vitest';
import { formatOutputValue } from '../flow-output';

describe('formatOutputValue', () => {
  it('pretty-prints an object with two spaces', () => {
    expect(formatOutputValue('{"a":1,"b":{"c":2}}')).toBe(
      '{\n  "a": 1,\n  "b": {\n    "c": 2\n  }\n}',
    );
  });

  it('pretty-prints an array', () => {
    expect(formatOutputValue('[1,2]')).toBe('[\n  1,\n  2\n]');
  });

  it('leaves plain text unchanged', () => {
    expect(formatOutputValue('hello world')).toBe('hello world');
  });

  it('leaves a number string unchanged', () => {
    expect(formatOutputValue('42')).toBe('42');
  });

  it('leaves invalid JSON unchanged', () => {
    expect(formatOutputValue('{"a":')).toBe('{"a":');
  });

  it('leaves an empty string unchanged', () => {
    expect(formatOutputValue('')).toBe('');
  });
});
