import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { useSettingsStore } from '@/stores/settings-store';
import { AssistantStartView } from '../AssistantStartView';

vi.mock('@/lib/assistant/assistant-session', () => ({ startAssistant: vi.fn() }));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [] }),
}));

describe('AssistantStartView with no agents', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('opens Settings at AI Agents from the no-agents hint', async () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AssistantStartView />
      </QueryClientProvider>,
    );
    expect(screen.getByText('No agents are configured yet.')).toBeInTheDocument();
    expect(screen.queryByText(/title bar/i)).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Open AI Agents settings' }));
    expect(useSettingsStore.getState()).toMatchObject({ open: true, section: 'ai' });
  });
});
