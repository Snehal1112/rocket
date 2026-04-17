import { useEffect, useRef } from 'react';

/**
 * Converts the browser's current cursor position inside `el` to a flat
 * character offset, treating the whole subtree as a plain-text stream.
 * Text nodes contribute their length; element nodes contribute their
 * textContent length (badge spans fall into this category).
 * Returns 0 if no selection is active or el is empty.
 */
export function saveCaret(el: HTMLElement): number {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return 0;

  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer)) return 0;
  let offset = 0;

  function walk(node: Node): boolean {
    if (node === range.startContainer) {
      offset += range.startOffset;
      return true; // done
    }
    if (node.nodeType === Node.TEXT_NODE) {
      offset += (node as Text).length;
      return false;
    }
    for (const child of node.childNodes) {
      if (walk(child)) return true;
    }
    return false;
  }

  walk(el);
  return offset;
}

/**
 * Places the browser cursor at `targetOffset` (flat character offset) inside `el`.
 * If the offset falls inside a badge span, the caret is placed at the text node
 * immediately before the span to prevent the caret from getting stuck inside it.
 */
export function restoreCaret(el: HTMLElement, targetOffset: number): void {
  const sel = window.getSelection();
  if (!sel) return;

  let remaining = targetOffset;
  let targetNode: Node | null = null;
  let localOffset = 0;

  function walk(node: Node): boolean {
    if (node.nodeType === Node.TEXT_NODE) {
      const len = (node as Text).length;
      if (remaining <= len) {
        targetNode = node;
        localOffset = remaining;
        return true;
      }
      remaining -= len;
      return false;
    }
    // Element node (badge span or other): traverse children but never
    // place the caret inside a [data-badge] span — land before it instead.
    if ((node as Element).hasAttribute?.('data-badge')) {
      const len = (node.textContent ?? '').length;
      if (remaining < len) {
        // Caret would land inside this badge — place it at the preceding text
        // node boundary instead. Find the previous sibling text node.
        let prev = node.previousSibling;
        while (prev && prev.nodeType !== Node.TEXT_NODE) {
          prev = prev.previousSibling;
        }
        if (prev) {
          targetNode = prev;
          localOffset = (prev as Text).length;
        } else {
          // No preceding text node — place at start of parent.
          targetNode = node.parentNode;
          localOffset = 0;
        }
        return true;
      }
      remaining -= len;
      return false;
    }
    for (const child of node.childNodes) {
      if (walk(child)) return true;
    }
    return false;
  }

  walk(el);

  if (!targetNode) {
    // Offset past end of content — place at last text node.
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    let last: Node | null = null;
    while (walker.nextNode()) last = walker.currentNode;
    if (last) {
      targetNode = last;
      localOffset = (last as Text).length;
    } else {
      return; // Empty element.
    }
  }

  try {
    const range = document.createRange();
    range.setStart(targetNode, localOffset);
    range.collapse(true);
    sel.removeAllRanges();
    sel.addRange(range);
  } catch {
    // Ignore stale node errors during rapid updates.
  }
}

/**
 * Reads the editor div's current DOM and returns the raw user string,
 * collapsing the node structure back into plain text.
 * Text nodes → their text. Element nodes → their textContent.
 */
export function serializeToText(el: HTMLElement): string {
  let result = '';
  for (const node of Array.from(el.childNodes)) {
    result += node.textContent ?? '';
  }
  return result;
}

export interface EditorToken {
  type: 'text' | 'badge';
  /** Display text — for badge tokens this is the full `{{name}}` or `:param` string. */
  content: string;
  rawLength: number;
  /** CSS classes applied to the badge span. Only present when type === 'badge'. */
  badgeClass?: string;
  /** Index stored as data-token-idx on the span, for popover targeting. */
  tokenIdx?: number;
}

/**
 * Imperatively diffs el.childNodes against tokens and mutates the DOM to match.
 * Unchanged nodes (same type and content) are left in place to preserve the
 * browser's internal caret tracking. After mutation, restores the caret to caretOffset.
 */
export function renderTokens(
  el: HTMLElement,
  tokens: EditorToken[],
  caretOffset: number,
): void {
  const desired: Node[] = tokens.map((token) => {
    if (token.type === 'text') {
      return document.createTextNode(token.content);
    }
    const span = document.createElement('span');
    span.setAttribute('data-badge', '');
    span.setAttribute('data-token-idx', String(token.tokenIdx ?? 0));
    if (token.badgeClass) span.className = token.badgeClass;
    span.textContent = token.content;
    return span;
  });

  const current = Array.from(el.childNodes);

  // Replace or insert nodes that differ.
  desired.forEach((node, i) => {
    const existing = current[i];
    if (!existing) {
      el.appendChild(node);
      return;
    }
    const sameType =
      node.nodeType === existing.nodeType &&
      (node.nodeType !== Node.ELEMENT_NODE ||
        (node as Element).tagName === (existing as Element).tagName);
    const sameContent = node.textContent === existing.textContent;
    const sameClass =
      node.nodeType !== Node.ELEMENT_NODE ||
      (node as Element).className === (existing as Element).className;
    const sameTokenIdx =
      node.nodeType !== Node.ELEMENT_NODE ||
      (node as Element).getAttribute('data-token-idx') ===
        (existing as Element).getAttribute('data-token-idx');

    if (sameType && sameContent && sameClass && sameTokenIdx) return; // Unchanged — leave it alone.
    el.replaceChild(node, existing);
  });

  // Remove extra nodes.
  while (el.childNodes.length > desired.length) {
    el.removeChild(el.lastChild!);
  }

  restoreCaret(el, caretOffset);
}

export interface UseContentEditableInputOptions {
  /** The editor div DOM element (must be stable across renders — use a ref). */
  editorEl: HTMLElement | null;
  value: string;
  onChange: (value: string) => void;
  tokens: EditorToken[];
  /** Called before the hook's paste handler. Return true if the event was fully handled. */
  onBeforePaste?: (e: ClipboardEvent) => boolean;
}

/**
 * Wires a contenteditable div to a React-controlled string value.
 * Also returns event handlers for use with React's synthetic event system.
 */
export function useContentEditableInput({
  editorEl,
  value,
  onChange,
  tokens,
  onBeforePaste,
}: UseContentEditableInputOptions) {
  const isComposing = useRef(false);
  // Keep a stable ref to onChange so event listeners don't need re-registration.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onBeforePasteRef = useRef(onBeforePaste);
  onBeforePasteRef.current = onBeforePaste;

  // Sync DOM → state when the user types (not during IME composition).
  function onInput() {
    if (isComposing.current) return;
    if (!editorEl) return;
    onChangeRef.current(serializeToText(editorEl));
  }

  function onCompositionStart() {
    isComposing.current = true;
  }

  function onCompositionEnd() {
    isComposing.current = false;
    if (!editorEl) return;
    onChangeRef.current(serializeToText(editorEl));
  }

  function onPaste(e: ClipboardEvent) {
    if (onBeforePasteRef.current?.(e)) return; // Caller handled it (e.g. cURL import).
    e.preventDefault();
    const plain = e.clipboardData?.getData('text/plain') ?? '';
    // insertText is the standard cross-browser way to insert at caret in contenteditable.
    document.execCommand('insertText', false, plain);
    if (!editorEl) return;
    onChangeRef.current(serializeToText(editorEl));
  }

  // Ejects the caret from badge spans when arrow-key navigation moves it inside one.
  useEffect(() => {
    const el = editorEl;
    if (!el) return;

    const onSelectionChange = () => {
      const sel = window.getSelection();
      if (!sel || sel.rangeCount === 0) return;
      // Only act when the selection is inside this specific editor element.
      if (!sel.anchorNode || !el.contains(sel.anchorNode)) return;
      const anchor = sel.anchorNode;
      // Check if the anchor is inside a [data-badge] span.
      const badge =
        anchor.nodeType === Node.ELEMENT_NODE
          ? (anchor as Element).closest('[data-badge]')
          : anchor.parentElement?.closest('[data-badge]');
      if (!badge) return;
      // Eject: place caret at end of preceding text node.
      let prev = badge.previousSibling;
      while (prev && prev.nodeType !== Node.TEXT_NODE) {
        prev = prev.previousSibling;
      }
      if (prev) {
        const range = document.createRange();
        range.setStart(prev, (prev as Text).length);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
      }
    };

    document.addEventListener('selectionchange', onSelectionChange);
    return () => {
      document.removeEventListener('selectionchange', onSelectionChange);
    };
  }, [editorEl]);

  // Keep a stable ref to the handlers so DOM listeners always call the latest versions.
  const handlersRef = useRef({ onInput, onCompositionStart, onCompositionEnd, onPaste });
  handlersRef.current = { onInput, onCompositionStart, onCompositionEnd, onPaste };

  // Attach DOM event listeners so raw DOM events (e.g. from tests) also work.
  useEffect(() => {
    if (!editorEl) return;

    const listener_input = () => handlersRef.current.onInput();
    const listener_compositionstart = () => handlersRef.current.onCompositionStart();
    const listener_compositionend = () => handlersRef.current.onCompositionEnd();
    const listener_paste = (e: Event) => handlersRef.current.onPaste(e as ClipboardEvent);

    editorEl.addEventListener('input', listener_input);
    editorEl.addEventListener('compositionstart', listener_compositionstart);
    editorEl.addEventListener('compositionend', listener_compositionend);
    editorEl.addEventListener('paste', listener_paste);

    return () => {
      editorEl.removeEventListener('input', listener_input);
      editorEl.removeEventListener('compositionstart', listener_compositionstart);
      editorEl.removeEventListener('compositionend', listener_compositionend);
      editorEl.removeEventListener('paste', listener_paste);
    };
  }, [editorEl]);

  // Sync state → DOM when value changes from outside. The equality guard
  // prevents a DOM rewrite (and caret reset) when the change originated
  // from onInput — in that case the DOM is already correct.
  useEffect(() => {
    if (!editorEl) return;
    if (serializeToText(editorEl) === value) return;
    const offset = saveCaret(editorEl);
    renderTokens(editorEl, tokens, offset);
  }, [value, tokens, editorEl]);

  return { onInput, onCompositionStart, onCompositionEnd, onPaste };
}
