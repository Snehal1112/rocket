import { resolve } from 'path';
import type { Page } from '@playwright/test';

const MOCK_SCRIPT_PATH = resolve('./e2e/helpers/tauri-mock.js');

/**
 * Inject the Tauri mock and navigate to the app.
 * Must be called at the start of every test.
 */
export async function setupPage(page: Page): Promise<void> {
  // Load the mock as a plain JS file to avoid TypeScript template-literal
  // transformation issues in Playwright's module bundler.
  await page.addInitScript({ path: MOCK_SCRIPT_PATH });
  await page.goto('/');
  // Wait until the React app has hydrated (sidebar visible)
  await page.waitForSelector('[data-slot="tree-item"]', { timeout: 15_000 });
}

/**
 * Reset mock contract state between tests via page.evaluate.
 */
export async function resetContracts(page: Page): Promise<void> {
  await page.evaluate(() => {
    if ((window as Record<string, unknown>).__e2eMockState) {
      ((window as Record<string, unknown>).__e2eMockState as { contracts: unknown[] }).contracts = [];
    }
    delete (window as Record<string, unknown>).__e2eDriftMode;
  });
}

/**
 * Tell the mock that the next recompute_drift call should return drift/breach.
 */
export async function setDriftMode(page: Page, mode: 'drift' | 'breach'): Promise<void> {
  await page.evaluate((m) => {
    (window as Record<string, unknown>).__e2eDriftMode = m;
  }, mode);
}
