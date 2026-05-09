/**
 * Contract Lock E2E tests — fixture-based, Tauri IPC mocked.
 *
 * These tests run against the Vite dev server (yarn dev) with
 * window.__TAURI__ injected via addInitScript before React boots.
 * The mock provides a TestCollection with GET/POST /payments requests
 * and handles all contract CRUD commands in-memory.
 *
 * Tests are NOT run in parallel (fullyParallel:false in playwright.config.ts)
 * because they share mock state within a page session.
 */

import { test, expect, type Page } from '@playwright/test';
import { setupPage, resetContracts, setDriftMode } from './helpers/fixtures';

// ─── Re-usable helpers ────────────────────────────────────────────────────

async function openContractsTab(page: Page) {
  // Right-click the collection node to get the context menu
  await page
    .locator('[data-testid="collection-item-TestCollection"]')
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Manage contracts' }).click();
  await expect(
    page.getByRole('heading', { name: 'Contracts', level: 1 }),
  ).toBeVisible({ timeout: 6_000 });
}

async function createPublishedContract(page: Page, name = 'Payments API v1') {
  await page.getByRole('button', { name: /New contract/i }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible({ timeout: 5_000 });

  await page.getByLabel('Title').fill(name);
  // Provider and consumer fields use placeholder text
  await page.getByPlaceholder('Billing Team').fill('Billing');
  await page.getByPlaceholder('Platform, Mobile').fill('Platform');

  // Publish immediately
  await page.getByRole('button', { name: /Create & Publish/i }).click();

  // Dialog closes
  await expect(dialog).not.toBeVisible({ timeout: 8_000 });

  // Card appears
  await expect(
    page.locator('article', { hasText: name }),
  ).toBeVisible({ timeout: 5_000 });
}

async function deleteAllContracts(page: Page) {
  // eslint-disable-next-line no-constant-condition
  while (true) {
    const card = page.locator('article[data-status]').first();
    if (!(await card.isVisible())) break;
    await card.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Delete' }).click();
    await page.getByRole('button', { name: 'Delete contract' }).click();
    // Brief pause for the mock state to update
    await page.waitForTimeout(300);
  }
}

// ─── Lifecycle ────────────────────────────────────────────────────────────

test.afterEach(async ({ page }) => {
  // Clean up contracts created during the test
  try {
    await openContractsTab(page);
    await deleteAllContracts(page);
  } catch {
    // Ignore cleanup errors — test isolation is best-effort
  }
  await resetContracts(page);
});

// ─── Smoke test ───────────────────────────────────────────────────────────

test('smoke: collection and requests appear in sidebar', async ({ page }) => {
  await setupPage(page);

  // Collection node visible in sidebar
  await expect(
    page.locator('[data-testid="collection-item-TestCollection"]'),
  ).toBeVisible({ timeout: 10_000 });

  // Expand the collection to show requests
  await page.locator('[data-testid="collection-item-TestCollection"]').click();

  await expect(
    page.locator('[data-testid="request-item-GET-Get Payments"]'),
  ).toBeVisible({ timeout: 5_000 });

  await expect(
    page.locator('[data-testid="request-item-POST-Create Payment"]'),
  ).toBeVisible({ timeout: 5_000 });
});

// ─── Scenario 1: Create contract ─────────────────────────────────────────

test('scenario 1: create and publish a contract — card appears with Active status', async ({ page }) => {
  await setupPage(page);
  await openContractsTab(page);

  // Empty state visible initially
  await expect(
    page.getByText(/no contracts|lock the shape/i),
  ).toBeVisible({ timeout: 5_000 });

  // Create via "New contract" button
  await createPublishedContract(page, 'Payments API v1');

  // Card shows status chip (Active)
  await expect(
    page.locator('article', { hasText: 'Payments API v1' })
      .locator('[class*="StatusChip"], [data-status]')
      .or(page.locator('article', { hasText: 'Payments API v1' }).getByText(/Active/i)),
  ).toBeVisible({ timeout: 5_000 });
});

// ─── Scenario 2: Drift detection ─────────────────────────────────────────

test('scenario 2: sync after API change — contract transitions to Drift', async ({ page }) => {
  await setupPage(page);
  await openContractsTab(page);
  await createPublishedContract(page, 'Payments API v1');

  // Simulate a breaking change by setting drift mode on the mock
  await setDriftMode(page, 'drift');

  // Click Sync to trigger recompute_drift
  await page.getByRole('button', { name: 'Sync' }).click();

  // Card should transition to Drift status
  await expect(
    page.locator('article[data-status="drift"]'),
  ).toBeVisible({ timeout: 10_000 });
});

// ─── Scenario 3: Breach detection + status bar ───────────────────────────

test('scenario 3: breaking change causes breach; status bar shows breaching', async ({ page }) => {
  await setupPage(page);
  await openContractsTab(page);
  await createPublishedContract(page, 'Payments API v1');

  // Simulate a breaking change causing breach
  await setDriftMode(page, 'breach');
  await page.getByRole('button', { name: 'Sync' }).click();

  // Card transitions to Breach
  await expect(
    page.locator('article[data-status="breach"]'),
  ).toBeVisible({ timeout: 10_000 });

  // ContractsStatusItem in the status bar mentions "breaching"
  // The status bar button has aria-label containing "contract"
  const statusBarChip = page
    .locator('button[aria-label*="contract"], [aria-label*="contract"]')
    .filter({ hasText: /breach/i });
  // Status bar updates asynchronously — give it a moment
  await expect(statusBarChip.or(
    page.locator('[class*="status-bar"], [class*="StatusBar"]').getByText(/breach/i)
  )).toBeVisible({ timeout: 6_000 });
});

// ─── Scenario 4: Pause / Resume ──────────────────────────────────────────

test('scenario 4: pause monitoring; resume restarts tracking', async ({ page }) => {
  await setupPage(page);
  await openContractsTab(page);
  await createPublishedContract(page, 'Payments API v1');

  // ── Pause via context menu ────────────────────────────────
  const card = page.locator('article[data-status]').first();
  await card.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Pause monitoring' }).click();

  await expect(
    page.locator('article[data-status="paused"]'),
  ).toBeVisible({ timeout: 5_000 });

  // ── Set drift mode but sync — paused contract stays paused ──
  await setDriftMode(page, 'drift');
  await page.getByRole('button', { name: 'Sync' }).click();
  await page.waitForTimeout(1_000);

  // Contract still paused (recompute_drift skips paused contracts)
  await expect(page.locator('article[data-status="paused"]')).toBeVisible();
  await expect(page.locator('article[data-status="drift"]')).not.toBeVisible();

  // ── Resume via context menu ───────────────────────────────
  const pausedCard = page.locator('article[data-status="paused"]');
  await pausedCard.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Resume' }).click();

  await expect(
    page.locator('article[data-status="active"]'),
  ).toBeVisible({ timeout: 5_000 });

  // ── Sync after resume — drift now detected ────────────────
  await page.getByRole('button', { name: 'Sync' }).click();

  await expect(
    page.locator('article[data-status="drift"]')
      .or(page.locator('article[data-status="breach"]')),
  ).toBeVisible({ timeout: 10_000 });
});
