// src/components/environments/VariableTable.test.tsx

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { VariableTable } from '@/components/environments/VariableTable';
import type { ResolvedVariable, Variable } from '@/lib/tauri-api';

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({ value }: { value: string }) => <input readOnly value={value} />,
}));

const own: Variable[] = [{ key: 'HOST', value: 'child.test', enabled: true, secret: false }];

function renderTable(inheritedRows?: ResolvedVariable[]) {
  return render(
    <VariableTable
      variables={own}
      onChange={vi.fn()}
      onAdd={vi.fn()}
      onRemove={vi.fn()}
      onSave={vi.fn()}
      isDirty={false}
      saveState='idle'
      inheritedRows={inheritedRows}
    />,
  );
}

describe('VariableTable inherited rows', () => {
  it('inherited_rows_show_the_source_environment_name', () => {
    renderTable([{ key: 'TOKEN', value: 'abc', enabled: true, secret: false, source: 'base' }]);
    expect(screen.getByText('base')).toBeInTheDocument();
    expect(screen.getByLabelText('Inherited key 1')).toHaveValue('TOKEN');
    expect(screen.getByLabelText('Inherited value 1')).toHaveValue('abc');
  });

  it('inherited_rows_are_read_only', () => {
    renderTable([{ key: 'TOKEN', value: 'abc', enabled: true, secret: false, source: 'base' }]);
    expect(screen.getByLabelText('Inherited key 1')).toHaveAttribute('readonly');
    expect(screen.getByLabelText('Inherited value 1')).toHaveAttribute('readonly');
  });

  it('an_inherited_secret_row_is_masked', () => {
    renderTable([{ key: 'PW', value: 'hunter2', enabled: true, secret: true, source: 'base' }]);
    expect(screen.getByLabelText('Inherited value 1')).not.toHaveValue('hunter2');
    expect(screen.queryByDisplayValue('hunter2')).not.toBeInTheDocument();
  });

  it('renders no inherited section without rows', () => {
    renderTable([]);
    expect(screen.queryByText(/inherited/i)).not.toBeInTheDocument();
  });
});
