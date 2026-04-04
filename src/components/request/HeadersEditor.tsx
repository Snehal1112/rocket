import type { KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

interface HeadersEditorProps {
  headers: KeyValueEntry[];
  onChange: (headers: KeyValueEntry[]) => void;
}

export function HeadersEditor({ headers, onChange }: HeadersEditorProps) {
  return (
    <KeyValueEditor
      entries={headers}
      onChange={onChange}
      keyPlaceholder='Header name'
      valuePlaceholder='Value'
      addLabel='Add Header'
    />
  );
}
