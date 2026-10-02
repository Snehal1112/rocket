import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// Read the stylesheet as plain text, so the test sees the real tokens.
const css = readFileSync(resolve(process.cwd(), 'src/globals.css'), 'utf8');

// WCAG minimum contrast for normal text.
const MIN_CONTRAST = 4.5;

type Hsl = { h: number; s: number; l: number };

function readToken(block: string, name: string): Hsl {
  const match = block.match(
    new RegExp(`--${name}:\\s*(\\d+(?:\\.\\d+)?)\\s+(\\d+(?:\\.\\d+)?)%\\s+(\\d+(?:\\.\\d+)?)%`),
  );
  if (!match) throw new Error(`Token --${name} not found`);
  return { h: Number(match[1]), s: Number(match[2]), l: Number(match[3]) };
}

function toRgb({ h, s, l }: Hsl): [number, number, number] {
  const sat = s / 100;
  const light = l / 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(light, 1 - light);
  const f = (n: number) => light - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0), f(8), f(4)];
}

function luminance(color: Hsl): number {
  const [r, g, b] = toRgb(color).map((v) =>
    v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: Hsl, b: Hsl): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

const darkStart = css.indexOf('.dark {');
const themes = {
  light: css.slice(css.indexOf(':root {'), darkStart),
  dark: css.slice(darkStart),
};

const pairs = [
  { type: 'error', fg: 'toast-error-fg', bg: 'destructive-soft' },
  { type: 'success', fg: 'toast-success-fg', bg: 'success-soft' },
  { type: 'warning', fg: 'toast-warning-fg', bg: 'warning-soft' },
] as const;

describe('toast text contrast', () => {
  for (const [theme, block] of Object.entries(themes)) {
    for (const { type, fg, bg } of pairs) {
      it(`${type} toast text is readable in the ${theme} theme`, () => {
        const ratio = contrast(readToken(block, fg), readToken(block, bg));
        expect(ratio).toBeGreaterThanOrEqual(MIN_CONTRAST);
      });
    }
  }
});
