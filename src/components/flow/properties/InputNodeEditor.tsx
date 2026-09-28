import { SingleLineEditor } from '@/components/editor';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

type InputKind = Extract<FlowNodeKind, { kind: 'Input' }>;

export function InputNodeEditor({
  kind,
  onChange,
}: {
  kind: InputKind;
  onChange: (kind: FlowNodeKind) => void;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Value</span>
        {typeof kind.value === 'string' ? (
          <SingleLineEditor
            aria-label='Input value'
            value={kind.value}
            onChange={(value) => onChange({ ...kind, value })}
            placeholder='Text or {{variable}}'
          />
        ) : (
          // A structured value from an older file. Editing it as text would
          // replace its shape, so it is shown but not editable.
          <p data-testid='input-value-readonly' className='text-xs text-muted-foreground'>
            This value has a structured form and can't be edited here:{' '}
            <code className='font-mono'>{JSON.stringify(kind.value)}</code>
          </p>
        )}
      </div>
    </div>
  );
}
