import { lazy, Suspense, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { WIRE_SCRIPT_TYPES, WIRE_SCRIPT_TYPES_PATH } from './wire-script-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const EXTRA_LIB = { content: WIRE_SCRIPT_TYPES, filePath: WIRE_SCRIPT_TYPES_PATH };

interface WireScriptDialogProps {
  edge: FlowEdge;
  targetNode: FlowNode;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCommit: (edge: FlowEdge) => void;
  // Called after the dialog closes, so the caller can put focus back on the canvas.
  onCloseFocus?: () => void;
}

// Returns the header name of a `headers[Name].value` target, or '' for any other target.
export function headerNameFromTarget(targetField: string): string {
  const match = /^headers\[(.+)\]\.value$/.exec(targetField);
  return match ? match[1] : '';
}

/** Existing header names on the target Request node, or [] for non-Request nodes. */
function targetHeaderNames(targetNode: FlowNode): string[] {
  if (targetNode.kind.kind !== 'Request') return [];
  const { source } = targetNode.kind;
  return source.type === 'Inline' ? source.request.headers.map((h) => h.name) : [];
}

export function WireScriptDialog({
  edge,
  targetNode,
  open,
  onOpenChange,
  onCommit,
  onCloseFocus,
}: WireScriptDialogProps) {
  const [expression, setExpression] = useState(edge.expression);
  const [headerName, setHeaderName] = useState(headerNameFromTarget(edge.targetField));
  const isHeadersTarget = edge.targetField === 'headers' || edge.targetField.startsWith('headers[');
  const existingHeaders = targetHeaderNames(targetNode);

  const handleSave = () => {
    let targetField = edge.targetField;
    if (isHeadersTarget) {
      const trimmed = headerName.trim();
      if (!trimmed) return;
      // Address the header by name, never by index: the frontend does not know a
      // Saved request's header order. A non-numeric selector is matched by name
      // case-insensitively. Reuse an existing inline header's spelling when one matches.
      const existing = existingHeaders.find((h) => h.toLowerCase() === trimmed.toLowerCase());
      targetField = `headers[${existing ?? trimmed}].value`;
    }
    onCommit({ ...edge, targetField, expression });
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* nokey keeps Backspace and Delete in the dialog from deleting canvas items. */}
      <DialogContent
        className='nokey max-w-3xl'
        onCloseAutoFocus={(event) => {
          // There is no trigger to focus, so the caller chooses where focus goes.
          if (!onCloseFocus) return;
          event.preventDefault();
          onCloseFocus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Value from source</DialogTitle>
          <DialogDescription>
            Write one expression, or several lines that end with return value. The source result is
            available as response. console.log output appears in the Console.
          </DialogDescription>
        </DialogHeader>
        {isHeadersTarget && (
          <div className='space-y-1'>
            <Label htmlFor='wire-header-name'>Header name</Label>
            <Input
              id='wire-header-name'
              value={headerName}
              onChange={(e) => setHeaderName(e.target.value)}
              placeholder='e.g. Authorization'
            />
          </div>
        )}
        <div className='h-80 overflow-hidden rounded border'>
          <Suspense fallback={<div className='h-full animate-pulse bg-muted' />}>
            <MonacoWrapper
              value={expression}
              onChange={setExpression}
              language='javascript'
              height='100%'
              extraLib={EXTRA_LIB}
            />
          </Suspense>
        </div>
        <DialogFooter>
          <Button variant='outline' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
