import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { SingleLineEditor } from '../SingleLineEditor';

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: unknown) => unknown) =>
    selector({
      activeEnvId: null,
      activeCollection: null,
    }),
}));

vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn().mockResolvedValue([]),
  getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
  getGlobalEnvironment: vi.fn().mockResolvedValue(null),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
}));

function makeContext(
  entries: Record<string, Pick<VariableScopeEntry, 'source' | 'value'>>,
): Map<string, VariableScopeEntry> {
  const map = new Map<string, VariableScopeEntry>();
  for (const [key, val] of Object.entries(entries)) {
    map.set(key, { value: val.value, source: val.source, label: val.source, secret: false });
  }
  return map;
}

function wrap(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(React.createElement(QueryClientProvider, { client: qc }, ui));
}

describe('SingleLineEditor', () => {
  it('renders without crashing', () => {
    const { container } = wrap(<SingleLineEditor value='hello' onChange={vi.fn()} />);
    expect(container.querySelector('.cm-editor')).not.toBeNull();
  });

  it('displays the initial value', () => {
    const { container } = wrap(<SingleLineEditor value='test content' onChange={vi.fn()} />);
    const content = container.querySelector('.cm-content');
    expect(content?.textContent).toContain('test content');
  });

  it('renders placeholder when value is empty', () => {
    const { container } = wrap(
      <SingleLineEditor value='' onChange={vi.fn()} placeholder='Enter URL' />,
    );
    const ph = container.querySelector('.cm-placeholder');
    expect(ph?.textContent).toBe('Enter URL');
  });

  it('applies variable highlighting when context is provided', () => {
    const ctx = makeContext({
      baseUrl: { source: 'environment', value: 'https://api.example.com' },
    });
    const { container } = wrap(
      <SingleLineEditor value='https://{{baseUrl}}/api' onChange={vi.fn()} variableContext={ctx} />,
    );
    const envVars = container.querySelectorAll('.cm-var-environment');
    expect(envVars.length).toBe(1);
  });

  it('applies unresolved class for unknown variables', () => {
    const { container } = wrap(
      <SingleLineEditor value='{{unknown}}' onChange={vi.fn()} variableContext={new Map()} />,
    );
    const unresolved = container.querySelectorAll('.cm-var-unresolved');
    expect(unresolved.length).toBe(1);
  });

  it('does not render variable extensions when context is undefined', () => {
    const { container } = wrap(<SingleLineEditor value='{{someVar}}' onChange={vi.fn()} />);
    expect(container.querySelectorAll('.cm-var').length).toBe(0);
  });
});
