import { describe, expect, it } from 'vitest';
import { saveCaret, restoreCaret, serializeToText } from '@/hooks/useContentEditableInput';

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
    placeCaretAt(el.firstChild!, 5);
    expect(saveCaret(el)).toBe(5);
    el.remove();
  });

  it('returns flat offset past a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    // 'ab' = 2 chars, '{{x}}' = 5 chars, place caret at offset 1 in 'cd'
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
    const sel = window.getSelection()!;
    expect(sel.anchorOffset).toBe(3);
    el.remove();
  });

  it('does not place caret inside a badge span', () => {
    const el = makeEditor('ab<span data-badge>{{x}}</span>cd');
    // Offset 3 falls inside the badge (ab=2, then char 0 of {{x}})
    restoreCaret(el, 3);
    const sel = window.getSelection()!;
    // Caret should be at the text node 'ab', offset 2 (end of it)
    expect(sel.anchorNode?.textContent).toBe('ab');
    expect(sel.anchorOffset).toBe(2);
    el.remove();
  });
});
