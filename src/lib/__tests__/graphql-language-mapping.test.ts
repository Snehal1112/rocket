import { getDiagnostics } from 'graphql-language-service';
import { describe, expect, it } from 'vitest';
import {
  completionDocumentation,
  diagnosticToMarker,
  isSnippetFormat,
  mapCompletionKind,
  maskPlaceholders,
} from '../graphql-language-mapping';

const kinds = {
  Field: 3,
  Variable: 4,
  Class: 5,
  Interface: 7,
  Property: 9,
  Value: 13,
  Enum: 15,
  EnumMember: 16,
  Keyword: 17,
  Text: 18,
  Constant: 14,
  Struct: 6,
};

describe('mapCompletionKind', () => {
  it('maps LSP kinds to Monaco kinds', () => {
    expect(mapCompletionKind(5, kinds)).toBe(kinds.Field);
    expect(mapCompletionKind(6, kinds)).toBe(kinds.Variable);
    expect(mapCompletionKind(7, kinds)).toBe(kinds.Class);
    expect(mapCompletionKind(13, kinds)).toBe(kinds.Enum);
    expect(mapCompletionKind(14, kinds)).toBe(kinds.Keyword);
    expect(mapCompletionKind(20, kinds)).toBe(kinds.EnumMember);
  });

  it('falls back to Text for an unknown or missing kind', () => {
    expect(mapCompletionKind(undefined, kinds)).toBe(kinds.Text);
    expect(mapCompletionKind(999, kinds)).toBe(kinds.Text);
  });
});

describe('completionDocumentation', () => {
  it('accepts a string, a markup object or nothing', () => {
    expect(completionDocumentation('plain')).toBe('plain');
    expect(completionDocumentation({ kind: 'markdown', value: 'md' })).toBe('md');
    expect(completionDocumentation(undefined)).toBeUndefined();
    expect(completionDocumentation(null)).toBeUndefined();
  });
});

describe('diagnosticToMarker', () => {
  const severities = { Hint: 1, Info: 2, Warning: 4, Error: 8 };

  it('moves zero-based LSP positions to one-based Monaco positions', () => {
    const marker = diagnosticToMarker(
      {
        message: 'Unknown field',
        severity: 1,
        range: { start: { line: 0, character: 2 }, end: { line: 0, character: 6 } },
      },
      severities,
    );
    expect(marker).toEqual({
      message: 'Unknown field',
      severity: 8,
      startLineNumber: 1,
      startColumn: 3,
      endLineNumber: 1,
      endColumn: 7,
    });
  });

  it('maps warning, info and hint severities and defaults to error', () => {
    const base = {
      message: 'm',
      range: { start: { line: 1, character: 0 }, end: { line: 1, character: 1 } },
    };
    expect(diagnosticToMarker({ ...base, severity: 2 }, severities).severity).toBe(4);
    expect(diagnosticToMarker({ ...base, severity: 3 }, severities).severity).toBe(2);
    expect(diagnosticToMarker({ ...base, severity: 4 }, severities).severity).toBe(1);
    expect(diagnosticToMarker(base, severities).severity).toBe(8);
  });
});

describe('maskPlaceholders', () => {
  it('keeps the text length so positions still line up', () => {
    const text = 'query { user(id: {{userId}}, n: "{{x}}") { name } }';
    expect(maskPlaceholders(text)).toHaveLength(text.length);
  });

  it('turns an unquoted placeholder into a value that parses', () => {
    // No schema: this checks syntax only. Under vitest the language service loads its own
    // copy of `graphql`, so passing a schema built here would be rejected as foreign.
    const text = 'query { user(id: {{userId}}, n: "{{x}}") }';
    expect(getDiagnostics(text).length).toBeGreaterThan(0);
    expect(getDiagnostics(maskPlaceholders(text))).toEqual([]);
  });

  it('leaves text without placeholders alone', () => {
    expect(maskPlaceholders('{ a }')).toBe('{ a }');
  });
});

describe('isSnippetFormat', () => {
  it('is true only for the LSP snippet format', () => {
    expect(isSnippetFormat(2)).toBe(true);
    expect(isSnippetFormat(1)).toBe(false);
    expect(isSnippetFormat(undefined)).toBe(false);
  });
});
