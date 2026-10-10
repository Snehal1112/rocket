import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render as rtlRender, screen, waitFor } from '@testing-library/react';
import type { ReactElement } from 'react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetStaleSweepForTests } from '@/lib/assistant/assistant-session';
import * as api from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useLayoutStore } from '@/stores/layout-store';
import { AssistantPanel, clampWidth } from '../AssistantPanel';
import { AssistantToggleButton } from '../AssistantToggleButton';

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  startWorkspaceAssistant: vi.fn(),
  endStaleAssistantSessions: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
  getCollectionSettings: vi.fn(),
  setCollectionCapability: vi.fn(),
  listConversations: vi.fn().mockResolvedValue([]),
  onAssistantConversationSaved: vi.fn().mockResolvedValue(() => undefined),
  onAssistantConversationDeleted: vi.fn().mockResolvedValue(() => undefined),
  onAssistantConversationsCleared: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));

vi.mock('@/lib/queries/collection-queries', () => ({
  collectionKeys: { all: ['collections'] },
  useCollections: () => ({ data: [] }),
}));

vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

// The composer reads workspace references through React Query.
function render(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return rtlRender(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

describe('AssistantPanel', () => {
  it('switches between the chat and the conversation history', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([]);
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Conversation history' }));
    expect(
      await screen.findByText('No saved conversations in this workspace yet.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back to the chat' }));
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    resetStaleSweepForTests();
    store().reset();
    useAssistantStore.setState({ panelOpen: true, focus: undefined });
    vi.mocked(api.endStaleAssistantSessions).mockResolvedValue(0);
    vi.mocked(api.endAgentSession).mockResolvedValue(undefined);
    vi.mocked(api.cancelAgentPrompt).mockResolvedValue(undefined);
  });

  it('starts a session with the first agent and shows the message box', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() =>
      expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit', undefined),
    );
    expect(
      await screen.findByRole('textbox', { name: 'Message the AI assistant' }),
    ).toBeInTheDocument();
  });

  it('shows a start error', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'agent not found');
    render(<AssistantPanel />);
    expect(screen.getByText('agent not found')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });

  it('keeps newlines in a multi-line start error', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'agent exited\nagent output:\nline one\nline two');
    render(<AssistantPanel />);
    const alert = screen.getByRole('alert');
    expect(alert.textContent).toBe('agent exited\nagent output:\nline one\nline two');
    expect(alert).toHaveClass('whitespace-pre-wrap', 'overflow-y-auto', 'max-h-40');
  });

  it('renders replies and one muted line per tool call', () => {
    activate();
    store().appendUserMessage('Why does login fail?');
    store().appendChunk('s1', 'Checking.');
    store().upsertToolActivity('s1', {
      callId: 'c1',
      title: 'Reading GET /orders',
      status: 'in_progress',
    });
    render(<AssistantPanel />);
    expect(screen.getByText('Why does login fail?')).toBeInTheDocument();
    expect(screen.getByText('Checking.')).toBeInTheDocument();
    expect(screen.getByText('Reading GET /orders')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeInTheDocument();
  });

  it('stops a running turn', async () => {
    activate();
    store().appendUserMessage('hi');
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(api.cancelAgentPrompt).toHaveBeenCalledWith('s1');
  });

  it('ends the session and keeps the transcript', async () => {
    activate();
    store().appendUserMessage('keep me');
    store().completeMessage('s1');
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'End session' }));
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(screen.getByText('keep me')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start a new session' })).toBeInTheDocument();
  });

  it('shows the notice of an ended session', () => {
    activate();
    store().endSession('The workspace changed.');
    render(<AssistantPanel />);
    expect(screen.getByText('The workspace changed.')).toBeInTheDocument();
  });

  it('closes from its header', async () => {
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Close AI Assistant' }));
    expect(store().panelOpen).toBe(false);
  });
});

describe('AssistantToggleButton', () => {
  it('opens and closes the panel', async () => {
    useAssistantStore.setState({ panelOpen: false });
    render(<AssistantToggleButton />);
    const button = screen.getByRole('button', { name: 'AI Assistant' });
    expect(button).toHaveAttribute('aria-pressed', 'false');
    await userEvent.click(button);
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(button).toHaveAttribute('aria-pressed', 'true');
    expect(button).toHaveAttribute('aria-controls', 'assistant-panel');
    await userEvent.click(button);
    expect(button).not.toHaveAttribute('aria-controls');
    expect(useAssistantStore.getState().panelOpen).toBe(false);
  });
});

describe('AssistantPanel width', () => {
  const originalWidth = window.innerWidth;
  const setWindowWidth = (w: number) => {
    Object.defineProperty(window, 'innerWidth', { value: w, configurable: true });
  };

  beforeEach(() => {
    store().reset();
    useAssistantStore.setState({ panelOpen: true });
    useLayoutStore.setState({ assistantPanelWidth: 700 });
  });

  afterEach(() => {
    setWindowWidth(originalWidth);
  });

  it('clamps to the panel limits and to the window', () => {
    expect(clampWidth(100, 1920)).toBe(320);
    expect(clampWidth(900, 1920)).toBe(720);
    expect(clampWidth(700, 900)).toBe(420);
    expect(clampWidth(700, 500)).toBe(320);
  });

  it('shows a narrower panel on a small window without losing the stored width', () => {
    setWindowWidth(900);
    render(<AssistantPanel />);
    const handle = screen.getByRole('separator', { name: 'Resize AI Assistant' });
    expect(handle).toHaveAttribute('aria-valuenow', '420');
    act(() => {
      setWindowWidth(1920);
      window.dispatchEvent(new Event('resize'));
    });
    expect(handle).toHaveAttribute('aria-valuenow', '700');
    expect(useLayoutStore.getState().assistantPanelWidth).toBe(700);
  });
});
