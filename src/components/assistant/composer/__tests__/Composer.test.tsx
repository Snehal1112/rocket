import { EditorView } from '@codemirror/view';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRememberedModel } from '@/lib/assistant/model-memory';
import { loadPromptHistory } from '@/lib/assistant/prompt-history';
import { createDefaultLeaf } from '@/lib/pane-utils';
import {
  buildAssistantChipResource,
  cancelAgentPrompt,
  type ConfigOption,
  listCollections,
  listEnvironments,
  sendAgentPrompt,
  setAgentConfigOption,
  setAssistantMode,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { createDeferred } from '@/test/deferred';
import { Composer } from '../Composer';

// Radix menus call pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  listCollections: vi.fn(),
  getCollectionSummaries: vi.fn(),
  listEnvironments: vi.fn(),
  buildAssistantChipResource: vi.fn(),
  maskAssistantResponse: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  setAgentConfigOption: vi.fn(),
  setAssistantMode: vi.fn(),
}));

const MODEL = {
  id: 'model',
  name: 'Model',
  currentValue: 'sonnet',
  choices: [
    { value: 'sonnet', name: 'Sonnet' },
    { value: 'opus', name: 'Opus' },
  ],
} as ConfigOption;

function renderComposer() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const utils = render(
    <QueryClientProvider client={qc}>
      <Composer />
    </QueryClientProvider>,
  );
  const content = utils.container.querySelector('.cm-content') as HTMLElement;
  const view = EditorView.findFromDOM(
    utils.container.querySelector('.cm-editor') as HTMLElement,
  ) as EditorView;
  return { ...utils, content, view, user: userEvent.setup() };
}

function typeInto(view: EditorView, text: string) {
  act(() => {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
  });
}

function pressEnter(content: HTMLElement) {
  act(() => {
    content.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        code: 'Enter',
        keyCode: 13,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(listCollections).mockResolvedValue([]);
  vi.mocked(listEnvironments).mockResolvedValue([]);
  vi.mocked(buildAssistantChipResource)
    .mockReset()
    .mockResolvedValue({
      uri: 'rocket://request/shop/orders/list.yml',
      mimeType: 'text/plain',
      text: 'Request: List orders',
    });
  vi.mocked(sendAgentPrompt).mockReset().mockResolvedValue('end_turn');
  vi.mocked(cancelAgentPrompt).mockReset().mockResolvedValue(undefined);
  vi.mocked(setAgentConfigOption).mockReset();
  vi.mocked(setAssistantMode).mockReset().mockResolvedValue(undefined);
  usePaneStore.setState({ root: createDefaultLeaf('g1'), activeGroupId: 'g1' });
  useWorkspaceStore.setState({ activeWorkspaceId: 'w1' });
  useAssistantStore.setState({
    session: {
      sessionId: 's1',
      agentConfigId: 'a1',
      status: 'active',
      configOptions: [MODEL],
      mode: 'ask',
    },
    focus: { collection: 'shop', path: 'orders/list.yml' },
    usage: undefined,
    // A streaming reply left by an earlier test would count as a running turn.
    messages: [],
    proposals: [],
  });
});

describe('Composer', () => {
  it('shows the focus chip and lets the user remove it', async () => {
    const { user } = renderComposer();
    expect(screen.getByText('list')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove list' }));
    expect(screen.queryByText('list')).toBeNull();
  });

  it('sends the prompt with the focus chip as a backend-built resource', async () => {
    const { content, view } = renderComposer();
    typeInto(view, 'Explain it');
    pressEnter(content);
    await waitFor(() =>
      expect(sendAgentPrompt).toHaveBeenCalledWith('s1', 'Explain it', [
        expect.objectContaining({
          uri: 'rocket://request/shop/orders/list.yml',
          mimeType: 'text/plain',
        }),
      ]),
    );
    expect(buildAssistantChipResource).toHaveBeenCalledWith('request', 'shop', 'orders/list.yml');
    expect(loadPromptHistory('w1')).toEqual(['Explain it']);
    await waitFor(() => expect(view.state.doc.toString()).toBe(''));
  });

  it('does not send when a chip fails to load and keeps the prompt', async () => {
    vi.mocked(buildAssistantChipResource).mockRejectedValue(new Error('boom'));
    const { content, view } = renderComposer();
    typeInto(view, 'Explain it');
    pressEnter(content);
    await waitFor(() => expect(buildAssistantChipResource).toHaveBeenCalled());
    await act(async () => {});
    expect(sendAgentPrompt).not.toHaveBeenCalled();
    expect(view.state.doc.toString()).toBe('Explain it');
    expect(screen.getByText('list')).toBeInTheDocument();
  });

  it('sends without resources when no chip is left', async () => {
    const { content, view, user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Remove list' }));
    typeInto(view, 'hello');
    pressEnter(content);
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalledWith('s1', 'hello', undefined));
  });

  it('sends once when Enter is pressed twice quickly', async () => {
    const turn = createDeferred<string>();
    vi.mocked(sendAgentPrompt).mockReturnValue(turn.promise);
    const { content, view } = renderComposer();
    typeInto(view, 'hello');
    act(() => {
      for (let i = 0; i < 2; i++) {
        content.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'Enter',
            keyCode: 13,
            bubbles: true,
            cancelable: true,
          }),
        );
      }
    });
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalledTimes(1));
    await act(async () => {
      turn.resolve('end_turn');
    });
  });

  it('shows Stop while the turn runs and cancels it', async () => {
    const turn = createDeferred<string>();
    vi.mocked(sendAgentPrompt).mockReturnValue(turn.promise);
    const { content, view, user } = renderComposer();
    typeInto(view, 'hello');
    pressEnter(content);
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalled());
    await user.click(await screen.findByRole('button', { name: 'Stop' }));
    expect(cancelAgentPrompt).toHaveBeenCalledWith('s1');
    await act(async () => {
      turn.resolve('cancelled');
      // The bridge does this on agent-session-finished; no bridge runs here.
      useAssistantStore.getState().completeMessage('s1');
    });
    expect(await screen.findByRole('button', { name: 'Send' })).toBeInTheDocument();
  });

  it('stays usable after a failed turn', async () => {
    vi.mocked(sendAgentPrompt).mockRejectedValueOnce('agent busy');
    const { content, view } = renderComposer();
    typeInto(view, 'first');
    pressEnter(content);
    await waitFor(() => expect(useAssistantStore.getState().session?.status).toBe('active'));
    await screen.findByRole('button', { name: 'Send' });
    typeInto(view, 'second');
    pressEnter(content);
    await waitFor(() => expect(sendAgentPrompt).toHaveBeenCalledTimes(2));
  });

  it('changes the model, stores the new options and remembers the choice', async () => {
    const effort = {
      id: 'effort',
      name: 'Effort',
      currentValue: 'high',
      choices: [{ value: 'high', name: 'High' }],
    } as ConfigOption;
    vi.mocked(setAgentConfigOption).mockResolvedValue([
      { ...MODEL, currentValue: 'opus' },
      effort,
    ]);
    const { user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Model: Sonnet' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Opus/ }));
    expect(setAgentConfigOption).toHaveBeenCalledWith('s1', 'model', 'opus');
    expect(await screen.findByRole('button', { name: 'Effort: High' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Model: Opus' })).toBeInTheDocument();
    expect(loadRememberedModel('a1')).toBe('opus');
  });

  it('switches the mode in the backend and the store', async () => {
    const { user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Mode: Ask' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Edit/ }));
    expect(setAssistantMode).toHaveBeenCalledWith('s1', 'edit');
    expect(await screen.findByRole('button', { name: 'Mode: Edit' })).toBeInTheDocument();
  });
});
