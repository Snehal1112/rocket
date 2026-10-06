import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { WebSocketPanel } from '@/components/request/websocket/WebSocketPanel';
import { createDefaultLeaf } from '@/lib/pane-utils';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import * as session from '@/lib/websocket-session';
import { usePaneStore } from '@/stores/pane-store';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/websocket-session', () => ({
  connectTab: vi.fn().mockResolvedValue(undefined),
  disconnectTab: vi.fn().mockResolvedValue(undefined),
  sendSelectedMessage: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('@/hooks/useWebSocketVariableContext', () => ({
  useWebSocketVariableContext: () => undefined,
}));
// CodeMirror and Monaco do not run in jsdom; stand-ins keep the panel testable.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => (
    <input
      aria-label='URL'
      placeholder={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Message body'
      value={value}
      onChange={(e) => onChange?.(e.target.value)}
    />
  ),
}));

function tab(): RequestTab {
  return {
    id: 'tab-1',
    title: 'Chat',
    tabType: 'request',
    request: createDefaultWebSocketRequestState('wss://echo.example.com/ws'),
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'chat.yml' },
  };
}

function mount() {
  const t = tab();
  const leaf = createDefaultLeaf();
  usePaneStore.setState({
    root: { ...leaf, tabs: [t], activeTabId: t.id },
    activeGroupId: leaf.groupId,
  });
  // The real pane passes the live tab from the store, so the harness does the same.
  function Live() {
    const live = usePaneStore((state) => {
      if (state.root.type !== 'leaf') return t;
      const found = state.root.tabs[0];
      return found.tabType === 'request' ? found : t;
    });
    return <WebSocketPanel tab={live} groupId={leaf.groupId} />;
  }
  return render(<Live />);
}

function setSession(status: 'idle' | 'connecting' | 'open' | 'closed' | 'failed') {
  useWebSocketStore.setState({
    byTab: {
      'tab-1': {
        sessionId: status === 'open' ? 's1' : null,
        status,
        subprotocol: null,
        error: null,
        log: [],
      },
    },
    tabBySession: {},
  });
}

beforeEach(() => {
  vi.mocked(session.connectTab).mockClear();
  vi.mocked(session.disconnectTab).mockClear();
  vi.mocked(session.sendSelectedMessage).mockClear();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('WebSocketPanel', () => {
  it('shows Connect and a Disconnected badge when idle, and disables Send', () => {
    mount();
    expect(screen.getByRole('button', { name: 'Connect' })).toBeEnabled();
    expect(screen.getByText('Disconnected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('Connect starts a session for the tab', async () => {
    mount();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Connect' }));
    expect(session.connectTab).toHaveBeenCalledTimes(1);
    expect(vi.mocked(session.connectTab).mock.calls[0][0]).toMatchObject({ id: 'tab-1' });
  });

  it('when open it offers Disconnect, enables Send, and Send sends the selected message', async () => {
    setSession('open');
    mount();
    expect(screen.getByText('Connected')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Send' }));
    expect(session.sendSelectedMessage).toHaveBeenCalledTimes(1);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(session.disconnectTab).toHaveBeenCalledWith('tab-1');
  });

  it('while connecting it shows a disabled Connecting button', () => {
    setSession('connecting');
    mount();
    expect(screen.getByRole('button', { name: 'Connecting...' })).toBeDisabled();
  });

  it('renders the log with direction, size and payload, and Clear empties it', async () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: null,
          error: null,
          log: [
            { id: 'a', direction: 'out', kind: 'text', data: 'ping', size: 4, timestampMs: 1000 },
            { id: 'b', direction: 'in', kind: 'text', data: 'pong', size: 4, timestampMs: 2000 },
          ],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    mount();

    expect(screen.getByText('ping')).toBeInTheDocument();
    expect(screen.getByText('pong')).toBeInTheDocument();
    expect(screen.getAllByText('4 B')).toHaveLength(2);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Clear log' }));
    expect(useWebSocketStore.getState().byTab['tab-1'].log).toEqual([]);
  });

  it('adds, selects and removes saved messages, always keeping one selected', async () => {
    mount();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Add message' }));
    const draft = () => {
      const found = usePaneStore.getState().root;
      if (found.type !== 'leaf') throw new Error('expected a leaf');
      const t = found.tabs[0];
      if (t.tabType !== 'request') throw new Error('expected a request tab');
      return t.request.websocket?.messages ?? [];
    };
    expect(draft().map((m) => m.title)).toEqual(['message 1', 'message 2']);
    expect(draft().map((m) => m.selected)).toEqual([false, true]);

    await user.click(screen.getByRole('button', { name: 'Select message 1' }));
    expect(draft().map((m) => m.selected)).toEqual([true, false]);

    await user.click(screen.getByRole('button', { name: 'Delete message 1' }));
    expect(draft().map((m) => m.title)).toEqual(['message 2']);
    expect(draft().map((m) => m.selected)).toEqual([true]);
  });

  it('editing the body marks the tab dirty', async () => {
    mount();
    await userEvent.setup().type(screen.getByLabelText('Message body'), 'x');
    const root = usePaneStore.getState().root;
    if (root.type !== 'leaf') throw new Error('expected a leaf');
    expect(root.tabs[0].isDirty).toBe(true);
  });
});
