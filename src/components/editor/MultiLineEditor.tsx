import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { basicSetup } from 'codemirror';
import { useEffect, useRef } from 'react';

import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import {
  detectLanguage,
  getLanguageExtension,
  multiLineTheme,
  rocketTheme,
  rocketThemeDark,
  setVariableContextEffect,
  variableContextFacet,
  variableHighlight,
  variableHoverTooltip,
} from './extensions';

export interface MultiLineEditorProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
  variableContext?: Map<string, VariableScopeEntry>;
}

/**
 * CodeMirror v6-based multi-line editor with language detection,
 * variable highlighting, and hover tooltips.
 */
export function MultiLineEditor({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  variableContext,
}: MultiLineEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const isSyncingRef = useRef(false);

  const resolvedLang = language ?? detectLanguage(bodyMode, contentType);

  // biome-ignore lint/correctness/useExhaustiveDependencies: recreate editor only when language or readOnly changes, value and variableContext are synced via separate effects below
  useEffect(() => {
    if (!containerRef.current) return;

    const extensions = [
      basicSetup,
      rocketTheme,
      rocketThemeDark,
      multiLineTheme,
      EditorView.lineWrapping,
    ];

    const langExt = getLanguageExtension(resolvedLang);
    if (langExt) extensions.push(langExt);

    if (variableContext) {
      extensions.push(
        variableContextFacet.of(variableContext),
        variableHighlight(),
        variableHoverTooltip(),
      );
    }

    if (readOnly) {
      extensions.push(EditorState.readOnly.of(true), EditorView.editable.of(false));
    }

    extensions.push(
      EditorView.updateListener.of((update) => {
        if (update.docChanged && !isSyncingRef.current) {
          onChangeRef.current?.(update.state.doc.toString());
        }
      }),
    );

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [resolvedLang, readOnly]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      isSyncingRef.current = true;
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value },
      });
      isSyncingRef.current = false;
    }
  }, [value]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view || !variableContext) return;
    view.dispatch({
      effects: setVariableContextEffect.of(variableContext),
    });
  }, [variableContext]);

  return <div ref={containerRef} className={cn('overflow-hidden border-0')} style={{ height }} />;
}
