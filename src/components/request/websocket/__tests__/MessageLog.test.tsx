import { render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MessageLog } from '@/components/request/websocket/MessageLog';
import type { MessageLogEntry } from '@/types/message-log';

const entry = (id: string): MessageLogEntry => ({
  id,
  direction: 'in',
  kind: 'text',
  data: id,
  size: 1,
  timestampMs: 0,
});

describe('MessageLog', () => {
  afterEach(() => vi.restoreAllMocks());

  it('keeps following new entries when the capped log length stays the same', () => {
    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(500);
    const { container, rerender } = render(
      <MessageLog entries={[entry('a'), entry('b')]} onClear={vi.fn()} />,
    );
    const scroller = container.querySelector('.overflow-auto') as HTMLElement;
    scroller.scrollTop = 0;

    // The oldest entry was dropped and a new one added: same length, different newest id.
    rerender(<MessageLog entries={[entry('b'), entry('c')]} onClear={vi.fn()} />);

    expect(scroller.scrollTop).toBe(500);
  });
});
