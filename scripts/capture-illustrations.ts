/**
 * Captures each SVG illustration as a standalone PNG in both themes.
 *
 * Usage: npx tsx scripts/capture-illustrations.ts
 */

import { chromium } from 'playwright';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const OUTPUT_DIR = resolve(__dirname, '../src/components/illustrations');

const ILLUSTRATIONS = [
  'RocketLaunch',
  'RocketOrbit',
  'RocketIdle',
  'RocketSearch',
  'RocketError',
  'RocketMinimal',
];

// CSS vars for light and dark themes (matches index.css).
const THEMES: Record<string, Record<string, string>> = {
  light: {
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
  dark: {
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
};

// Build an HTML page that renders one SVG illustration using CSS vars.
function buildPage(svgName: string, theme: 'light' | 'dark'): string {
  const vars = THEMES[theme];
  const cssVars = Object.entries(vars).map(([k, v]) => `${k}: ${v};`).join('\n    ');
  const bg = theme === 'light' ? '#f5f7fa' : '#1e1e1e';

  // We load the illustration from the preview page's inline SVGs.
  // For simplicity, render a centered container and inject via the illustrations.html data.
  return `<!DOCTYPE html>
<html><head><style>
  :root { ${cssVars} }
  body { margin: 0; display: flex; align-items: center; justify-content: center;
         width: 300px; height: 300px; background: ${bg}; }
  svg { width: 240px; height: 240px; }
</style></head>
<body><div id="root"></div></body></html>`;
}

async function main(): Promise<void> {
  const browser = await chromium.launch({ headless: true });

  // Read the illustrations.html to extract SVG markup for each illustration.
  const fs = await import('fs');
  const htmlPath = resolve(__dirname, '../public/illustrations.html');
  const htmlContent = fs.readFileSync(htmlPath, 'utf-8');

  // Parse SVG blocks from the illustrations data in the HTML file.
  const svgMap = new Map<string, string>();
  const regex = /name:\s*'(\w+)'[\s\S]*?svg:\s*`([\s\S]*?)`/g;
  let match;
  while ((match = regex.exec(htmlContent)) !== null) {
    svgMap.set(match[1], match[2].trim());
  }

  for (const name of ILLUSTRATIONS) {
    const svg = svgMap.get(name);
    if (!svg) {
      console.log(`  Skipped: ${name} (not found in illustrations.html)`);
      continue;
    }

    for (const theme of ['light', 'dark'] as const) {
      const page = await browser.newPage({ viewport: { width: 300, height: 300 } });
      const vars = THEMES[theme];
      const cssVars = Object.entries(vars).map(([k, v]) => `${k}: ${v};`).join(' ');
      const bg = theme === 'light' ? '#f5f7fa' : '#1e1e1e';

      const html = `<!DOCTYPE html>
<html><head><style>
  :root { ${cssVars} }
  body { margin: 0; display: flex; align-items: center; justify-content: center;
         width: 300px; height: 300px; background: ${bg}; }
  svg { width: 240px; height: 240px; }
</style></head>
<body>${svg}</body></html>`;

      await page.setContent(html, { waitUntil: 'networkidle' });
      await page.waitForTimeout(200);

      const path = `${OUTPUT_DIR}/${name}-${theme}.png`;
      await page.screenshot({ path });
      console.log(`  Captured: ${name}-${theme}.png`);
      await page.close();
    }
  }

  await browser.close();
  console.log('\nAll illustration previews captured.');
}

main();
