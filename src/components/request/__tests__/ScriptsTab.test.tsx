import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptsTab } from '../ScriptsTab';

type EditorStub = {
  getModel: () => { getLineCount: () => number; getLineMaxColumn: () => number };
  getPosition: () => null;
  executeEdits: ReturnType<typeof vi.fn>;
  focus: ReturnType<typeof vi.fn>;
};

const editorStubs: Record<string, EditorStub> = {};

function makeEditorStub(): EditorStub {
  return {
    getModel: () => ({ getLineCount: () => 1, getLineMaxColumn: () => 1 }),
    getPosition: () => null,
    executeEdits: vi.fn(),
    focus: vi.fn(),
  };
}

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({
    phase,
    onEditorReady,
  }: {
    phase: string;
    onEditorReady: (editor: EditorStub) => void;
  }) => {
    const editor = makeEditorStub();
    editorStubs[phase] = editor;
    onEditorReady(editor);
    return <div data-testid={`monaco-${phase}`} />;
  },
}));

let latestOnInsertCode: ((code: string) => void) | undefined;
vi.mock('../AgentChatPanel', () => ({
  AgentChatPanel: ({
    tabId,
    collectionName,
    onInsertCode,
  }: {
    tabId: string;
    collectionName?: string;
    onInsertCode: (code: string) => void;
  }) => {
    latestOnInsertCode = onInsertCode;
    return (
      <div data-testid='agent-chat-panel' data-tab-id={tabId} data-collection={collectionName} />
    );
  },
}));

function renderScriptsTab() {
  return render(
    <ScriptsTab
      tabId='tab-1'
      collectionName='my-collection'
      preRequestScript=''
      postResponseScript=''
      testsScript=''
      onChangePreRequest={vi.fn()}
      onChangePostResponse={vi.fn()}
      onChangeTests={vi.fn()}
    />,
  );
}

describe('ScriptsTab — AI Assist panel wiring', () => {
  beforeEach(() => {
    latestOnInsertCode = undefined;
    for (const key of Object.keys(editorStubs)) delete editorStubs[key];
  });

  it('hides the AI Assist panel until its toggle is clicked', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByTestId('agent-chat-panel')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();
  });

  it('passes tabId and collectionName through to AgentChatPanel', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    const panel = screen.getByTestId('agent-chat-panel');
    expect(panel.dataset.tabId).toBe('tab-1');
    expect(panel.dataset.collection).toBe('my-collection');
  });

  it('keeps the AI Assist panel mounted across phase switches', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();
  });

  it('inserts code from the agent into whichever phase editor is currently active', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));

    latestOnInsertCode?.('pm.test("ok", () => {});');
    expect(editorStubs['pre-request'].executeEdits).toHaveBeenCalledWith('snippet-insert', [
      expect.objectContaining({ text: '\npm.test("ok", () => {});\n' }),
    ]);

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
    latestOnInsertCode?.('pm.test("second", () => {});');
    expect(editorStubs['post-response'].executeEdits).toHaveBeenCalledWith('snippet-insert', [
      expect.objectContaining({ text: '\npm.test("second", () => {});\n' }),
    ]);
  });
});

function renderWith(extra: Partial<ComponentProps<typeof ScriptsTab>> = {}) {
  return render(
    <ScriptsTab
      tabId='tab-1'
      collectionName='my-collection'
      preRequestScript=''
      postResponseScript=''
      testsScript=''
      onChangePreRequest={vi.fn()}
      onChangePostResponse={vi.fn()}
      onChangeTests={vi.fn()}
      {...extra}
    />,
  );
}

describe('ScriptsTab phases and agentAssist props', () => {
  it('renders all three phase tabs by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('phases limits the visible phase tabs', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Tests' })).not.toBeInTheDocument();
  });

  it('starts on the first allowed phase', async () => {
    renderWith({ phases: ['tests'] });
    await waitFor(() => expect(screen.getByTestId('monaco-tests')).toBeInTheDocument());
    expect(screen.queryByTestId('monaco-pre-request')).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Pre Request' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('passes the phase to every editor', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
  });

  it('agentAssist false hides AI Assist', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'AI Assist' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('agent-chat-panel')).not.toBeInTheDocument();
  });

  it('keeps AI Assist visible by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'AI Assist' })).toBeInTheDocument();
  });
});
