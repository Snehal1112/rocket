import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { BodyState } from '@/types/pane-types';
import { BodyEditor } from '../BodyEditor';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: () => <div data-testid='monaco' />,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn().mockResolvedValue([]),
  getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
  getGlobalEnvironment: vi.fn().mockResolvedValue(null),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
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
});
