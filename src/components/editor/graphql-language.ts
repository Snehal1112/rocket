import type { GraphQLSchema } from 'graphql';
import { getAutocompleteSuggestions, getDiagnostics, Position } from 'graphql-language-service';
import * as monaco from 'monaco-editor';
import {
  completionDocumentation,
  diagnosticToMarker,
  isSnippetFormat,
  mapCompletionKind,
  maskPlaceholders,
} from '@/lib/graphql-language-mapping';

export interface GraphQlSupport {
  /** Re-run the diagnostics, for example after the schema changed. */
  revalidate: () => void;
  dispose: () => void;
}

// Registers schema-aware completion for one editor and keeps its diagnostics
// current. The completion provider is registered for the `graphql` language, so
// it checks the model id and answers only for this editor's model.
function noop(): void {
  // Nothing to undo when there is no model.
}

export function attachGraphQlSupport(
  editor: monaco.editor.IStandaloneCodeEditor,
  getSchema: () => GraphQLSchema | undefined,
): GraphQlSupport {
  const model = editor.getModel();
  if (!model) return { revalidate: noop, dispose: noop };

  const completion = monaco.languages.registerCompletionItemProvider('graphql', {
    triggerCharacters: ['{', '(', ' ', ':', '$', '@', '.', '\n'],
    provideCompletionItems(m, position) {
      const schema = getSchema();
      if (m.id !== model.id || !schema) return { suggestions: [] };
      const items = getAutocompleteSuggestions(
        schema,
        m.getValue(),
        new Position(position.lineNumber - 1, position.column - 1),
      );
      const word = m.getWordUntilPosition(position);
      const range = new monaco.Range(
        position.lineNumber,
        word.startColumn,
        position.lineNumber,
        word.endColumn,
      );
      return {
        suggestions: items.map((item) => ({
          label: item.label,
          kind: mapCompletionKind(item.kind, monaco.languages.CompletionItemKind),
          detail: item.detail ?? undefined,
          documentation: completionDocumentation(item.documentation),
          insertText: item.insertText ?? item.label,
          insertTextRules: isSnippetFormat(item.insertTextFormat)
            ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
            : undefined,
          range,
        })),
      };
    },
  });

  const revalidate = () => {
    // Without a schema this still reports syntax errors.
    const diagnostics = getDiagnostics(maskPlaceholders(model.getValue()), getSchema());
    monaco.editor.setModelMarkers(
      model,
      'graphql',
      diagnostics.map((d) => diagnosticToMarker(d, monaco.MarkerSeverity)),
    );
  };

  let timer: ReturnType<typeof setTimeout> | undefined;
  const onChange = editor.onDidChangeModelContent(() => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(revalidate, 300);
  });
  revalidate();

  return {
    revalidate,
    dispose() {
      if (timer) clearTimeout(timer);
      onChange.dispose();
      completion.dispose();
      monaco.editor.setModelMarkers(model, 'graphql', []);
    },
  };
}
