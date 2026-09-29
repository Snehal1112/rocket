import {
  ArrowLeftFromLine,
  ArrowRightToLine,
  GitBranch,
  Globe,
  Plus,
  Repeat,
  Split,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { FlowNode } from '@/lib/tauri-api';

let nextId = 0;
function newNodeId(prefix: string) {
  nextId += 1;
  return `${prefix}-${Date.now()}-${nextId}`;
}

export function NodePalette({ onAddNode }: { onAddNode: (node: FlowNode) => void }) {
  const defaultPosition = { x: 100, y: 100 };

  return (
    <div className='nokey absolute left-3 top-3 z-10'>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant='outline' size='sm' className='gap-1.5'>
            <Plus className='h-3.5 w-3.5' aria-hidden='true' />
            Add node
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='start'>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('input'),
                kind: { kind: 'Input', label: 'New Input', value: '' },
                position: defaultPosition,
              })
            }
          >
            <ArrowRightToLine className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Input
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('output'),
                kind: { kind: 'Output', label: 'New Output' },
                position: defaultPosition,
              })
            }
          >
            <ArrowLeftFromLine className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Output
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('request'),
                kind: {
                  kind: 'Request',
                  label: 'New Request',
                  source: {
                    type: 'Inline',
                    request: { method: 'GET', url: '', headers: [] },
                  },
                },
                position: defaultPosition,
              })
            }
          >
            <Globe className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Inline Request
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('request'),
                kind: {
                  kind: 'Request',
                  label: 'New Poll',
                  source: {
                    type: 'Inline',
                    request: { method: 'GET', url: '', headers: [] },
                  },
                  repeatUntil: { ...DEFAULT_REPEAT_UNTIL },
                },
                position: defaultPosition,
              })
            }
          >
            <Repeat className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Poll request
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('if'),
                kind: { kind: 'If', label: 'New If', condition: 'response.status === 200' },
                position: defaultPosition,
              })
            }
          >
            <GitBranch className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            If
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('switch'),
                kind: {
                  kind: 'Switch',
                  label: 'New Switch',
                  value: 'response.body.type',
                  cases: [{ id: crypto.randomUUID(), label: 'Case 1', matches: '' }],
                },
                position: defaultPosition,
              })
            }
          >
            <Split className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Switch
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
