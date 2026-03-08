import { expect, test, type Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// GHOST ADE Dashboard — Responsive shell and PWA tests.
//
// These tests track the current dashboard contract:
// - the shell is a split-pane layout rendered by PanelLayout
// - primary navigation lives in the sidebar at every viewport
// - page content remains accessible across viewport sizes
// - touch-capable canvases expose observable accessibility/gesture affordances
// ---------------------------------------------------------------------------

const GATEWAY = 'http://127.0.0.1:39780';
const SIDEBAR_SELECTOR = '[role="complementary"][aria-label="Sidebar"]';
const PRIMARY_NAV_SELECTOR = 'nav[aria-label="Primary navigation"]';
const MAIN_CONTENT_SELECTOR = '.main-content[role="main"][aria-label="Main content"]';

async function authenticate(page: Page) {
  await page.addInitScript((gateway) => {
    localStorage.setItem('ghost-gateway-url', gateway);
    sessionStorage.setItem('ghost-token', 'test-token-playwright');
  }, GATEWAY);
}

async function mockAllApis(page: Page) {
  // Register the generic fallback first so the more specific mocks below win.
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

  await page.route('**/api/convergence/scores', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        scores: [{ score: 0.82, level: 3 }],
      }),
    }),
  );

  await page.route('**/api/agents', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        { id: 'a-1', name: 'Agent Alpha', status: 'running' },
        { id: 'a-2', name: 'Agent Beta', status: 'idle' },
      ]),
    }),
  );

  await page.route('**/api/goals*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        proposals: [
          {
            id: 'p-1',
            agent_id: 'a-1',
            session_id: 's-1',
            proposer_type: 'llm',
            operation: 'write_file',
            target_type: 'filesystem',
            decision: null,
            dimension_scores: {},
            flags: [],
            created_at: '2025-12-01T00:00:00Z',
            resolved_at: null,
          },
        ],
        page: 1,
        page_size: 50,
        total: 1,
      }),
    }),
  );

  await page.route('**/api/workflows*', (route) => {
    if (route.request().method() === 'GET') {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          workflows: [
            {
              id: 'wf-1',
              name: 'CI Pipeline',
              description: 'Build and test',
              nodes: [],
              edges: [],
              created_at: '2025-11-01T00:00:00Z',
              updated_at: '2025-12-01T00:00:00Z',
            },
          ],
        }),
      });
    }
    return route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });

  await page.route('**/api/mesh/trust-graph', (route) =>
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

  await page.route('**/api/mesh/consensus', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ rounds: [] }),
    }),
  );

  await page.route('**/api/mesh/delegations', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        delegations: [],
        sybil_metrics: { total_delegations: 0, max_chain_depth: 0, unique_delegators: 0 },
      }),
    }),
  );

  await page.route('**/api/a2a/discover', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ agents: [] }),
    }),
  );

  await page.route('**/api/a2a/tasks', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ tasks: [] }),
    }),
  );

  await page.route('**/api/sessions*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ sessions: [] }),
    }),
  );

  await page.route('**/api/memory*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ memories: [] }),
    }),
  );

  await page.route('**/api/push/vapid-key', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ key: null }),
    }),
  );

  await page.route('**/api/auth/login', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ access_token: 'mock-jwt' }),
    }),
  );

  await page.route('**/service-worker.js', (route) =>
    route.fulfill({ status: 200, contentType: 'application/javascript', body: '' }),
  );
}

async function navigateTo(page: Page, path: string) {
  await mockAllApis(page);
  await authenticate(page);
  await page.goto(path, { waitUntil: 'networkidle' });
}

function sidebar(page: Page) {
  return page.locator(SIDEBAR_SELECTOR);
}

function primaryNav(page: Page) {
  return page.locator(PRIMARY_NAV_SELECTOR);
}

function mainContent(page: Page) {
  return page.locator(MAIN_CONTENT_SELECTOR);
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. RESPONSIVE SHELL
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Responsive shell', () => {
  test.describe('sm breakpoint (<640px / iPhone 14)', () => {
    test.use({ viewport: { width: 390, height: 844 } });

    test('split-pane shell still renders the sidebar and main content', async ({ page }) => {
      await navigateTo(page, '/');
      await expect(sidebar(page)).toBeVisible();
      await expect(primaryNav(page)).toBeVisible();
      await expect(mainContent(page)).toBeVisible();
      await expect(page.locator('nav.bottom-nav')).toHaveCount(0);
    });

    test('overview grid collapses to a single column on mobile', async ({ page }) => {
      await navigateTo(page, '/');
      const grid = page.locator('.grid');
      await grid.waitFor({ state: 'attached' });
      const gridCols = await grid.evaluate((el) => getComputedStyle(el).gridTemplateColumns);
      const tracks = gridCols.split(/\s+/).filter(Boolean);
      expect(tracks.length).toBe(1);
    });
  });

  test.describe('md breakpoint (641–1024px / iPad)', () => {
    test.use({ viewport: { width: 820, height: 1180 } });

    test('sidebar shell remains visible on tablet', async ({ page }) => {
      await navigateTo(page, '/');
      await expect(sidebar(page)).toBeVisible();
      await expect(primaryNav(page)).toBeVisible();
      await expect(page.locator('.sidebar-footer')).toBeVisible();
      await expect(page.locator('nav.bottom-nav')).toHaveCount(0);
    });
  });

  test.describe('lg breakpoint (>1024px / Desktop)', () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test('desktop shell exposes the primary navigation links', async ({ page }) => {
      await navigateTo(page, '/');
      const nav = primaryNav(page);
      await expect(nav).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Overview' })).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Memory' })).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Goals' })).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Workflows' })).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Orchestration' })).toBeVisible();
      await expect(nav.getByRole('link', { name: 'Settings' })).toBeVisible();
    });

    test('desktop shell keeps the sidebar footer visible', async ({ page }) => {
      await navigateTo(page, '/');
      await expect(page.locator('.sidebar-footer')).toBeVisible();
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. TOUCH-READY VISUALS
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Touch-ready visuals', () => {
  test.describe('Workflow canvas', () => {
    test.use({ viewport: { width: 390, height: 844 } });

    test('workflow canvas exposes accessibility metadata and touch-action none', async ({ page }) => {
      await navigateTo(page, '/workflows');
      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      await expect(canvas).toHaveAttribute('role', 'img');
      await expect(canvas).toHaveAttribute('aria-label', 'Workflow canvas');

      const touchAction = await canvas.evaluate((el) => getComputedStyle(el).touchAction);
      expect(touchAction).toBe('none');
    });

    test('workflow canvas exposes a viewBox for gesture-driven transforms', async ({ page }) => {
      await navigateTo(page, '/workflows');
      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      const viewBox = await canvas.getAttribute('viewBox');
      expect(viewBox).toBeTruthy();
      expect(viewBox!.split(/\s+/).map(Number)).toHaveLength(4);
    });
  });

  test.describe('Orchestration trust graph', () => {
    test.use({ viewport: { width: 390, height: 844 } });

    test('trust graph renders an accessible SVG', async ({ page }) => {
      await navigateTo(page, '/orchestration');
      const graphSvg = page.locator('svg.graph-svg');
      await graphSvg.waitFor({ state: 'attached' });

      await expect(graphSvg).toHaveAttribute('role', 'img');
      await expect(graphSvg).toHaveAttribute('aria-label', 'Trust graph');
    });

    test('trust graph renders node circles from mocked data', async ({ page }) => {
      await navigateTo(page, '/orchestration');
      const nodeCircles = page.locator('svg.graph-svg g circle[r="24"]');
      await expect(nodeCircles).toHaveCount(2);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. NAVIGATION
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Primary navigation', () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test('Goals link navigates and is marked current', async ({ page }) => {
    await navigateTo(page, '/');
    const nav = primaryNav(page);
    await nav.getByRole('link', { name: 'Goals' }).click();
    await page.waitForURL('**/goals');
    await expect(nav.locator('a[href="/goals"]')).toHaveAttribute('aria-current', 'page');
  });

  test('Memory link navigates and is marked current', async ({ page }) => {
    await navigateTo(page, '/');
    const nav = primaryNav(page);
    await nav.getByRole('link', { name: 'Memory' }).click();
    await page.waitForURL('**/memory');
    await expect(nav.locator('a[href="/memory"]')).toHaveAttribute('aria-current', 'page');
  });

  test('Orchestration link navigates and is marked current', async ({ page }) => {
    await navigateTo(page, '/');
    const nav = primaryNav(page);
    await nav.getByRole('link', { name: 'Orchestration' }).click();
    await page.waitForURL('**/orchestration');
    await expect(nav.locator('a[href="/orchestration"]')).toHaveAttribute('aria-current', 'page');
  });

  test('Settings route reveals the settings sub-navigation', async ({ page }) => {
    await navigateTo(page, '/settings');
    const subnav = page.locator('.settings-subnav');
    await expect(subnav).toBeVisible();

    const hrefs = await subnav.locator('a').evaluateAll((els) =>
      els.map((el) => el.getAttribute('href')),
    );
    expect(hrefs).toContain('/settings/profiles');
    expect(hrefs).toContain('/settings/policies');
    expect(hrefs).toContain('/settings/backups');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. PWA INSTALL FLOW
// ═══════════════════════════════════════════════════════════════════════════

test.describe('PWA install prerequisites', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('manifest.json link is present in head', async ({ page }) => {
    await navigateTo(page, '/');
    const manifest = page.locator('link[rel="manifest"]');
    await expect(manifest).toBeAttached();
    await expect(manifest).toHaveAttribute('href', '/manifest.json');
  });

  test('theme-color meta tag is set', async ({ page }) => {
    await navigateTo(page, '/');
    const themeColor = page.locator('meta[name="theme-color"]');
    await expect(themeColor).toBeAttached();
    await expect(themeColor).toHaveAttribute('content', '#1a1a2e');
  });

  test('apple-mobile-web-app-capable meta tag is set', async ({ page }) => {
    await navigateTo(page, '/');
    const capable = page.locator('meta[name="apple-mobile-web-app-capable"]');
    await expect(capable).toBeAttached();
    await expect(capable).toHaveAttribute('content', 'yes');
  });

  test('apple-mobile-web-app-status-bar-style meta tag is set', async ({ page }) => {
    await navigateTo(page, '/');
    const statusBar = page.locator('meta[name="apple-mobile-web-app-status-bar-style"]');
    await expect(statusBar).toBeAttached();
    await expect(statusBar).toHaveAttribute('content', 'black-translucent');
  });

  test('manifest.json is fetchable and contains required PWA fields', async ({ page }) => {
    await navigateTo(page, '/');
    const manifestResp = await page.request.get('/manifest.json');
    expect(manifestResp.ok()).toBe(true);

    const manifest = await manifestResp.json();
    expect(manifest.name).toBe('GHOST Dashboard');
    expect(manifest.short_name).toBe('GHOST');
    expect(manifest.display).toBe('standalone');
    expect(manifest.start_url).toBe('/');
    expect(manifest.theme_color).toBe('#1a1a2e');
    expect(manifest.icons).toBeDefined();
    expect(manifest.icons.length).toBeGreaterThanOrEqual(2);

    const maskable = manifest.icons.find((icon: { purpose?: string }) => icon.purpose === 'maskable');
    expect(maskable).toBeDefined();
  });

  test('install banner appears when beforeinstallprompt fires', async ({ page }) => {
    await navigateTo(page, '/');

    await page.evaluate(() => {
      const event = new Event('beforeinstallprompt', {
        bubbles: true,
        cancelable: true,
      });
      (event as Event & { prompt?: () => Promise<void>; userChoice?: Promise<{ outcome: string }> }).prompt =
        () => Promise.resolve();
      (
        event as Event & { prompt?: () => Promise<void>; userChoice?: Promise<{ outcome: string }> }
      ).userChoice = Promise.resolve({ outcome: 'dismissed' });
      window.dispatchEvent(event);
    });

    const installBanner = page.locator('.install-banner');
    await expect(installBanner).toBeVisible();
    await expect(installBanner).toContainText('Install GHOST Dashboard');
    await expect(installBanner.locator('button', { hasText: 'Install' })).toBeVisible();
    await expect(installBanner.locator('button', { hasText: 'Dismiss' })).toBeVisible();
  });

  test('dismiss button hides the install banner', async ({ page }) => {
    await navigateTo(page, '/');

    await page.evaluate(() => {
      const event = new Event('beforeinstallprompt', {
        bubbles: true,
        cancelable: true,
      });
      (event as Event & { prompt?: () => Promise<void>; userChoice?: Promise<{ outcome: string }> }).prompt =
        () => Promise.resolve();
      (
        event as Event & { prompt?: () => Promise<void>; userChoice?: Promise<{ outcome: string }> }
      ).userChoice = Promise.resolve({ outcome: 'dismissed' });
      window.dispatchEvent(event);
    });

    const installBanner = page.locator('.install-banner');
    await expect(installBanner).toBeVisible();
    await installBanner.locator('button', { hasText: 'Dismiss' }).click();
    await expect(installBanner).toBeHidden();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. OFFLINE BANNER
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Offline awareness', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('offline banner appears when browser goes offline', async ({ page, context }) => {
    await navigateTo(page, '/');
    await context.setOffline(true);
    await page.evaluate(() => window.dispatchEvent(new Event('offline')));

    const offlineBanner = page.locator('.offline-banner');
    await expect(offlineBanner).toBeVisible();
    await expect(offlineBanner).toContainText('Offline');
    await expect(offlineBanner).toHaveAttribute('role', 'alert');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 6. AUTH REDIRECT
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Auth redirect', () => {
  test('unauthenticated user is redirected to /login', async ({ page }) => {
    await mockAllApis(page);
    await page.route('**/api/auth/session', (route) =>
      route.fulfill({
        status: 401,
        contentType: 'application/json',
        body: JSON.stringify({
          error: {
            code: 'MISSING_TOKEN',
            message: 'Authorization header with Bearer token required',
          },
        }),
      }),
    );
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(page.url()).toContain('/login');
  });

  test('login page renders without the application shell', async ({ page }) => {
    await mockAllApis(page);
    await page.goto('/login', { waitUntil: 'networkidle' });

    await expect(page.locator(PRIMARY_NAV_SELECTOR)).toHaveCount(0);
    await expect(page.locator(SIDEBAR_SELECTOR)).toHaveCount(0);
    await expect(page.locator('.login-card')).toBeVisible();
  });
});
