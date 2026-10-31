import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'src');
const css = readFileSync(join(root, 'globals.css'), 'utf8');

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.(ts|tsx)$/.test(entry.name) && !/\.test\./.test(entry.name) ? [path] : [];
  });
}

describe('border consistency', () => {
  it('uses the plain border token, not opacity variants, on solid borders', () => {
    const offenders = sourceFiles(root).flatMap((file) =>
      readFileSync(file, 'utf8')
        .split('\n')
        .filter((line) => /(?<![:\w-])border-border\/\d+/.test(line) && !line.includes('border-dashed'))
        .map((line) => `${file}: ${line.trim()}`),
    );
    expect(offenders).toEqual([]);
  });

  it('draws dark cards with one 1px border and no extra ring', () => {
    const card = /\.dark \[data-slot="card"\]\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
    expect(card).toContain('border-color: hsl(var(--border))');
    expect(card).toMatch(/box-shadow:\s*none/);
  });

  it('draws dark nested cards with the same border colour', () => {
    const nested = /\.dark \[data-slot="card"\] \[data-slot="card"\]\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
    expect(nested).toContain('border-color: hsl(var(--border))');
  });
});
