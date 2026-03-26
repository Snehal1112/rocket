import { KeyValueEditor } from './KeyValueEditor';
import type { KeyValueEntry } from '@/types/pane-types';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function PathParamsPanel({ params, onChange }: PathParamsPanelProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder="Path key (e.g. customerId)"
      valuePlaceholder="Value"
      addLabel="Add Path Param"
      label="Path Params"
    />
  );
}
