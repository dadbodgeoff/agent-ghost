import { expect, test, type Page } from '@playwright/test';

const GATEWAY = 'http://127.0.0.1:39780';

async function seedRuntime(page: Page) {
  await page.addInitScript(({ gateway }) => {
    localStorage.setItem('ghost-gateway-url', gateway);
    sessionStorage.setItem('ghost-token', 'test-token-playwright');

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
  }, { gateway: GATEWAY });
}

function compiledSkill(overrides: Record<string, unknown>) {
  return {
    id: 'skill-id',
    name: 'skill-name',
    version: '0.1.0',
    description: 'Compiled test skill',
    source: 'compiled',
    removable: true,
    installable: true,
    execution_mode: 'native',
    policy_capability: 'skill:test-skill',
    privileges: ['Use the compiled skill pipeline through the gateway runtime'],
    state: 'available',
    capabilities: ['skill:test-skill'],
    ...overrides,
  };
}

async function mockSkillsPage(page: Page) {
  let installed = [
    compiledSkill({
      id: 'convergence_check',
      name: 'convergence_check',
      description: 'Always-on convergence safety checks',
      removable: false,
      installable: false,
      policy_capability: 'skill:convergence_check',
      privileges: ['Read agent convergence scores, levels, and safety metrics from the gateway database'],
      state: 'always_on',
    }),
  ];
  let available = [
    compiledSkill({
      id: 'note_take',
      name: 'note_take',
      description: 'Structured note-taking skill',
      policy_capability: 'skill:note_take',
      privileges: [
        'Create, read, update, delete, and search notes stored in the gateway database',
      ],
      state: 'available',
    }),
  ];

  await page.route('**/api/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    }),
  );

  await page.route('**/api/auth/session', (route) =>
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

  await page.route('**/api/skills', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ installed, available }),
    }),
  );

  await page.route('**/api/skills/note_take/install', async (route) => {
    const installedSkill = compiledSkill({
      id: 'note_take',
      name: 'note_take',
      description: 'Structured note-taking skill',
      policy_capability: 'skill:note_take',
      privileges: [
        'Create, read, update, delete, and search notes stored in the gateway database',
      ],
      state: 'installed',
    });
    installed = [...installed, installedSkill];
    available = [];

    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(installedSkill),
    });
  });
}

test('skills page reviews real privileges and respects always-on state', async ({ page }) => {
  await seedRuntime(page);
  await mockSkillsPage(page);

  await page.goto('/skills', { waitUntil: 'networkidle' });

  await expect(page.getByRole('heading', { name: 'Skills' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Always on' })).toBeDisabled();

  await page.getByRole('button', { name: /Available \(1\)/ }).click();
  await page.getByRole('button', { name: 'Install', exact: true }).click();

  const dialog = page.getByRole('dialog', { name: 'Review privileges' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('Create, read, update, delete, and search notes stored in the gateway database')).toBeVisible();
  await expect(dialog.getByText('Runtime policy capability')).toBeVisible();

  await page.getByRole('button', { name: 'Approve & Install' }).click();
  await page.getByRole('button', { name: /Installed \(2\)/ }).click();

  await expect(page.getByText('Structured note-taking skill')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Uninstall' })).toBeVisible();
});
