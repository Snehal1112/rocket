import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { CollectionVariable, FolderSettings } from '@/lib/tauri-api';
import { VarsSection } from '../VarsSection';

const token: CollectionVariable = {
  key: 'token',
  value: 'abc',
  initialValue: 'init',
  enabled: true,
  secret: true,
};
const host: CollectionVariable = {
  key: 'host',
  value: 'localhost',
  initialValue: '',
  enabled: false,
  secret: false,
};

function renderSection(variables: CollectionVariable[]) {
  const onChange = vi.fn();
  const settings: FolderSettings = { headers: [], variables, docs: 'keep me' };
  render(
    <VarsSection collectionName='col' folderPath='a/b' settings={settings} onChange={onChange} />,
  );
  return onChange;
}

describe('VarsSection', () => {
  it('shows the Pre Request heading with its description', () => {
    renderSection([]);
    expect(screen.getByText('Pre Request')).toBeInTheDocument();
    expect(screen.getByText(/resolved before each request/i)).toBeInTheDocument();
    expect(screen.queryByText(/Post Response/i)).not.toBeInTheDocument();
  });

  it('shows the empty state and adds a variable', async () => {
    const onChange = renderSection([]);
    await userEvent.click(screen.getByRole('button', { name: /Add Variable/ }));
    expect(onChange).toHaveBeenCalledWith({
      variables: [{ key: '', value: '', initialValue: '', enabled: true, secret: false }],
    });
  });

  it('renders one row per variable', () => {
    renderSection([token, host]);
    expect(screen.getByLabelText('Variable name, row 1')).toHaveValue('token');
    expect(screen.getByLabelText('Variable name, row 2')).toHaveValue('host');
  });

  it('keeps the secret flag when another field is edited', () => {
    const onChange = renderSection([token, host]);
    fireEvent.change(screen.getByLabelText('Variable name, row 1'), {
      target: { value: 'token2' },
    });
    expect(onChange).toHaveBeenCalledWith({
      variables: [{ ...token, key: 'token2' }, host],
    });
  });

  it('toggles the secret flag without touching the other fields', async () => {
    const onChange = renderSection([host]);
    await userEvent.click(screen.getByTitle('Hide value (mark as secret)'));
    expect(onChange).toHaveBeenCalledWith({ variables: [{ ...host, secret: true }] });
  });

  it('patches only variables', () => {
    const onChange = renderSection([token]);
    fireEvent.change(screen.getByLabelText('Current value, row 1'), { target: { value: 'xyz' } });
    const patch = onChange.mock.calls[0][0] as Record<string, unknown>;
    expect(Object.keys(patch)).toEqual(['variables']);
  });

  it('removes a variable', async () => {
    const onChange = renderSection([token, host]);
    await userEvent.click(screen.getByRole('button', { name: 'Delete variable 1' }));
    expect(onChange).toHaveBeenCalledWith({ variables: [host] });
  });
});
