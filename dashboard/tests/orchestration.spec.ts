import { expect, test, type Page } from '@playwright/test';

const GATEWAY = 'http://127.0.0.1:39780';

async function authenticate(page: Page) {
  await page.addInitScript((gateway) => {
    localStorage.setItem('ghost-gateway-url', gateway);
    sessionStorage.setItem('ghost-token', 'test-token-playwright');

    if ('serviceWorker' in navigator) {
      const fakeRegistration = {
        active: null,
        installing: null,
        waiting: null,
        pushManager: {
          getSubscription: async () => null,
          subscribe: async () => null,
        },
        sync: {
          register: async () => undefined,
        },
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        unregister: async () => true,
        update: async () => undefined,
      };

      Object.defineProperty(navigator.serviceWorker, 'register', {
        configurable: true,
        value: async () => fakeRegistration,
      });
      Object.defineProperty(navigator.serviceWorker, 'ready', {
        configurable: true,
        value: Promise.resolve(fakeRegistration),
      });
      Object.defineProperty(navigator.serviceWorker, 'getRegistrations', {
        configurable: true,
        value: async () => [],
      });
    }
  }, GATEWAY);
}

async function mockApis(page: Page) {
  await page.route('**/api/**', (route) =>
    route.fulfill({
      status: 500,
      contentType: 'application/json',
      body: JSON.stringify({ error: `unmocked api route: ${route.request().method()} ${route.request().url()}` }),
    }),
  );

  await page.route('**/api/auth/session**', (route) =>
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

  await page.route('**/api/compatibility**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        supported: true,
        gatewayVersion: 'test',
        client: { name: 'dashboard', version: '0.1.0' },
      }),
    }),
  );

  await page.route('**/api/mesh/trust-graph**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        nodes: [
          { id: 'n1', name: 'Node-1', activity: 10, convergence_level: 2 },
          { id: 'n2', name: 'Node-2', activity: 5, convergence_level: 1 },
        ],
        edges: [{ source: 'n1', target: 'n2', trust_score: 0.85 }],
      }),
    }),
  );

  await page.route('**/api/mesh/consensus**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        rounds: [
          {
            proposal_id: 'proposal-1',
            status: 'approved',
            approvals: 1,
            rejections: 0,
            threshold: 1,
          },
        ],
      }),
    }),
  );

  await page.route('**/api/mesh/delegations**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        delegations: [
          {
            delegator_id: 'agent-a',
            delegate_id: 'agent-b',
            scope: 'triage',
            state: 'Accepted',
            created_at: '2026-03-11T00:00:00Z',
          },
        ],
        sybil_metrics: { total_delegations: 1, max_chain_depth: 1, unique_delegators: 1 },
      }),
    }),
  );

  await page.route('**/api/a2a/discover**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        agents: [
          {
            name: 'Remote Planner',
            description: 'Planning agent',
            endpoint_url: 'http://remote-agent.test',
            capabilities: ['planning'],
            trust_score: 1,
            version: '1.0.0',
            reachable: true,
            verified: true,
          },
        ],
      }),
    }),
  );

  await page.route('**/api/a2a/tasks**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        tasks: [
          {
            task_id: 'task-1',
            target_agent: 'Remote Planner',
            target_url: 'http://remote-agent.test',
            method: 'tasks/send',
            status: 'working',
            created_at: '2026-03-11T00:00:00Z',
            input: { text: 'hello' },
          },
        ],
      }),
    }),
  );

  await page.route('**/api/ws/tickets**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ticket: 'ticket-1',
        expires_at: '2026-03-11T00:00:30Z',
        expires_in_secs: 30,
      }),
    }),
  );

  await page.route('**/service-worker.js', (route) =>
    route.fulfill({ status: 200, contentType: 'application/javascript', body: '' }),
  );
}

async function navigateToOrchestration(page: Page) {
  await mockApis(page);
  await authenticate(page);
  await page.goto('/orchestration', { waitUntil: 'networkidle' });
}

test.describe('Orchestration route', () => {
  test('keeps tracked task count aligned with the rendered task table', async ({ page }) => {
    await navigateToOrchestration(page);

    await page.getByRole('tab', { name: 'A2A Discovery' }).click();

    await expect(page.getByRole('heading', { name: 'Tracked Tasks (1)' })).toBeVisible();
    await expect(page.locator('.tracker tbody tr')).toHaveCount(1);
    await expect(page.locator('.tracker tbody tr')).toContainText('Remote Planner');
  });

  test('lets a discovered verified agent populate the send form directly', async ({ page }) => {
    await navigateToOrchestration(page);

    await page.getByRole('tab', { name: 'A2A Discovery' }).click();
    await page.getByRole('button', { name: 'Discover Agents' }).click();

    await expect(page.getByText('Remote Planner')).toBeVisible();
    await page.locator('.agent-grid').getByRole('button', { name: 'Send Task' }).click();

    await expect(page.locator('input.send-input')).toHaveValue(
      'http://remote-agent.test/.well-known/agent.json',
    );
  });

  test('reuses a discovered agent card URL without duplicating the well-known path', async ({ page }) => {
    await mockApis(page);
    await page.route('**/api/a2a/discover**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          agents: [
            {
              name: 'Remote Planner',
              description: 'Planning agent',
              endpoint_url: 'http://remote-agent.test/.well-known/agent.json',
              capabilities: ['planning'],
              trust_score: 1,
              version: '1.0.0',
              reachable: true,
              verified: true,
            },
          ],
        }),
      }),
    );
    await authenticate(page);
    await page.goto('/orchestration', { waitUntil: 'networkidle' });

    await page.getByRole('tab', { name: 'A2A Discovery' }).click();
    await page.getByRole('button', { name: 'Discover Agents' }).click();
    await page.locator('.agent-grid').getByRole('button', { name: 'Send Task' }).click();

    await expect(page.locator('input.send-input')).toHaveValue(
      'http://remote-agent.test/.well-known/agent.json',
    );
  });

  test('surfaces invalid JSON before attempting to send a task', async ({ page }) => {
    await navigateToOrchestration(page);

    await page.getByRole('tab', { name: 'A2A Discovery' }).click();
    await page.locator('input.send-input').fill('http://remote-agent.test/.well-known/agent.json');
    await page.locator('textarea.send-textarea').fill('{not valid json');

    await expect(page.getByText('Task input must be valid JSON.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Send Task' })).toBeDisabled();
  });
});
