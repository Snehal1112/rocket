import './monaco-setup';
import Editor, { type OnMount } from '@monaco-editor/react';
import * as monacoNs from 'monaco-editor';
import { useEffect, useRef } from 'react';
import { generateDynamicVar, isDynamicVar } from '@/lib/dynamic-vars';
import { parseTextTokens } from '@/lib/text-variables';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { EditorSkeleton } from './EditorSkeleton';
import { BASE_EDITOR_OPTIONS, detectLanguage, READONLY_OPTIONS } from './monaco-config';
import type { ScriptPhase } from './rok-types';
import { ROK_TYPE_DEFS_FOR_PHASE } from './rok-types';
import { useMonacoTheme } from './useMonacoTheme';

interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
  variableContext?: Map<string, VariableScopeEntry>;
  phase?: ScriptPhase;
  onEditorReady?: (editor: monacoNs.editor.IStandaloneCodeEditor) => void;
}

// CSS class names used for Monaco inline decorations per variable source.
const SOURCE_CLASS: Record<string, string> = {
  environment: 'rocket-var-environment',
  collection: 'rocket-var-collection',
  global: 'rocket-var-global',
  folder: 'rocket-var-folder',
  request: 'rocket-var-request',
  process: 'rocket-var-process',
  runtime: 'rocket-var-runtime',
  dynamic: 'rocket-var-dynamic',
};

// Injects decoration styles once into the document head.
function ensureDecorationStyles() {
  const styleId = 'rocket-monaco-var-decorations';
  if (document.getElementById(styleId)) return;
  const style = document.createElement('style');
  style.id = styleId;
  style.textContent = `
    .rocket-var-environment { background: hsl(var(--primary)/0.15); color: hsl(var(--primary)); border-radius: 2px; }
    .rocket-var-collection { background: rgb(59 130 246 / 0.15); color: rgb(59 130 246); border-radius: 2px; }
    .rocket-var-global { background: rgb(20 184 166 / 0.15); color: rgb(20 184 166); border-radius: 2px; }
    .rocket-var-folder { background: rgb(245 158 11 / 0.15); color: rgb(245 158 11); border-radius: 2px; }
    .rocket-var-request { background: rgb(168 85 247 / 0.15); color: rgb(168 85 247); border-radius: 2px; }
    .rocket-var-process { background: hsl(var(--muted)); color: hsl(var(--muted-foreground)); border-radius: 2px; }
    .rocket-var-runtime { background: rgb(249 115 22 / 0.15); color: rgb(249 115 22); border-radius: 2px; }
    .rocket-var-dynamic { background: rgb(6 182 212 / 0.15); color: rgb(6 182 212); border-radius: 2px; }
    .rocket-var-unresolved { background: hsl(var(--destructive)/0.15); color: hsl(var(--destructive)); border-radius: 2px; }
  `;
  document.head.appendChild(style);
}

export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  variableContext,
  phase,
  onEditorReady,
}: MonacoWrapperProps) {
  const { themeName } = useMonacoTheme();
  const resolvedLanguage = language ?? detectLanguage(bodyMode, contentType);
  const options = readOnly ? READONLY_OPTIONS : BASE_EDITOR_OPTIONS;

  // Keep a ref so the hover provider closure always reads the latest context.
  const variableContextRef = useRef(variableContext);
  useEffect(() => {
    variableContextRef.current = variableContext;
  }, [variableContext]);

  // Holds the decoration collection so updates don't accumulate stale decorations.
  const decorationCollectionRef = useRef<monacoNs.editor.IEditorDecorationsCollection | null>(null);

  // Refs for disposables that must be cleaned up when the editor unmounts.
  const contentChangeDisposableRef = useRef<monacoNs.IDisposable | null>(null);
  const hoverDisposablesRef = useRef<monacoNs.IDisposable[]>([]);
  const extraLibDisposableRef = useRef<monacoNs.IDisposable | null>(null);

  // Inject decoration styles once on mount.
  useEffect(() => {
    ensureDecorationStyles();
  }, []);

  // Phase changes need fresh type stubs because rok/req/res availability differs per phase.
  useEffect(() => {
    if (!phase) return;
    extraLibDisposableRef.current?.dispose();
    extraLibDisposableRef.current = monacoNs.typescript.javascriptDefaults.addExtraLib(
      ROK_TYPE_DEFS_FOR_PHASE(phase),
      `ts:rok-${phase}.d.ts`,
    );
    return () => {
      extraLibDisposableRef.current?.dispose();
      extraLibDisposableRef.current = null;
    };
  }, [phase]);

  // Clean up content-change and hover disposables on unmount; extraLibDisposableRef is cleaned by the phase effect.
  useEffect(() => {
    return () => {
      contentChangeDisposableRef.current?.dispose();
      for (const d of hoverDisposablesRef.current) d.dispose();
    };
  }, []);

  const handleMount: OnMount = (editor, monaco) => {
    const model = editor.getModel();
    if (!model) return;

    // Build and apply decorations for the given model.
    function applyDecorations(m: monacoNs.editor.ITextModel) {
      if (!decorationCollectionRef.current) return;
      const text = m.getValue();
      const tokens = parseTextTokens(text);
      const decorations: monacoNs.editor.IModelDeltaDecoration[] = [];
      let charOffset = 0;

      for (const token of tokens) {
        const start = charOffset;
        const end = charOffset + token.rawLength;

        if (token.type === 'variable') {
          let cssClass: string;
          if (token.content.startsWith('$')) {
            // Dynamic variable — cyan if known, unresolved if not.
            cssClass = isDynamicVar(token.content.slice(1))
              ? 'rocket-var-dynamic'
              : 'rocket-var-unresolved';
          } else {
            const entry = variableContextRef.current?.get(token.content);
            cssClass = entry
              ? (SOURCE_CLASS[entry.source] ?? 'rocket-var-unresolved')
              : 'rocket-var-unresolved';
          }

          const startPos = m.getPositionAt(start);
          const endPos = m.getPositionAt(end);

          decorations.push({
            range: new monaco.Range(
              startPos.lineNumber,
              startPos.column,
              endPos.lineNumber,
              endPos.column,
            ),
            options: { inlineClassName: cssClass },
          });
        }

        charOffset = end;
      }

      decorationCollectionRef.current.set(decorations);
    }

    // Create the decoration collection and apply initial decorations.
    decorationCollectionRef.current = editor.createDecorationsCollection([]);
    applyDecorations(model);

    // Re-apply decorations whenever the content changes.
    contentChangeDisposableRef.current = editor.onDidChangeModelContent(() => {
      const m = editor.getModel();
      if (m) applyDecorations(m);
    });

    // Register hover providers for common body language IDs.
    const langIds = ['json', 'xml', 'plaintext', 'graphql'];
    for (const langId of langIds) {
      const d = monaco.languages.registerHoverProvider(langId, {
        provideHover(hoverModel: monacoNs.editor.ITextModel, position: monacoNs.Position) {
          // Guard against hover events from a different editor instance using the same language.
          if (editor.getModel()?.id !== hoverModel.id) return null;

          const text = hoverModel.getValue();
          const offset = hoverModel.getOffsetAt(position);
          const tokens = parseTextTokens(text);
          let charPos = 0;

          for (const token of tokens) {
            const tokenStart = charPos;
            const tokenEnd = charPos + token.rawLength;

            if (token.type === 'variable' && offset >= tokenStart && offset < tokenEnd) {
              const hoverRange = new monaco.Range(
                hoverModel.getPositionAt(tokenStart).lineNumber,
                hoverModel.getPositionAt(tokenStart).column,
                hoverModel.getPositionAt(tokenEnd).lineNumber,
                hoverModel.getPositionAt(tokenEnd).column,
              );

              // Dynamic variable ($-prefixed) — show a dedicated hover card.
              if (token.content.startsWith('$')) {
                const stripped = token.content.slice(1);
                if (!isDynamicVar(stripped)) return null;
                const preview = generateDynamicVar(stripped) ?? '';
                return {
                  range: hoverRange,
                  contents: [
                    { value: `**Dynamic Variable** \`{{$${stripped}}}\`` },
                    { value: `Preview: \`${preview}\`` },
                    { value: '_Generates a fresh value on each request send._' },
                  ],
                };
              }

              // Non-dynamic variable — look up in the scope context.
              const ctx = variableContextRef.current;
              if (!ctx) return null;
              const entry = ctx.get(token.content);
              const sourceLabel = entry
                ? entry.source.charAt(0).toUpperCase() + entry.source.slice(1)
                : 'Unresolved';
              const displayValue = entry
                ? entry.secret
                  ? '●●●●'
                  : entry.value || '*(not set)*'
                : '*(not set)*';

              return {
                range: hoverRange,
                contents: [
                  { value: `**\`{{${token.content}}}\`**` },
                  { value: `Source: ${sourceLabel}` },
                  { value: `Value: ${displayValue}` },
                ],
              };
            }

            charPos = tokenEnd;
          }

          return null;
        },
      });
      hoverDisposablesRef.current.push(d);
    }

    onEditorReady?.(editor);
  };

  return (
    <Editor
      height={height}
      language={resolvedLanguage}
      value={value}
      onChange={(val) => onChange?.(val ?? '')}
      onMount={handleMount}
      theme={themeName}
      options={options}
      loading={<EditorSkeleton />}
    />
  );
}
