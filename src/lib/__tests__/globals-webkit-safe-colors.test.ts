import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// WebKitGTK 2.44 hangs while painting color-mix(in oklab, rgb(189 16 0), transparent).
// Tailwind compiles bg-destructive/10 to that mix, and the delete confirm bars use it.
// So the dark colours below must never be exactly rgb(189, 16, 0), which is 5 100% 37%.
const css = readFileSync(resolve(process.cwd(), 'src/globals.css'), 'utf8');
const darkBlock = css.slice(css.indexOf('\n  .dark {'));

function darkToken(name: string): string | undefined {
  return new RegExp(`${name}:\\s*([^;]+);`).exec(darkBlock)?.[1]?.trim();
}

describe('dark theme colours that WebKitGTK 2.44 cannot paint with an opacity modifier', () => {
  it.each(['--destructive', '--chart-5'])('%s is not 5 100% 37%', (name) => {
    expect(darkToken(name)).toBeDefined();
    expect(darkToken(name)).not.toBe('5 100% 37%');
  });
});
