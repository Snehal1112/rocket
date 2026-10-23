import { FileText } from 'lucide-react';
import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { ResponseHeadersTable } from '@/components/response/ResponseHeadersTable';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { METHOD_BADGE_COLOR, statusBadgeColor } from '@/lib/colors';
import type { RequestExample } from '@/lib/tauri-api';
import { listRequestExamples, onCollectionChanged } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { ExampleTab } from '@/types/pane-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

type NameValue = { name: string; value: string };
type LoadState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'notFound' }
  | { status: 'ready'; example: RequestExample };

// Maps a stored body type to a Monaco language.
function languageFor(type: string | undefined): string {
  if (type === 'json' || type === 'xml' || type === 'html') return type;
  return 'plaintext';
}

// Turns a stored body into text. A hand-edited file can hold a number, a list or an object.
function bodyText(data: unknown): string {
  if (data === undefined || data === null) return '';
  return typeof data === 'string' ? data : (JSON.stringify(data, null, 2) ?? '');
}

// Pretty-prints JSON for display and leaves anything else as stored.
function displayBody(type: string | undefined, data: unknown): string {
  const text = bodyText(data);
  if (type !== 'json') return text;
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

// True when a collection-changed event can affect this request file.
// The event path is relative from a domain event and absolute from the file watcher.
function affectsRequest(
  event: { collection?: string | null; path?: string },
  collection: string,
  requestPath: string,
): boolean {
  if (event.collection) {
    const last = event.collection.split('/').pop();
    if (last !== collection) return false;
  }
  if (!event.path) return true;
  const path = event.path.replace(/\\/g, '/');
  return path === requestPath || path.endsWith(`/${requestPath}`);
}

function EmptyNote({ label }: { label: string }) {
  return <p className='p-3 text-xs text-muted-foreground'>{label}</p>;
}

// Read-only name and value rows.
function NameValueTable({ rows, empty }: { rows: NameValue[]; empty: string }) {
  if (rows.length === 0) return <EmptyNote label={empty} />;
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>Value</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((row, i) => (
          // Names can repeat, so the index is part of the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: rows are static and read-only
          <TableRow key={`${row.name}-${i}`}>
            <TableCell className='font-mono text-xs'>{row.name}</TableCell>
            <TableCell className='font-mono text-xs break-all'>{row.value}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

// Read-only Monaco view of a body text.
function BodyEditor({ value, language }: { value: string; language: string }) {
  return (
    <div className='h-full min-h-40'>
      <Suspense fallback={<EditorSkeleton />}>
        <MonacoWrapper value={value} language={language} readOnly height='100%' />
      </Suspense>
    </div>
  );
}

function RequestBody({ body }: { body: NonNullable<RequestExample['request']>['body'] }) {
  if (!body) return <EmptyNote label='No body' />;
  if (body.type === 'form-urlencoded' && Array.isArray(body.data)) {
    const rows = body.data.filter((r) => r && typeof r === 'object');
    return <NameValueTable rows={rows} empty='No body' />;
  }
  const text = displayBody(body.type, body.data);
  if (!text) return <EmptyNote label='No body' />;
  return <BodyEditor value={text} language={languageFor(body.type)} />;
}

function ResponseBody({ body }: { body: NonNullable<RequestExample['response']>['body'] }) {
  const text = body ? displayBody(body.type, body.data) : '';
  if (!text) {
    // A binary body is saved empty, so say why nothing shows.
    return (
      <EmptyNote
        label={
          body?.type === 'binary'
            ? 'The body was not saved (binary or too large)'
            : 'No response body'
        }
      />
    );
  }
  return <BodyEditor value={text} language={languageFor(body?.type)} />;
}

const PANEL_TAB_CLASS = 'h-7 px-2 text-xs';

export function ExampleTabView({ tab }: { tab: ExampleTab }) {
  const { collection, path } = tab.source;
  const [state, setState] = useState<LoadState>({ status: 'loading' });

  const load = useCallback(async () => {
    try {
      const examples = await listRequestExamples(collection, path);
      const example = examples.find((e) => e.name === tab.exampleName);
      setState(example ? { status: 'ready', example } : { status: 'notFound' });
    } catch (err) {
      setState({ status: 'error', message: err instanceof Error ? err.message : String(err) });
    }
  }, [collection, path, tab.exampleName]);

  useEffect(() => {
    void load();
  }, [load]);

  // Reload when the request file is saved, so a rename or delete shows up here.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      if (event.type === 'requestSaved' && affectsRequest(event, collection, path)) void load();
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [collection, path, load]);

  const example = state.status === 'ready' ? state.example : null;
  const request = example?.request;
  const response = example?.response;
  const params = request?.params ?? [];
  const requestHeaders = request?.headers ?? [];
  const responseHeaders = useMemo(
    () => (response?.headers ?? []).map((h) => ({ key: h.name, value: h.value, enabled: true })),
    [response?.headers],
  );

  if (state.status === 'loading') return <EditorSkeleton />;
  if (state.status === 'error') {
    return <EmptyNote label={`Could not load the example: ${state.message}`} />;
  }
  if (state.status === 'notFound') {
    return (
      <EmptyNote
        label={`This example no longer exists. It may have been renamed or deleted: ${tab.exampleName}`}
      />
    );
  }

  const method = (request?.method ?? '').toUpperCase();
  const statusText =
    response?.status !== undefined
      ? `${response.status} ${response.statusText ?? ''}`.trim()
      : null;

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      {/* Top bar: example name, method and status. */}
      <div className='flex items-center gap-2 border-b border-border/70 px-3 py-1.5 shrink-0 bg-card'>
        <FileText aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
        <span className='truncate text-sm font-medium'>{tab.exampleName}</span>
        {method && (
          <span
            className={cn(
              'shrink-0 text-[10px] font-semibold uppercase px-1 py-0.5 rounded border',
              METHOD_BADGE_COLOR[method] ?? METHOD_BADGE_COLOR['GET'],
            )}
          >
            {method}
          </span>
        )}
        {statusText ? (
          <span
            className={`ml-auto inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-semibold ${statusBadgeColor(response?.status ?? 0)}`}
          >
            {statusText}
          </span>
        ) : (
          <span className='ml-auto text-xs text-muted-foreground'>No status</span>
        )}
      </div>

      <div className='grid flex-1 min-h-0 grid-rows-2'>
        <section aria-label='Request' className='flex min-h-0 flex-col border-b border-border/70'>
          <div className='flex items-center gap-2 px-3 py-1.5 shrink-0'>
            {method && <span className='text-xs font-semibold'>{method}</span>}
            <span className='truncate font-mono text-xs'>{request?.url || 'No URL'}</span>
          </div>
          <Tabs defaultValue='params' className='flex min-h-0 flex-1 flex-col px-3'>
            <TabsList className='h-8 w-fit shrink-0'>
              <TabsTrigger value='params' className={PANEL_TAB_CLASS}>
                Params
              </TabsTrigger>
              <TabsTrigger value='headers' className={PANEL_TAB_CLASS}>
                Headers
              </TabsTrigger>
              <TabsTrigger value='body' className={PANEL_TAB_CLASS}>
                Body
              </TabsTrigger>
            </TabsList>
            <TabsContent value='params' className='min-h-0 flex-1 overflow-auto'>
              <NameValueTable rows={params} empty='No params' />
            </TabsContent>
            <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto'>
              <NameValueTable rows={requestHeaders} empty='No headers' />
            </TabsContent>
            <TabsContent value='body' className='min-h-0 flex-1 overflow-auto'>
              <RequestBody body={request?.body} />
            </TabsContent>
          </Tabs>
        </section>

        <section aria-label='Response' className='flex min-h-0 flex-col'>
          <Tabs defaultValue='body' className='flex min-h-0 flex-1 flex-col px-3 pt-1.5'>
            <TabsList className='h-8 w-fit shrink-0'>
              <TabsTrigger value='body' className={PANEL_TAB_CLASS}>
                Response
              </TabsTrigger>
              <TabsTrigger value='headers' className={PANEL_TAB_CLASS}>
                Headers
              </TabsTrigger>
            </TabsList>
            <TabsContent value='body' className='min-h-0 flex-1 overflow-auto'>
              <ResponseBody body={response?.body} />
            </TabsContent>
            <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto'>
              {responseHeaders.length === 0 ? (
                <EmptyNote label='No headers' />
              ) : (
                <ResponseHeadersTable headers={responseHeaders} />
              )}
            </TabsContent>
          </Tabs>
        </section>
      </div>
    </div>
  );
}
