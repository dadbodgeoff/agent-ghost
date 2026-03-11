import { expect, test, type Page } from '@playwright/test';

const GATEWAY = 'http://127.0.0.1:39780';

async function seedRuntime(page: Page, token = 'test-token-playwright') {
  await page.addInitScript(
    ({ gateway, authToken }) => {
      localStorage.setItem('ghost-gateway-url', gateway);
      sessionStorage.setItem('ghost-token', authToken);

      class FakeWebSocket {
        static readonly CONNECTING = 0;
        static readonly OPEN = 1;
        static readonly CLOSING = 2;
        static readonly CLOSED = 3;
        static instances: FakeWebSocket[] = [];

        readyState = FakeWebSocket.CONNECTING;
        onopen: ((event: Event) => void) | null = null;
        onmessage: ((event: MessageEvent) => void) | null = null;
        onclose: ((event: CloseEvent) => void) | null = null;
        onerror: ((event: Event) => void) | null = null;

        constructor(_url: string, _protocols?: string | string[]) {
          FakeWebSocket.instances.push(this);
          queueMicrotask(() => {
            this.readyState = FakeWebSocket.OPEN;
            this.onopen?.(new Event('open'));
          });
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

      Object.defineProperty(window, 'BroadcastChannel', {
        configurable: true,
        writable: true,
        value: class {
          constructor() {
            throw new Error('disabled in tests');
          }
        },
      });

      Object.defineProperty(window, '__emitWsEvent', {
        configurable: true,
        writable: false,
        value: (event: Record<string, unknown>) => {
          const payload = JSON.stringify({
            seq: Date.now(),
            timestamp: new Date().toISOString(),
            event,
          });
          for (const socket of FakeWebSocket.instances) {
            socket.onmessage?.({ data: payload } as MessageEvent);
          }
        },
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

async function mockSecurityApis(
  page: Page,
  session: {
    authenticated: boolean;
    subject: string;
    role: string;
    capabilities?: string[];
    authz_v?: number | null;
    mode: 'jwt' | 'legacy' | 'none';
  },
  state: {
    auditEntries: Array<Record<string, unknown>>;
    killAllHits: number;
    exportUrls: string[];
  },
) {
  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());

    if (url.pathname === '/api/auth/session') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(session),
      });
      return;
    }

    if (url.pathname === '/api/compatibility') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          gateway_version: '1.0.0',
          compatibility_contract_version: 1,
          policy_a_writes_require_explicit_client_identity: true,
          required_mutation_headers: [],
          supported_clients: [
            {
              client_name: 'dashboard',
              minimum_version: '0.0.1',
              maximum_version_exclusive: '99.0.0',
              enforcement: 'warn',
            },
          ],
        }),
      });
      return;
    }

    if (url.pathname === '/api/ws/tickets') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ticket: 'test-ticket' }),
      });
      return;
    }

    if (url.pathname === '/api/safety/status') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          platform_level: 'KillAll',
          platform_killed: true,
          per_agent: {
            'agent-1': { level: 'Pause', trigger: 'manual_pause' },
          },
          distributed_kill: {
            enabled: false,
            status: 'gated',
            authoritative: false,
          },
          convergence_protection: {
            execution_mode: 'block',
            stale_after_secs: 60,
            agents: { healthy: 1, missing: 0, stale: 1, corrupted: 0 },
          },
        }),
      });
      return;
    }

    if (url.pathname === '/api/safety/sandbox-reviews') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          reviews: [
            {
              id: 'review-1',
              agent_id: 'agent-1',
              session_id: 'session-1',
              tool_name: 'shell',
              violation_reason: 'attempted unsafe write',
              sandbox_mode: 'workspace_write',
              status: 'pending',
              requested_at: '2026-03-10T12:00:00Z',
            },
          ],
        }),
      });
      return;
    }

    if (url.pathname === '/api/agents') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{ id: 'agent-1', name: 'Agent One' }]),
      });
      return;
    }

    if (url.pathname === '/api/audit/export') {
      state.exportUrls.push(request.url());
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(state.auditEntries),
      });
      return;
    }

    if (url.pathname === '/api/audit') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          entries: state.auditEntries,
          page: 1,
          page_size: 50,
          total: state.auditEntries.length,
        }),
      });
      return;
    }

    if (url.pathname === '/api/safety/kill-all') {
      state.killAllHits += 1;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          status: 'kill_all_activated',
          reason: 'Manual kill',
          initiated_by: 'test',
        }),
      });
      return;
    }

    if (url.pathname.endsWith('/approve') || url.pathname.endsWith('/reject')) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ review_id: 'review-1', status: 'approved' }),
      });
      return;
    }

    if (url.pathname === '/api/push/vapid-key') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ key: null }),
      });
      return;
    }

    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    });
  });
}

test.describe('Security page wiring', () => {
  test('operator cannot trigger kill-all or approve sandbox reviews', async ({ page }) => {
    const state = {
      auditEntries: [
        {
          id: 'audit-1',
          timestamp: '2026-03-10T12:00:00Z',
          event_type: 'sandbox_review_requested',
          severity: 'warn',
          details: 'review requested',
          agent_id: 'agent-1',
        },
      ],
      killAllHits: 0,
      exportUrls: [] as string[],
    };

    await seedRuntime(page);
    await mockSecurityApis(page, {
      authenticated: true,
      subject: 'operator-1',
      role: 'operator',
      capabilities: [],
      authz_v: 1,
      mode: 'jwt',
    }, state);

    await page.goto('/security', { waitUntil: 'networkidle' });

    await expect(page.getByRole('button', { name: 'KILL ALL' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Approve' })).toHaveCount(0);
    await expect(page.getByText('Approval requires admin or operator with `safety_review`.')).toBeVisible();

    await page.evaluate(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'K',
        metaKey: true,
        shiftKey: true,
        bubbles: true,
      }));
    });

    expect(state.killAllHits).toBe(0);
  });

  test('superadmin can trigger kill-all via keyboard shortcut', async ({ page }) => {
    const state = {
      auditEntries: [],
      killAllHits: 0,
      exportUrls: [] as string[],
    };

    await seedRuntime(page);
    await mockSecurityApis(page, {
      authenticated: true,
      subject: 'root',
      role: 'superadmin',
      capabilities: [],
      authz_v: 1,
      mode: 'jwt',
    }, state);

    page.on('dialog', async (dialog) => {
      await dialog.accept();
    });

    await page.goto('/security', { waitUntil: 'networkidle' });

    await expect(page.getByRole('button', { name: 'KILL ALL' })).toBeVisible();

    await page.evaluate(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'K',
        metaKey: true,
        shiftKey: true,
        bubbles: true,
      }));
    });

    await expect.poll(() => state.killAllHits).toBe(1);
  });

  test('export uses active filters', async ({ page }) => {
    const state = {
      auditEntries: [
        {
          id: 'audit-1',
          timestamp: '2026-03-10T12:00:00Z',
          event_type: 'kill_all',
          severity: 'critical',
          details: 'kill all triggered',
          agent_id: 'agent-1',
        },
      ],
      killAllHits: 0,
      exportUrls: [] as string[],
    };

    await seedRuntime(page);
    await mockSecurityApis(page, {
      authenticated: true,
      subject: 'root',
      role: 'superadmin',
      capabilities: [],
      authz_v: 1,
      mode: 'jwt',
    }, state);

    await page.goto('/security', { waitUntil: 'networkidle' });

    await page.selectOption('#filter-event-type', 'kill_all');
    await page.getByLabel('critical').check();
    await page.getByLabel('Search').fill('kill');
    await page.getByRole('button', { name: 'JSON', exact: true }).click();

    await expect.poll(() => state.exportUrls.length).toBe(1);
    expect(state.exportUrls[0]).toContain('format=json');
    expect(state.exportUrls[0]).toContain('event_type=kill_all');
    expect(state.exportUrls[0]).toContain('severity=critical');
    expect(state.exportUrls[0]).toContain('search=kill');
  });

  test('websocket security events refresh the audit timeline', async ({ page }) => {
    const state = {
      auditEntries: [
        {
          id: 'audit-1',
          timestamp: '2026-03-10T12:00:00Z',
          event_type: 'sandbox_review_requested',
          severity: 'warn',
          details: 'review requested',
          agent_id: 'agent-1',
        },
      ],
      killAllHits: 0,
      exportUrls: [] as string[],
    };

    await seedRuntime(page);
    await mockSecurityApis(page, {
      authenticated: true,
      subject: 'reviewer',
      role: 'operator',
      capabilities: ['safety_review'],
      authz_v: 1,
      mode: 'jwt',
    }, state);

    await page.goto('/security', { waitUntil: 'networkidle' });
    await expect(page.getByText('review requested')).toBeVisible();

    state.auditEntries = [
      {
        id: 'audit-2',
        timestamp: '2026-03-10T12:05:00Z',
        event_type: 'sandbox_review_approved',
        severity: 'info',
        details: 'review approved',
        agent_id: 'agent-1',
      },
    ];

    await page.evaluate(() => {
      (window as { __emitWsEvent: (event: Record<string, unknown>) => void }).__emitWsEvent({
        type: 'SandboxReviewResolved',
      });
    });

    await expect(page.getByText('review approved')).toBeVisible();
  });

  test('url-backed search handoff hydrates and clears cleanly', async ({ page }) => {
    const state = {
      auditEntries: [
        {
          id: 'audit-1',
          timestamp: '2026-03-10T12:00:00Z',
          event_type: 'memory_write',
          severity: 'info',
          details: 'marker audit entry',
          agent_id: 'agent-1',
        },
      ],
      killAllHits: 0,
      exportUrls: [] as string[],
    };

    await seedRuntime(page);
    await mockSecurityApis(page, {
      authenticated: true,
      subject: 'root',
      role: 'superadmin',
      capabilities: [],
      authz_v: 1,
      mode: 'jwt',
    }, state);

    await page.goto('/security?search=marker&focus=audit-1', { waitUntil: 'networkidle' });

    await expect(page.getByLabel('Search')).toHaveValue('marker');
    await expect(page.locator('.timeline-entry.focused')).toContainText('marker audit entry');

    await page.goto('/security', { waitUntil: 'networkidle' });

    await expect(page.getByLabel('Search')).toHaveValue('');
    await expect(page.locator('.timeline-entry.focused')).toHaveCount(0);
  });
});
