import { useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

interface WireExpressionPopoverProps {
  edge: FlowEdge;
  targetNode: FlowNode;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCommit: (edge: FlowEdge) => void;
  children: React.ReactNode;
}

/** Existing header names on the target Request node, or [] for non-Request nodes. */
function targetHeaderNames(targetNode: FlowNode): string[] {
  if (targetNode.kind.kind !== 'Request') return [];
  const { source } = targetNode.kind;
  return source.type === 'Inline' ? source.request.headers.map((h) => h.name) : [];
}

export function WireExpressionPopover({
  edge,
  targetNode,
  open,
  onOpenChange,
  onCommit,
  children,
}: WireExpressionPopoverProps) {
  const [expression, setExpression] = useState(edge.expression);
  const [headerName, setHeaderName] = useState('');
  const isHeadersTarget = edge.targetField === 'headers';
  const existingHeaders = targetHeaderNames(targetNode);

  const handleCommit = () => {
    let targetField = edge.targetField;
    if (isHeadersTarget) {
      const trimmed = headerName.trim();
      if (!trimmed) return;
      // Address the header by name, never by index: the frontend does not
      // know a Saved request's header order. Plan 05's apply_wired_overrides
      // treats a non-numeric selector as a case-insensitive name, updating
      // the matching header or appending it if absent. Reuse an existing
      // inline header's spelling when one matches.
      const existing = existingHeaders.find((h) => h.toLowerCase() === trimmed.toLowerCase());
      targetField = `headers[${existing ?? trimmed}].value`;
    }
    onCommit({ ...edge, targetField, expression });
    onOpenChange(false);
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent className='w-72 space-y-2'>
        {isHeadersTarget && (
          <div>
            <Label className='text-xs font-medium'>Header name</Label>
            <Input
              value={headerName}
              onChange={(e) => setHeaderName(e.target.value)}
              placeholder='e.g. Authorization'
              className='h-8 text-sm'
            />
          </div>
        )}
        <div>
          <Label className='text-xs font-medium'>Value from source</Label>
          <SingleLineEditor
            value={expression}
            onChange={setExpression}
            placeholder='response.body'
            className='text-xs'
          />
        </div>
        <Button size='sm' onClick={handleCommit}>
          Save
        </Button>
      </PopoverContent>
    </Popover>
  );
}
