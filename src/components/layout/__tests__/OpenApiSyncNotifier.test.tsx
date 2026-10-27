import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { OpenApiSyncNotifier } from '../OpenApiSyncNotifier';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, onOpenApiSyncChecked: vi.fn(), onOpenApiSyncApplied: vi.fn() };
});

describe('OpenApiSyncNotifier', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.onOpenApiSyncChecked).mockResolvedValue(() => undefined);
    vi.mocked(tauriApi.onOpenApiSyncApplied).mockResolvedValue(() => undefined);
  });

  it('mounts_the_global_listeners_once', () => {
    const client = new QueryClient();
    const { container } = render(
      <QueryClientProvider client={client}>
        <OpenApiSyncNotifier />
      </QueryClientProvider>,
    );
    expect(container).toBeEmptyDOMElement();
    expect(tauriApi.onOpenApiSyncChecked).toHaveBeenCalledTimes(1);
    expect(tauriApi.onOpenApiSyncApplied).toHaveBeenCalledTimes(1);
  });
});
