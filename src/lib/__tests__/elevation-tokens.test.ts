import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const css = readFileSync(resolve(process.cwd(), 'src/globals.css'), 'utf8');

type Hsl = [number, number, number];

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// Returns the body of the first rule whose selector is exactly `selector`.
function cssBlock(source: string, selector: string): string {
  const match = new RegExp(`^\\s*${escapeRegExp(selector).replace(/,\s/g, ',\\s*')}\\s*\\{`, 'm').exec(source);
  if (!match) throw new Error(`No rule for ${selector}`);
  const start = match.index + match[0].length;
  return source.slice(start, source.indexOf('}', start));
}

function hslTriple(block: string, name: string): Hsl {
  const match = new RegExp(`${escapeRegExp(name)}:\\s*([\\d.]+)\\s+([\\d.]+)%\\s+([\\d.]+)%`).exec(block);
  if (!match) throw new Error(`No HSL triple for ${name}`);
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function luminance([h, s, l]: Hsl): number {
  const sat = s / 100;
  const light = l / 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(light, 1 - light);
  const channel = (n: number) => {
    const c = light - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(8) + 0.0722 * channel(4);
}

function contrast(a: Hsl, b: Hsl): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const LEVELS = ['sunken', 'surface', 'raised', 'overlay'] as const;
const light = cssBlock(css, ':root');
const dark = cssBlock(css, '.dark');

const maxAlpha = (value: string) =>
  Math.max(...[...value.matchAll(/rgba\([^)]*,\s*([\d.]+)\)/g)].map((m) => Number(m[1])));

const shadowOf = (block: string, name: string) => {
  const match = new RegExp(`${name}:\\s*([^;]+);`).exec(block);
  if (!match) throw new Error(`No shadow ${name}`);
  return match[1];
};

describe('elevation tokens', () => {
  it('defines all four surfaces in light and dark', () => {
    for (const block of [light, dark]) {
      for (const level of LEVELS) {
        expect(() => hslTriple(block, `--elevation-${level}-bg`)).not.toThrow();
      }
    }
  });

  it('uses VS Code 2026 Dark colours for the four levels and never steps darker going up', () => {
    const lightness = LEVELS.map((level) => hslTriple(dark, `--elevation-${level}-bg`)[2]);
    // Sunken is the editor, default and raised are the sidebar colour, overlay is editorWidget.
    expect(lightness).toEqual([7.5, 10, 10, 13]);
    for (let i = 1; i < lightness.length; i++) {
      expect(lightness[i]).toBeGreaterThanOrEqual(lightness[i - 1]);
    }
  });

  it('keeps the raised level on the sidebar colour in both themes', () => {
    expect(hslTriple(dark, '--elevation-raised-bg')).toEqual([210, 5, 10]);
    expect(hslTriple(light, '--elevation-raised-bg')).toEqual([240, 50, 99]);
  });

  it("keeps light surfaces on today's values", () => {
    expect(hslTriple(light, '--elevation-sunken-bg')).toEqual([0, 0, 100]);
    expect(hslTriple(light, '--elevation-surface-bg')).toEqual([220, 21.4, 94.5]);
    expect(hslTriple(light, '--elevation-raised-bg')).toEqual([240, 50, 99]);
    expect(hslTriple(light, '--elevation-overlay-bg')).toEqual([0, 0, 100]);
  });

  it('defines raised and overlay shadows in both themes, darker in dark', () => {
    for (const name of ['--elevation-shadow-raised', '--elevation-shadow-overlay']) {
      expect(maxAlpha(shadowOf(dark, name))).toBeGreaterThan(maxAlpha(shadowOf(light, name)));
    }
  });

  it('keeps body text at 4.5:1 or better on every dark level', () => {
    const fg = hslTriple(dark, '--foreground');
    for (const level of LEVELS) {
      expect(contrast(fg, hslTriple(dark, `--elevation-${level}-bg`))).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('keeps secondary text at 4.5:1 on overlay in dark and leaves the sidebar text alone', () => {
    const scoped = cssBlock(css, '.dark .elevation-overlay');
    const muted = hslTriple(scoped, '--muted-foreground');
    expect(muted).toEqual([0, 0, 56]);
    expect(contrast(muted, hslTriple(dark, '--elevation-overlay-bg'))).toBeGreaterThanOrEqual(4.5);
    // The raised level is the sidebar, whose text colour must not change.
    expect(css).not.toMatch(/\.dark \.elevation-raised/);
  });

  it('gives each class its surface, and raised and overlay their shadow', () => {
    for (const level of LEVELS) {
      const block = cssBlock(css, `.elevation-${level}`);
      expect(block).toContain(`background-color: hsl(var(--elevation-${level}-bg))`);
      if (level === 'raised' || level === 'overlay') {
        expect(block).toContain(`box-shadow: var(--elevation-shadow-${level})`);
      } else {
        expect(block).not.toContain('box-shadow');
      }
    }
  });

  it('forbids color-mix and backdrop-filter in elevation rules', () => {
    const rules = css.match(/[^{}]*elevation[^{}]*\{[^}]*\}/g) ?? [];
    expect(rules.length).toBeGreaterThan(0);
    for (const rule of rules) {
      expect(rule).not.toMatch(/color-mix|backdrop-filter/);
    }
  });
});

describe('elevation Tailwind colours', () => {
  it('exposes the four surfaces as Tailwind colours', async () => {
    // @ts-expect-error The config is plain JavaScript without types.
    const config = (await import('../../../tailwind.config.js')).default;
    expect(config.theme.extend.colors.elevation).toEqual({
      sunken: 'hsl(var(--elevation-sunken-bg))',
      surface: 'hsl(var(--elevation-surface-bg))',
      raised: 'hsl(var(--elevation-raised-bg))',
      overlay: 'hsl(var(--elevation-overlay-bg))',
    });
  });
});

describe('raised sidebar', () => {
  it('aliases --sidebar-bg to the raised token in both themes', () => {
    for (const block of [light, dark]) {
      expect(block).toMatch(/--sidebar-bg:\s*var\(--elevation-raised-bg\)/);
    }
  });

  it('no longer switches the sidebar shadow off in dark', () => {
    expect(css).not.toContain('sidebar-elevation');
  });

  it('puts elevation-raised on the sidebar wrapper and keeps its stacking', () => {
    const app = readFileSync(resolve(process.cwd(), 'src/App.tsx'), 'utf8');
    expect(app).toContain('elevation-raised w-(--sidebar-w) shrink-0 relative z-[5]');
    expect(app).not.toContain('sidebar-elevation');
  });
});

describe('default surface', () => {
  it('aliases the dark shell tokens to the default surface', () => {
    for (const name of ['--background', '--statusbar-bg', '--panel-bg', '--titlebar-bg']) {
      expect(dark).toMatch(new RegExp(`${name}:\\s*var\\(--elevation-surface-bg\\)`));
    }
  });

  it('keeps the same visible default surface value', () => {
    expect(hslTriple(dark, '--elevation-surface-bg')).toEqual([210, 5, 10]);
  });
});

describe('cards', () => {
  it('keeps dark cards flat on the shell colour and nested cards on --accent', () => {
    expect(css).toMatch(/\.dark \[data-slot="card"\]\s*\{[^}]*background-color:\s*hsl\(var\(--background\)\)/);
    expect(css).toMatch(/\.dark \[data-slot="card"\] \[data-slot="card"\]\s*\{[^}]*hsl\(var\(--accent\)\)/);
  });
});
