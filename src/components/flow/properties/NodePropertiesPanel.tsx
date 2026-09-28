import { X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { InputNodeEditor } from './InputNodeEditor';
import { LabelOnlyEditor } from './LabelOnlyEditor';

// Picks the editor for the node's kind. Each editor reports a whole new kind,
// and the caller applies it with one store update.
function editorFor(kind: FlowNodeKind, onChange: (kind: FlowNodeKind) => void) {
  switch (kind.kind) {
    case 'Input':
      return <InputNodeEditor kind={kind} onChange={onChange} />;
    case 'If':
      return (
        <LabelOnlyEditor
          kind={kind}
          onChange={onChange}
          note='The condition is edited on the node itself.'
        />
      );
    case 'Switch':
      return (
        <LabelOnlyEditor
          kind={kind}
          onChange={onChange}
          note='The value and cases are edited on the node itself.'
        />
      );
    case 'Output':
    case 'Request':
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
  }
}

export function NodePropertiesPanel({
  node,
  onChange,
  onClose,
}: {
  node: FlowNode;
  onChange: (kind: FlowNodeKind) => void;
  onClose: () => void;
}) {
  return (
    <aside
      aria-label='Node properties'
      data-testid='node-properties-panel'
      className='flex h-full flex-col bg-background'
    >
      <div className='flex items-center justify-between gap-2 border-b px-3 py-2'>
        <span className='truncate text-xs font-medium'>
          {node.kind.kind} · {node.kind.label}
        </span>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-6 w-6'
          aria-label='Close properties'
          onClick={onClose}
        >
          <X className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </div>
      <div className='flex-1 overflow-y-auto p-3'>{editorFor(node.kind, onChange)}</div>
    </aside>
  );
}
