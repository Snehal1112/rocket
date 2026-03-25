/**
 * Captures each SVG illustration as a standalone PNG in both themes.
 * Reads the TSX files, extracts SVG markup, and renders via Playwright.
 *
 * Usage: npx tsx scripts/capture-illustrations.ts
 */

import { chromium } from 'playwright';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { readFileSync, readdirSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const ILLUST_DIR = resolve(__dirname, '../src/components/illustrations');
const OUTPUT_DIR = ILLUST_DIR;

const THEMES = {
  light: {
    bg: '#f5f7fa',
    vars: {
      '--color-primary': 'hsl(213, 88%, 42%)',
      '--color-primary-foreground': 'hsl(0, 0%, 100%)',
      '--color-background': 'hsl(214, 44%, 98%)',
      '--color-foreground': 'hsl(222, 47%, 11%)',
      '--color-muted': 'hsl(213, 35%, 94%)',
      '--color-muted-foreground': 'hsl(215, 17%, 40%)',
      '--color-accent': 'hsl(212, 86%, 94%)',
      '--color-destructive': 'hsl(0, 84%, 60%)',
      '--color-destructive-foreground': 'hsl(0, 0%, 98%)',
      '--color-border': 'hsl(214, 31%, 88%)',
    },
  },
  dark: {
    bg: '#1e1e1e',
    vars: {
      '--color-primary': 'hsl(207, 100%, 42%)',
      '--color-primary-foreground': 'hsl(0, 0%, 98%)',
      '--color-background': 'hsl(0, 0%, 12%)',
      '--color-foreground': 'hsl(0, 0%, 87%)',
      '--color-muted': 'hsl(0, 0%, 14%)',
      '--color-muted-foreground': 'hsl(0, 0%, 60%)',
      '--color-accent': 'hsl(204, 100%, 18%)',
      '--color-destructive': 'hsl(0, 72%, 45%)',
      '--color-destructive-foreground': 'hsl(0, 0%, 98%)',
      '--color-border': 'hsl(0, 0%, 19%)',
    },
  },
};

// Tailwind class to inline CSS attribute mapping.
function convertClassName(className: string): string {
  const attrs: string[] = [];

  for (const cls of className.split(/\s+/)) {
    const m = cls.match(/^(fill|stroke)-(.+?)(?:\/(\d+))?$/);
    if (!m) continue;
    const [, prop, color, opacity] = m;
    const op = opacity ? parseInt(opacity) / 100 : 1;

    // Map color names to CSS values.
    const colorMap: Record<string, string> = {
      'primary': 'var(--color-primary)',
      'primary-foreground': 'var(--color-primary-foreground)',
      'background': 'var(--color-background)',
      'foreground': 'var(--color-foreground)',
      'muted': 'var(--color-muted)',
      'muted-foreground': 'var(--color-muted-foreground)',
      'accent': 'var(--color-accent)',
      'destructive': 'var(--color-destructive)',
      'destructive-foreground': 'var(--color-destructive-foreground)',
      'border': 'var(--color-border)',
    };

    const rgbaMap: Record<string, string> = {
      'white': '255,255,255',
      'orange-400': '251,146,60',
      'yellow-400': '250,204,21',
      'emerald-400': '52,211,153',
      'purple-400': '192,132,252',
      'red-400': '248,113,113',
      'green-400': '74,222,128',
      'blue-400': '96,165,250',
    };

    if (colorMap[color]) {
      if (op < 1) {
        attrs.push(`${prop}="color-mix(in srgb, ${colorMap[color]} ${Math.round(op * 100)}%, transparent)"`);
      } else {
        attrs.push(`${prop}="${colorMap[color]}"`);
      }
    } else if (rgbaMap[color]) {
      attrs.push(`${prop}="rgba(${rgbaMap[color]},${op})"`);
    }
  }

  return attrs.join(' ');
}

// Extract SVG from a TSX file and convert Tailwind classes to inline attrs.
function extractSvg(tsxContent: string): string {
  // Find the <svg ...>...</svg> block.
  const svgMatch = tsxContent.match(/<svg[\s\S]*?<\/svg>/);
  if (!svgMatch) return '';

  let svg = svgMatch[0];

  // Remove JSX comments {/* ... */}.
  svg = svg.replace(/\{\/\*[\s\S]*?\*\/\}/g, '');

  // Convert className="..." to inline fill/stroke attrs.
  svg = svg.replace(/className="([^"]+)"/g, (_match, classes) => convertClassName(classes));

  // Convert JSX camelCase attrs to SVG kebab-case.
  svg = svg.replace(/strokeWidth=/g, 'stroke-width=');
  svg = svg.replace(/strokeLinecap=/g, 'stroke-linecap=');
  svg = svg.replace(/strokeLinejoin=/g, 'stroke-linejoin=');
  svg = svg.replace(/strokeDasharray=/g, 'stroke-dasharray=');
  svg = svg.replace(/fillRule=/g, 'fill-rule=');
  svg = svg.replace(/clipRule=/g, 'clip-rule=');

  // Expand .map() gear teeth for RocketConstruction (8 teeth at 45 degree intervals).
  if (svg.includes('{[0, 45, 90')) {
    // Big gear teeth.
    const bigTeeth = [0, 45, 90, 135, 180, 225, 270, 315]
      .map(a => `<rect x="-3" y="-15" width="6" height="6" rx="1" transform="rotate(${a})" fill="color-mix(in srgb, var(--color-muted-foreground) 20%, transparent)"/>`)
      .join('\n          ');
    svg = svg.replace(/\{[^}]*\[0,\s*45,\s*90[^}]*\.map\(\(angle\)[^}]*\)\}/s, bigTeeth);

    // Small gear teeth.
    const smallTeeth = [0, 60, 120, 180, 240, 300]
      .map(a => `<rect x="-2.5" y="-10" width="5" height="4" rx="1" transform="rotate(${a})" fill="color-mix(in srgb, var(--color-muted-foreground) 15%, transparent)"/>`)
      .join('\n          ');
    svg = svg.replace(/\{[^}]*\[0,\s*60,\s*120[^}]*\.map\(\(angle\)[^}]*\)\}/s, smallTeeth);
  }

  // Expand .map() clock marks for RocketClock (12 marks at 30 degree intervals).
  if (svg.includes('{[0, 30, 60')) {
    const marks = [0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330]
      .map(a => `<line x1="0" y1="-19" x2="0" y2="-17" transform="rotate(${a})" stroke="color-mix(in srgb, var(--color-muted-foreground) 25%, transparent)" stroke-width="1.5" stroke-linecap="round"/>`)
      .join('\n          ');
    svg = svg.replace(/\{[^}]*\[0,\s*30,\s*60[^}]*\.map\(\(angle\)[^}]*\)\}/s, marks);
  }

  // Remove any remaining JSX expressions like {name} or {' '}.
  svg = svg.replace(/\{[^}]*\}/g, '');

  return svg;
}

async function main(): Promise<void> {
  // Find all Rocket*.tsx files.
  const files = readdirSync(ILLUST_DIR)
    .filter(f => f.startsWith('Rocket') && f.endsWith('.tsx'))
    .sort();

  console.log(`Found ${files.length} illustrations.\n`);

  const browser = await chromium.launch({ headless: true });

  for (const file of files) {
    const name = file.replace('.tsx', '');
    const content = readFileSync(resolve(ILLUST_DIR, file), 'utf-8');
    const svg = extractSvg(content);

    if (!svg) {
      console.log(`  Skipped: ${name} (no SVG found)`);
      continue;
    }

    for (const [theme, config] of Object.entries(THEMES)) {
      const cssVars = Object.entries(config.vars).map(([k, v]) => `${k}: ${v};`).join(' ');

      const html = `<!DOCTYPE html><html><head><style>
        :root { ${cssVars} }
        body { margin:0; display:flex; align-items:center; justify-content:center;
               width:300px; height:300px; background:${config.bg}; }
        svg { width:240px; height:240px; }
      </style></head><body>${svg}</body></html>`;

      const page = await browser.newPage({ viewport: { width: 300, height: 300 } });
      await page.setContent(html, { waitUntil: 'networkidle' });
      await page.waitForTimeout(150);

      const path = `${OUTPUT_DIR}/${name}-${theme}.png`;
      await page.screenshot({ path });
      console.log(`  Captured: ${name}-${theme}.png`);
      await page.close();
    }
  }

  await browser.close();
  console.log(`\nAll illustration previews captured.`);
}

main();
