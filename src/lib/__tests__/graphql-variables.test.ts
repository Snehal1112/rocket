import { describe, expect, it } from 'vitest';
import { validateVariablesText } from '../graphql-variables';

describe('validateVariablesText', () => {
  it('accepts blank text, objects and null', () => {
    expect(validateVariablesText('')).toBeNull();
    expect(validateVariablesText('  \n')).toBeNull();
    expect(validateVariablesText('{"a": [1, 2]}')).toBeNull();
    expect(validateVariablesText('null')).toBeNull();
  });

  it('rejects arrays and scalars', () => {
    expect(validateVariablesText('[1]')).toBe('Variables must be a JSON object.');
    expect(validateVariablesText('5')).toBe('Variables must be a JSON object.');
  });

  it('rejects invalid JSON with the parser message', () => {
    expect(validateVariablesText('{"a": ')).toMatch(/^Variables are not valid JSON/);
  });

  it('does not judge text that holds a placeholder, which is not JSON until resolved', () => {
    expect(validateVariablesText('{"n": {{count}}}')).toBeNull();
  });
});
