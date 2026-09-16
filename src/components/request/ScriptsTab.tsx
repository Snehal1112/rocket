import { PanelRight } from 'lucide-react';
import type * as monacoNs from 'monaco-editor';
import { lazy, Suspense, useRef, useState } from 'react';
import {
  POST_RESPONSE_SNIPPETS,
  PRE_REQUEST_SNIPPETS,
  type ScriptPhase,
} from '@/components/editor/rok-types';
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
}

// Inserts a snippet at the cursor (or appends at the end with no cursor).
// A no-op when `editor` is undefined — e.g. the target tab's Monaco instance
// hasn't finished mounting yet after a fast tab switch.
function insertSnippet(editor: monacoNs.editor.IStandaloneCodeEditor | undefined, code: string) {
  if (!editor) return;
  const model = editor.getModel();
  if (!model) return;
  const position = editor.getPosition();
  const range = position
    ? {
        startLineNumber: position.lineNumber,
        startColumn: position.column,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      }
    : (() => {
        const lastLine = model.getLineCount();
        const lastCol = model.getLineMaxColumn(lastLine);
        return {
          startLineNumber: lastLine,
          startColumn: lastCol,
          endLineNumber: lastLine,
          endColumn: lastCol,
        };
      })();
  editor.executeEdits('snippet-insert', [{ range, text: `\n${code}\n`, forceMoveMarkers: true }]);
  editor.focus();
}

export function ScriptsTab({
  preRequestScript,
  postResponseScript,
  testsScript,
  onChangePreRequest,
  onChangePostResponse,
  onChangeTests,
}: ScriptsTabProps) {
  // Keyed per phase (not a single shared ref) — each tab's Monaco instance is
  // unmounted when its TabsContent goes inactive, so a shared ref could point
  // at a disposed editor from a different tab right after switching.
  const editorRefs = useRef<Partial<Record<ScriptPhase, monacoNs.editor.IStandaloneCodeEditor>>>(
    {},
  );

  const [activeTab, setActiveTab] = useState<ScriptPhase>('pre-request');
  const [showPreRequestSidebar, setShowPreRequestSidebar] = useState(false);
  const [showPostResponseSidebar, setShowPostResponseSidebar] = useState(false);

  return (
    <Tabs
      value={activeTab}
      onValueChange={(v) => setActiveTab(v as ScriptPhase)}
      className='flex flex-col h-full'
    >
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
        {activeTab === 'pre-request' && (
          <Button
            variant='ghost'
            size='sm'
            className='ml-auto h-7 gap-1 text-xs'
            onClick={() => setShowPreRequestSidebar((v) => !v)}
          >
            <PanelRight className='h-3.5 w-3.5' />
            Snippets
          </Button>
        )}
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

      <TabsContent value='pre-request' className='flex-1 m-0 p-0 flex overflow-hidden'>
        <div className='flex-1 min-w-0'>
          <Suspense fallback={null}>
            <MonacoWrapper
              language='javascript'
              value={preRequestScript}
              onChange={onChangePreRequest}
              height='100%'
              phase='pre-request'
              onEditorReady={(editor) => {
                editorRefs.current['pre-request'] = editor;
              }}
            />
          </Suspense>
        </div>
        {showPreRequestSidebar && (
          <ScriptSnippetSidebar
            snippets={PRE_REQUEST_SNIPPETS}
            onInsert={(code) => insertSnippet(editorRefs.current['pre-request'], code)}
          />
        )}
      </TabsContent>

      <TabsContent value='post-response' className='flex-1 m-0 p-0 flex overflow-hidden'>
        <div className='flex-1 min-w-0'>
          <Suspense fallback={null}>
            <MonacoWrapper
              language='javascript'
              value={postResponseScript}
              onChange={onChangePostResponse}
              height='100%'
              phase='post-response'
              onEditorReady={(editor) => {
                editorRefs.current['post-response'] = editor;
              }}
            />
          </Suspense>
        </div>
        {showPostResponseSidebar && (
          <ScriptSnippetSidebar
            snippets={POST_RESPONSE_SNIPPETS}
            onInsert={(code) => insertSnippet(editorRefs.current['post-response'], code)}
          />
        )}
      </TabsContent>

      <TabsContent value='tests' className='flex-1 m-0 p-0 flex overflow-hidden'>
        <div className='flex-1 min-w-0'>
          <Suspense fallback={null}>
            <MonacoWrapper
              language='javascript'
              value={testsScript}
              onChange={onChangeTests}
              height='100%'
              phase='tests'
              onEditorReady={(editor) => {
                editorRefs.current.tests = editor;
              }}
            />
          </Suspense>
        </div>
        <ScriptSnippetSidebar onInsert={(code) => insertSnippet(editorRefs.current.tests, code)} />
      </TabsContent>
    </Tabs>
  );
}
