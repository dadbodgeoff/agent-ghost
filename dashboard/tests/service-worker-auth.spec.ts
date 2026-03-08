import { expect, test, type Page } from '@playwright/test';

test.describe.configure({ mode: 'serial' });

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
      const request = indexedDB.open('ghost-pending-actions', 2);
      request.onupgradeneeded = () => {
        const upgradeDb = request.result;
        if (!upgradeDb.objectStoreNames.contains('pending_actions')) {
          upgradeDb.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
        }
        if (!upgradeDb.objectStoreNames.contains('auth_state')) {
          upgradeDb.createObjectStore('auth_state', { keyPath: 'key' });
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
      const request = indexedDB.open('ghost-pending-actions', 2);
      request.onupgradeneeded = () => {
        const upgradeDb = request.result;
        if (!upgradeDb.objectStoreNames.contains('pending_actions')) {
          upgradeDb.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
        }
        if (!upgradeDb.objectStoreNames.contains('auth_state')) {
          upgradeDb.createObjectStore('auth_state', { keyPath: 'key' });
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
          'Content-Type': 'application/json',
        },
        body: '{"message":"stale"}',
        client_id: 'client-1',
        session_epoch: 7,
        operation_envelope: {
          request_id: 'req-1',
          operation_id: 'op-1',
          idempotency_key: 'idem-1',
        },
      });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
    db.close();
  });
}

async function postWorkerMessage(
  page: Page,
  type:
    | 'ghost-auth-session'
    | 'ghost-auth-changed'
    | 'ghost-auth-cleared'
    | 'ghost-replay-pending-actions',
  auth?: { client_id: string; session_epoch: number; token: string | null },
) {
  await page.evaluate(async ({ payload, auth }) => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('ghost-pending-actions', 2);
      request.onupgradeneeded = () => {
        const upgradeDb = request.result;
        if (!upgradeDb.objectStoreNames.contains('pending_actions')) {
          upgradeDb.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
        }
        if (!upgradeDb.objectStoreNames.contains('auth_state')) {
          upgradeDb.createObjectStore('auth_state', { keyPath: 'key' });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });

    await new Promise<void>((resolve, reject) => {
      const storeNames =
        payload.type === 'ghost-auth-session'
          ? ['auth_state']
          : ['pending_actions', 'auth_state'];
      const tx = db.transaction(storeNames, 'readwrite');
      if (payload.type === 'ghost-auth-session' && auth) {
        tx.objectStore('auth_state').put({ key: 'active', ...auth });
      } else if (payload.type === 'ghost-auth-changed' && auth) {
        tx.objectStore('pending_actions').clear();
        tx.objectStore('auth_state').put({ key: 'active', ...auth });
      } else if (payload.type === 'ghost-auth-cleared') {
        tx.objectStore('pending_actions').clear();
        tx.objectStore('auth_state').delete('active');
      }
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
    db.close();

    const registration = await navigator.serviceWorker.ready;
    navigator.serviceWorker.controller?.postMessage(payload);
    registration.active?.postMessage(payload);
    registration.waiting?.postMessage(payload);
    registration.installing?.postMessage(payload);
  }, { payload: auth ? { type, auth } : { type }, auth });
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

  test('queued offline writes replay only for the active auth session', async ({ page, context }) => {
    await bootControlledLoginPage(page);
    await postWorkerMessage(page, 'ghost-auth-session', {
      client_id: 'client-1',
      session_epoch: 7,
      token: 'queued-token',
    });

    await page.context().route('**/api/workflows', async (route) => {
      await route.abort('internetdisconnected');
    });
    const queued = await page.evaluate(async () => {
      const response = await fetch('/api/workflows', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ name: 'offline workflow' }),
      });
      return { status: response.status, body: await response.json() };
    });
    expect(queued.status).toBe(202);
    expect(queued.body.queued).toBe(true);
    await expect.poll(() => pendingActionCount(page)).toBe(1);

    await page.context().unroute('**/api/workflows');
    let replayedHeaders: Record<string, string> | null = null;
    await page.context().route('**/api/workflows', async (route) => {
      replayedHeaders = route.request().headers();
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true }),
      });
    });

    await postWorkerMessage(page, 'ghost-replay-pending-actions');

    await expect.poll(() => pendingActionCount(page)).toBe(0);
    expect(replayedHeaders?.authorization).toBe('Bearer queued-token');
    expect(replayedHeaders?.['x-request-id']).toBeTruthy();
    expect(replayedHeaders?.['x-ghost-operation-id']).toBeTruthy();
    expect(replayedHeaders?.['idempotency-key']).toBeTruthy();
    expect(replayedHeaders?.['x-ghost-expected-seq']).toBeUndefined();
  });
});
