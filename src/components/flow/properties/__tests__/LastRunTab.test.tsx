import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  FlowDebugRequest,
  FlowDebugResponse,
  FlowNode,
  FlowNodeKind,
  FlowRejectedCall,
  FlowStepTrace,
} from '@/lib/tauri-api';
import { copyTextAsync } from '@/lib/clipboard';
import { LastRunTab } from '../LastRunTab';

// Monaco cannot run in jsdom. A read-only textarea stands in for it.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string; readOnly?: boolean; language?: string }) => (
    <textarea
      aria-label='Body viewer'
      readOnly
      data-readonly={String(props.readOnly)}
      data-language={props.language}
      value={props.value}
    />
  ),
}));

vi.mock('@/lib/clipboard', () => ({ copyTextAsync: vi.fn(async () => undefined) }));

const node = (kind: FlowNodeKind): FlowNode => ({ id: 'n1', kind, position: { x: 0, y: 0 } });
const request = node({
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});

describe('LastRunTab status', () => {
  it('shows the never-ran state', () => {
    render(<LastRunTab node={request} status='idle' />);
    expect(screen.getByText('Not run yet. Run the flow to see results here.')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-status')).not.toBeInTheDocument();
  });

  it('shows the status line of a success', () => {
    render(
      <LastRunTab node={request} status='success' detail={{ statusCode: 200, durationMs: 184 }} />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Success');
    expect(line).toHaveTextContent('200');
    expect(line).toHaveTextContent('184ms');
  });

  it('shows attempts and total time for a polled request', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{ statusCode: 200, durationMs: 14200, attempts: 7 }}
      />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('7 attempts');
    expect(line).toHaveTextContent('14.2s');
  });

  it('shows progress while running', () => {
    render(<LastRunTab node={request} status='running' detail={{ progress: 'attempt 3/30' }} />);
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Running');
    expect(line).toHaveTextContent('attempt 3/30');
  });

  it('shows the full error text', () => {
    const error = `condition not met after 30 attempts (60.0s) ${'x'.repeat(400)}`;
    render(<LastRunTab node={request} status='failed' detail={{ statusCode: 404, error }} />);
    const box = screen.getByTestId('last-run-error');
    expect(box).toHaveTextContent(error);
    expect(box.className).not.toContain('line-clamp');
    expect(box.className).toContain('select-text');
  });

  it('explains an upstream skip', () => {
    render(
      <LastRunTab node={request} status='skipped' detail={{ skipReason: 'upstream_failed' }} />,
    );
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Skipped');
    expect(screen.getByText('An earlier node failed.')).toBeInTheDocument();
  });

  it('explains a branch that was not taken', () => {
    render(
      <LastRunTab node={request} status='skipped' detail={{ skipReason: 'branch_not_taken' }} />,
    );
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Not taken');
    expect(screen.getByText('Its branch was not taken.')).toBeInTheDocument();
  });
});

const baseResponse: FlowDebugResponse = {
  status: 200,
  statusText: 'OK',
  durationMs: 184,
  sizeBytes: 15,
  headers: [{ key: 'content-type', value: 'application/json' }],
  body: '{"token":"abc"}',
};

const exchange: FlowDebugRequest = {
  method: 'POST',
  url: 'https://api.example.com/login',
  headers: [
    { key: 'Content-Type', value: 'application/json' },
    { key: 'Authorization', value: '[REDACTED]' },
  ],
  body: '{"user":"ada"}',
  response: baseResponse,
};

describe('LastRunTab exchange', () => {
  it('shows the response headers and a pretty body', async () => {
    render(<LastRunTab node={request} status='success' detail={{ statusCode: 200, exchange }} />);
    const response = screen.getByTestId('last-run-response');
    expect(response).toHaveTextContent('content-type');
    expect(response).toHaveTextContent('application/json');
    expect(await screen.findByLabelText('Body viewer')).toHaveValue('{\n  "token": "abc"\n}');
  });

  it('shows a text body unchanged', async () => {
    const html = { ...exchange, response: { ...baseResponse, body: '<h1>Hi</h1>' } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: html }} />);
    expect(await screen.findByLabelText('Body viewer')).toHaveValue('<h1>Hi</h1>');
  });

  it('notes a truncated body', async () => {
    const cut = { ...exchange, response: { ...baseResponse, truncated: true } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: cut }} />);
    expect(await screen.findByLabelText('Body viewer')).toBeInTheDocument();
    expect(screen.getByText('Truncated at 256 KB')).toBeInTheDocument();
  });

  it('shows the request as sent when expanded', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    expect(screen.queryByTestId('last-run-request')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Request as sent/ }));
    const sent = screen.getByTestId('last-run-request');
    expect(sent).toHaveTextContent('POST');
    expect(sent).toHaveTextContent('https://api.example.com/login');
    expect(sent).toHaveTextContent('[REDACTED]');
    expect(sent).toHaveTextContent('{"user":"ada"}');
  });

  it('shows no exchange sections when nothing was sent', () => {
    render(
      <LastRunTab node={request} status='failed' detail={{ error: 'could not resolve host' }} />,
    );
    expect(screen.queryByTestId('last-run-response')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Request as sent/ })).not.toBeInTheDocument();
  });

  it('shows the send error of an exchange without a response', async () => {
    const noResponse = { ...exchange, response: undefined, error: 'connection refused' };
    render(<LastRunTab node={request} status='failed' detail={{ exchange: noResponse }} />);
    expect(screen.queryByTestId('last-run-response')).not.toBeInTheDocument();
    expect(screen.getByTestId('last-run-send-error')).toHaveTextContent('connection refused');
    await userEvent.click(screen.getByRole('button', { name: /Request as sent/ }));
    expect(screen.getByTestId('last-run-request')).toHaveTextContent(
      'https://api.example.com/login',
    );
  });

  it('does not repeat a send error equal to the step error', () => {
    const noResponse = { ...exchange, response: undefined, error: 'connection refused' };
    render(
      <LastRunTab
        node={request}
        status='failed'
        detail={{ error: 'connection refused', exchange: noResponse }}
      />,
    );
    expect(screen.getByTestId('last-run-error')).toHaveTextContent('connection refused');
    expect(screen.queryByTestId('last-run-send-error')).not.toBeInTheDocument();
  });

  it('shows a send error next to a response', () => {
    const both = { ...exchange, error: 'retry failed' };
    render(<LastRunTab node={request} status='failed' detail={{ exchange: both }} />);
    expect(screen.getByTestId('last-run-send-error')).toHaveTextContent('retry failed');
    expect(screen.getByTestId('last-run-response')).toBeInTheDocument();
  });

  it('opens the response body read-only with the matching language', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    const viewer = await screen.findByLabelText('Body viewer');
    expect(viewer).toHaveAttribute('data-readonly', 'true');
    expect(viewer).toHaveAttribute('data-language', 'json');
  });

  it('uses plaintext for a body that is not JSON', async () => {
    const html = { ...exchange, response: { ...baseResponse, body: '<h1>Hi</h1>' } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: html }} />);
    expect(await screen.findByLabelText('Body viewer')).toHaveAttribute(
      'data-language',
      'plaintext',
    );
  });

  it('says so when the response body is empty', () => {
    const empty = { ...exchange, response: { ...baseResponse, body: '' } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: empty }} />);
    expect(screen.getByText('No body.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Body viewer')).not.toBeInTheDocument();
  });

  it('notes a truncated request body under Request as sent', async () => {
    const cut = { ...exchange, bodyTruncated: true };
    render(<LastRunTab node={request} status='success' detail={{ exchange: cut }} />);
    await userEvent.click(screen.getByRole('button', { name: /Request as sent/ }));
    expect(screen.getByTestId('last-run-request')).toHaveTextContent('Truncated at 256 KB');
  });

  it('does not note an uncut request body', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    await userEvent.click(screen.getByRole('button', { name: /Request as sent/ }));
    expect(screen.getByTestId('last-run-request')).not.toHaveTextContent('Truncated');
  });

  it('parses the response body once per body', async () => {
    const parse = vi.spyOn(JSON, 'parse');
    try {
      render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
      await screen.findByLabelText('Body viewer');
      const calls = parse.mock.calls.filter(([text]) => text === baseResponse.body);
      expect(calls).toHaveLength(1);
    } finally {
      parse.mockRestore();
    }
  });

  it('copies the raw response body', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy response body' }));
    expect(copyTextAsync).toHaveBeenCalledTimes(1);
    await expect(vi.mocked(copyTextAsync).mock.calls[0][0]).resolves.toBe('{"token":"abc"}');
  });
});

describe('LastRunTab per kind', () => {
  it('shows the branch an If took', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
    render(<LastRunTab node={ifNode} status='success' detail={{ branch: 'true' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: true');
  });

  it('shows the case label a Switch took', () => {
    const sw = node({
      kind: 'Switch',
      label: 'Type',
      value: 'response.body.type',
      cases: [{ id: 'c1', label: 'Admin', matches: 'admin' }],
    });
    render(<LastRunTab node={sw} status='success' detail={{ branch: 'case:c1' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: Admin');
  });

  it('shows a deleted Switch case as (deleted case)', () => {
    const sw = node({ kind: 'Switch', label: 'Type', value: 'x', cases: [] });
    render(<LastRunTab node={sw} status='success' detail={{ branch: 'case:gone' }} />);
    const took = screen.getByTestId('last-run-branch');
    expect(took).toHaveTextContent('Took: (deleted case)');
    expect(took).not.toHaveTextContent('case:gone');
  });

  it('shows a received callback as the call, without Request as sent', () => {
    const wait = node({ kind: 'WaitForCallback', label: 'Hook', name: 'hook', timeoutMs: 60000 });
    const call: FlowDebugRequest = {
      method: 'POST',
      url: '/hook?id=7',
      headers: [],
      response: { ...baseResponse, statusText: 'POST' },
    };
    render(<LastRunTab node={wait} status='success' detail={{ exchange: call }} />);
    expect(screen.getByTestId('last-run-response')).toHaveTextContent(
      'Received call · POST /hook?id=7',
    );
    expect(screen.queryByText(/^Response ·/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Request as sent/ })).not.toBeInTheDocument();
  });

  it('lets long log lines wrap anywhere', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{ logs: [{ level: 'log', message: 'x'.repeat(300) }] }}
      />,
    );
    expect(screen.getByText('x'.repeat(300)).className).toContain('[overflow-wrap:anywhere]');
  });

  it('shows an Output value pretty-printed with a copy button', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: '{"a":1}' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('"a": 1');
    expect(screen.getByRole('button', { name: 'Copy value' })).toBeInTheDocument();
  });

  it('shows an Input value', () => {
    const input = node({ kind: 'Input', label: 'User', value: '{{user}}' });
    render(<LastRunTab node={input} status='success' detail={{ value: 'ada' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('ada');
  });

  it('shows (empty) for an empty value', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: '' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('(empty)');
  });

  it('lists the node logs with their level', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{
          logs: [
            { level: 'log', message: 'wire data: {}' },
            { level: 'error', message: 'boom' },
          ],
        }}
      />,
    );
    const logs = screen.getByTestId('last-run-logs');
    expect(logs).toHaveTextContent('wire data: {}');
    expect(logs).toHaveTextContent('boom');
    expect(screen.getByText('boom').className).toContain('text-red-600');
  });

  it('shows no logs section without logs', () => {
    render(<LastRunTab node={request} status='success' detail={{ statusCode: 200 }} />);
    expect(screen.queryByTestId('last-run-logs')).not.toBeInTheDocument();
  });
});

describe('LastRunTab for a Transform node', () => {
  const transform = node({ kind: 'Transform', label: 'Pick', script: 'return 1;' });

  it('shows a Transform value pretty-printed', () => {
    render(<LastRunTab node={transform} status='success' detail={{ value: '{"a":1}' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('"a": 1');
    expect(screen.getByRole('button', { name: 'Copy value' })).toBeInTheDocument();
  });

  it('shows a plain text value as is', () => {
    render(<LastRunTab node={transform} status='success' detail={{ value: 'PRO' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('PRO');
  });

  it('shows the logs of a failed Transform', () => {
    render(
      <LastRunTab
        node={transform}
        status='failed'
        detail={{
          error: 'script returned no value',
          logs: [{ level: 'log', message: 'checking token' }],
        }}
      />,
    );
    expect(screen.getByTestId('last-run-error')).toHaveTextContent('script returned no value');
    expect(screen.getByText('checking token')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-value')).not.toBeInTheDocument();
  });

  it('explains a skipped Transform without showing a value', () => {
    render(
      <LastRunTab node={transform} status='skipped' detail={{ skipReason: 'branch_not_taken' }} />,
    );
    expect(screen.getByText(/branch was not taken/i)).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-value')).not.toBeInTheDocument();
  });
});

describe('LastRunTab trace', () => {
  const login = node({
    kind: 'Request',
    label: 'Login',
    source: { type: 'Saved', requestPath: 'auth/login.yml' },
  });
  const sourceNode: FlowNode = {
    id: 'src',
    kind: { kind: 'Input', label: 'API Key', value: '{{apiKey}}' },
    position: { x: 0, y: 0 },
  };

  it('lists the inputs the step received, as the backend masked them', () => {
    const trace: FlowStepTrace = {
      wires: [
        { edgeId: 'e1', sourceNodeId: 'src', targetField: 'headers[X-Key].value', value: '••••••' },
        { edgeId: 'e2', sourceNodeId: 'src', targetField: 'body', value: 'x'.repeat(10), truncated: true },
      ],
    };
    render(<LastRunTab node={login} nodes={[sourceNode]} status='success' detail={{ trace }} />);
    const rows = screen.getAllByTestId('last-run-input');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('X-Key');
    expect(rows[0]).toHaveTextContent('← API Key');
    expect(rows[0]).toHaveTextContent('••••••');
    expect(rows[1]).toHaveTextContent('Cut at 16 KB.');
  });

  it('never shows a value on a credential input', () => {
    const trace: FlowStepTrace = {
      wires: [
        {
          edgeId: 'ea',
          sourceNodeId: 'src',
          targetField: 'auth',
          credential: true,
          value: 'leaked-token-123456',
        },
      ],
    };
    render(<LastRunTab node={login} status='success' detail={{ trace }} />);
    expect(screen.getByTestId('last-run-input')).toHaveTextContent('Credential (hidden)');
    expect(document.body).not.toHaveTextContent('leaked-token-123456');
  });

  it('marks the input that failed and shows its error', () => {
    const trace: FlowStepTrace = {
      wires: [
        { edgeId: 'e1', sourceNodeId: 'src', targetField: 'url', value: 'ok' },
        { edgeId: 'e2', sourceNodeId: 'src', targetField: 'body', error: 'ReferenceError: x' },
      ],
      failedEdgeId: 'e2',
    };
    render(<LastRunTab node={login} status='failed' detail={{ error: 'wire failed', trace }} />);
    const rows = screen.getAllByTestId('last-run-input');
    expect(rows[0]).not.toHaveAttribute('data-failed');
    expect(rows[1]).toHaveAttribute('data-failed', 'true');
    expect(screen.getByTestId('last-run-input-error')).toHaveTextContent('ReferenceError: x');
  });

  it('labels an input from a deleted node by its id', () => {
    const trace: FlowStepTrace = {
      wires: [{ edgeId: 'e1', sourceNodeId: 'gone', targetField: 'url', value: 'v' }],
    };
    render(<LastRunTab node={login} nodes={[]} status='success' detail={{ trace }} />);
    expect(screen.getByTestId('last-run-input')).toHaveTextContent('← gone');
  });

  it('shows the If condition result', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
    render(
      <LastRunTab
        node={ifNode}
        status='success'
        detail={{ branch: 'true', trace: { route: { kind: 'if', value: 'true' } } }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Condition → true');
    expect(screen.getByText('Passes its input through.')).toBeInTheDocument();
  });

  it('shows the Switch value and the case it matched', () => {
    const sw = node({
      kind: 'Switch',
      label: 'Type',
      value: 'response.body.type',
      cases: [{ id: 'c1', label: 'Admin', matches: 'admin' }],
    });
    render(
      <LastRunTab
        node={sw}
        status='success'
        detail={{
          branch: 'case:c1',
          trace: { route: { kind: 'switch', value: 'admin', matchedCase: 'c1' } },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Value admin → case Admin');
  });

  it('shows a Switch that fell to the default exit', () => {
    const sw = node({ kind: 'Switch', label: 'Type', value: 'x', cases: [] });
    render(
      <LastRunTab
        node={sw}
        status='success'
        detail={{ branch: 'default', trace: { route: { kind: 'switch', value: 'pro' } } }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Value pro → default');
  });

  it('keeps the Took line when no route was recorded', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'true' });
    render(<LastRunTab node={ifNode} status='success' detail={{ branch: 'false' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: false');
  });

  it('says an Auth credential is hidden', () => {
    const auth = node({
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: true,
    });
    render(<LastRunTab node={auth} status='success' detail={{ durationMs: 1 }} />);
    expect(screen.getByText('The credential is hidden.')).toBeInTheDocument();
  });

  it('notes a value that was cut', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(
      <LastRunTab
        node={out}
        status='success'
        detail={{ value: 'abc', trace: { valueTruncated: true } }}
      />,
    );
    expect(screen.getByText('Cut at 256 KB.')).toBeInTheDocument();
  });

  it('shows the duration of a non-HTTP node', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: 'abc', durationMs: 3 }} />);
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('3ms');
  });
});

describe('LastRunTab live progress', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  const poller = node({
    kind: 'Request',
    label: 'Job',
    source: { type: 'Saved', requestPath: 'jobs/status.yml' },
  });
  const wait = node({ kind: 'WaitForCallback', label: 'Hook', name: 'hook', timeoutMs: 60000 });
  it('shows the callback URL of a waiting node', () => {
    render(
      <LastRunTab
        node={wait}
        status='running'
        callbackUrl='http://10.0.0.5:4000/cb/tok'
        detail={{ live: { ignored: 0, remainingMs: 1000 } }}
      />,
    );
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
  });

  it('shows no callback URL for a finished wait without one', () => {
    render(<LastRunTab node={wait} status='success' />);
    expect(screen.queryByTestId('callback-url')).not.toBeInTheDocument();
  });

  const rejected: FlowRejectedCall = {
    method: 'POST',
    url: '/cb/…?event=pending',
    headers: [{ key: 'Authorization', value: '••••••' }],
    body: '{"event":"pending"}',
    bodyTruncated: true,
    reason: 'Accept when returned false.',
  };

  it('shows live poll progress with a countdown', () => {
    vi.useFakeTimers();
    render(
      <LastRunTab
        node={poller}
        status='running'
        detail={{
          progress: 'attempt 2/5 · condition false',
          live: { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll-live')).toHaveTextContent(
      'Last status 202 · condition false · 12s left',
    );
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByTestId('last-run-poll-live')).toHaveTextContent('9s left');
  });

  it('hides the live line once the node finished and stops its timer', () => {
    vi.useFakeTimers();
    const live = { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 };
    const { rerender } = render(<LastRunTab node={poller} status='running' detail={{ live }} />);
    expect(vi.getTimerCount()).toBe(1);
    rerender(<LastRunTab node={poller} status='success' detail={{ statusCode: 200 }} />);
    expect(screen.queryByTestId('last-run-poll-live')).not.toBeInTheDocument();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('shows the poll result after the run', () => {
    render(
      <LastRunTab
        node={poller}
        status='failed'
        detail={{
          error: 'condition not met after 30 attempts (60.0s)',
          trace: {
            poll: {
              attempts: 30,
              maxAttempts: 30,
              lastStatusCode: 202,
              conditionMet: false,
              elapsedMs: 60000,
              timeoutMs: 60000,
            },
          },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll')).toHaveTextContent(
      'Last status 202 · condition false · 30 of 30 attempts',
    );
  });

  it('says when a poll reached no verdict', () => {
    render(
      <LastRunTab
        node={poller}
        status='failed'
        detail={{
          trace: {
            poll: {
              attempts: 1,
              maxAttempts: 5,
              lastStatusCode: 200,
              elapsedMs: 10,
              timeoutMs: 60000,
            },
          },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-poll')).toHaveTextContent('no verdict');
  });

  it('shows ignored calls and a countdown while waiting', () => {
    vi.useFakeTimers();
    render(
      <LastRunTab
        node={wait}
        status='running'
        detail={{ live: { ignored: 2, remainingMs: 42000, lastRejected: rejected } }}
      />,
    );
    expect(screen.getByTestId('last-run-wait-live')).toHaveTextContent('2 ignored · 42s left');
    expect(screen.getByTestId('last-run-rejected')).toBeInTheDocument();
  });

  it('keeps the last rejected call collapsed until asked', async () => {
    render(
      <LastRunTab
        node={wait}
        status='running'
        detail={{ live: { ignored: 1, remainingMs: 42000, lastRejected: rejected } }}
      />,
    );
    expect(screen.queryByLabelText('Body viewer')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Last rejected call/ }));
    const section = screen.getByTestId('last-run-rejected');
    expect(section).toHaveTextContent('POST /cb/…?event=pending');
    expect(section).toHaveTextContent('Accept when returned false.');
    expect(section).toHaveTextContent('Authorization');
    expect(section).toHaveTextContent('••••••');
    expect(section).toHaveTextContent('Body truncated.');
    expect(await screen.findByLabelText('Body viewer')).toBeInTheDocument();
  });

  it('shows the wait result after a timeout', () => {
    render(
      <LastRunTab
        node={wait}
        status='failed'
        detail={{
          error: 'no matching callback within 60s (3 ignored)',
          trace: { wait: { ignored: 3, timeoutMs: 60000, lastRejected: rejected } },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-wait')).toHaveTextContent('3 ignored');
    expect(screen.getByTestId('last-run-rejected')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-wait-live')).not.toBeInTheDocument();
  });
});
