import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { METHOD_FILL } from '../SyncBadge';

type Hsl = [number, number, number];

function hslToRgb([h, s, l]: Hsl): [number, number, number] {
  const sat = s / 100;
  const lig = l / 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(lig, 1 - lig);
  const f = (n: number) => lig - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
  return [f(0), f(8), f(4)];
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(a: [number, number, number], b: [number, number, number]): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const css = readFileSync(resolve(__dirname, '../../../../globals.css'), 'utf8');

/** The method tokens of the first block (light) or the `.dark` block that defines them. */
function tokens(theme: 'light' | 'dark'): Record<string, Hsl> {
  const all = [...css.matchAll(/--color-method-([a-z]+):\s*(\d+)\s+(\d+)%\s+(\d+)%;/g)];
  const out: Record<string, Hsl> = {};
  // The light block comes first in the file, the dark block second.
  const seen = new Set<string>();
  for (const m of all) {
    const name = m[1];
    const isDark = seen.has(name);
    seen.add(name);
    if ((theme === 'dark') === isDark) out[name] = [Number(m[2]), Number(m[3]), Number(m[4])];
  }
  return out;
}

const WHITE: [number, number, number] = [1, 1, 1];
const INK: [number, number, number] = [0x11 / 255, 0x18 / 255, 0x27 / 255];

/** Picks the fill and text of a theme from the class string, the way Tailwind resolves it. */
function pick(classes: string, theme: 'light' | 'dark', token: Hsl) {
  const prefix = theme === 'dark' ? 'dark:' : '';
  let fill = token;
  let text: [number, number, number] = WHITE;
  for (const c of classes.split(/\s+/)) {
    const isDarkClass = c.startsWith('dark:');
    if (isDarkClass && theme === 'light') continue;
    const bare = isDarkClass ? c.slice(prefix.length) : c;
    const lit = bare.match(/^bg-\[hsl\((\d+)_(\d+)%_(\d+)%\)\]$/);
    if (lit) fill = [Number(lit[1]), Number(lit[2]), Number(lit[3])];
    else if (bare.startsWith('bg-[hsl(var(')) fill = token;
    if (bare === 'text-white') text = WHITE;
    if (bare === 'text-[#111827]') text = INK;
  }
  return { fill, text };
}

describe('method pill contrast', () => {
  it('the css defines seven tokens per theme', () => {
    expect(Object.keys(tokens('light'))).toHaveLength(7);
    expect(Object.keys(tokens('dark'))).toHaveLength(7);
  });

  for (const theme of ['light', 'dark'] as const) {
    for (const [method, classes] of Object.entries(METHOD_FILL)) {
      it(`${method} is readable in ${theme}`, () => {
        const token = tokens(theme)[method.toLowerCase()];
        expect(token).toBeDefined();
        const { fill, text } = pick(classes, theme, token);
        expect(contrast(text, hslToRgb(fill))).toBeGreaterThanOrEqual(4.5);
      });
    }
  }
});
