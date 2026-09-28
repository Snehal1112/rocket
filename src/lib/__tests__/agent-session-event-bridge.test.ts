import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import type { AgentChatSession, PaneNode, RequestTab, Tab } from '@/types/pane-types';
import { useAgentSessionEventBridge } from '../agent-session-event-bridge';

type ChunkHandler = (e: { session_id: string; text: string }) => void;
type FinishedHandler = (e: { session_id: string; stop_reason: string }) => void;
type FailedHandler = (e: { session_id: string; error: string }) => void;

let chunkHandler: ChunkHandler | undefined;
let finishedHandler: FinishedHandler | undefined;
let failedHandler: FailedHandler | undefined;

vi.mock('@/lib/tauri-api', () => ({
  onAgentSessionChunk: vi.fn((h: ChunkHandler) => {
    chunkHandler = h;
    return Promise.resolve(() => undefined);
  }),
  onAgentSessionFinished: vi.fn((h: FinishedHandler) => {
    finishedHandler = h;
    return Promise.resolve(() => undefined);
  }),
  onAgentSessionFailed: vi.fn((h: FailedHandler) => {
    failedHandler = h;
    return Promise.resolve(() => undefined);
  }),
}));

const mockState = vi.hoisted(() => ({
  root: undefined as unknown as PaneNode,
  collectionTabState: {} as Record<string, { tabs: Tab[]; activeTabId: string }>,
  appendAgentChatChunk: vi.fn(),
  completeAgentChatMessage: vi.fn(),
  failAgentChatMessage: vi.fn(),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: { getState: () => mockState },
}));

function requestTab(id: string, agentSession?: AgentChatSession): RequestTab {
  return {
    id,
    title: id,
    tabType: 'request',
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
    agentSession,
  };
}

function streamingSession(sessionId: string): AgentChatSession {
  return {
    agentConfigId: 'agent-1',
    sessionId,
    status: 'active',
    messages: [
      { id: 'u1', role: 'user', text: 'hi' },
      { id: 'm1', role: 'agent', text: '', streaming: true },
    ],
  };
}

// Builds a two-pane tree with the given tabs in the second pane.
function splitTreeWith(tabs: Tab[]): PaneNode {
  return {
    type: 'split',
    id: 's1',
    direction: 'horizontal',
    sizes: [50, 50],
    children: [
      { ...createDefaultLeaf('g1'), tabs: [requestTab('other-tab')], activeTabId: 'other-tab' },
      { ...createDefaultLeaf('g2'), tabs, activeTabId: tabs[0]?.id ?? '' },
    ],
  };
}

async function mountBridge() {
  renderHook(() => useAgentSessionEventBridge());
  await waitFor(() => {
    expect(chunkHandler).toBeDefined();
    expect(finishedHandler).toBeDefined();
    expect(failedHandler).toBeDefined();
  });
}

describe('useAgentSessionEventBridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    chunkHandler = undefined;
    finishedHandler = undefined;
    failedHandler = undefined;
    mockState.root = createDefaultLeaf('g0');
    mockState.collectionTabState = {};
  });

  it('routes a chunk event to the owning tab and its streaming message by session id', async () => {
    mockState.root = splitTreeWith([requestTab('tab-1', streamingSession('session-1'))]);
    await mountBridge();

    chunkHandler?.({ session_id: 'session-1', text: 'Hello' });

    expect(mockState.appendAgentChatChunk).toHaveBeenCalledWith('tab-1', 'm1', 'Hello');
  });

  it('routes a chunk event to a tab parked in a collection snapshot', async () => {
    mockState.collectionTabState = {
      'other-collection': {
        tabs: [requestTab('tab-parked', streamingSession('session-parked'))],
        activeTabId: 'tab-parked',
      },
    };
    await mountBridge();

    chunkHandler?.({ session_id: 'session-parked', text: 'Hi' });

    expect(mockState.appendAgentChatChunk).toHaveBeenCalledWith('tab-parked', 'm1', 'Hi');
  });

  it('ignores a chunk event whose session id matches no tab', async () => {
    mockState.root = splitTreeWith([requestTab('tab-1', streamingSession('session-1'))]);
    await mountBridge();

    chunkHandler?.({ session_id: 'unknown-session', text: 'Hello' });

    expect(mockState.appendAgentChatChunk).not.toHaveBeenCalled();
  });

  it('completes the matching streaming message on a finished event', async () => {
    mockState.root = splitTreeWith([requestTab('tab-1', streamingSession('session-1'))]);
    await mountBridge();

    finishedHandler?.({ session_id: 'session-1', stop_reason: 'end_turn' });

    expect(mockState.completeAgentChatMessage).toHaveBeenCalledWith('tab-1', 'm1');
  });

  it('fails the matching streaming message on a failed event', async () => {
    mockState.root = splitTreeWith([requestTab('tab-1', streamingSession('session-1'))]);
    await mountBridge();

    failedHandler?.({ session_id: 'session-1', error: 'crashed' });

    expect(mockState.failAgentChatMessage).toHaveBeenCalledWith('tab-1', 'm1', 'crashed');
  });
});
