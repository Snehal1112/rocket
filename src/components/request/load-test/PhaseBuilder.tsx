import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { LoadTestPhase, PhaseKind } from '@/lib/tauri-api';

interface Props {
  phases: LoadTestPhase[];
  onChange: (phases: LoadTestPhase[]) => void;
}

const KIND_COLORS: Record<PhaseKind, string> = {
  RampUp: '#7F77DD',
  Hold: '#1D9E75',
  RampDown: '#E24B4A',
};

const KIND_LABELS: Record<PhaseKind, string> = {
  RampUp: 'Ramp up',
  Hold: 'Hold',
  RampDown: 'Ramp down',
};

export function PhaseBuilder({ phases, onChange }: Props) {
  const update = (index: number, patch: Partial<LoadTestPhase>) => {
    onChange(phases.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };

  const remove = (index: number) => {
    onChange(phases.filter((_, i) => i !== index));
  };

  const moveUp = (index: number) => {
    if (index === 0) return;
    const next = phases.slice();
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    onChange(next);
  };

  const moveDown = (index: number) => {
    if (index === phases.length - 1) return;
    const next = phases.slice();
    [next[index], next[index + 1]] = [next[index + 1], next[index]];
    onChange(next);
  };

  const addPhase = () => {
    onChange([...phases, { kind: 'Hold', durationSecs: 30, targetConcurrency: 10 }]);
  };

  return (
    <div className='flex flex-col gap-2'>
      {phases.map((phase, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: phases are ordered and edited in place.
        <div key={i} className='flex items-center gap-1.5'>
          <span
            className='h-2 w-2 shrink-0 rounded-full'
            style={{ backgroundColor: KIND_COLORS[phase.kind] }}
          />
          <Select value={phase.kind} onValueChange={(v) => update(i, { kind: v as PhaseKind })}>
            <SelectTrigger className='h-7 w-[110px] text-xs'>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value='RampUp'>Ramp up</SelectItem>
              <SelectItem value='Hold'>Hold</SelectItem>
              <SelectItem value='RampDown'>Ramp down</SelectItem>
            </SelectContent>
          </Select>
          <Input
            type='number'
            min={1}
            value={phase.durationSecs}
            onChange={(e) => update(i, { durationSecs: Number(e.target.value) })}
            className='h-7 w-14 text-xs'
            aria-label={`${KIND_LABELS[phase.kind]} duration seconds`}
          />
          <span className='text-xs text-muted-foreground'>s @</span>
          <Input
            type='number'
            min={0}
            value={phase.targetConcurrency}
            onChange={(e) => update(i, { targetConcurrency: Number(e.target.value) })}
            className='h-7 w-14 text-xs'
            aria-label={`${KIND_LABELS[phase.kind]} target concurrency`}
          />
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => moveUp(i)}
            disabled={i === 0}
            aria-label='Move phase up'
          >
            <ArrowUp className='h-3.5 w-3.5' />
          </Button>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => moveDown(i)}
            disabled={i === phases.length - 1}
            aria-label='Move phase down'
          >
            <ArrowDown className='h-3.5 w-3.5' />
          </Button>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7 text-destructive'
            onClick={() => remove(i)}
            aria-label='Remove phase'
          >
            <Trash2 className='h-3.5 w-3.5' />
          </Button>
        </div>
      ))}
      <Button variant='ghost' size='sm' className='h-7 justify-start text-xs' onClick={addPhase}>
        <Plus className='mr-1 h-3.5 w-3.5' />
        Add phase
      </Button>
    </div>
  );
}
