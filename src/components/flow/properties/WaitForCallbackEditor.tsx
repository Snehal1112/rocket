import { useEffect, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { NumberInput } from '@/components/ui/number-input';
import { callbackVariable, isValidCallbackName } from '@/lib/flow-callback';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { CallbackUrlField } from '../CallbackUrlField';
import { LabelField } from './LabelField';

type WaitKind = Extract<FlowNodeKind, { kind: 'WaitForCallback' }>;

const MIN_SECONDS = 1;
const MAX_SECONDS = 3600;

// Whole seconds from typed text, or null while empty, invalid or below the minimum.
function parsedSeconds(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Math.round(Number(text));
  return Number.isFinite(n) && n >= MIN_SECONDS ? Math.min(n, MAX_SECONDS) : null;
}

// Keeps its own draft so clearing the field does not snap back. It commits only
// a valid value and restores the stored one on blur or an outside change.
function TimeoutField({
  timeoutMs,
  onCommit,
}: {
  timeoutMs: number;
  onCommit: (timeoutMs: number) => void;
}) {
  const shown = Math.round(timeoutMs / 1000);
  const [draft, setDraft] = useState(String(shown));

  // Follow the stored value when it changes from outside the field.
  useEffect(() => {
    setDraft((current) => (parsedSeconds(current) === shown ? current : String(shown)));
  }, [shown]);

  return (
    <div className='space-y-1'>
      <Label htmlFor='flow-callback-timeout' className='text-xs'>
        Timeout (seconds)
      </Label>
      <NumberInput
        id='flow-callback-timeout'
        min={MIN_SECONDS}
        max={MAX_SECONDS}
        step={1}
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          const seconds = parsedSeconds(e.target.value);
          if (seconds !== null) onCommit(seconds * 1000);
        }}
        onBlur={() => setDraft(String(shown))}
        className='h-8 text-xs'
      />
    </div>
  );
}

export function WaitForCallbackEditor({
  kind,
  onChange,
  callbackUrl,
}: {
  kind: WaitKind;
  onChange: (kind: FlowNodeKind) => void;
  // This run's URL for the node. Absent when no run is active.
  callbackUrl?: string;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />

      <div className='space-y-1'>
        <Label htmlFor='flow-callback-name' className='text-xs'>
          Name
        </Label>
        <Input
          id='flow-callback-name'
          value={kind.name}
          onChange={(e) => onChange({ ...kind, name: e.target.value })}
          className='h-8 font-mono text-xs'
        />
        {!isValidCallbackName(kind.name) && (
          <p data-testid='callback-name-hint' className='text-xs text-red-600'>
            Use letters, digits and _ only.
          </p>
        )}
        <p className='text-xs text-muted-foreground'>
          Send this URL in an earlier request as{' '}
          <code className='font-mono'>{callbackVariable(kind.name)}</code>
        </p>
        {callbackUrl && <CallbackUrlField url={callbackUrl} />}
      </div>

      <TimeoutField
        timeoutMs={kind.timeoutMs}
        onCommit={(timeoutMs) => onChange({ ...kind, timeoutMs })}
      />

      <div className='space-y-1'>
        <span className='text-xs font-medium'>Accept when</span>
        <SingleLineEditor
          aria-label='Accept when'
          value={kind.acceptWhen ?? ''}
          onChange={(value) => onChange({ ...kind, acceptWhen: value.trim() ? value : null })}
          placeholder="request.body.event === 'payment.completed'"
        />
        <p className='text-xs text-muted-foreground'>
          Optional. <code className='font-mono'>request</code> has method, path, query, headers and
          body. Calls that do not match are answered and ignored. Empty accepts the first call.
        </p>
      </div>

      <p className='text-xs text-muted-foreground'>
        While a run is active, the callback URL is reachable from your local network.
      </p>
    </div>
  );
}
