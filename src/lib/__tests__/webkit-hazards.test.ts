import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// WebKitGTK on Linux hangs on some color-mix() paints and has compositing turned off, so blur does
// nothing there. These checks guard the whole app, not only the elevation rules.

const srcRoot = resolve(process.cwd(), 'src');
const css = readFileSync(join(srcRoot, 'globals.css'), 'utf8');

function sourceFiles(): string[] {
  return (readdirSync(srcRoot, { recursive: true }) as string[])
    .filter((f) => /\.(tsx?|css)$/.test(f))
    .filter((f) => !/(^|[\\/])__tests__[\\/]|\.test\.tsx?$/.test(f))
    .map((f) => join(srcRoot, f));
}

// The blur helpers that already exist, plus the fallback that switches them off. Nothing else may blur.
const BLUR_RULES = /\.backdrop-blur-(?:sm|md)(?:,\s*\.backdrop-blur-(?:sm|md))?\s*\{[^}]*\}/g;

export const hasColorMix = (text: string) => /color-mix\s*\(/.test(text);
// Comments, the blur helpers, and the fallback that turns the blur off (`backdrop-filter: none`).
export const withoutBlurHelpers = (text: string) =>
  text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(BLUR_RULES, '')
    .replace(/@supports not \(backdrop-filter:[^)]*\)/g, '')
    .replace(/(?:-webkit-)?backdrop-filter:\s*none;?/g, '');

// Existing uses, mixing two opaque theme colours (not a colour with transparent). Do not add to this.
const KNOWN_COLOR_MIX = ['components/contracts/ContractCard.tsx'];
// An opacity modifier on an elevation colour would compile to color-mix().
export const elevationOpacity = (text: string) =>
  text.match(/\b[a-z]+-elevation-(?:sunken|surface|raised|overlay)\/\d+/g) ?? [];

describe('detectors', () => {
  it('see color-mix, stray blur and elevation opacity in samples', () => {
    expect(hasColorMix('a { color: color-mix(in oklab, red, transparent); }')).toBe(true);
    expect(hasColorMix('a { color: red; }')).toBe(false);
    expect(withoutBlurHelpers('.card { backdrop-filter: blur(4px); }')).toContain('backdrop-filter');
    expect(withoutBlurHelpers('.backdrop-blur-sm { backdrop-filter: blur(8px); }')).not.toContain(
      'backdrop-filter',
    );
    expect(withoutBlurHelpers('/* backdrop-filter note */ .a { color: red; }')).not.toContain(
      'backdrop-filter',
    );
    expect(elevationOpacity("className='bg-elevation-sunken/50 border-border'")).toEqual([
      'bg-elevation-sunken/50',
    ]);
    expect(elevationOpacity("className='bg-elevation-sunken'")).toEqual([]);
  });
});

describe('WebKitGTK hazards', () => {
  it('has no color-mix() in globals.css', () => {
    expect(hasColorMix(css)).toBe(false);
  });

  it('has no backdrop-filter outside the existing blur helpers', () => {
    expect(withoutBlurHelpers(css)).not.toMatch(/backdrop-filter/);
  });

  it('has no new hand-written color-mix() in any source file', () => {
    const offenders = sourceFiles()
      .filter((f) => hasColorMix(readFileSync(f, 'utf8')))
      .map((f) => f.slice(srcRoot.length + 1).replace(/\\/g, '/'))
      .filter((f) => !KNOWN_COLOR_MIX.includes(f));
    expect(offenders).toEqual([]);
  });

  it('puts no opacity modifier on an elevation colour', () => {
    const offenders = sourceFiles().filter((f) => elevationOpacity(readFileSync(f, 'utf8')).length > 0);
    expect(offenders).toEqual([]);
  });
});
