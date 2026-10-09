import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { ScriptsTab } from '../ScriptsTab';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ phase }: { phase: string }) => <div data-testid={`monaco-${phase}`} />,
}));

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

describe('ScriptsTab — AI Assist shortcut', () => {
  beforeEach(() => {
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
  });

  it('opens the assistant panel with the saved request in focus', async () => {
    renderWith({ requestPath: 'orders/get.yml' });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(useAssistantStore.getState().focus).toEqual({
      collection: 'my-collection',
      path: 'orders/get.yml',
    });
  });

  it('clears the focus for a request that is not saved yet', async () => {
    useAssistantStore.setState({ focus: { collection: 'other', path: 'x.yml' } });
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(useAssistantStore.getState().focus).toBeUndefined();
  });

  it('keeps AI Assist visible by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'AI Assist' })).toBeInTheDocument();
  });

  it('agentAssist false hides AI Assist', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'AI Assist' })).not.toBeInTheDocument();
  });
});

describe('ScriptsTab phases', () => {
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

  it('right-aligns Snippets when AI Assist is hidden', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Snippets' })).toHaveClass('ml-auto');
  });

  it('leaves Snippets unshifted when AI Assist is shown', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Snippets' })).not.toHaveClass('ml-auto');
  });

  it('falls back to a visible phase when phases change on rerender', async () => {
    const props = {
      tabId: 'tab-1',
      preRequestScript: '',
      postResponseScript: '',
      testsScript: '',
      onChangePreRequest: vi.fn(),
      onChangePostResponse: vi.fn(),
      onChangeTests: vi.fn(),
    };
    const { rerender } = render(<ScriptsTab {...props} />);
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Tests' }));
    await waitFor(() => expect(screen.getByTestId('monaco-tests')).toBeInTheDocument());
    rerender(<ScriptsTab {...props} phases={['pre-request', 'post-response']} />);
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByTestId('monaco-tests')).not.toBeInTheDocument();
  });
});
