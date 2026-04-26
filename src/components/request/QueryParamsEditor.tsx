import { SlidersHorizontal } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
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
    <Card>
      <CardContent className='p-4 space-y-3'>
        <div className='flex items-center gap-2 mb-1'>
          <SlidersHorizontal className='h-3.5 w-3.5 text-muted-foreground' />
          <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
            Query
          </span>
        </div>
        <KeyValueEditor
          entries={params}
          onChange={onChange}
          keyPlaceholder='Param name'
          valuePlaceholder='Value'
          addLabel='Add Query Param'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      </CardContent>
    </Card>
  );
}
