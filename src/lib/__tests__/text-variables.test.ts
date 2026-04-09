import { describe, expect, it } from 'vitest';
import { parseTextTokens } from '../text-variables';

describe('parseTextTokens', () => {
  it('returns single text token for plain text', () => {
    expect(parseTextTokens('hello world')).toEqual([
      { type: 'text', content: 'hello world', rawLength: 11 },
    ]);
  });

  it('returns empty array for empty string', () => {
    expect(parseTextTokens('')).toEqual([]);
  });

  it('parses a single variable', () => {
    expect(parseTextTokens('{{token}}')).toEqual([
      { type: 'variable', content: 'token', rawLength: 9 },
    ]);
  });

  it('parses variable surrounded by text', () => {
    expect(parseTextTokens('Bearer {{token}} extra')).toEqual([
      { type: 'text', content: 'Bearer ', rawLength: 7 },
      { type: 'variable', content: 'token', rawLength: 9 },
      { type: 'text', content: ' extra', rawLength: 6 },
    ]);
  });

  it('parses multiple variables', () => {
    expect(parseTextTokens('{{a}}/{{b}}')).toEqual([
      { type: 'variable', content: 'a', rawLength: 5 },
      { type: 'text', content: '/', rawLength: 1 },
      { type: 'variable', content: 'b', rawLength: 5 },
    ]);
  });

  it('trims whitespace inside braces but rawLength covers full match', () => {
    expect(parseTextTokens('{{ key }}')).toEqual([
      { type: 'variable', content: 'key', rawLength: 9 },
    ]);
  });

  it('handles process.env dot notation', () => {
    expect(parseTextTokens('{{process.env.KEY}}')).toEqual([
      { type: 'variable', content: 'process.env.KEY', rawLength: 19 },
    ]);
  });

  it('does not emit empty text tokens', () => {
    const tokens = parseTextTokens('{{a}}{{b}}');
    expect(tokens).toEqual([
      { type: 'variable', content: 'a', rawLength: 5 },
      { type: 'variable', content: 'b', rawLength: 5 },
    ]);
  });

  it('handles hyphenated variable names', () => {
    expect(parseTextTokens('{{my-var}}')).toEqual([
      { type: 'variable', content: 'my-var', rawLength: 10 },
    ]);
  });
});
