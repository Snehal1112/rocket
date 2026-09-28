import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

// Output nodes have only a label. If and Switch nodes edit their condition,
// value and cases on the card, so the panel offers the label and a pointer.
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
