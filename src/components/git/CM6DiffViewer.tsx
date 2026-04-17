import { MergeView } from '@codemirror/merge';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { basicSetup } from 'codemirror';
import { useEffect, useRef } from 'react';
import {
  getLanguageExtensionForFile,
  multiLineTheme,
  rocketTheme,
  rocketThemeDark,
} from '@/components/editor/extensions';

interface CM6DiffViewerProps {
  oldContent: string;
  newContent: string;
  filePath: string;
}

export function CM6DiffViewer({ oldContent, newContent, filePath }: CM6DiffViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mergeViewRef = useRef<MergeView | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    containerRef.current.innerHTML = '';

    const sharedExtensions = [
      basicSetup,
      rocketTheme,
      rocketThemeDark,
      multiLineTheme,
      EditorView.lineWrapping,
      EditorView.editable.of(false),
      EditorState.readOnly.of(true),
    ];

    const langExt = getLanguageExtensionForFile(filePath);
    if (langExt) sharedExtensions.push(langExt);

    const mv = new MergeView({
      a: {
        doc: oldContent,
        extensions: [...sharedExtensions],
      },
      b: {
        doc: newContent,
        extensions: [...sharedExtensions],
      },
      parent: containerRef.current,
      highlightChanges: true,
      gutter: true,
      collapseUnchanged: { margin: 3, minSize: 4 },
    });

    mergeViewRef.current = mv;

    return () => {
      mv.destroy();
      mergeViewRef.current = null;
    };
  }, [oldContent, newContent, filePath]);

  return <div ref={containerRef} className='h-full overflow-auto' />;
}
