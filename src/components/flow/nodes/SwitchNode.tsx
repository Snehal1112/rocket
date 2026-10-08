import { Handle, type NodeProps, Position, useUpdateNodeInternals } from '@xyflow/react';
import { Plus, Split, X } from 'lucide-react';
import { useEffect } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { caseHandle, DEFAULT_HANDLE, INPUT_HANDLE } from '@/lib/flow-handles';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason, SwitchCase } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { caseDisplayLabel, exitLabel } from '../flowExits';
import { DurationChip } from './DurationChip';
import { useFlowNodeActions } from './FlowNodeActionsContext';
import { NodeIssueBadge } from './NodeIssueBadge';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { NodeStatusIcon } from './NodeStatusIcon';
import { issueRingClassName, nodeStatusClassName } from './nodeStatus';

export type SwitchNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'Switch' }>;
  status: FlowNodeStatus;
  error?: string;
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
  /** True while a partial run is in progress and this result is from the earlier run. */
  cached?: boolean;
  /** How long the last run of this node took. */
  durationMs?: number;
  skipReason?: FlowSkipReason;
  /** Exit chosen by the last run: "case:<id>" or "default". */
  branch?: string;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
};

// Match values that appear more than once. The backend rejects them on save
// (rule V7), so flag them while the user is still editing.
function duplicateMatches(cases: SwitchCase[]): Set<string> {
  const seen = new Set<string>();
  const dupes = new Set<string>();
  for (const c of cases) {
    if (seen.has(c.matches)) dupes.add(c.matches);
    seen.add(c.matches);
  }
  return dupes;
}

// Number for a new case's "Case N" label. It skips numbers an existing
// label already uses, so two cases never share a default name.
function nextCaseNumber(cases: SwitchCase[]): number {
  const used = new Set(cases.map((c) => c.label));
  let n = cases.length + 1;
  while (used.has(`Case ${n}`)) n += 1;
  return n;
}

export function SwitchNode({ id, data, isConnectable }: NodeProps & { data: SwitchNodeData }) {
  const { updateNodeKind, removeSwitchCase } = useFlowNodeActions();
  const { kind, status } = data;
  const dupes = duplicateMatches(kind.cases);

  // React Flow only learns about handles added or removed after mount when
  // told explicitly. Without this, a new case's exit cannot be connected.
  const updateNodeInternals = useUpdateNodeInternals();
  const caseKey = kind.cases.map((c) => c.id).join('|');
  // biome-ignore lint/correctness/useExhaustiveDependencies: caseKey changes exactly when exit handles are added or removed.
  useEffect(() => {
    updateNodeInternals(id);
  }, [id, caseKey, updateNodeInternals]);

  const setCase = (caseId: string, patch: Partial<SwitchCase>) =>
    updateNodeKind(id, {
      ...kind,
      cases: kind.cases.map((c) => (c.id === caseId ? { ...c, ...patch } : c)),
    });

  const addCase = () => {
    const n = nextCaseNumber(kind.cases);
    updateNodeKind(id, {
      ...kind,
      cases: [...kind.cases, { id: crypto.randomUUID(), label: `Case ${n}`, matches: '' }],
    });
  };

  return (
    <div
      data-testid='switch-node-card'
      data-status={status}
      className={cn(
        'w-72 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        issueRingClassName(data.issues),
      )}
    >
      <Handle
        type='target'
        id={INPUT_HANDLE}
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <Split className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Switch</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeStatusIcon status={data.status} />
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      {status === 'success' && data.branch && (
        <div className='px-2 pt-1'>
          <Badge variant='secondary' data-testid='branch-badge'>
            → {exitLabel(kind, data.branch) ?? data.branch}
          </Badge>
        </div>
      )}
      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
        cached={data.cached}
      />
      <DurationChip durationMs={data.durationMs} />

      <div className='nodrag nowheel nokey space-y-1 px-2 py-1.5'>
        <span className='text-muted-foreground'>value</span>
        <SingleLineEditor
          aria-label='Switch value'
          value={kind.value}
          onChange={(value) => updateNodeKind(id, { ...kind, value })}
          placeholder='response.body.type'
          className='text-xs'
        />

        {kind.cases.map((c, i) => (
          <div key={c.id} className='relative flex items-center gap-1 pr-3'>
            <Input
              aria-label={`Case ${i + 1} label`}
              value={c.label}
              onChange={(e) => setCase(c.id, { label: e.target.value })}
              className='h-6 px-1 text-xs'
            />
            <span className='text-muted-foreground'>=</span>
            <Input
              aria-label={`Case ${i + 1} matches`}
              aria-invalid={dupes.has(c.matches)}
              value={c.matches}
              onChange={(e) => setCase(c.id, { matches: e.target.value })}
              className={cn('h-6 px-1 font-mono text-xs', dupes.has(c.matches) && 'border-red-500')}
            />
            <Button
              variant='ghost'
              size='icon'
              aria-label={`Remove case ${caseDisplayLabel(c, i)}`}
              className='h-6 w-6 shrink-0'
              onClick={() => removeSwitchCase(id, c.id)}
            >
              <X className='h-3 w-3' aria-hidden='true' />
            </Button>
            {/* The row sits inside the px-2 wrapper, so the handle shifts
                right by that padding to line up with the default exit. */}
            <Handle
              type='source'
              id={caseHandle(c.id)}
              position={Position.Right}
              isConnectable={isConnectable}
              className='!-right-2 !h-2 !w-2'
            />
          </div>
        ))}

        {dupes.size > 0 && (
          <div role='alert' className='text-[10px] text-red-600'>
            Two cases match the same value.
          </div>
        )}

        <Button
          variant='ghost'
          size='sm'
          aria-label='Add case'
          className='h-6 gap-1 px-1 text-xs'
          onClick={addCase}
        >
          <Plus className='h-3 w-3' aria-hidden='true' />
          Add case
        </Button>
      </div>

      <div className='relative flex justify-end px-2 pb-1.5 pr-4'>
        <span className='text-muted-foreground'>default</span>
        <Handle
          type='source'
          id={DEFAULT_HANDLE}
          position={Position.Right}
          isConnectable={isConnectable}
          className='!h-2 !w-2'
        />
      </div>
    </div>
  );
}
