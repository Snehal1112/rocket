/**
 * Automated screenshot capture for the Rocket user manual.
 *
 * Starts the Vite dev server, navigates through each feature screen,
 * and captures screenshots in both light and dark mode.
 *
 * Usage: npx tsx scripts/capture-screenshots.ts
 */

import { type ChildProcess, spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Browser, chromium, type Page } from 'playwright';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const BASE_URL = 'http://localhost:1420';
const SCREENSHOT_DIR = resolve(__dirname, '../docs/manual/screenshots');
const VIEWPORT = { width: 1440, height: 900 };

// Wait for the dev server to respond.
async function waitForServer(url: string, timeoutMs = 30_000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // Server not ready yet.
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`Dev server did not respond at ${url} within ${timeoutMs}ms`);
}

// Toggle dark mode by adding/removing the class on documentElement.
async function setTheme(page: Page, theme: 'light' | 'dark'): Promise<void> {
  await page.evaluate((t) => {
    document.documentElement.classList.toggle('dark', t === 'dark');
    localStorage.setItem('rocket-theme', t);
  }, theme);
  // Let the theme transition settle.
  await page.waitForTimeout(300);
}

// Take a screenshot with a descriptive name.
async function capture(page: Page, name: string, theme: 'light' | 'dark'): Promise<void> {
  const path = `${SCREENSHOT_DIR}/${name}-${theme}.png`;
  await page.screenshot({ path, fullPage: false });
  console.log(`  Captured: ${name}-${theme}.png`);
}

// Capture a screen in both light and dark mode.
async function captureBoth(page: Page, name: string): Promise<void> {
  await setTheme(page, 'light');
  await capture(page, name, 'light');
  await setTheme(page, 'dark');
  await capture(page, name, 'dark');
}

// Click a tab by its text content.
async function clickTab(page: Page, text: string): Promise<void> {
  await page.getByRole('tab', { name: text }).first().click();
  await page.waitForTimeout(200);
}

// Feature screenshot definitions.
async function captureAllScreens(page: Page): Promise<void> {
  // 1. Full app overview — branded empty state.
  console.log('Capturing: app overview (empty state)');
  await captureBoth(page, '01-app-overview');

  // 2. Open a new request tab via the "New Request" button.
  console.log('Opening new request tab...');
  const newRequestBtn = page.getByRole('button', { name: /new request/i }).first();
  if (await newRequestBtn.isVisible()) {
    await newRequestBtn.click();
    await page.waitForTimeout(500);
  }

  // 3. Request builder — URL bar, method selector, send button.
  console.log('Capturing: request builder');
  const urlInput = page.locator('input[placeholder*="api.example.com"]').first();
  if (await urlInput.isVisible()) {
    await urlInput.fill('https://api.example.com/users');
  }
  await captureBoth(page, '02-request-builder');

  // 3. Params tab.
  console.log('Capturing: params editor');
  await clickTab(page, 'Params');
  await captureBoth(page, '03-params-editor');

  // 4. Headers tab.
  console.log('Capturing: headers editor');
  await clickTab(page, 'Headers');
  await captureBoth(page, '04-headers-editor');

  // 5. Body tab — JSON mode.
  console.log('Capturing: body editor');
  await clickTab(page, 'Body');
  await page.waitForTimeout(500);
  await captureBoth(page, '05-body-editor');

  // 6. Auth tab.
  console.log('Capturing: auth editor');
  await clickTab(page, 'Auth');
  await captureBoth(page, '06-auth-editor');

  // 7. Response panel — if a response exists, capture it.
  // Send a request to localhost to populate the response.
  console.log('Capturing: response panel');
  const sendButton = page.getByRole('button', { name: /send/i }).first();
  if (await sendButton.isVisible()) {
    // Fill a simple URL that will give us a response to show.
    if (await urlInput.isVisible()) {
      await urlInput.fill('https://httpbin.org/json');
    }
    await sendButton.click();
    // Wait for response to load.
    await page.waitForTimeout(3000);
    await captureBoth(page, '07-response-pretty');

    // Switch to Raw tab in response.
    const rawTab = page.locator('button').filter({ hasText: /^raw$/i }).first();
    if (await rawTab.isVisible()) {
      await rawTab.click();
      await page.waitForTimeout(300);
      await captureBoth(page, '08-response-raw');
    }

    // Switch to Headers tab in response.
    const headersTab = page
      .locator('button')
      .filter({ hasText: /^headers$/i })
      .last();
    if (await headersTab.isVisible()) {
      await headersTab.click();
      await page.waitForTimeout(300);
      await captureBoth(page, '09-response-headers');
    }
  }

  // 8. Collections sidebar.
  console.log('Capturing: collections sidebar');
  await captureBoth(page, '10-collections-sidebar');

  // 9. Environment switcher.
  console.log('Capturing: environment switcher');
  await captureBoth(page, '11-environment-switcher');
}

async function main(): Promise<void> {
  let devServer: ChildProcess | null = null;
  let browser: Browser | null = null;

  try {
    // Start Vite dev server.
    console.log('Starting Vite dev server...');
    devServer = spawn('npx', ['vite', '--port', '1420'], {
      cwd: resolve(__dirname, '..'),
      stdio: 'pipe',
      shell: true,
    });

    devServer.stderr?.on('data', (data) => {
      const msg = data.toString();
      if (msg.includes('error') || msg.includes('Error')) {
        console.error('Vite error:', msg);
      }
    });

    await waitForServer(BASE_URL);
    console.log('Dev server ready.\n');

    // Launch browser.
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ viewport: VIEWPORT });
    const page = await context.newPage();

    // Navigate to app.
    await page.goto(BASE_URL, { waitUntil: 'networkidle' });
    // Wait for the splash screen to dismiss.
    await page.waitForTimeout(3000);
    // Click anywhere to dismiss splash if still showing.
    await page.mouse.click(VIEWPORT.width / 2, VIEWPORT.height / 2);
    await page.waitForTimeout(500);

    // Capture all screens.
    await captureAllScreens(page);

    console.log('\nAll screenshots captured successfully.');
  } catch (err) {
    console.error('Screenshot capture failed:', err);
    process.exitCode = 1;
  } finally {
    if (browser) await browser.close();
    if (devServer) {
      devServer.kill('SIGTERM');
      // Give it a moment to clean up.
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
}

main();
