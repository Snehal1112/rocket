import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  indexWiresOutOfRange,
  inlineHasContent,
  type SavedRequestEntry,
  savedToInline,
} from '@/lib/flow-node-edits';
import {
  type FlowEdge,
  type FlowNodeKind,
  getRequest,
  type InlineRequestData,
} from '@/lib/tauri-api';
import { InlineSourceEditor } from './InlineSourceEditor';
import { LabelField } from './LabelField';
import { focusIsLost, usePanelRefocus } from './panelFocus';
import { RepeatUntilSection } from './RepeatUntilSection';
import { RequestPicker } from './RequestPicker';
import { SavedRequestDetails } from './SavedRequestDetails';
import { SavedSourceEditor } from './SavedSourceEditor';

type RequestKind = Extract<FlowNodeKind, { kind: 'Request' }>;

// A source switch waiting for the user's confirmation.
type Pending =
  | { type: 'convert'; inline: InlineRequestData; dropped: string[] }
  | { type: 'use-saved'; entry: SavedRequestEntry };

export function RequestNodeEditor({
  nodeId,
  kind,
  edges,
  collection,
  onChange,
}: {
  nodeId: string;
  kind: RequestKind;
  edges: FlowEdge[];
  collection: string;
  onChange: (kind: FlowNodeKind) => void;
}) {
  const [pending, setPending] = useState<Pending | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [converting, setConverting] = useState(false);
  const source = kind.source;
  const refocusPanel = usePanelRefocus();

  // Identifies the current source. A pending confirmation or load error only
  // makes sense for the source it was computed from.
  const sourceKey = source.type === 'Saved' ? `saved:${source.requestPath}` : 'inline';
  const sourceKeyRef = useRef(sourceKey);
  useEffect(() => {
    // A source switch swaps the whole editor branch. When that removed the
    // focused element, keep focus in the panel.
    if (sourceKeyRef.current !== sourceKey && focusIsLost()) refocusPanel();
    sourceKeyRef.current = sourceKey;
    setPending(null);
    setLoadError(null);
  }, [sourceKey, refocusPanel]);

  const applySaved = (entry: SavedRequestEntry) =>
    onChange({ ...kind, source: { type: 'Saved', requestPath: entry.path } });

  // Loads the saved file and shows what the copy keeps and drops. Nothing
  // changes until the user confirms.
  const startConvert = async () => {
    if (source.type !== 'Saved') return;
    const requestPath = source.requestPath;
    // A late result is dropped when the node has moved to another source.
    const isCurrent = () => sourceKeyRef.current === `saved:${requestPath}`;
    setLoadError(null);
    setConverting(true);
    try {
      const request = await getRequest(collection, requestPath);
      if (!isCurrent()) return;
      const { inline, dropped } = savedToInline(request);
      setPending({ type: 'convert', inline, dropped });
    } catch (err) {
      if (isCurrent()) setLoadError(`Could not load "${requestPath}": ${String(err)}`);
    } finally {
      setConverting(false);
    }
  };

  // Switching an inline request back to a saved one only asks when it would
  // throw away something the user typed.
  const pickSavedForInline = (entry: SavedRequestEntry) => {
    if (source.type === 'Inline' && inlineHasContent(source.request)) {
      setPending({ type: 'use-saved', entry });
      return;
    }
    applySaved(entry);
  };

  // The confirmation buttons unmount with their fieldset, so focus moves to
  // the panel first.
  const dismissPending = () => {
    refocusPanel();
    setPending(null);
    setLoadError(null);
  };

  const confirm = () => {
    if (!pending) return;
    refocusPanel();
    if (pending.type === 'convert') {
      onChange({ ...kind, source: { type: 'Inline', request: pending.inline } });
    } else {
      applySaved(pending.entry);
    }
    setPending(null);
    setLoadError(null);
  };

  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <div className='space-y-1'>
        <div className='flex items-center justify-between gap-2'>
          <Label htmlFor='request-debug-mode'>Debug mode</Label>
          <Switch
            id='request-debug-mode'
            checked={kind.debug === true}
            onCheckedChange={(debug) => onChange({ ...kind, debug })}
          />
        </div>
        <p className='text-xs text-muted-foreground'>
          Logs the request as sent and its response to the Console on each run. Secrets are masked.
        </p>
      </div>
      <RepeatUntilSection
        value={kind.repeatUntil ?? null}
        onChange={(repeatUntil) => onChange({ ...kind, repeatUntil })}
      />
      <p className='text-xs text-muted-foreground'>
        Source: {source.type === 'Saved' ? 'saved request' : 'inline request'}
      </p>

      {source.type === 'Saved' ? (
        <>
          <SavedSourceEditor
            requestPath={source.requestPath}
            collection={collection}
            onPick={applySaved}
            onConvertToInline={() => void startConvert()}
            converting={converting}
          />
          <SavedRequestDetails collection={collection} requestPath={source.requestPath} />
        </>
      ) : (
        <>
          <InlineSourceEditor
            request={source.request}
            onChange={(request) => onChange({ ...kind, source: { type: 'Inline', request } })}
            outOfRangeWires={indexWiresOutOfRange(edges, nodeId, source.request.headers.length)}
          />
          <RequestPicker
            collection={collection}
            triggerLabel='Use a saved request…'
            onPick={pickSavedForInline}
          />
        </>
      )}

      {loadError && (
        <p role='alert' className='text-xs text-red-600'>
          {loadError}
        </p>
      )}

      {pending && (
        <fieldset
          aria-label='Confirm source change'
          className='min-w-0 space-y-2 rounded border p-2'
        >
          <p className='text-xs'>
            {pending.type === 'convert'
              ? pending.dropped.length > 0
                ? `Convert to inline? This copies the request into this flow. These parts are not carried over: ${pending.dropped.join(', ')}.`
                : 'Convert to inline? This copies the request into this flow. The saved file is not changed.'
              : `Use "${pending.entry.name}"? The inline method, URL, headers and body will be discarded.`}
          </p>
          <div className='flex justify-end gap-2'>
            <Button
              type='button'
              variant='outline'
              size='sm'
              className='h-7 text-xs'
              onClick={dismissPending}
            >
              Cancel
            </Button>
            <Button type='button' size='sm' className='h-7 text-xs' onClick={confirm}>
              {pending.type === 'convert' ? 'Convert' : 'Use saved request'}
            </Button>
          </div>
        </fieldset>
      )}
    </div>
  );
}
