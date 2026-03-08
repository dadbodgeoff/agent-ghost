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

async function seedPendingWorkflowAction(page: Page) {
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
        url: '/api/workflows',
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: '{"name":"offline workflow"}',
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
  test('auth endpoints are served network-only and never cached', async ({ page, context }) => {
    await bootControlledLoginPage(page);

    const onlineResponse = await page.evaluate(async () => {
      const res = await fetch(new URL('/api/auth/session', window.location.origin).toString());
      return { status: res.status, body: await res.text() };
    });

    expect(onlineResponse.status).toBeGreaterThanOrEqual(100);
    await expect.poll(() => cacheUrls(page)).not.toContain('/api/auth/session');

    await context.setOffline(true);

    const offlineOutcome = await page.evaluate(async () => {
      try {
        const res = await fetch(new URL('/api/auth/session', window.location.origin).toString());
        return {
          kind: 'response',
          status: res.status,
          body: await res.json(),
        };
      } catch (error) {
        return {
          kind: 'error',
          message: error instanceof Error ? error.message : String(error),
        };
      }
    });

    if (offlineOutcome.kind === 'response') {
      expect(offlineOutcome.status).toBe(503);
      expect(offlineOutcome.body.error).toBe('offline');
    } else {
      expect(offlineOutcome.message.toLowerCase()).toContain('load failed');
    }
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
      try {
        const res = await fetch(new URL('/api/memory', window.location.origin).toString(), {
          headers: {
            Authorization: 'Bearer rotated-token',
          },
        });
        return {
          kind: 'response',
          status: res.status,
          body: await res.json(),
        };
      } catch (error) {
        return {
          kind: 'error',
          message: error instanceof Error ? error.message : String(error),
        };
      }
    });

    if (response.kind === 'response') {
      expect(response.status).toBe(200);
      expect(response.body.items).toHaveLength(1);
    } else {
      expect(response.message.toLowerCase()).toContain('pattern');
    }
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

  test('queued offline writes replay only for the active auth session', async ({ page }) => {
    await bootControlledLoginPage(page);
    await seedPendingWorkflowAction(page);
    await expect.poll(() => pendingActionCount(page)).toBe(1);

    let replayedRequest:
      | { headers: Record<string, string>; method: string; postData: string | null }
      | null = null;
    await page.context().route('**/api/workflows', async (route) => {
      replayedRequest = {
        headers: route.request().headers(),
        method: route.request().method(),
        postData: route.request().postData(),
      };
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true }),
      });
    });

    await postWorkerMessage(page, 'ghost-replay-pending-actions');

    await page.waitForTimeout(250);
    expect(replayedRequest).toBeNull();
    await expect.poll(() => pendingActionCount(page)).toBe(1);

    await postWorkerMessage(page, 'ghost-auth-session', {
      client_id: 'client-1',
      session_epoch: 7,
      token: 'queued-token',
    });
    await postWorkerMessage(page, 'ghost-replay-pending-actions');

    await expect.poll(() => pendingActionCount(page)).toBe(0);
    if (replayedRequest) {
      expect(replayedRequest.method).toBe('POST');
      expect(replayedRequest.postData).toBe('{"name":"offline workflow"}');

      const replayedHeaders = replayedRequest.headers;
      if (replayedHeaders.authorization !== undefined) {
        expect(replayedHeaders.authorization).toBe('Bearer queued-token');
      }
      if (replayedHeaders['x-request-id'] !== undefined) {
        expect(replayedHeaders['x-request-id']).toBeTruthy();
      }
      if (replayedHeaders['x-ghost-operation-id'] !== undefined) {
        expect(replayedHeaders['x-ghost-operation-id']).toBeTruthy();
      }
      if (replayedHeaders['idempotency-key'] !== undefined) {
        expect(replayedHeaders['idempotency-key']).toBeTruthy();
      }
      expect(replayedHeaders['x-ghost-expected-seq']).toBeUndefined();
    }
  });
});
