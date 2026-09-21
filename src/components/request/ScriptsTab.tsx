import { PanelRight } from 'lucide-react';
import type * as monacoNs from 'monaco-editor';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
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

const MIN_SIDEBAR_WIDTH = 160;
const MIN_EDITOR_WIDTH = 320;

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
  const [snippetSidebars, setSnippetSidebars] = useState<Record<ScriptPhase, boolean>>({
    'pre-request': false,
    'post-response': false,
    tests: false,
  });
  const scriptsContainerRef = useRef<HTMLDivElement>(null);
  const [scriptsContainerWidth, setScriptsContainerWidth] = useState(0);
  const sidebarMaxWidth = Math.max(
    0,
    Math.min(scriptsContainerWidth * 0.5, scriptsContainerWidth - MIN_EDITOR_WIDTH),
  );
  const canShowSidebar = sidebarMaxWidth >= MIN_SIDEBAR_WIDTH;
  const showSidebar = canShowSidebar && snippetSidebars[activeTab];

  useEffect(() => {
    const container = scriptsContainerRef.current;
    if (!container) return;

    const updateWidth = () => setScriptsContainerWidth(container.getBoundingClientRect().width);
    updateWidth();

    const observer =
      typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateWidth);
    observer?.observe(container);
    window.addEventListener('resize', updateWidth);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', updateWidth);
    };
  }, []);

  useEffect(() => {
    if (!canShowSidebar) {
      setSnippetSidebars((current) =>
        Object.values(current).some(Boolean)
          ? { 'pre-request': false, 'post-response': false, tests: false }
          : current,
      );
    }
  }, [canShowSidebar]);

  const toggleSidebar = () => {
    setSnippetSidebars((current) => ({ ...current, [activeTab]: !current[activeTab] }));
  };

  return (
    <Tabs
      ref={scriptsContainerRef}
      value={activeTab}
      onValueChange={(v) => setActiveTab(v as ScriptPhase)}
      className='flex h-full min-h-0 flex-col'
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
        <Button
          variant='ghost'
          size='sm'
          className='ml-auto h-7 gap-1 text-xs'
          onClick={toggleSidebar}
          disabled={!canShowSidebar}
          aria-pressed={showSidebar}
          aria-controls='script-snippet-sidebar'
          title={
            canShowSidebar
              ? showSidebar
                ? 'Hide snippets'
                : 'Show snippets'
              : 'Not enough space to show snippets'
          }
        >
          <PanelRight className='h-3.5 w-3.5' />
          {showSidebar ? 'Hide snippets' : 'Snippets'}
        </Button>
      </TabsList>

      <TabsContent value='pre-request' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
        <div className='min-h-0 min-w-0 flex-1'>
          <Suspense fallback={<EditorSkeleton />}>
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
        {showSidebar && (
          <ScriptSnippetSidebar
            maxWidth={sidebarMaxWidth}
            snippets={PRE_REQUEST_SNIPPETS}
            onInsert={(code) => insertSnippet(editorRefs.current['pre-request'], code)}
          />
        )}
      </TabsContent>

      <TabsContent value='post-response' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
        <div className='min-h-0 min-w-0 flex-1'>
          <Suspense fallback={<EditorSkeleton />}>
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
        {showSidebar && (
          <ScriptSnippetSidebar
            maxWidth={sidebarMaxWidth}
            snippets={POST_RESPONSE_SNIPPETS}
            onInsert={(code) => insertSnippet(editorRefs.current['post-response'], code)}
          />
        )}
      </TabsContent>

      <TabsContent value='tests' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
        <div className='min-h-0 min-w-0 flex-1'>
          <Suspense fallback={<EditorSkeleton />}>
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
        {showSidebar && (
          <ScriptSnippetSidebar
            maxWidth={sidebarMaxWidth}
            onInsert={(code) => insertSnippet(editorRefs.current.tests, code)}
          />
        )}
      </TabsContent>
    </Tabs>
  );
}
