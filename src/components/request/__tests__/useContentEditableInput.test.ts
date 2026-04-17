import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  type EditorToken,
  restoreCaret,
  saveCaret,
  serializeToText,
  useContentEditableInput,
} from '@/hooks/useContentEditableInput';

function makeEditor(html: string): HTMLDivElement {
  const div = document.createElement('div');
  div.contentEditable = 'true';
  div.innerHTML = html;
  document.body.appendChild(div);
  return div;
}

function placeCaretAt(node: Node, offset: number) {
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  // biome-ignore lint/style/noNonNullAssertion: getSelection() is always non-null in jsdom
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
}

describe('serializeToText', () => {
  it('reads plain text node', () => {
    const el = makeEditor('hello');
    expect(serializeToText(el)).toBe('hello');
    el.remove();
  });

  it('reads text + badge span', () => {
    const el = makeEditor('Bearer <span data-badge>{{token}}</span>');
    expect(serializeToText(el)).toBe('Bearer {{token}}');
    el.remove();
  });
});

describe('saveCaret', () => {
  it('returns flat offset in plain text node', () => {
    const el = makeEditor('hello world');
    // biome-ignore lint/style/noNonNullAssertion: makeEditor always produces a text child node
    placeCaretAt(el.firstChild!, 5);
    expect(saveCaret(el)).toBe(5);
    el.remove();
  });

  it('returns flat offset past a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    // 'ab' = 2 chars, '{{x}}' = 5 chars, place caret at offset 1 in 'cd'
    // biome-ignore lint/style/noNonNullAssertion: makeEditor with this html always has a trailing text node
    const cdNode = el.lastChild!;
    placeCaretAt(cdNode, 1);
    expect(saveCaret(el)).toBe(8); // 2 + 5 + 1
    el.remove();
  });
});

describe('restoreCaret', () => {
  it('places caret at correct offset in plain text', () => {
    const el = makeEditor('hello');
    restoreCaret(el, 3);
    // biome-ignore lint/style/noNonNullAssertion: getSelection() is always non-null in jsdom
    const sel = window.getSelection()!;
    expect(sel.anchorOffset).toBe(3);
    el.remove();
  });

  it('does not place caret inside a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    // Offset 3 falls inside the badge (ab=2, then char 0 of {{x}})
    restoreCaret(el, 3);
    // biome-ignore lint/style/noNonNullAssertion: getSelection() is always non-null in jsdom
    const sel = window.getSelection()!;
    // Caret should be at the text node 'ab', offset 2 (end of it)
    expect(sel.anchorNode?.textContent).toBe('ab');
    expect(sel.anchorOffset).toBe(2);
    el.remove();
  });
});

describe('useContentEditableInput', () => {
  it('calls onChange when the user types', () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    document.body.appendChild(el);

    const onChange = vi.fn();
    const tokens: EditorToken[] = [{ type: 'text', content: 'hello', rawLength: 5 }];

    renderHook(() => useContentEditableInput({ editorEl: el, value: 'hello', onChange, tokens }));

    // Simulate a user typing: modify DOM then fire input event.
    act(() => {
      el.textContent = 'hello!';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    });

    expect(onChange).toHaveBeenCalledWith('hello!');
    el.remove();
  });

  it('does not call onChange during IME composition', () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    document.body.appendChild(el);

    const onChange = vi.fn();
    renderHook(() => useContentEditableInput({ editorEl: el, value: '', onChange, tokens: [] }));

    act(() => {
      el.dispatchEvent(new CompositionEvent('compositionstart'));
      el.textContent = 'interim';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    });

    expect(onChange).not.toHaveBeenCalled();

    act(() => {
      el.dispatchEvent(new CompositionEvent('compositionend'));
    });

    expect(onChange).toHaveBeenCalledWith('interim');
    el.remove();
  });
});

describe('selectionchange caret guard', () => {
  it('ejects caret from inside a badge span when selection moves into it', async () => {
    const el = document.createElement('div');
    el.contentEditable = 'true';
    el.innerHTML = 'ab<span data-badge data-token-idx="0">{{x}}</span>cd';
    document.body.appendChild(el);

    const onChange = vi.fn();
    const tokens: EditorToken[] = [
      { type: 'text', content: 'ab', rawLength: 2 },
      { type: 'badge', content: '{{x}}', rawLength: 5, tokenIdx: 0 },
      { type: 'text', content: 'cd', rawLength: 2 },
    ];

    renderHook(() =>
      useContentEditableInput({ editorEl: el, value: 'ab{{x}}cd', onChange, tokens }),
    );

    // Simulate caret landing inside the badge span.
    // biome-ignore lint/style/noNonNullAssertion: badge is present in the innerHTML set above
    const badge = el.querySelector('[data-badge]')!;
    const range = document.createRange();
    // biome-ignore lint/style/noNonNullAssertion: badge span always has a text child node
    range.setStart(badge.firstChild!, 1); // Inside '{{x}}', offset 1.
    range.collapse(true);
    // biome-ignore lint/style/noNonNullAssertion: getSelection() is always non-null in jsdom
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);

    // Fire a selectionchange event to trigger the guard.
    document.dispatchEvent(new Event('selectionchange'));

    // Wait one microtask for the guard to run.
    await new Promise((r) => setTimeout(r, 0));

    // biome-ignore lint/style/noNonNullAssertion: getSelection() is always non-null in jsdom
    const finalSel = window.getSelection()!;
    // Caret must no longer be inside the badge span.
    const anchorNode = finalSel.anchorNode;
    const isInsideBadge =
      anchorNode?.nodeType === Node.ELEMENT_NODE
        ? (anchorNode as Element).hasAttribute('data-badge') ||
          (anchorNode as Element).closest('[data-badge]') !== null
        : anchorNode?.parentElement?.closest('[data-badge]') !== null;
    expect(isInsideBadge).toBe(false);

    el.remove();
  });
});
