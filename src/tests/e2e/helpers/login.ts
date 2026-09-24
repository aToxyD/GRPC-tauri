import type { Page } from '@playwright/test';

/**
 * SEC-026 / ADR-0052: the login identity is no longer free-text. WILAYA nodes
 * render a read-only `admin` input; UNIT nodes render a fixed [user, admin]
 * selector. This helper guarantees the requested identity is selected before
 * the password step, regardless of node class.
 */
export async function ensureLoginIdentity(page: Page, username = 'admin'): Promise<void> {
  await page.waitForSelector('#username', { state: 'visible' });
  const tag = await page.locator('#username').evaluate((el) => el.tagName);
  if (tag === 'SELECT') {
    await page.locator('#username').selectOption(username);
  }
  // Read-only inputs are already pinned by the UI — nothing to do.
}
