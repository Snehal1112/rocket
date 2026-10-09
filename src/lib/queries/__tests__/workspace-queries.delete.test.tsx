import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadPromptHistory, savePromptHistory } from '@/lib/assistant/prompt-history';
import { deleteWorkspace } from '@/lib/tauri-api';
import { useDeleteWorkspace } from '../workspace-queries';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  deleteWorkspace: vi.fn(),
}));

beforeEach(() => {
  localStorage.clear();
  vi.mocked(deleteWorkspace).mockReset().mockResolvedValue(undefined);
});

describe('useDeleteWorkspace', () => {
  it('forgets the assistant prompt history of the deleted workspace only', async () => {
    savePromptHistory('w1', ['one']);
    savePromptHistory('w2', ['two']);
    const client = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useDeleteWorkspace(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync('w1');
    });
    expect(loadPromptHistory('w1')).toEqual([]);
    expect(loadPromptHistory('w2')).toEqual(['two']);
  });
});
