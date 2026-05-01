import { GripVertical, Plus, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { LoadTestPhase, PhaseKind, TargetUnit } from '@/lib/tauri-api';

interface Props {
  phases: LoadTestPhase[];
  onChange: (phases: LoadTestPhase[]) => void;
  disabled?: boolean;
  unit: TargetUnit;
}

const KIND_COLORS: Record<PhaseKind, string> = {
  RampUp: 'hsl(var(--chart-4))',
  Hold: 'hsl(var(--chart-2))',
  RampDown: 'hsl(var(--destructive))',
};

export function PhaseBuilder({ phases, onChange, disabled, unit }: Props) {
  const dragIndex = useRef<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const phaseIds = useRef<string[]>([]);
  // Keep phaseIds in sync with phases length.
  while (phaseIds.current.length < phases.length) {
    phaseIds.current.push(crypto.randomUUID());
  }
  phaseIds.current = phaseIds.current.slice(0, phases.length);

  const update = (index: number, patch: Partial<LoadTestPhase>) => {
    onChange(phases.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };

  const remove = (index: number) => {
    onChange(phases.filter((_, i) => i !== index));
  };

  const addPhase = () => {
    const target =
      unit === 'rps'
        ? { kind: 'rps' as const, value: 50 }
        : { kind: 'concurrency' as const, value: 10 };
    onChange([...phases, { kind: 'Hold', durationSecs: 30, target }]);
  };

  const handleDragStart = (index: number) => {
    dragIndex.current = index;
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    setDragOverIndex(index);
  };

  const handleDrop = (dropIndex: number) => {
    const from = dragIndex.current;
    if (from === null || from === dropIndex) {
      dragIndex.current = null;
      setDragOverIndex(null);
      return;
    }
    const next = phases.slice();
    const [moved] = next.splice(from, 1);
    next.splice(dropIndex, 0, moved);
    onChange(next);
    dragIndex.current = null;
    setDragOverIndex(null);
  };

  const handleDragEnd = () => {
    dragIndex.current = null;
    setDragOverIndex(null);
  };

  return (
    <div className='flex flex-col gap-2'>
      {phases.map((phase, i) => (
        // biome-ignore lint/a11y/noStaticElementInteractions: drag-and-drop reorder handle
        <div
          key={phaseIds.current[i]}
          draggable={!disabled}
          onDragStart={() => handleDragStart(i)}
          onDragOver={(e) => handleDragOver(e, i)}
          onDrop={() => handleDrop(i)}
          onDragEnd={handleDragEnd}
          className={`rounded-md border bg-muted/30 p-2 transition-all ${
            dragOverIndex === i ? 'ring-1 ring-primary' : 'border-border/60'
          }`}
        >
          {/* Top row: grip + kind selector + delete */}
          <div className='mb-2 flex items-center gap-1.5'>
            <GripVertical className='h-3 w-3 shrink-0 cursor-grab text-muted-foreground' />
            <span
              className='h-2 w-2 shrink-0 rounded-full'
              style={{ backgroundColor: KIND_COLORS[phase.kind] }}
            />
            <Select
              value={phase.kind}
              onValueChange={(v) => update(i, { kind: v as PhaseKind })}
              disabled={disabled}
            >
              <SelectTrigger className='h-6 flex-1 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='RampUp'>Ramp up</SelectItem>
                <SelectItem value='Hold'>Hold</SelectItem>
                <SelectItem value='RampDown'>Ramp down</SelectItem>
              </SelectContent>
            </Select>
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 text-destructive hover:text-destructive'
              onClick={() => remove(i)}
              disabled={disabled}
              aria-label='Remove phase'
            >
              <Trash2 className='h-3 w-3' />
            </Button>
          </div>

          {/* Bottom row: duration + concurrency inputs */}
          <div className='grid grid-cols-2 gap-2'>
            <div className='flex flex-col gap-1'>
              <Label className='text-[10px] uppercase tracking-wider text-muted-foreground'>
                Duration (s)
              </Label>
              <Input
                type='number'
                min={1}
                value={phase.durationSecs}
                onChange={(e) => update(i, { durationSecs: Number(e.target.value) })}
                disabled={disabled}
                className='h-6 text-xs'
                aria-label='Duration in seconds'
              />
            </div>
            <div className='flex flex-col gap-1'>
              <Label className='text-[10px] uppercase tracking-wider text-muted-foreground'>
                {phase.target.kind === 'rps' ? 'Rate' : 'Concurrency'}
              </Label>
              <div className='flex items-center gap-1'>
                <Input
                  type='number'
                  min={0}
                  value={phase.target.value}
                  onChange={(e) => {
                    const value = Number(e.target.value);
                    if (Number.isNaN(value)) return;
                    update(i, {
                      target:
                        phase.target.kind === 'rps'
                          ? { kind: 'rps', value }
                          : { kind: 'concurrency', value },
                    });
                  }}
                  disabled={disabled}
                  className='h-6 text-xs'
                  aria-label='Target value'
                />
                <span className='text-[10px] text-muted-foreground whitespace-nowrap'>
                  {phase.target.kind === 'rps' ? 'req/sec' : 'users'}
                </span>
              </div>
            </div>
          </div>
        </div>
      ))}

      <Button
        variant='ghost'
        size='sm'
        className='h-7 justify-start text-xs'
        onClick={addPhase}
        disabled={disabled}
      >
        <Plus className='mr-1 h-3.5 w-3.5' />
        Add phase
      </Button>
    </div>
  );
}
