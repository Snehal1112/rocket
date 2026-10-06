import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultLeaf, createDefaultRequestFor, findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { useGrpcStore } from '@/stores/grpc-store';
import { usePaneStore } from '@/stores/pane-store';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { GrpcPanel } from '../GrpcPanel';

const events = vi.hoisted(() => ({
  handlers: {} as Record<string, (event: { payload: unknown }) => void>,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    events.handlers[name] = handler;
    return () => undefined;
  }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea
      aria-label='Message JSON'
      value={value}
      onChange={(e) => onChange?.(e.target.value)}
    />
  ),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    grpcUnaryCall: vi.fn(),
    grpcStartSession: vi.fn(),
    grpcSendMessage: vi.fn(),
    grpcEndRequests: vi.fn(),
    grpcCancelSession: vi.fn(),
    grpcListServices: vi.fn().mockResolvedValue([]),
    listEnvironments: vi.fn().mockResolvedValue([]),
    getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
    getGlobalEnvironment: vi.fn().mockResolvedValue(null),
    listGlobalEnvironments: vi.fn().mockResolvedValue([]),
    getProcessEnvVars: vi.fn().mockResolvedValue({}),
    getCollectionSettings: vi.fn().mockResolvedValue({ variables: [], headers: [] }),
    getFolderVariables: vi.fn().mockResolvedValue([]),
    getRequestVariables: vi.fn().mockResolvedValue([]),
  };
});

const TAB_ID = 'tab-g';

function grpcTab(methodType: tauriApi.GrpcMethodType, method = 'demo.Greeter/Say'): RequestTab {
  const request = createDefaultRequestFor('grpc');
  request.url = 'localhost:50051';
  if (request.grpc) {
    request.grpc.method = method;
    request.grpc.methodType = methodType;
    request.grpc.messages = [{ id: 'm1', title: '', content: '{"name":"ada"}' }];
  }
  return {
    id: TAB_ID,
    title: 'Say',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'api', path: 'say.yml' },
  };
}

// Reads the tab from the store like the real pane does, so edits show up.
function Harness() {
  const tab = usePaneStore((s) => findTabInTree(s.root, TAB_ID)?.tab);
  return tab && isRequestTab(tab) ? <GrpcPanel tab={tab} groupId='g' /> : null;
}

function mount(tab: RequestTab) {
  const leaf = { ...createDefaultLeaf(), tabs: [tab], activeTabId: tab.id };
  usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <Harness />
    </QueryClientProvider>,
  );
}

// The panel picks the session id and passes it to the start command.
function startedId(): string {
  return vi.mocked(tauriApi.grpcStartSession).mock.calls[0][1];
}

function emit(name: string, payload: object) {
  act(() => events.handlers[name]({ payload }));
}

const okResponse: tauriApi.GrpcUnaryResponse = {
  headers: [{ name: 'content-type', value: 'application/grpc' }],
  trailers: [{ name: 'x-end', value: '1' }],
  messageJson: '{\n  "message": "hello ada"\n}',
  status: { code: 0, codeName: 'OK', message: '' },
  durationMs: 12,
};

describe('GrpcPanel unary', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  });

  it('sends the editor state and shows the reply and status', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockResolvedValue(okResponse);
    mount(grpcTab('unary'));

    await userEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(tauriApi.grpcUnaryCall).toHaveBeenCalledTimes(1));
    const input = vi.mocked(tauriApi.grpcUnaryCall).mock.calls[0][0];
    expect(input.collection).toBe('api');
    expect(input.requestPath).toBe('say.yml');
    expect(input.message).toBe('{"name":"ada"}');
    expect(input.request.method).toBe('demo.Greeter/Say');
    expect(input.request.url).toBe('localhost:50051');
    expect(input.timeoutMs).toBe(30000);

    expect(await screen.findByText('0 OK')).toBeInTheDocument();
    expect(screen.getByText(/hello ada/)).toBeInTheDocument();
  });

  it('shows a failing status with its message', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockResolvedValue({
      headers: [],
      trailers: [],
      messageJson: null,
      status: { code: 5, codeName: 'NOT_FOUND', message: 'no such user' },
      durationMs: 3,
    });
    mount(grpcTab('unary'));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('5 NOT_FOUND')).toBeInTheDocument();
    expect(screen.getByText('no such user')).toBeInTheDocument();
    expect(screen.getByText('The call returned no message.')).toBeInTheDocument();
  });

  it('shows a transport error from the backend', async () => {
    vi.mocked(tauriApi.grpcUnaryCall).mockRejectedValue(
      'Http error: could not connect to http://h:1',
    );
    mount(grpcTab('unary'));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not connect');
  });

  it('asks for a method instead of calling the backend', async () => {
    mount(grpcTab('unary', ''));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Choose a method first.')).toBeInTheDocument();
    expect(tauriApi.grpcUnaryCall).not.toHaveBeenCalled();
  });

  it('writes edits to the tab, so a save keeps them', async () => {
    mount(grpcTab('unary'));
    await userEvent.clear(screen.getByLabelText('gRPC URL'));
    await userEvent.type(screen.getByLabelText('gRPC URL'), 'api.test:9');
    await userEvent.click(screen.getByRole('button', { name: 'Add message' }));

    const tab = findTabInTree(usePaneStore.getState().root, TAB_ID)?.tab;
    const request = tab && isRequestTab(tab) ? tab.request : undefined;
    expect(request?.url).toBe('api.test:9');
    expect(request?.grpc?.messages).toHaveLength(2);
    expect(request?.grpc?.activeMessage).toBe(1);
    expect(tab?.isDirty).toBe(true);
  });

  it('removes the active message and keeps one selected', async () => {
    const tab = grpcTab('unary');
    tab.request.grpc?.messages.push({ id: 'm2', title: 'second', content: '{}' });
    mount(tab);
    await userEvent.click(screen.getByRole('button', { name: 'second' }));
    await userEvent.click(screen.getByRole('button', { name: 'Remove message' }));

    const saved = findTabInTree(usePaneStore.getState().root, TAB_ID)?.tab;
    const grpc = saved && isRequestTab(saved) ? saved.request.grpc : undefined;
    expect(grpc?.messages.map((m) => m.id)).toEqual(['m1']);
    expect(grpc?.activeMessage).toBe(0);
  });
});

describe('GrpcPanel streaming', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  });

  it('starts a server stream with the message and shows messages as they arrive', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockImplementation(async (_input, id) => id);
    mount(grpcTab('server-streaming'));

    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.grpcStartSession).mock.calls[0][0].message).toBe('{"name":"ada"}');
    expect(await screen.findByRole('button', { name: /Cancel/ })).toBeInTheDocument();

    emit('grpc-session-started', { session_id: startedId(), method_type: 'server-streaming' });
    emit('grpc-session-message', { session_id: startedId(), index: 0, json: '{"message":"one"}' });
    emit('grpc-session-message', { session_id: startedId(), index: 1, json: '{"message":"two"}' });

    const log = await screen.findByRole('list', { name: 'Message log' });
    expect(log).toHaveTextContent('one');
    expect(log).toHaveTextContent('two');
    expect(screen.getByText('Messages (2)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Send message' })).toBeNull();

    emit('grpc-session-finished', {
      session_id: startedId(),
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [],
      duration_ms: 9,
    });
    expect(await screen.findByText('0 OK')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });

  it('sends messages and ends the requests of a bidi stream', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockImplementation(async (_input, id) => id);
    vi.mocked(tauriApi.grpcSendMessage).mockResolvedValue(undefined);
    vi.mocked(tauriApi.grpcEndRequests).mockResolvedValue(undefined);
    mount(grpcTab('bidi-streaming'));

    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalledTimes(1));
    expect(vi.mocked(tauriApi.grpcStartSession).mock.calls[0][0].message).toBe('{"name":"ada"}');
    emit('grpc-session-started', { session_id: startedId(), method_type: 'bidi-streaming' });

    await userEvent.click(await screen.findByRole('button', { name: 'Send message' }));
    await waitFor(() =>
      expect(tauriApi.grpcSendMessage).toHaveBeenCalledWith(startedId(), '{"name":"ada"}'),
    );
    expect(screen.getByText('Sent')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'End requests' }));
    await waitFor(() => expect(tauriApi.grpcEndRequests).toHaveBeenCalledWith(startedId()));
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled();
  });

  it('cancels a running stream', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockImplementation(async (_input, id) => id);
    vi.mocked(tauriApi.grpcCancelSession).mockResolvedValue(undefined);
    mount(grpcTab('bidi-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalled());
    emit('grpc-session-started', { session_id: startedId(), method_type: 'bidi-streaming' });

    await userEvent.click(await screen.findByRole('button', { name: /Cancel/ }));
    await waitFor(() => expect(tauriApi.grpcCancelSession).toHaveBeenCalledWith(startedId()));

    emit('grpc-session-finished', {
      session_id: startedId(),
      code: 1,
      code_name: 'CANCELLED',
      message: 'cancelled by the user',
      trailers: [],
      duration_ms: 5,
    });
    expect(await screen.findByText('1 CANCELLED')).toBeInTheDocument();
  });

  it('shows the error when a stream cannot start', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockRejectedValue(
      'Invalid input: invalid HelloRequest message',
    );
    mount(grpcTab('server-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('invalid HelloRequest message');
    expect(screen.getByRole('button', { name: 'Start' })).toBeEnabled();
  });

  it('shows an event that arrives before the start command returns', async () => {
    let resolveStart: (id: string) => void = () => undefined;
    vi.mocked(tauriApi.grpcStartSession).mockReturnValue(
      new Promise<string>((resolve) => {
        resolveStart = resolve;
      }),
    );
    mount(grpcTab('server-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalled());

    emit('grpc-session-message', {
      session_id: startedId(),
      index: 0,
      json: '{"message":"early"}',
    });
    await act(async () => resolveStart(startedId()));

    expect(await screen.findByRole('list', { name: 'Message log' })).toHaveTextContent('early');
  });

  it('follows the call shape the server reports over a wrong stored one', async () => {
    vi.mocked(tauriApi.grpcStartSession).mockImplementation(async (_input, id) => id);
    // Stored as server-streaming, but the method is really bidirectional.
    mount(grpcTab('server-streaming'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() => expect(tauriApi.grpcStartSession).toHaveBeenCalled());
    emit('grpc-session-started', { session_id: startedId(), method_type: 'bidi-streaming' });

    expect(await screen.findByRole('button', { name: 'Send message' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'End requests' })).toBeInTheDocument();
  });
});
