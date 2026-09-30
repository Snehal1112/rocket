import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowDebugRequest, FlowDebugResponse, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
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
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy response body' }));
    expect(writeText).toHaveBeenCalledWith('{"token":"abc"}');
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
