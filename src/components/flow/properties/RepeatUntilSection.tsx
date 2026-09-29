import { useEffect, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { RepeatUntil } from '@/lib/tauri-api';

// Returns the number typed into a field, or null while it is empty, not a
// number, or not above zero.
function parsed(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isFinite(n) && n > 0 ? n : null;
}

// A number field that keeps its own draft text while typing. It commits only
// a valid value and restores the stored value on blur or an outside change.
function NumberField({
  id,
  label,
  stored,
  shown,
  min,
  max,
  step,
  toStored,
  onCommit,
}: {
  id: string;
  label: string;
  stored: number;
  shown: number;
  min: number;
  max: number;
  step: number;
  toStored: (typed: number) => number | null;
  onCommit: (stored: number) => void;
}) {
  const [draft, setDraft] = useState(String(shown));

  // Follow the stored value when it changes from outside the field.
  useEffect(() => {
    setDraft((current) => {
      const typed = parsed(current);
      return typed !== null && toStored(typed) === stored ? current : String(shown);
    });
  }, [stored, shown, toStored]);

  return (
    <div className='space-y-1'>
      <Label htmlFor={id} className='text-xs'>
        {label}
      </Label>
      <Input
        id={id}
        type='number'
        min={min}
        max={max}
        step={step}
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          const typed = parsed(e.target.value);
          const stored = typed === null ? null : toStored(typed);
          if (stored !== null) onCommit(stored);
        }}
        onBlur={() => setDraft(String(shown))}
        className='h-8 text-xs'
      />
    </div>
  );
}

// Whole milliseconds from typed seconds, or null when that rounds to zero.
const secondsToMs = (seconds: number) => {
  const ms = Math.round(seconds * 1000);
  return ms >= 1 ? ms : null;
};
// Whole attempts from a typed count, or null when that rounds to zero.
const wholeAttempts = (n: number) => {
  const whole = Math.round(n);
  return whole >= 1 ? whole : null;
};

export function RepeatUntilSection({
  value,
  onChange,
}: {
  value: RepeatUntil | null;
  onChange: (value: RepeatUntil | null) => void;
}) {
  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between gap-2'>
        <Label htmlFor='request-repeat-until'>Repeat until</Label>
        <Switch
          id='request-repeat-until'
          checked={value !== null}
          onCheckedChange={(checked) => onChange(checked ? { ...DEFAULT_REPEAT_UNTIL } : null)}
        />
      </div>
      <p className='text-xs text-muted-foreground'>
        Sends the request again until the condition is true. The node fails if the condition is
        still false after the last attempt or the timeout.
      </p>
      {value !== null && (
        <div className='space-y-2'>
          <div className='space-y-1'>
            <span className='text-xs font-medium'>Condition</span>
            <SingleLineEditor
              aria-label='Repeat condition'
              value={value.condition}
              onChange={(condition) => onChange({ ...value, condition })}
              placeholder='response.status === 200'
              className='text-xs'
            />
          </div>
          <div className='grid grid-cols-3 gap-2'>
            <NumberField
              id='repeat-interval'
              label='Interval (s)'
              stored={value.intervalMs}
              shown={value.intervalMs / 1000}
              min={0.1}
              max={3600}
              step={0.1}
              toStored={secondsToMs}
              onCommit={(intervalMs) => onChange({ ...value, intervalMs })}
            />
            <NumberField
              id='repeat-max-attempts'
              label='Max attempts'
              stored={value.maxAttempts}
              shown={value.maxAttempts}
              min={1}
              max={1000}
              step={1}
              toStored={wholeAttempts}
              onCommit={(maxAttempts) => onChange({ ...value, maxAttempts })}
            />
            <NumberField
              id='repeat-timeout'
              label='Timeout (s)'
              stored={value.timeoutMs}
              shown={value.timeoutMs / 1000}
              min={1}
              max={3600}
              step={1}
              toStored={secondsToMs}
              onCommit={(timeoutMs) => onChange({ ...value, timeoutMs })}
            />
          </div>
        </div>
      )}
    </div>
  );
}
