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

  useEffect(() => {
    if (!containerRef.current) return;

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

    return () => {
      mv.destroy();
    };
  }, [oldContent, newContent, filePath]);

  return <div ref={containerRef} className='h-full overflow-auto' />;
}
