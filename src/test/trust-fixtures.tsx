import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type RenderResult, render } from '@testing-library/react';
import type { ReactElement } from 'react';
import type { CapabilityState, CollectionTrust } from '@/lib/tauri-api';

const off: CapabilityState = { requested: false, granted: false, effective: false };

/** A trust state with nothing requested and nothing allowed, plus overrides. */
export function makeTrust(overrides: Partial<CollectionTrust> = {}): CollectionTrust {
  return {
    developerMode: off,
    contextRoots: { requested: [], granted: [], effective: [], pending: [] },
    agentRun: off,
    processEnv: { granted: false },
    pending: false,
    fingerprint: 'fp-1',
    storeError: null,
    ...overrides,
  };
}

export const allowed: CapabilityState = { requested: true, granted: true, effective: true };
export const requestedOnly: CapabilityState = {
  requested: true,
  granted: false,
  effective: false,
};

/** Renders with a fresh query client, as the app root does. */
export function renderWithQuery(ui: ReactElement): RenderResult {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}
