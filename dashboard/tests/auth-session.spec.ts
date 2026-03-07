import { test, expect, type Page } from '@playwright/test';

const GATEWAY = 'http://127.0.0.1:39780';
const AUTH_ERROR_BANNER = 'Dashboard could not verify the current session. The gateway may be unavailable.';

async function seedRuntime(page: Page, token: string | null = 'test-token-playwright') {
  await page.addInitScript(
    ({ gateway, authToken }) => {
      localStorage.setItem('ghost-gateway-url', gateway);
      if (authToken) {
        sessionStorage.setItem('ghost-token', authToken);
      } else {
        sessionStorage.removeItem('ghost-token');
      }

      class FakeWebSocket {
        static readonly CONNECTING = 0;
        static readonly OPEN = 1;
        static readonly CLOSING = 2;
        static readonly CLOSED = 3;

        readyState = FakeWebSocket.OPEN;
        onopen: ((event: Event) => void) | null = null;
        onmessage: ((event: MessageEvent) => void) | null = null;
        onclose: ((event: CloseEvent) => void) | null = null;
        onerror: ((event: Event) => void) | null = null;

        constructor(_url: string, _protocols?: string | string[]) {
          queueMicrotask(() => this.onopen?.(new Event('open')));
        }

        send(_data?: string) {}

        close() {
          this.readyState = FakeWebSocket.CLOSED;
          this.onclose?.(new CloseEvent('close', { code: 1000, reason: 'test close' }));
        }
      }

      Object.defineProperty(window, 'WebSocket', {
        configurable: true,
        writable: true,
        value: FakeWebSocket,
      });

      Object.defineProperty(navigator, 'serviceWorker', {
        configurable: true,
        value: {
          controller: null,
          ready: Promise.resolve({
            pushManager: {
              getSubscription: async () => null,
              subscribe: async () => ({ toJSON: () => ({}) }),
            },
          }),
          register: async () => ({ active: null, waiting: null, installing: null }),
          getRegistrations: async () => [],
        },
      });

      if (typeof Notification !== 'undefined') {
        Object.defineProperty(Notification, 'requestPermission', {
          configurable: true,
          value: async () => 'denied',
        });
      }
    },
    { gateway: GATEWAY, authToken: token },
  );
}

async function mockDashboardBootApis(page: Page) {
  await page.route('**/api/push/vapid-key', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ key: null }),
    }),
  );

  await page.route('**/service-worker.js', (route) =>
    route.fulfill({ status: 200, contentType: 'application/javascript', body: '' }),
  );

  await page.route('**/api/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    }),
  );
}

async function mockSession(page: Page, status: number, body?: unknown) {
  await page.route('**/api/auth/session', (route) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(
        body ??
          (status >= 400
            ? { error: { code: `HTTP_${status}`, message: `Session failed with ${status}` } }
            : { authenticated: true, subject: 'tester', role: 'admin', mode: 'legacy' }),
      ),
    }),
  );
}

async function mockSessionNetworkFailure(page: Page) {
  await page.route('**/api/auth/session', (route) => route.abort('failed'));
}

async function readStoredToken(page: Page) {
  return page.evaluate(() => sessionStorage.getItem('ghost-token'));
}

test.describe('Auth/session failure semantics', () => {
  test('401 session response clears auth and redirects to /login', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSession(page, 401, {
      error: { code: 'MISSING_TOKEN', message: 'Authorization header with Bearer token required' },
    });

    await page.goto('/', { waitUntil: 'networkidle' });

    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(await readStoredToken(page)).toBeNull();
  });

  test('403 session response clears auth and redirects to /login', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSession(page, 403, {
      error: { code: 'FORBIDDEN', message: 'Insufficient permissions for this operation' },
    });

    await page.goto('/', { waitUntil: 'networkidle' });

    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(await readStoredToken(page)).toBeNull();
  });

  test('500 session response preserves auth and shows availability banner', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSession(page, 500, {
      error: { code: 'INTERNAL', message: 'server exploded' },
    });

    await page.goto('/', { waitUntil: 'networkidle' });

    await expect(page.locator('.offline-banner')).toContainText(AUTH_ERROR_BANNER);
    expect(page.url()).not.toContain('/login');
    expect(await readStoredToken(page)).toBe('test-token-playwright');
  });

  test('network failure preserves auth and shows availability banner', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSessionNetworkFailure(page);

    await page.goto('/', { waitUntil: 'networkidle' });

    await expect(page.locator('.offline-banner')).toContainText(AUTH_ERROR_BANNER);
    expect(page.url()).not.toContain('/login');
    expect(await readStoredToken(page)).toBe('test-token-playwright');
  });

  test('logout treats 401 revocation as local success and clears auth', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSession(page, 200);
    await page.route('**/api/auth/logout', (route) =>
      route.fulfill({
        status: 401,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'TOKEN_REVOKED', message: 'Token already revoked' } }),
      }),
    );

    let dialogSeen = false;
    page.on('dialog', async (dialog) => {
      dialogSeen = true;
      await dialog.dismiss();
    });

    await page.goto('/settings', { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: 'Logout' }).click();

    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(await readStoredToken(page)).toBeNull();
    expect(dialogSeen).toBe(false);
  });

  test('logout surfaces remote failure but still clears local auth', async ({ page }) => {
    await seedRuntime(page);
    await mockDashboardBootApis(page);
    await mockSession(page, 200);
    await page.route('**/api/auth/logout', (route) =>
      route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'INTERNAL', message: 'revocation unavailable' } }),
      }),
    );

    let dialogMessage = '';
    page.on('dialog', async (dialog) => {
      dialogMessage = dialog.message();
      await dialog.accept();
    });

    await page.goto('/settings', { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: 'Logout' }).click();

    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(dialogMessage).toContain('Signed out locally');
    expect(dialogMessage).toContain('revocation unavailable');
    expect(await readStoredToken(page)).toBeNull();
  });
});
