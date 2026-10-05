import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { BodyState } from '@/types/pane-types';
import { BodyEditor } from '../BodyEditor';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: () => <div data-testid='monaco' />,
}));

const openDialog = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: (...a: unknown[]) => openDialog(...a) }));

vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn().mockResolvedValue([]),
  getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
  getGlobalEnvironment: vi.fn().mockResolvedValue(null),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
  getActiveWorkspace: vi.fn().mockResolvedValue({ id: 'w', name: 'W', path: '/home/me/ws' }),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: unknown) => unknown) =>
    selector({ activeEnvId: null, activeCollection: null }),
}));

function wrap(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(React.createElement(QueryClientProvider, { client: qc }, ui));
}

function makeBody(overrides: Partial<BodyState> = {}): BodyState {
  return {
    mode: 'none',
    content: '',
    formData: [],
    ...overrides,
  };
}

describe('BodyEditor', () => {
  it('renders KeyValueEditor for formurlencoded mode', () => {
    const body = makeBody({
      mode: 'formurlencoded',
      formData: [{ id: '1', key: 'username', value: 'alice', enabled: true }],
    });
    wrap(<BodyEditor body={body} onChange={vi.fn()} />);
    expect(screen.getByDisplayValue('username')).toBeInTheDocument();
  });

  it('does not render form editor for none mode', () => {
    const body = makeBody({ mode: 'none' });
    wrap(<BodyEditor body={body} onChange={vi.fn()} />);
    expect(screen.queryByDisplayValue('username')).not.toBeInTheDocument();
  });

  it('renders the code editor for sparql mode', async () => {
    const body = makeBody({ mode: 'sparql', content: 'SELECT * WHERE { ?s ?p ?o }' });
    wrap(<BodyEditor body={body} onChange={vi.fn()} />);
    expect(await screen.findByTestId('monaco')).toBeInTheDocument();
  });

  it('stores a workspace-relative path for a binary file inside the workspace', async () => {
    openDialog.mockResolvedValue('/home/me/ws/files/blob.bin');
    const onChange = vi.fn();
    wrap(<BodyEditor body={makeBody({ mode: 'binary' })} onChange={onChange} />);
    // Let the active workspace query resolve before picking.
    await new Promise((r) => setTimeout(r, 50));
    fireEvent.click(screen.getByRole('button', { name: /choose file/i }));
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({ filePath: 'files/blob.bin', fileName: 'blob.bin' }),
      ),
    );
  });

  it('rejects a binary file outside the workspace with a message', async () => {
    openDialog.mockResolvedValue('/home/me/other/blob.bin');
    const onChange = vi.fn();
    wrap(<BodyEditor body={makeBody({ mode: 'binary' })} onChange={onChange} />);
    await new Promise((r) => setTimeout(r, 50));
    fireEvent.click(screen.getByRole('button', { name: /choose file/i }));
    expect(await screen.findByText('Files must be inside the workspace folder')).toBeVisible();
    expect(onChange).not.toHaveBeenCalled();
  });
});
