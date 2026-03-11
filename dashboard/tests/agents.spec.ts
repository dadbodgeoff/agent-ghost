import { expect, test, type Page } from '@playwright/test';

const GATEWAY = 'http://127.0.0.1:39780';

async function authenticate(page: Page) {
  await page.addInitScript((gateway) => {
    localStorage.setItem('ghost-gateway-url', gateway);
    sessionStorage.setItem('ghost-token', 'test-token-playwright');
  }, GATEWAY);
}

async function mockBase(page: Page) {
  await page.route('**/api/**', async (route) => {
    const url = route.request().url();
    if (url.endsWith('/api/auth/session')) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          authenticated: true,
          subject: 'tester',
          role: 'admin',
          mode: 'legacy',
        }),
      });
      return;
    }

    if (url.endsWith('/api/push/vapid-key')) {
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

test('agents list renders canonical effective states', async ({ page }) => {
  await mockBase(page);
  await authenticate(page);

  await page.route('**/api/agents', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: 'agent-ready',
          name: 'Agent Ready',
          status: 'ready',
          lifecycle_state: 'ready',
          safety_state: 'normal',
          effective_state: 'ready',
          spending_cap: 10,
        },
        {
          id: 'agent-quarantined',
          name: 'Agent Quarantine',
          status: 'quarantined',
          lifecycle_state: 'ready',
          safety_state: 'quarantined',
          effective_state: 'quarantined',
          spending_cap: 10,
        },
      ]),
    });
  });

  await page.route('**/api/convergence/scores', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ scores: [] }),
    });
  });

  await page.goto('/agents', { waitUntil: 'networkidle' });

  await expect(page.getByText('Agent Ready')).toBeVisible();
  await expect(page.locator('.status-badge', { hasText: 'Ready' })).toBeVisible();
  await expect(page.getByText('Agent Quarantine')).toBeVisible();
  await expect(page.locator('.status-badge', { hasText: 'Quarantined' })).toBeVisible();
});

test('agent detail uses overview read model and gated quarantine resume flow', async ({ page }) => {
  await mockBase(page);
  await authenticate(page);

  await page.route('**/api/agents/agent-1/overview**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        agent: {
          id: 'agent-1',
          name: 'Agent One',
          status: 'quarantined',
          lifecycle_state: 'ready',
          safety_state: 'quarantined',
          effective_state: 'quarantined',
          spending_cap: 10,
          isolation: 'in_process',
          capabilities: ['shell_execute'],
          sandbox: {
            enabled: true,
            mode: 'workspace_write',
            on_violation: 'pause',
            network_access: false,
            allowed_shell_prefixes: [],
          },
          sandbox_metrics: {
            pending_reviews: 1,
            total_reviews: 4,
            approved_reviews: 2,
            rejected_reviews: 1,
            expired_reviews: 0,
            last_requested_at: null,
          },
          action_policy: {
            can_pause: false,
            can_quarantine: false,
            can_resume: true,
            can_delete: false,
            resume_kind: 'quarantine',
            requires_forensic_review: true,
            requires_second_confirmation: true,
            monitoring_duration_hours: 24,
          },
        },
        convergence: null,
        cost: {
          agent_id: 'agent-1',
          agent_name: 'Agent One',
          daily_total: 1.2,
          compaction_cost: 0.1,
          spending_cap: 10,
          cap_remaining: 8.8,
          cap_utilization_pct: 12,
        },
        recent_sessions: [
          {
            session_id: 'session-for-agent-1',
            started_at: '2026-03-10T10:00:00Z',
            last_event_at: '2026-03-10T10:10:00Z',
            event_count: 4,
            agent_ids: ['agent-1'],
            chain_valid: true,
            cumulative_cost: 0.25,
            branched_from: null,
          },
        ],
        recent_audit_entries: [],
        crdt_summary: null,
        integrity_summary: null,
        panel_health: {
          convergence: { state: 'empty' },
          cost: { state: 'ready' },
          recent_sessions: { state: 'ready' },
          recent_audit_entries: { state: 'empty' },
          crdt_summary: { state: 'empty' },
          integrity_summary: { state: 'empty' },
        },
      }),
    });
  });

  await page.route('**/api/agents/agent-1', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: 'agent-1',
          name: 'Agent One',
          status: 'quarantined',
          lifecycle_state: 'ready',
          safety_state: 'quarantined',
          effective_state: 'quarantined',
          spending_cap: 10,
          isolation: 'in_process',
          capabilities: ['shell_execute'],
          sandbox: {
            enabled: true,
            mode: 'workspace_write',
            on_violation: 'pause',
            network_access: false,
            allowed_shell_prefixes: [],
          },
          sandbox_metrics: {
            pending_reviews: 1,
            total_reviews: 4,
            approved_reviews: 2,
            rejected_reviews: 1,
            expired_reviews: 0,
            last_requested_at: null,
          },
          action_policy: {
            can_pause: false,
            can_quarantine: false,
            can_resume: true,
            can_delete: false,
            resume_kind: 'quarantine',
            requires_forensic_review: true,
            requires_second_confirmation: true,
            monitoring_duration_hours: 24,
          },
        }),
      });
      return;
    }

    await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });

  await page.route('**/api/safety/resume/agent-1', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        status: 'resumed',
        agent_id: 'agent-1',
        heightened_monitoring: true,
        monitoring_duration_hours: 24,
      }),
    });
  });

  await page.goto('/agents/agent-1', { waitUntil: 'networkidle' });

  await expect(page.getByText('Resume After Review')).toBeVisible();
  await expect(page.getByTitle('session-for-agent-1')).toBeVisible();
  await page.getByRole('button', { name: 'Resume After Review' }).click();
  await expect(page.getByText('Resume Quarantined Agent')).toBeVisible();
  await page.getByLabel('I reviewed the forensic evidence for this quarantine.').check();
  await page.getByLabel('I confirm this agent should resume despite prior quarantine.').check();
  const resumeRequest = page.waitForRequest('**/api/safety/resume/agent-1');
  await page.getByRole('button', { name: 'Resume With Monitoring' }).click();

  expect((await resumeRequest).postDataJSON()).toEqual({
    level: 'QUARANTINE',
    forensic_reviewed: true,
    second_confirmation: true,
  });
});
