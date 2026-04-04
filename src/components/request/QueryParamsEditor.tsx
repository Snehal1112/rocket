import type { KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

interface QueryParamsEditorProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function QueryParamsEditor({ params, onChange }: QueryParamsEditorProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder='Param name'
      valuePlaceholder='Value'
      addLabel='Add Query Param'
      label='Query'
    />
  );
}
