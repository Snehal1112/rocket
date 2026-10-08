import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { setVariableContextEffect, variableContextField } from '../variable-context-facet';
import {
  buildVariableHoverDom,
  truncateForHover,
  variableHover,
  variableHoverSource,
} from '../variable-hover';
import { findVarTokenAt, openPopoverEffect, variablePopoverExtension } from '../variable-popover';

const CANARY = 'sk-live-canary-do-not-show-9f3a';

const entry = (over: Partial<VariableScopeEntry> = {}): VariableScopeEntry => ({
  value: 'https://api.test',
  source: 'environment',
  label: 'dev',
  secret: false,
  ...over,
});

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(doc: string, context: Map<string, VariableScopeEntry>) {
  container = document.createElement('div');
  document.body.appendChild(container);
  const state = EditorState.create({
    doc,
    extensions: [variableContextField, variablePopoverExtension(), variableHover()],
  });
  view = new EditorView({ state, parent: container });
  view.dispatch({ effects: setVariableContextEffect.of(context) });
  return view;
}

// The tooltip DOM for the token at `pos`, or null when there is none.
function hoverDom(v: EditorView, pos: number, side: -1 | 1 = 1): HTMLElement | null {
  const tip = variableHoverSource(v, pos, side);
  return tip ? tip.create(v).dom : null;
}

describe('findVarTokenAt', () => {
  it('finds the token under a position, inclusive of both edges', () => {
    expect(findVarTokenAt('a {{host}} b', 5)).toEqual({ varName: 'host', from: 2, to: 10 });
    expect(findVarTokenAt('a {{host}} b', 2)?.varName).toBe('host');
    expect(findVarTokenAt('a {{host}} b', 10)?.varName).toBe('host');
    expect(findVarTokenAt('a {{host}} b', 11)).toBeNull();
  });
});

describe('truncateForHover', () => {
  it('keeps a short value and cuts a long one with an ellipsis', () => {
    expect(truncateForHover('short')).toBe('short');
    const cut = truncateForHover('a'.repeat(500));
    expect(cut.length).toBe(201);
    expect(cut.endsWith('…')).toBe(true);
  });
});

describe('buildVariableHoverDom', () => {
  it('shows the scope label and the value of a resolved variable', () => {
    const dom = buildVariableHoverDom('host', entry());
    expect(dom.textContent).toContain('dev');
    expect(dom.textContent).toContain('https://api.test');
  });

  it('masks a secret and keeps the value out of the whole DOM, including attributes', () => {
    const dom = buildVariableHoverDom('apiKey', entry({ value: CANARY, secret: true }));
    expect(dom.textContent).toContain('●●●●');
    expect(dom.textContent).not.toContain(CANARY);
    expect(dom.outerHTML).not.toContain(CANARY);
  });

  it('sets no title and no data attributes on any element', () => {
    const dom = buildVariableHoverDom('host', entry({ value: CANARY }));
    for (const el of [dom, ...Array.from(dom.querySelectorAll('*'))]) {
      for (const name of el.getAttributeNames()) {
        expect(name === 'title' || name.startsWith('data-')).toBe(false);
      }
    }
  });

  it('renders the value as text, never as markup', () => {
    const dom = buildVariableHoverDom('x', entry({ value: '<img src=x onerror=alert(1)>' }));
    expect(dom.querySelector('img')).toBeNull();
    expect(dom.textContent).toContain('<img src=x onerror=alert(1)>');
  });

  it('shows an empty value as (empty)', () => {
    expect(buildVariableHoverDom('x', entry({ value: '' })).textContent).toContain('(empty)');
  });

  it('shows Unresolved for a missing variable', () => {
    expect(buildVariableHoverDom('nope', undefined).textContent).toContain('Unresolved');
  });

  it('describes a known dynamic variable and treats an unknown $name as unresolved', () => {
    expect(buildVariableHoverDom('$guid', undefined).textContent).toContain(
      'Dynamic ($guid), generated at send',
    );
    expect(buildVariableHoverDom('$notAThing', undefined).textContent).toContain('Unresolved');
  });

  it('truncates a long value', () => {
    const dom = buildVariableHoverDom('x', entry({ value: 'a'.repeat(500) }));
    expect(dom.textContent).not.toContain('a'.repeat(250));
    expect(dom.textContent).toContain('…');
  });
});

describe('variableHoverSource', () => {
  const ctx = new Map([['host', entry()]]);

  it('returns a tooltip anchored to the token, for a position inside it', () => {
    const v = createView('go {{host}} now', ctx);
    const tip = variableHoverSource(v, 6, 1);
    expect(tip).not.toBeNull();
    expect(tip?.pos).toBe(3);
    expect(tip?.end).toBe(11);
    expect(hoverDom(v, 6)?.textContent).toContain('https://api.test');
  });

  it('returns nothing off a token', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 1, 1)).toBeNull();
    expect(variableHoverSource(v, 13, 1)).toBeNull();
  });

  it('needs the pointer on the token at its edges', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 3, -1)).toBeNull();
    expect(variableHoverSource(v, 3, 1)).not.toBeNull();
    expect(variableHoverSource(v, 11, 1)).toBeNull();
    expect(variableHoverSource(v, 11, -1)).not.toBeNull();
  });

  it('shows nothing while the click popover is open', () => {
    const v = createView('go {{host}} now', ctx);
    expect(variableHoverSource(v, 6, 1)).not.toBeNull();
    v.dispatch({
      effects: openPopoverEffect.of({
        varName: 'host',
        from: 3,
        to: 11,
        tokenType: 'variable',
        entry: ctx.get('host'),
      }),
    });
    expect(variableHoverSource(v, 6, 1)).toBeNull();
  });

  it('shows Unresolved for a token the context does not know', () => {
    const v = createView('{{ghost}}', new Map());
    expect(hoverDom(v, 4)?.textContent).toContain('Unresolved');
  });

  it('keeps a secret value out of the tooltip DOM', () => {
    const v = createView(
      '{{apiKey}}',
      new Map([['apiKey', entry({ value: CANARY, secret: true })]]),
    );
    const dom = hoverDom(v, 4);
    expect(dom?.textContent).toContain('●●●●');
    expect(dom?.outerHTML).not.toContain(CANARY);
  });
});

describe('variableHover extension', () => {
  it('installs in an editor without throwing', () => {
    const v = createView('{{host}}', new Map([['host', entry()]]));
    expect(v.dom.querySelector('.cm-content')).not.toBeNull();
  });
});
