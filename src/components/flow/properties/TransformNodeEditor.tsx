import { MonacoWrapper } from '@/components/editor/MonacoWrapper';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { WIRE_SCRIPT_TYPES, WIRE_SCRIPT_TYPES_PATH } from '../wire-script-types';
import { LabelField } from './LabelField';

type TransformKind = Extract<FlowNodeKind, { kind: 'Transform' }>;

// The script sees the same `response` object as a wire script.
const EXTRA_LIB = { content: WIRE_SCRIPT_TYPES, filePath: WIRE_SCRIPT_TYPES_PATH };

export function TransformNodeEditor({
  kind,
  onChange,
}: {
  kind: TransformKind;
  onChange: (kind: FlowNodeKind) => void;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Script</span>
        <p className='text-xs text-muted-foreground'>
          Write one expression, or several lines that end with return value. The upstream value is
          available as response. console.log output appears in Last run.
        </p>
        {/* biome-ignore lint/a11y/useSemanticElements: a fieldset is a form control group; div role=group labels the Monaco editor, which has no aria-label prop */}
        <div role='group' aria-label='Script' className='h-64 overflow-hidden rounded border'>
          <MonacoWrapper
            value={kind.script}
            onChange={(script) => onChange({ ...kind, script })}
            language='javascript'
            height='100%'
            extraLib={EXTRA_LIB}
          />
        </div>
        {kind.script.trim() === '' && (
          <p role='alert' className='text-xs text-amber-600 dark:text-amber-400'>
            The script is empty, so the flow cannot be saved.
          </p>
        )}
      </div>
    </div>
  );
}
