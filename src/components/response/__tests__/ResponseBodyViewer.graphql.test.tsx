import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ResponseState } from '@/types/pane-types';
import { ResponseBodyViewer } from '../ResponseBodyViewer';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value }: { value: string }) => <pre data-testid='monaco'>{value}</pre>,
}));

function response(body: string, extra: Partial<ResponseState> = {}): ResponseState {
  return {
    status: 200,
    statusText: 'OK',
    headers: [{ id: 'h', key: 'content-type', value: 'application/json', enabled: true }],
    body,
    durationMs: 5,
    ttfbMs: 3,
    sizeBytes: body.length,
    activeView: 'pretty',
    ...extra,
  };
}

describe('ResponseBodyViewer for GraphQL', () => {
  it('shows Data and Errors tabs only for a graphql response', () => {
    const { rerender } = render(<ResponseBodyViewer response={response('{"data":{"a":1}}')} />);
    expect(screen.queryByRole('tab', { name: /^Data/ })).toBeNull();

    rerender(
      <ResponseBodyViewer response={response('{"data":{"a":1}}', { protocol: 'graphql' })} />,
    );
    expect(screen.getByRole('tab', { name: /^Data/ })).toBeTruthy();
    expect(screen.getByRole('tab', { name: /^Errors/ })).toBeTruthy();
  });

  it('puts the error count on the Errors tab and lists each message', async () => {
    const body = JSON.stringify({
      data: null,
      errors: [{ message: 'boom', path: ['a', 'b'] }, { message: 'second' }],
    });
    render(
      <ResponseBodyViewer
        response={response(body, { protocol: 'graphql', activeView: 'errors' })}
      />,
    );
    expect(screen.getByRole('tab', { name: /Errors\s*\(2\)/ })).toBeTruthy();
    expect(screen.getByText('boom')).toBeTruthy();
    expect(screen.getByText('a.b')).toBeTruthy();
    expect(screen.getByText('second')).toBeTruthy();
  });

  it('shows only the data member in the Data tab', async () => {
    const body = JSON.stringify({ data: { a: 1 }, extensions: { cost: 3 } });
    render(<ResponseBodyViewer response={response(body, { protocol: 'graphql' })} />);
    await userEvent.click(screen.getByRole('tab', { name: /^Data/ }));
    const shown = screen.getByTestId('monaco').textContent ?? '';
    expect(JSON.parse(shown)).toEqual({ a: 1 });
  });
});
