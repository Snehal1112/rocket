import { PanelRight } from 'lucide-react';
import type * as monacoNs from 'monaco-editor';
import { lazy, Suspense, useCallback, useRef, useState } from 'react';
import { POST_RESPONSE_SNIPPETS } from '@/components/editor/rok-types';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScriptSnippetSidebar } from './ScriptSnippetSidebar';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface ScriptsTabProps {
  preRequestScript: string;
  postResponseScript: string;
  testsScript: string;
  onChangePreRequest: (value: string) => void;
  onChangePostResponse: (value: string) => void;
  onChangeTests: (value: string) => void;
  readOnly?: boolean;
}

export function ScriptsTab({
  preRequestScript,
  postResponseScript,
  testsScript,
  onChangePreRequest,
  onChangePostResponse,
  onChangeTests,
  readOnly = false,
}: ScriptsTabProps) {
  const editorRef = useRef<monacoNs.editor.IStandaloneCodeEditor | null>(null);

  const handleEditorReady = useCallback((editor: monacoNs.editor.IStandaloneCodeEditor) => {
    editorRef.current = editor;
  }, []);

  const handleInsert = useCallback((code: string) => {
    const editor = editorRef.current;
    if (!editor) return;
    const model = editor.getModel();
    if (!model) return;
    const position = editor.getPosition();
    if (!position) {
      // No cursor — append at end with leading and trailing newlines.
      const lastLine = model.getLineCount();
      const lastCol = model.getLineMaxColumn(lastLine);
      editor.executeEdits('snippet-insert', [
        {
          range: {
            startLineNumber: lastLine,
            startColumn: lastCol,
            endLineNumber: lastLine,
            endColumn: lastCol,
          },
          text: `\n${code}\n`,
          forceMoveMarkers: true,
        },
      ]);
      editor.focus();
      return;
    }
    editor.executeEdits('snippet-insert', [
      {
        range: {
          startLineNumber: position.lineNumber,
          startColumn: position.column,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        },
        text: `\n${code}\n`,
        forceMoveMarkers: true,
      },
    ]);
    editor.focus();
  }, []);

  const [activeTab, setActiveTab] = useState('pre-request');
  const [showPostResponseSidebar, setShowPostResponseSidebar] = useState(false);

  return (
    <Tabs value={activeTab} onValueChange={setActiveTab} className='flex flex-col h-full'>
      <TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
        <TabsTrigger value='pre-request' className='text-xs'>
          Pre Request
        </TabsTrigger>
        <TabsTrigger value='post-response' className='text-xs'>
          Post Response
        </TabsTrigger>
        <TabsTrigger value='tests' className='text-xs'>
          Tests
        </TabsTrigger>
        {activeTab === 'post-response' && (
          <Button
            variant='ghost'
            size='sm'
            className='ml-auto h-7 gap-1 text-xs'
            onClick={() => setShowPostResponseSidebar((v) => !v)}
          >
            <PanelRight className='h-3.5 w-3.5' />
            Snippets
          </Button>
        )}
      </TabsList>

      <TabsContent value='pre-request' className='flex-1 m-0 p-0'>
        <Suspense fallback={null}>
          <MonacoWrapper
            language='javascript'
            value={preRequestScript}
            onChange={readOnly ? undefined : onChangePreRequest}
            readOnly={readOnly}
            height='100%'
            phase='pre-request'
          />
        </Suspense>
      </TabsContent>

      <TabsContent value='post-response' className='flex-1 m-0 p-0 flex overflow-hidden'>
        <div className='flex-1 min-w-0'>
          <Suspense fallback={null}>
            <MonacoWrapper
              language='javascript'
              value={postResponseScript}
              onChange={readOnly ? undefined : onChangePostResponse}
              readOnly={readOnly}
              height='100%'
              phase='post-response'
              onEditorReady={handleEditorReady}
            />
          </Suspense>
        </div>
        {!readOnly && showPostResponseSidebar && (
          <ScriptSnippetSidebar snippets={POST_RESPONSE_SNIPPETS} onInsert={handleInsert} />
        )}
      </TabsContent>

      <TabsContent value='tests' className='flex-1 m-0 p-0 flex overflow-hidden'>
        <div className='flex-1 min-w-0'>
          <Suspense fallback={null}>
            <MonacoWrapper
              language='javascript'
              value={testsScript}
              onChange={readOnly ? undefined : onChangeTests}
              readOnly={readOnly}
              height='100%'
              phase='tests'
              onEditorReady={handleEditorReady}
            />
          </Suspense>
        </div>
        {!readOnly && <ScriptSnippetSidebar onInsert={handleInsert} />}
      </TabsContent>
    </Tabs>
  );
}
