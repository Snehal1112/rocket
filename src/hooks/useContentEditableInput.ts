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
