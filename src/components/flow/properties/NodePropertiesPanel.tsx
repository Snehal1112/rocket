import { X } from 'lucide-react';
import { useEffect, useRef } from 'react';
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
  autoFocusLabel = false,
}: {
  node: FlowNode;
  onChange: (kind: FlowNodeKind) => void;
  onClose: () => void;
  autoFocusLabel?: boolean;
}) {
  const asideRef = useRef<HTMLElement>(null);
  // A menu returns focus to its trigger as it closes, so wait a frame and a tick
  // before moving focus to the Label field.
  // biome-ignore lint/correctness/useExhaustiveDependencies: node.id is the trigger.
  useEffect(() => {
    if (!autoFocusLabel) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const frame = requestAnimationFrame(() => {
      timer = setTimeout(() => {
        const field = asideRef.current?.querySelector<HTMLInputElement>('#flow-node-label');
        field?.focus();
        field?.select();
      }, 0);
    });
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(timer);
    };
  }, [autoFocusLabel, node.id]);

  return (
    <aside
      ref={asideRef}
      tabIndex={-1}
      aria-label='Node properties'
      data-testid='node-properties-panel'
      className='nokey flex h-full flex-col bg-background outline-none'
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
      <div key={node.id} className='flex-1 overflow-y-auto p-3'>
        {editorFor(node.kind, onChange)}
      </div>
    </aside>
  );
}
