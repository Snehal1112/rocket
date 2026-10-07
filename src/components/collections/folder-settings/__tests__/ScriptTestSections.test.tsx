import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { ScriptSection } from '../ScriptSection';
import { TestSection } from '../TestSection';

vi.mock('@/components/request/ScriptsTab', () => ({
  ScriptsTab: (props: {
    tabId: string;
    phases?: string[];
    agentAssist?: boolean;
    preRequestScript: string;
    postResponseScript: string;
    testsScript: string;
    onChangePreRequest: (v: string) => void;
    onChangePostResponse: (v: string) => void;
    onChangeTests: (v: string) => void;
  }) => {
    // Local state proves whether React reused or recreated the instance.
    const [phase, setPhase] = useState('initial');
    return (
      <div
        data-testid='scripts-tab'
        data-tab-id={props.tabId}
        data-phases={(props.phases ?? []).join(',')}
        data-agent-assist={String(props.agentAssist)}
        data-local={phase}
        data-pre={props.preRequestScript}
        data-post={props.postResponseScript}
        data-tests={props.testsScript}
      >
        <button type='button' onClick={() => props.onChangePreRequest('pre!')}>
          edit-pre
        </button>
        <button type='button' onClick={() => props.onChangePostResponse('post!')}>
          edit-post
        </button>
        <button type='button' onClick={() => props.onChangeTests('tests!')}>
          edit-tests
        </button>
        <button type='button' onClick={() => props.onChangePreRequest('')}>
          clear-pre
        </button>
        <button type='button' onClick={() => props.onChangePostResponse('')}>
          clear-post
        </button>
        <button type='button' onClick={() => props.onChangeTests('')}>
          clear-tests
        </button>
        <button type='button' onClick={() => setPhase('changed')}>
          mutate-local
        </button>
      </div>
    );
  },
}));

const base: FolderSettings = {
  headers: [],
  variables: [],
  preRequestScript: 'a',
  postResponseScript: 'b',
  testsScript: 'c',
};

function sectionProps(over: { collectionName?: string; folderPath?: string } = {}) {
  return {
    collectionName: over.collectionName ?? 'col',
    folderPath: over.folderPath ?? 'users',
    settings: base,
    onChange: vi.fn(),
  };
}

describe('folder Script and Test sections', () => {
  it('ScriptSection writes pre-request and post-response patches', async () => {
    const props = sectionProps();
    render(<ScriptSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.phases).toBe('pre-request,post-response');
    expect(tab.dataset.pre).toBe('a');
    expect(tab.dataset.post).toBe('b');
    fireEvent.click(screen.getByText('edit-pre'));
    expect(props.onChange).toHaveBeenLastCalledWith({ preRequestScript: 'pre!' });
    fireEvent.click(screen.getByText('edit-post'));
    expect(props.onChange).toHaveBeenLastCalledWith({ postResponseScript: 'post!' });
  });

  it('TestSection writes the tests patch', async () => {
    const props = sectionProps();
    render(<TestSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.phases).toBe('tests');
    expect(tab.dataset.tests).toBe('c');
    fireEvent.click(screen.getByText('edit-tests'));
    expect(props.onChange).toHaveBeenLastCalledWith({ testsScript: 'tests!' });
  });

  it('saves a cleared script as undefined so folder.yml drops it', async () => {
    const props = sectionProps();
    render(<ScriptSection {...props} />);
    await screen.findByTestId('scripts-tab');
    fireEvent.click(screen.getByText('clear-pre'));
    expect(props.onChange).toHaveBeenLastCalledWith({ preRequestScript: undefined });
    fireEvent.click(screen.getByText('clear-post'));
    expect(props.onChange).toHaveBeenLastCalledWith({ postResponseScript: undefined });
  });

  it('TestSection saves a cleared tests script as undefined', async () => {
    const props = sectionProps();
    render(<TestSection {...props} />);
    await screen.findByTestId('scripts-tab');
    fireEvent.click(screen.getByText('clear-tests'));
    expect(props.onChange).toHaveBeenLastCalledWith({ testsScript: undefined });
  });

  it('treats missing scripts as empty strings', async () => {
    const props = {
      ...sectionProps(),
      settings: {
        ...base,
        preRequestScript: null,
        postResponseScript: undefined,
        testsScript: null,
      },
    } as unknown as ReturnType<typeof sectionProps>;
    render(<ScriptSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.pre).toBe('');
    expect(tab.dataset.post).toBe('');
  });

  it('folder sections hide AI Assist', async () => {
    const { unmount } = render(<ScriptSection {...sectionProps()} />);
    expect((await screen.findByTestId('scripts-tab')).dataset.agentAssist).toBe('false');
    unmount();
    render(<TestSection {...sectionProps()} />);
    expect((await screen.findByTestId('scripts-tab')).dataset.agentAssist).toBe('false');
  });

  it('gives each folder and section its own editor key', async () => {
    const a = render(<ScriptSection {...sectionProps({ folderPath: 'users' })} />);
    const idA = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    a.unmount();
    const b = render(<ScriptSection {...sectionProps({ folderPath: 'orders' })} />);
    const idB = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    b.unmount();
    render(<TestSection {...sectionProps({ folderPath: 'users' })} />);
    const idC = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    expect(idA).toBe('folder-script:col:users');
    expect(idB).toBe('folder-script:col:orders');
    expect(idC).toBe('folder-test:col:users');
  });

  it('a different folder gets a fresh ScriptsTab instance', async () => {
    const { rerender } = render(<ScriptSection {...sectionProps({ folderPath: 'users' })} />);
    const first = await screen.findByTestId('scripts-tab');
    fireEvent.click(screen.getByText('mutate-local'));
    await waitFor(() => expect(first.dataset.local).toBe('changed'));
    // Same element position, different folder: the key must remount the editor stack.
    rerender(<ScriptSection {...sectionProps({ folderPath: 'orders' })} />);
    const second = await screen.findByTestId('scripts-tab');
    expect(second.dataset.tabId).toBe('folder-script:col:orders');
    expect(second.dataset.local).toBe('initial');
  });
});
