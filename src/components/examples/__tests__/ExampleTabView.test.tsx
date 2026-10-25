import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ExampleTabView } from '@/components/examples/ExampleTabView';
import type { CollectionChangedEvent, RequestExample } from '@/lib/tauri-api';
import { listRequestExamples, onCollectionChanged } from '@/lib/tauri-api';
import { useLayoutStore } from '@/stores/layout-store';
import { usePaneStore } from '@/stores/pane-store';
import type { ExampleTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listRequestExamples: vi.fn(), onCollectionChanged: vi.fn() };
});
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, readOnly }: { value: string; readOnly?: boolean }) => (
    <pre data-testid='monaco' data-readonly={String(readOnly)}>
      {value}
    </pre>
  ),
}));

const tab: ExampleTab = {
  id: 'example:1',
  title: '401',
  tabType: 'example',
  source: { collection: 'col', path: 'users.yml' },
  exampleName: '401',
  isDirty: false,
};

const full: RequestExample = {
  name: '401',
  request: {
    method: 'POST',
    url: 'https://api.example.com/users',
    params: [{ name: 'page', value: '2', type: 'query' }],
    headers: [{ name: 'X-Trace', value: 'abc' }],
    body: { type: 'json', data: '{"a":1}' },
  },
  response: {
    status: 401,
    statusText: 'Unauthorized',
    headers: [{ name: 'content-type', value: 'application/json' }],
    body: { type: 'json', data: '{"error":"no"}' },
  },
};

let changeHandler: ((e: CollectionChangedEvent) => void) | null = null;

describe('ExampleTabView', () => {
  beforeEach(() => {
    changeHandler = null;
    vi.mocked(listRequestExamples).mockReset();
    vi.mocked(onCollectionChanged).mockReset();
    vi.mocked(onCollectionChanged).mockImplementation(async (handler) => {
      changeHandler = handler;
      return () => undefined;
    });
  });

  it('try_opens_a_temporary_tab_with_the_example', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([full]);
    const openTryTab = vi.fn().mockResolvedValue(undefined);
    usePaneStore.setState({ openTryTab });
    render(<ExampleTabView tab={tab} />);

    await userEvent.click(await screen.findByRole('button', { name: 'Try' }));
    expect(openTryTab).toHaveBeenCalledWith('col', 'users.yml', full);
  });

  it('follows_the_request_layout_setting', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([full]);
    const { container } = render(<ExampleTabView tab={tab} />);
    await screen.findByRole('region', { name: 'Request' });
    expect(container.querySelector('.grid-rows-2')).not.toBeNull();

    act(() => useLayoutStore.getState().setRequestLayout('side-by-side'));
    expect(container.querySelector('.grid-cols-2')).not.toBeNull();
    expect(container.querySelector('.grid-rows-2')).toBeNull();
    act(() => useLayoutStore.getState().setRequestLayout('stacked'));
  });

  it('renders_request_and_response_panes', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([{ name: '200 OK' }, full]);
    render(<ExampleTabView tab={tab} />);

    const request = await screen.findByRole('region', { name: 'Request' });
    const response = screen.getByRole('region', { name: 'Response' });
    expect(listRequestExamples).toHaveBeenCalledWith('col', 'users.yml');

    expect(within(request).getByText('POST')).toBeInTheDocument();
    expect(within(request).getByText('https://api.example.com/users')).toBeInTheDocument();
    for (const name of ['Params', 'Headers', 'Body']) {
      expect(within(request).getByRole('tab', { name })).toBeInTheDocument();
    }
    for (const name of ['Response', 'Headers']) {
      expect(within(response).getByRole('tab', { name })).toBeInTheDocument();
    }
    expect(screen.getByText('401 Unauthorized')).toBeInTheDocument();

    // Params show first, the other request tabs open on click.
    expect(within(request).getByText('page')).toBeInTheDocument();
    await userEvent.click(within(request).getByRole('tab', { name: 'Headers' }));
    expect(within(request).getByText('X-Trace')).toBeInTheDocument();
    await userEvent.click(within(request).getByRole('tab', { name: 'Body' }));
    expect(within(request).getByTestId('monaco').textContent).toBe('{\n  "a": 1\n}');
    expect(within(request).getByTestId('monaco')).toHaveAttribute('data-readonly', 'true');

    expect(within(response).getByTestId('monaco').textContent).toContain('"error"');
    await userEvent.click(within(response).getByRole('tab', { name: 'Headers' }));
    expect(within(response).getByText('content-type')).toBeInTheDocument();
  });

  it('missing_parts_do_not_crash', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([{ name: '401' }]);
    render(<ExampleTabView tab={tab} />);

    const request = await screen.findByRole('region', { name: 'Request' });
    const response = screen.getByRole('region', { name: 'Response' });
    expect(within(request).getByText('No params')).toBeInTheDocument();
    await userEvent.click(within(request).getByRole('tab', { name: 'Headers' }));
    expect(within(request).getByText('No headers')).toBeInTheDocument();
    await userEvent.click(within(request).getByRole('tab', { name: 'Body' }));
    expect(within(request).getByText('No body')).toBeInTheDocument();
    expect(within(response).getByText('No response body')).toBeInTheDocument();
    await userEvent.click(within(response).getByRole('tab', { name: 'Headers' }));
    expect(within(response).getByText('No headers')).toBeInTheDocument();
    expect(screen.getByText('No status')).toBeInTheDocument();
  });

  it('shows_not_found_when_the_example_was_deleted_elsewhere', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([{ name: '200 OK' }]);
    render(<ExampleTabView tab={tab} />);
    expect(await screen.findByText(/This example no longer exists/)).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Request' })).toBeNull();
  });

  it('refetches_when_the_request_is_saved_and_handles_a_deletion', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([full]);
    render(<ExampleTabView tab={tab} />);
    await screen.findByRole('region', { name: 'Request' });
    await waitFor(() => expect(changeHandler).not.toBeNull());

    // An event for another request is ignored.
    act(() => changeHandler?.({ type: 'requestSaved', collection: 'col', path: 'other.yml' }));
    expect(listRequestExamples).toHaveBeenCalledTimes(1);

    vi.mocked(listRequestExamples).mockResolvedValue([]);
    act(() => changeHandler?.({ type: 'requestSaved', collection: 'col', path: 'users.yml' }));
    expect(await screen.findByText(/This example no longer exists/)).toBeInTheDocument();
    expect(listRequestExamples).toHaveBeenCalledTimes(2);
  });

  it.each([
    ['a number', 42, '42'],
    ['an object', { a: 1 }, '{\n  "a": 1\n}'],
    ['a list', [1, 2], '[\n  1,\n  2\n]'],
  ])('a_hand_written_body_data_that_is_%s_renders_as_text', async (_label, data, shown) => {
    vi.mocked(listRequestExamples).mockResolvedValue([
      {
        name: '401',
        request: { body: { type: 'json', data } },
        response: { status: 200, body: { type: 'text', data } },
      },
    ] as unknown as RequestExample[]);
    render(<ExampleTabView tab={tab} />);
    const response = await screen.findByRole('region', { name: 'Response' });
    expect(within(response).getByTestId('monaco').textContent).toBe(shown);
    const request = screen.getByRole('region', { name: 'Request' });
    await userEvent.click(within(request).getByRole('tab', { name: 'Body' }));
    expect(within(request).getByTestId('monaco').textContent).toBe(shown);
  });

  it('a_null_body_data_shows_the_empty_states', async () => {
    vi.mocked(listRequestExamples).mockResolvedValue([
      {
        name: '401',
        request: { body: { type: 'json', data: null } },
        response: { status: 200, body: { type: 'json', data: null } },
      },
    ] as unknown as RequestExample[]);
    render(<ExampleTabView tab={tab} />);
    const request = await screen.findByRole('region', { name: 'Request' });
    expect(within(screen.getByRole('region', { name: 'Response' })).getByText('No response body')).toBeInTheDocument();
    await userEvent.click(within(request).getByRole('tab', { name: 'Body' }));
    expect(within(request).getByText('No body')).toBeInTheDocument();
  });
});
