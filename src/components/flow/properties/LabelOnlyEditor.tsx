import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

// Output, If and Switch nodes edit only their label here. Their details show below it.
export function LabelOnlyEditor({
  kind,
  onChange,
  note,
}: {
  kind: FlowNodeKind;
  onChange: (kind: FlowNodeKind) => void;
  note?: string;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      {note && <p className='text-xs text-muted-foreground'>{note}</p>}
    </div>
  );
}
