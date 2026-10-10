import type { Ref } from 'react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { runRecordLabel } from '@/lib/flow-run-history';
import type { FlowRunRecord } from '@/types/pane-types';

interface RunHistorySelectProps {
  // Newest first.
  history: FlowRunRecord[];
  // The shown past run, or null for the live results.
  viewedRunId: string | null;
  // True while a run is active, because the live results are still being written.
  disabled: boolean;
  // Receives a past run's id, or null when the newest run is chosen.
  onChange: (runId: string | null) => void;
  // Receives the trigger element, so the caller can move focus to it.
  triggerRef?: Ref<HTMLButtonElement>;
}

// Lists the recorded runs. The newest entry is the live one, so choosing it
// reports null instead of its id.
export function RunHistorySelect({
  history,
  viewedRunId,
  disabled,
  onChange,
  triggerRef,
}: RunHistorySelectProps) {
  if (history.length < 2) return null;
  const latestId = history[0].runId;
  const value =
    viewedRunId !== null && history.some((r) => r.runId === viewedRunId) ? viewedRunId : latestId;
  return (
    <Select
      value={value}
      disabled={disabled}
      onValueChange={(runId) => onChange(runId === latestId ? null : runId)}
    >
      <SelectTrigger ref={triggerRef} className='nokey h-8 w-48 text-xs' aria-label='Run history'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {history.map((record, index) => (
          <SelectItem key={record.runId} value={record.runId} className='text-xs'>
            {index === 0 ? `Latest · ${runRecordLabel(record)}` : runRecordLabel(record)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
