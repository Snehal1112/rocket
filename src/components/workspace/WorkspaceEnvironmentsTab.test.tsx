// src/components/workspace/WorkspaceEnvironmentsTab.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { WorkspaceEnvironmentsTab } from '@/components/workspace/WorkspaceEnvironmentsTab';
import type { Environment } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listGlobalEnvironments: vi.fn(),
    saveGlobalEnvironment: vi.fn().mockResolvedValue(undefined),
    deleteGlobalEnvironment: vi.fn(),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
    getResolvedEnvironment: vi.fn(),
  };
});

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), warning: vi.fn() } }));
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op in jsdom.
  };
}

const env = (name: string, extra: Partial<Environment> = {}): Environment => ({
  name,
  variables: [],
  externalSecrets: [],
  ...extra,
});

function renderTab() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <WorkspaceEnvironmentsTab />
    </QueryClientProvider>,
  );
}

describe('WorkspaceEnvironmentsTab inheritance', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.getResolvedEnvironment).mockResolvedValue({
      name: '',
      variables: [],
      externalSecrets: [],
      clientCertificates: [],
      warnings: [],
    });
  });

  it('global_environments_offer_the_picker_and_the_path_field', async () => {
    vi.mocked(tauriApi.listGlobalEnvironments).mockResolvedValue([env('base'), env('child')]);
    renderTab();
    expect(await screen.findByRole('combobox', { name: 'Inherits from' })).toBeInTheDocument();
    expect(screen.getByLabelText('.env file path')).toBeInTheDocument();
  });

  it('a_global_environment_lists_only_global_environments_as_candidates', async () => {
    vi.mocked(tauriApi.listGlobalEnvironments).mockResolvedValue([
      env('a'),
      env('b', { extends: 'a' }),
      env('c'),
    ]);
    renderTab();
    const user = userEvent.setup();
    // "a" is selected first, so its descendant "b" and itself are excluded.
    await user.click(await screen.findByRole('combobox', { name: 'Inherits from' }));
    expect(await screen.findByRole('option', { name: 'c' })).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: 'b' })).not.toBeInTheDocument();
    expect(screen.queryByRole('option', { name: 'a' })).not.toBeInTheDocument();
  });

  it('global_inherited_rows_show_their_source', async () => {
    vi.mocked(tauriApi.listGlobalEnvironments).mockResolvedValue([
      env('child', { extends: 'base' }),
      env('base'),
    ]);
    vi.mocked(tauriApi.getResolvedEnvironment).mockResolvedValue({
      name: 'base',
      variables: [{ key: 'HOST', value: 'x', enabled: true, secret: false, source: 'base' }],
      externalSecrets: [],
      clientCertificates: [],
      warnings: [],
    });
    renderTab();
    expect(await screen.findByLabelText('Inherited key 1')).toHaveValue('HOST');
    expect(screen.getByTitle('From base')).toBeInTheDocument();
  });
});
