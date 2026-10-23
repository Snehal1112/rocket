import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ResponseState } from '@/types/pane-types';
import { ResponseBodyViewer } from '../ResponseBodyViewer';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value }: { value: string }) => <pre data-testid='monaco'>{value}</pre>,
}));

const response: ResponseState = {
  status: 200,
  statusText: 'OK',
  headers: [],
  body: '{"a":1}',
  durationMs: 5,
  ttfbMs: 3,
  sizeBytes: 7,
  activeView: 'pretty',
};

describe('ResponseBodyViewer save as example', () => {
  it('renders disabled with the reason as title', () => {
    render(
      <ResponseBodyViewer
        response={response}
        onSaveAsExample={vi.fn()}
        saveExampleDisabledReason='Save the request first'
      />,
    );
    const button = screen.getByRole('button', { name: 'Save as example' }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toBe('Save the request first');
  });

  it('calls the callback when enabled', async () => {
    const onSave = vi.fn();
    render(
      <ResponseBodyViewer
        response={response}
        onSaveAsExample={onSave}
        saveExampleDisabledReason={null}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Save as example' }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('hides the button without a callback', () => {
    render(<ResponseBodyViewer response={response} />);
    expect(screen.queryByRole('button', { name: 'Save as example' })).toBeNull();
  });
});

describe('ResponseBodyViewer empty body', () => {
  const noBody: ResponseState = { ...response, status: 204, statusText: 'No Content', body: '' };

  it('says the response has no body and points to the headers', () => {
    render(<ResponseBodyViewer response={noBody} />);
    expect(screen.getByText('The response has no body')).toBeInTheDocument();
    expect(screen.getByText(/See the Headers tab/)).toBeInTheDocument();
  });

  it('does not use an offline icon for a body-less answer', () => {
    const { container } = render(<ResponseBodyViewer response={noBody} />);
    expect(container.querySelector('.lucide-wifi-off')).toBeNull();
  });

  it('says there is nothing to preview on the Preview tab', async () => {
    render(<ResponseBodyViewer response={noBody} />);
    await userEvent.click(screen.getByRole('tab', { name: /preview/i }));
    expect(screen.getByText('Nothing to preview')).toBeInTheDocument();
  });
});
