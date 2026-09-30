import type { FlowEdge, FlowNode, SwitchCase } from '@/lib/tauri-api';

// If and Switch are edited on the node. The panel shows the same values so
// a long condition is readable.
export function IfDetails({ condition }: { condition: string }) {
  return (
    <div className='space-y-1 text-xs'>
      <span className='font-medium'>Condition</span>
      <pre
        data-testid='if-details-condition'
        className='whitespace-pre-wrap break-words rounded border bg-muted p-2 font-mono'
      >
        {condition || '—'}
      </pre>
      <p className='text-muted-foreground'>Edit on the node.</p>
    </div>
  );
}

export function SwitchDetails({ value, cases }: { value: string; cases: SwitchCase[] }) {
  return (
    <div className='space-y-1 text-xs'>
      <span className='font-medium'>Value</span>
      <pre
        data-testid='switch-details-value'
        className='whitespace-pre-wrap break-words rounded border bg-muted p-2 font-mono'
      >
        {value || '—'}
      </pre>
      <span className='font-medium'>Cases</span>
      {cases.length === 0 ? (
        <p className='text-muted-foreground'>No cases.</p>
      ) : (
        cases.map((c) => (
          <p
            key={c.id}
            data-testid='switch-details-case'
            className='whitespace-pre-wrap break-words font-mono'
          >
            {c.label} = {c.matches}
          </p>
        ))
      )}
      <p className='text-muted-foreground'>Edit on the node.</p>
    </div>
  );
}

export function OutputDetails({
  nodeId,
  edges,
  nodes,
}: {
  nodeId: string;
  edges: FlowEdge[];
  nodes: FlowNode[];
}) {
  const wire = edges.find((e) => e.targetNodeId === nodeId && e.targetField === 'value');
  const source = wire ? nodes.find((n) => n.id === wire.sourceNodeId) : undefined;
  return (
    <div data-testid='output-details' className='space-y-1 text-xs'>
      {wire ? (
        <p>
          Shows{' '}
          <code className='rounded bg-muted px-1 font-mono'>
            {wire.expression || '(no script)'}
          </code>{' '}
          from <span className='font-medium'>{source?.kind.label ?? '(missing node)'}</span>
        </p>
      ) : (
        <p className='text-muted-foreground'>No value wire.</p>
      )}
    </div>
  );
}
