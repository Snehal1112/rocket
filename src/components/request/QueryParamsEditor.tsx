import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

interface QueryParamsEditorProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function QueryParamsEditor({
  params,
  onChange,
  variableContext,
  onNavigateToSource,
}: QueryParamsEditorProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder='Param name'
      valuePlaceholder='Value'
      addLabel='Add Query Param'
      label='Query'
      variableContext={variableContext}
      onNavigateToSource={onNavigateToSource}
    />
  );
}
