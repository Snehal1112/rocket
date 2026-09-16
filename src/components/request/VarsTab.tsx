import { FlaskConical, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { type ActionEntry, evaluateVarExpression } from '@/lib/tauri-api';

interface VarsTabProps {
  actions: ActionEntry[];
  onChange: (actions: ActionEntry[]) => void;
  /** Needed by the "Test" preview button — collection-scope vars only. */
  collectionRoot: string | undefined;
  /** The tab's last response, sent as jsonq context. Test is disabled without one. */
  responseJson: string | undefined;
}

const SCOPE_GROUPS = [
  {
    label: 'Ephemeral',
    scopes: [{ value: 'runtime', label: 'runtime (memory only, not persisted)' }],
  },
  {
    label: 'Persisted',
    scopes: [
      { value: 'request', label: 'request' },
      { value: 'folder', label: 'folder' },
      { value: 'collection', label: 'collection' },
      { value: 'environment', label: 'environment' },
    ],
  },
];

/** Drops the entry at `removedIndex` and shifts every later index down by one. */
function reindexAfterRemove(
  record: Record<number, string>,
  removedIndex: number,
): Record<number, string> {
  const next: Record<number, string> = {};
  for (const [key, value] of Object.entries(record)) {
    const i = Number(key);
    if (i < removedIndex) {
      next[i] = value;
    } else if (i > removedIndex) {
      next[i - 1] = value;
    }
  }
  return next;
}

function newAction(): ActionEntry {
  return {
    phase: 'after-response',
    selector: { expression: '', method: 'jsonq' },
    variable: { name: '', scope: 'runtime' },
  };
}

export function VarsTab({ actions, onChange, collectionRoot, responseJson }: VarsTabProps) {
  const [testResults, setTestResults] = useState<Record<number, string>>({});
  const [testErrors, setTestErrors] = useState<Record<number, string>>({});

  function update(index: number, patch: Partial<ActionEntry>) {
    const next = actions.map((a, i) => (i === index ? { ...a, ...patch } : a));
    onChange(next);
  }

  function updateSelector(index: number, expression: string) {
    update(index, { selector: { ...actions[index].selector, expression } });
  }

  function updateVariable(index: number, patch: Partial<ActionEntry['variable']>) {
    update(index, { variable: { ...actions[index].variable, ...patch } });
  }

  function remove(index: number) {
    onChange(actions.filter((_, i) => i !== index));
    setTestResults((prev) => reindexAfterRemove(prev, index));
    setTestErrors((prev) => reindexAfterRemove(prev, index));
  }

  function add() {
    onChange([...actions, newAction()]);
  }

  async function runTest(index: number) {
    if (!collectionRoot || !responseJson) return;
    setTestErrors((prev) => ({ ...prev, [index]: '' }));
    try {
      const result = await evaluateVarExpression(
        collectionRoot,
        actions[index].selector.expression,
        responseJson,
      );
      setTestResults((prev) => ({ ...prev, [index]: JSON.stringify(result) }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setTestErrors((prev) => ({ ...prev, [index]: msg }));
    }
  }

  const canTest = Boolean(collectionRoot && responseJson);

  if (actions.length === 0) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground'>
          <p className='text-sm'>No post-response variables yet.</p>
          <Button size='sm' variant='outline' onClick={add}>
            <Plus className='mr-1.5 h-3.5 w-3.5' />
            Add Variable
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full overflow-hidden'>
      {/* Header row — mirrors column proportions of the data rows below. */}
      <div className='flex items-center gap-1 border-b bg-muted/40 px-2 py-1.5 shrink-0 text-xs font-medium text-muted-foreground'>
        <span className='w-9 shrink-0'>On</span>
        <span className='flex-1 min-w-0'>Expression (jsonq)</span>
        <span className='w-36 shrink-0'>Variable name</span>
        <span className='w-40 shrink-0'>Scope</span>
        <span className='w-7 shrink-0' />
        <span className='w-7 shrink-0' />
      </div>

      <div className='flex-1 overflow-y-auto overflow-x-hidden'>
        {actions.map((action, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: actions have no stable id; index reflects insertion order
          <div key={i}>
            <div className='flex items-center gap-1 border-b px-2 py-1 hover:bg-muted/20'>
              <div className='w-9 shrink-0 flex items-center'>
                <Switch
                  checked={!action.disabled}
                  onCheckedChange={(checked) => update(i, { disabled: !checked })}
                  className='scale-75'
                />
              </div>

              <div className='flex-1 min-w-0'>
                <SingleLineEditor
                  value={action.selector.expression}
                  onChange={(v) => updateSelector(i, v)}
                  placeholder='res.body.token'
                  className='h-7 text-xs'
                />
              </div>

              <div className='w-36 shrink-0'>
                <SingleLineEditor
                  value={action.variable.name}
                  onChange={(v) => updateVariable(i, { name: v })}
                  placeholder='authToken'
                  className='h-7 text-xs'
                />
              </div>

              <div className='w-40 shrink-0'>
                <Select
                  value={action.variable.scope}
                  onValueChange={(v) =>
                    updateVariable(i, { scope: v as ActionEntry['variable']['scope'] })
                  }
                >
                  <SelectTrigger className='h-7 text-xs w-full'>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SCOPE_GROUPS.map((group) => (
                      <SelectGroup key={group.label}>
                        <SelectLabel className='text-xs'>{group.label}</SelectLabel>
                        {group.scopes.map((s) => (
                          <SelectItem key={s.value} value={s.value} className='text-xs'>
                            {s.label}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className='w-7 shrink-0 flex items-center justify-center'>
                <Button
                  size='icon'
                  variant='ghost'
                  className='h-6 w-6'
                  disabled={!canTest}
                  title={canTest ? 'Test against last response' : 'Send the request first'}
                  onClick={() => runTest(i)}
                >
                  <FlaskConical className='h-3.5 w-3.5 text-muted-foreground' />
                </Button>
              </div>

              <div className='w-7 shrink-0 flex items-center justify-center'>
                <Button size='icon' variant='ghost' className='h-6 w-6' onClick={() => remove(i)}>
                  <Trash2 className='h-3.5 w-3.5 text-muted-foreground' />
                </Button>
              </div>
            </div>
            {(testResults[i] || testErrors[i]) && (
              <div className='px-2 py-1 border-b bg-muted/10 text-xs'>
                {testErrors[i] ? (
                  <span className='text-destructive'>{testErrors[i]}</span>
                ) : (
                  <span className='text-muted-foreground'>
                    Result: <code>{testResults[i]}</code>
                  </span>
                )}
              </div>
            )}
          </div>
        ))}
      </div>

      <div className='shrink-0 border-t px-3 py-2'>
        <Button size='sm' variant='ghost' onClick={add}>
          <Plus className='mr-1.5 h-3.5 w-3.5' />
          Add Variable
        </Button>
      </div>
    </div>
  );
}
