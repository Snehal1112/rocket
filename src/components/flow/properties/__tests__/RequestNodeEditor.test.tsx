import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type Collection,
  type FlowNodeKind,
  getCollection,
  getRequest,
  type Request,
} from '@/lib/tauri-api';
import { RequestNodeEditor } from '../RequestNodeEditor';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getRequest: vi.fn() };
});

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Body'
      value={props.value}
      onChange={(e) => props.onChange?.(e.target.value)}
    />
  ),
}));

type RequestKind = Extract<FlowNodeKind, { kind: 'Request' }>;

const savedKind: RequestKind = {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'login.yml' },
};

const emptyInline: RequestKind = {
  kind: 'Request',
  label: 'Draft',
  source: { type: 'Inline', request: { method: 'GET', url: '', headers: [], body: null } },
};

const filledInline: RequestKind = {
  kind: 'Request',
  label: 'Draft',
  source: {
    type: 'Inline',
    request: { method: 'POST', url: 'https://x/y', headers: [], body: null },
  },
};

const withAuth: Request = {
  uid: 'u1',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [{ key: 'Content-Type', value: 'application/json', enabled: true }],
  body: { mode: 'json', content: '{}' },
  auth: { authType: 'bearer', token: 't' },
  preRequestScript: 'rok.setVar("a", 1);',
};

const collection: Collection = {
  name: 'demo',
  settings: { headers: [], variables: [], sandboxMode: 'safe' },
  root: {
    uid: 'root',
    name: 'demo',
    items: [
      { type: 'summary', uid: 's1', name: 'Me', method: 'GET', url: '/me', fileName: 'me.yml' },
    ],
  },
};

function renderEditor(kind: RequestKind) {
  const onChange = vi.fn();
  render(
    <RequestNodeEditor nodeId='r1' kind={kind} edges={[]} collection='demo' onChange={onChange} />,
  );
  return onChange;
}

describe('RequestNodeEditor', () => {
  beforeEach(() => {
    vi.mocked(getRequest).mockReset();
    vi.mocked(getCollection).mockReset();
  });

  it('shows the saved editor for a Saved source and repoints on pick', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...savedKind,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('shows what is dropped and waits for confirmation', async () => {
    vi.mocked(getRequest).mockResolvedValue(withAuth);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(await screen.findByText(/bearer auth, pre-request script/)).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole('button', { name: 'Convert' }));
    expect(onChange).toHaveBeenCalledWith({
      ...savedKind,
      source: {
        type: 'Inline',
        request: {
          method: 'POST',
          url: '{{baseUrl}}/login',
          headers: [{ name: 'Content-Type', value: 'application/json' }],
          body: '{}',
        },
      },
    });
    expect(getRequest).toHaveBeenCalledWith('demo', 'login.yml');
  });

  it('cancelling a conversion changes nothing', async () => {
    vi.mocked(getRequest).mockResolvedValue(withAuth);
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Convert' })).not.toBeInTheDocument();
  });

  it('shows a load error and changes nothing', async () => {
    vi.mocked(getRequest).mockRejectedValue('file not found');
    const onChange = renderEditor(savedKind);
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not load "login.yml": file not found',
    );
    expect(onChange).not.toHaveBeenCalled();
  });

  it('switches an empty inline request to a saved one at once', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(emptyInline);
    await userEvent.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...emptyInline,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('asks before discarding a non-empty inline request', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const onChange = renderEditor(filledInline);
    await userEvent.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await userEvent.click(await screen.findByRole('button', { name: /Me/ }));
    expect(onChange).not.toHaveBeenCalled();
    expect(
      screen.getByText(/The inline method, URL, headers and body will be discarded/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Use saved request' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...filledInline,
      source: { type: 'Saved', requestPath: 'me.yml' },
    });
  });

  it('edits the inline request through the inline editor', async () => {
    const onChange = renderEditor(filledInline);
    await userEvent.type(screen.getByLabelText('URL'), 'z');
    expect(onChange).toHaveBeenLastCalledWith({
      ...filledInline,
      source: {
        type: 'Inline',
        request: { method: 'POST', url: 'https://x/yz', headers: [], body: null },
      },
    });
  });
});
