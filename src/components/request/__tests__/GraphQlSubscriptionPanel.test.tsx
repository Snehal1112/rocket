import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GraphQlSubscriptionPanel } from '@/components/request/GraphQlSubscriptionPanel';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

// CodeMirror does not run in jsdom; a stand-in keeps the panel testable.
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
      aria-label='Connection params'
      placeholder={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

function tab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.graphql = { query: 'subscription { n }', variables: '', connectionParams: '' };
  return {
    id: 'tab-1',
    title: 'Updates',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
  };
}

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('GraphQlSubscriptionPanel', () => {
  it('invites the user to subscribe when there is nothing yet', () => {
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);
    expect(screen.getByText('Subscribe to see streamed results here.')).toBeInTheDocument();
    expect(screen.getByText('Not subscribed')).toBeInTheDocument();
  });

  it('shows the status and the streamed results with their labels', () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: 'graphql-transport-ws',
          error: null,
          log: [
            {
              id: 'a',
              direction: 'in',
              label: 'next',
              kind: 'text',
              data: '{"data":1}',
              size: 10,
              timestampMs: 1000,
            },
            {
              id: 'b',
              direction: 'in',
              label: 'complete',
              kind: 'text',
              data: '',
              size: 0,
              timestampMs: 2000,
            },
          ],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);

    expect(screen.getByText('Subscribed')).toBeInTheDocument();
    expect(screen.getByText('graphql-transport-ws')).toBeInTheDocument();
    expect(screen.getByText('next')).toBeInTheDocument();
    expect(screen.getByText('complete')).toBeInTheDocument();
    expect(screen.getByText('{"data":1}')).toBeInTheDocument();
  });

  it('a failed subscription shows its reason', () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: null,
          status: 'failed',
          subprotocol: null,
          error: 'the server reported an error',
          log: [],
        },
      },
      tabBySession: {},
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);
    expect(screen.getByText('Failed')).toBeInTheDocument();
    expect(screen.getByText('the server reported an error')).toBeInTheDocument();
  });

  it('clearing the log empties only the log', async () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: null,
          error: null,
          log: [
            {
              id: 'a',
              direction: 'in',
              label: 'next',
              kind: 'text',
              data: 'x',
              size: 1,
              timestampMs: 1,
            },
          ],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Clear log' }));

    expect(useWebSocketStore.getState().byTab['tab-1'].log).toEqual([]);
    expect(useWebSocketStore.getState().byTab['tab-1'].sessionId).toBe('s1');
  });

  it('edits to the connection params are reported', async () => {
    const onChange = vi.fn();
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={onChange} />);
    await userEvent.setup().type(screen.getByLabelText('Connection params'), '{{');
    expect(onChange).toHaveBeenCalledWith('{');
  });
});
