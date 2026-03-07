import { expect, test, type Page } from '@playwright/test';

async function bootControlledLoginPage(page: Page) {
  await page.addInitScript(() => {
    if (typeof Notification !== 'undefined') {
      Object.defineProperty(Notification, 'requestPermission', {
        configurable: true,
        value: async () => 'denied',
      });
    }
  });

  await page.goto('/login', { waitUntil: 'networkidle' });
  await page.waitForFunction(async () => {
    if (!('serviceWorker' in navigator)) return false;
    const registration = await navigator.serviceWorker.ready;
    return !!registration.active;
  });

  if (!(await page.evaluate(() => !!navigator.serviceWorker.controller))) {
    await page.reload({ waitUntil: 'networkidle' });
    await page.waitForFunction(() => !!navigator.serviceWorker.controller);
  }
}

async function cacheUrls(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const names = await caches.keys();
    const ghostCache = names.find((name) => name.startsWith('ghost-cache-'));
    if (!ghostCache) return [];
    const cache = await caches.open(ghostCache);
    const keys = await cache.keys();
    return keys.map((request) => new URL(request.url).pathname).sort();
  });
}

async function seedCacheEntry(page: Page, path: string, body: string) {
  await page.evaluate(
    async ({ targetPath, payload }) => {
      const names = await caches.keys();
      const ghostCache = names.find((name) => name.startsWith('ghost-cache-'));
      if (!ghostCache) throw new Error('ghost cache not initialized');

      const cache = await caches.open(ghostCache);
      await cache.put(
        new Request(targetPath),
        new Response(payload, {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    },
    { targetPath: path, payload: body },
  );
}

async function pendingActionCount(page: Page): Promise<number> {
  return page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('ghost-pending-actions', 1);
      request.onupgradeneeded = () => {
        const upgradeDb = request.result;
        if (!upgradeDb.objectStoreNames.contains('pending_actions')) {
          upgradeDb.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });

    const count = await new Promise<number>((resolve, reject) => {
      const tx = db.transaction('pending_actions', 'readonly');
      const req = tx.objectStore('pending_actions').count();
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    db.close();
    return count;
  });
}

async function seedPendingAction(page: Page) {
  await page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('ghost-pending-actions', 1);
      request.onupgradeneeded = () => {
        const upgradeDb = request.result;
        if (!upgradeDb.objectStoreNames.contains('pending_actions')) {
          upgradeDb.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });

    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('pending_actions', 'readwrite');
      tx.objectStore('pending_actions').add({
        url: '/api/studio/sessions/session-1/messages',
        method: 'POST',
        headers: {
          Authorization: 'Bearer stale-token',
          'Content-Type': 'application/json',
        },
        body: '{"message":"stale"}',
        session_seq: 41,
      });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
    db.close();
  });
}

async function postWorkerMessage(page: Page, type: 'ghost-auth-changed' | 'ghost-auth-cleared') {
  await page.evaluate(async (messageType) => {
    const registration = await navigator.serviceWorker.ready;
    registration.active?.postMessage({ type: messageType });
  }, type);
}

test.describe('Service worker auth/session safety', () => {
  test('auth endpoints are served network-only and never cached', async ({ page }) => {
    await bootControlledLoginPage(page);
    await page.context().route('**/api/auth/session', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          authenticated: true,
          subject: 'tester',
          role: 'admin',
          mode: 'legacy',
        }),
      }),
    );

    const response = await page.evaluate(async () => {
      const res = await fetch('/api/auth/session');
      return { status: res.status, body: await res.json() };
    });

    expect(response.status).toBe(200);
    expect(response.body.authenticated).toBe(true);
    await expect.poll(() => cacheUrls(page)).not.toContain('/api/auth/session');
  });

  test('bearer-authenticated API requests never populate the offline cache', async ({ page }) => {
    await bootControlledLoginPage(page);
    await page.context().route('**/api/memory', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items: [{ id: 'm-1', content: 'secret' }] }),
      }),
    );

    const response = await page.evaluate(async () => {
      const res = await fetch('/api/memory', {
        headers: {
          Authorization: 'Bearer rotated-token',
        },
      });
      return { status: res.status, body: await res.json() };
    });

    expect(response.status).toBe(200);
    expect(response.body.items).toHaveLength(1);
    await expect.poll(() => cacheUrls(page)).not.toContain('/api/memory');
  });

  test('auth boundary clears cached API data but preserves non-API cache entries', async ({ page }) => {
    await bootControlledLoginPage(page);
    await seedCacheEntry(page, '/api/agents', '{"agents":[{"id":"a-1"}]}');
    await seedCacheEntry(page, '/__static_marker__', '{"ok":true}');

    await expect.poll(() => cacheUrls(page)).toContain('/api/agents');
    await expect.poll(() => cacheUrls(page)).toContain('/__static_marker__');

    await postWorkerMessage(page, 'ghost-auth-changed');

    await expect.poll(() => cacheUrls(page)).not.toContain('/api/agents');
    await expect.poll(() => cacheUrls(page)).toContain('/__static_marker__');
  });

  test('auth boundary clears queued offline actions', async ({ page }) => {
    await bootControlledLoginPage(page);
    await seedPendingAction(page);

    expect(await pendingActionCount(page)).toBe(1);

    await postWorkerMessage(page, 'ghost-auth-cleared');

    await expect.poll(() => pendingActionCount(page)).toBe(0);
  });
});
