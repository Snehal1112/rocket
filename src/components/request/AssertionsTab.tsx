import { Plus, Trash2 } from 'lucide-react';
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
import { SingleLineEditor } from '@/components/editor';
import type { AssertionEntry } from '@/lib/tauri-api';

interface AssertionsTabProps {
  assertions: AssertionEntry[];
  onChange: (assertions: AssertionEntry[]) => void;
}

const UNARY_OPERATORS = new Set([
  'isString',
  'isNumber',
  'isBoolean',
  'isArray',
  'isNull',
  'isDefined',
  'isUndefined',
  'isEmpty',
  'isNotEmpty',
  'isTruthy',
  'isFalsy',
  'isJson',
]);

const OPERATOR_GROUPS = [
  {
    label: 'Equality',
    operators: [
      { value: 'eq', label: 'eq (equals)' },
      { value: 'neq', label: 'neq (not equals)' },
    ],
  },
  {
    label: 'Numeric',
    operators: [
      { value: 'gt', label: 'gt (greater than)' },
      { value: 'gte', label: 'gte (≥)' },
      { value: 'lt', label: 'lt (less than)' },
      { value: 'lte', label: 'lte (≤)' },
    ],
  },
  {
    label: 'String',
    operators: [
      { value: 'contains', label: 'contains' },
      { value: 'notContains', label: 'notContains' },
      { value: 'startsWith', label: 'startsWith' },
      { value: 'endsWith', label: 'endsWith' },
      { value: 'matches', label: 'matches (regex)' },
      { value: 'notMatches', label: 'notMatches (regex)' },
    ],
  },
  {
    label: 'Type',
    operators: [
      { value: 'isString', label: 'isString' },
      { value: 'isNumber', label: 'isNumber' },
      { value: 'isBoolean', label: 'isBoolean' },
      { value: 'isArray', label: 'isArray' },
      { value: 'isNull', label: 'isNull' },
      { value: 'isDefined', label: 'isDefined' },
      { value: 'isUndefined', label: 'isUndefined' },
    ],
  },
  {
    label: 'Value',
    operators: [
      { value: 'isEmpty', label: 'isEmpty' },
      { value: 'isNotEmpty', label: 'isNotEmpty' },
      { value: 'isTruthy', label: 'isTruthy' },
      { value: 'isFalsy', label: 'isFalsy' },
      { value: 'isJson', label: 'isJson' },
    ],
  },
  {
    label: 'Set',
    operators: [
      { value: 'in', label: 'in (JSON array)' },
      { value: 'notIn', label: 'notIn (JSON array)' },
    ],
  },
  {
    label: 'Range & Length',
    operators: [
      { value: 'between', label: 'between (min,max)' },
      { value: 'length', label: 'length' },
    ],
  },
];

export function AssertionsTab({ assertions, onChange }: AssertionsTabProps) {
  function update(index: number, patch: Partial<AssertionEntry>) {
    const next = assertions.map((a, i) => (i === index ? { ...a, ...patch } : a));
    onChange(next);
  }

  function remove(index: number) {
    onChange(assertions.filter((_, i) => i !== index));
  }

  function add() {
    onChange([...assertions, { expression: '', operator: 'eq', value: '' }]);
  }

  if (assertions.length === 0) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground'>
          <p className='text-sm'>No assertions yet.</p>
          <Button size='sm' variant='outline' onClick={add}>
            <Plus className='mr-1.5 h-3.5 w-3.5' />
            Add Assertion
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full overflow-hidden'>
      <div className='flex-1 overflow-auto'>
        <table className='w-full text-sm border-collapse'>
          <thead>
            <tr className='border-b bg-muted/40'>
              <th className='w-10 px-2 py-1.5 text-left font-medium text-muted-foreground'>On</th>
              <th className='px-2 py-1.5 text-left font-medium text-muted-foreground'>
                Expression
              </th>
              <th className='w-44 px-2 py-1.5 text-left font-medium text-muted-foreground'>
                Operator
              </th>
              <th className='px-2 py-1.5 text-left font-medium text-muted-foreground'>Value</th>
              <th className='w-8' />
            </tr>
          </thead>
          <tbody>
            {assertions.map((assertion, i) => {
              const isUnary = UNARY_OPERATORS.has(assertion.operator);
              return (
                <tr key={i} className='border-b hover:bg-muted/20'>
                  <td className='px-2 py-1'>
                    <Switch
                      checked={!assertion.disabled}
                      onCheckedChange={(checked) => update(i, { disabled: !checked })}
                      className='scale-75'
                    />
                  </td>
                  <td className='px-1 py-1'>
                    <SingleLineEditor
                      value={assertion.expression}
                      onChange={(v) => update(i, { expression: v })}
                      placeholder='res.status'
                      className='h-7 text-xs'
                    />
                  </td>
                  <td className='px-1 py-1'>
                    <Select
                      value={assertion.operator}
                      onValueChange={(v) =>
                        update(i, {
                          operator: v,
                          value: isUnary && !UNARY_OPERATORS.has(v) ? '' : assertion.value,
                        })
                      }
                    >
                      <SelectTrigger className='h-7 text-xs'>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {OPERATOR_GROUPS.map((group) => (
                          <SelectGroup key={group.label}>
                            <SelectLabel className='text-xs'>{group.label}</SelectLabel>
                            {group.operators.map((op) => (
                              <SelectItem key={op.value} value={op.value} className='text-xs'>
                                {op.label}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        ))}
                      </SelectContent>
                    </Select>
                  </td>
                  <td className='px-1 py-1'>
                    {isUnary ? (
                      <span className='text-xs text-muted-foreground italic px-2'>—</span>
                    ) : (
                      <SingleLineEditor
                        value={assertion.value ?? ''}
                        onChange={(v) => update(i, { value: v })}
                        placeholder='expected value'
                        className='h-7 text-xs'
                      />
                    )}
                  </td>
                  <td className='px-1 py-1'>
                    <Button
                      size='icon'
                      variant='ghost'
                      className='h-6 w-6'
                      onClick={() => remove(i)}
                    >
                      <Trash2 className='h-3.5 w-3.5 text-muted-foreground' />
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className='shrink-0 border-t px-3 py-2'>
        <Button size='sm' variant='ghost' onClick={add}>
          <Plus className='mr-1.5 h-3.5 w-3.5' />
          Add Assertion
        </Button>
      </div>
    </div>
  );
}
