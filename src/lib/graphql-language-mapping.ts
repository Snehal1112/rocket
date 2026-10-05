// Pure mapping between graphql-language-service results and Monaco values. The
// Monaco enums are passed in, so this file imports nothing from Monaco and the
// tests need no editor.

export interface MonacoCompletionKinds {
  Field: number;
  Variable: number;
  Class: number;
  Interface: number;
  Property: number;
  Value: number;
  Enum: number;
  EnumMember: number;
  Keyword: number;
  Text: number;
  Constant: number;
  Struct: number;
}

// LSP CompletionItemKind numbers used by graphql-language-service.
export function mapCompletionKind(kind: number | undefined, kinds: MonacoCompletionKinds): number {
  switch (kind) {
    case 5:
      return kinds.Field;
    case 6:
      return kinds.Variable;
    case 7:
      return kinds.Class;
    case 8:
      return kinds.Interface;
    case 10:
      return kinds.Property;
    case 12:
      return kinds.Value;
    case 13:
      return kinds.Enum;
    case 14:
      return kinds.Keyword;
    case 20:
      return kinds.EnumMember;
    case 21:
      return kinds.Constant;
    case 22:
      return kinds.Struct;
    default:
      return kinds.Text;
  }
}

export function completionDocumentation(
  doc: string | { kind?: string; value: string } | null | undefined,
): string | undefined {
  if (doc === null || doc === undefined) return undefined;
  return typeof doc === 'string' ? doc : doc.value;
}

export interface LspDiagnostic {
  /** The language service may return a markup object instead of a plain string. */
  message: string | { value: string };
  /** 1 error, 2 warning, 3 information, 4 hint. Missing means error. */
  severity?: number;
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
}

export interface MonacoSeverities {
  Hint: number;
  Info: number;
  Warning: number;
  Error: number;
}

export interface EditorMarker {
  message: string;
  severity: number;
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
}

// LSP positions are zero-based, Monaco's are one-based.
export function diagnosticToMarker(d: LspDiagnostic, severities: MonacoSeverities): EditorMarker {
  const severity =
    d.severity === 2
      ? severities.Warning
      : d.severity === 3
        ? severities.Info
        : d.severity === 4
          ? severities.Hint
          : severities.Error;
  return {
    message: typeof d.message === 'string' ? d.message : d.message.value,
    severity,
    startLineNumber: d.range.start.line + 1,
    startColumn: d.range.start.character + 1,
    endLineNumber: d.range.end.line + 1,
    endColumn: d.range.end.character + 1,
  };
}

// `{{variable}}` placeholders are resolved before a request is sent, but they are not
// valid GraphQL when unquoted. Swap each for `null` padded to the same length, so the
// diagnostics parse and their positions still match the editor text.
export function maskPlaceholders(text: string): string {
  return text.replace(/\{\{[^{}]*\}\}/g, (m) => `null${' '.repeat(m.length - 4)}`);
}

// LSP `InsertTextFormat.Snippet`, whose text holds tab stops such as `$1`.
export function isSnippetFormat(format: number | undefined): boolean {
  return format === 2;
}
