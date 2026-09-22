// src/components/environments/EnvironmentDialog.test.tsx

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
import type { Environment } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listEnvironments: vi.fn(),
    saveEnvironment: vi.fn(),
    deleteEnvironment: vi.fn(),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getGlobalEnvironment: vi.fn().mockResolvedValue(null),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
  };
});

const prodEnv: Environment = {
  name: 'prod',
  variables: [{ key: 'HOST', value: 'https://api.example.com', enabled: true, secret: false }],
  externalSecrets: [],
};

function renderDialog() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  useEnvStore.setState({ activeCollection: 'my-collection', activeEnvId: null });
  return render(
    <QueryClientProvider client={queryClient}>
      <EnvironmentDialog open onOpenChange={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe('EnvironmentDialog tab switcher', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listEnvironments).mockResolvedValue([prodEnv]);
  });

  it('defaults to the Variables tab and shows the variable table', async () => {
    renderDialog();
    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /fetch secrets/i })).not.toBeInTheDocument();
  });

  it('switches to External Secrets and hides the variable table', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));

    expect(screen.queryByLabelText('Variable key 1')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /add binding/i })).toBeInTheDocument();
  });

  it('switches back to Variables and shows the variable table again', async () => {
    renderDialog();
    await screen.findByLabelText('Variable key 1');
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: /external secrets/i }));
    await screen.findByRole('button', { name: /add binding/i });
    await user.click(screen.getByRole('tab', { name: /^variables$/i }));

    expect(await screen.findByLabelText('Variable key 1')).toBeInTheDocument();
  });
});
