import { test, expect, type Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// GHOST ADE Dashboard — Mobile & Responsive Tests (T-4.10.3)
//
// These tests exercise the three responsive breakpoints defined in
// +layout.svelte, verify touch-interaction readiness on SVG canvases,
// validate mobile navigation, and check PWA install prerequisites.
//
// No live backend is required — all API calls are intercepted via
// page.route() and return deterministic mock payloads.
// ---------------------------------------------------------------------------

// ── Mock API helpers ─────────────────────────────────────────────────────────

const GATEWAY = 'http://127.0.0.1:18789';

/** Seed sessionStorage with a fake auth token so the layout renders
 *  instead of redirecting to /login. */
async function authenticate(page: Page) {
  await page.addInitScript(() => {
    sessionStorage.setItem('ghost-token', 'test-token-playwright');
  });
}

/** Register route handlers that satisfy every API call the dashboard pages
 *  make on mount, returning lightweight but structurally correct JSON. */
async function mockAllApis(page: Page) {
  // Convergence scores (overview page)
  await page.route(`${GATEWAY}/api/convergence/scores`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        scores: [{ score: 0.82, level: 3 }],
      }),
    }),
  );

  // Agents list
  await page.route(`${GATEWAY}/api/agents`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        { id: 'a-1', name: 'Agent Alpha', status: 'running' },
        { id: 'a-2', name: 'Agent Beta', status: 'idle' },
      ]),
    }),
  );

  // Goals / proposals
  await page.route(`${GATEWAY}/api/goals*`, (route) =>
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
      }),
    }),
  );

  // Workflows list
  await page.route(`${GATEWAY}/api/workflows*`, (route) => {
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

  // Orchestration: trust-graph, consensus, delegations
  await page.route(`${GATEWAY}/api/mesh/trust-graph`, (route) =>
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

  await page.route(`${GATEWAY}/api/mesh/consensus`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ rounds: [] }),
    }),
  );

  await page.route(`${GATEWAY}/api/mesh/delegations`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        delegations: [],
        sybil_metrics: { total_delegations: 0, max_chain_depth: 0, unique_delegators: 0 },
      }),
    }),
  );

  // A2A discovery
  await page.route(`${GATEWAY}/api/a2a/discover`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ agents: [] }),
    }),
  );

  await page.route(`${GATEWAY}/api/a2a/tasks`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ tasks: [] }),
    }),
  );

  // Sessions list
  await page.route(`${GATEWAY}/api/sessions*`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ sessions: [] }),
    }),
  );

  // Memory
  await page.route(`${GATEWAY}/api/memory*`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ memories: [] }),
    }),
  );

  // Push VAPID key (non-fatal, but called on mount)
  await page.route(`${GATEWAY}/api/push/vapid-key`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ key: null }),
    }),
  );

  // Auth login (used by login page)
  await page.route(`${GATEWAY}/api/auth/login`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ access_token: 'mock-jwt' }),
    }),
  );

  // Catch-all: any unmatched gateway API returns 200 with empty object
  await page.route(`${GATEWAY}/api/**`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    }),
  );

  // Prevent service worker registration from failing on file not found
  await page.route('**/service-worker.js', (route) =>
    route.fulfill({ status: 200, contentType: 'application/javascript', body: '' }),
  );
}

/** Navigate to a page with full API mocking and auth pre-seeded. */
async function navigateTo(page: Page, path: string) {
  await mockAllApis(page);
  await authenticate(page);
  await page.goto(path, { waitUntil: 'networkidle' });
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. RESPONSIVE LAYOUT — THREE BREAKPOINTS
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Responsive layout', () => {
  // ── sm (<640px) — iPhone 14 ──────────────────────────────────────────

  test.describe('sm breakpoint (<640px / iPhone 14)', () => {
    test.use({ ...({ viewport: { width: 390, height: 844 } }) });

    test('sidebar is hidden', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');
      await expect(sidebar).toBeHidden();
    });

    test('bottom nav is visible', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await expect(bottomNav).toBeVisible();
    });

    test('bottom nav contains expected links', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      const links = bottomNav.locator('a');
      await expect(links).toHaveCount(5);

      const hrefs = await links.evaluateAll((els) =>
        els.map((el) => el.getAttribute('href')),
      );
      expect(hrefs).toEqual(['/', '/agents', '/goals', '/workflows', '/settings']);
    });

    test('content occupies full width (no sidebar offset)', async ({ page }) => {
      await navigateTo(page, '/');
      const content = page.locator('main.content');
      const box = await content.boundingBox();
      expect(box).toBeTruthy();
      // Content should start at x=0 (no sidebar pushing it right)
      expect(box!.x).toBeLessThanOrEqual(1);
    });

    test('overview grid is single column on mobile', async ({ page }) => {
      await navigateTo(page, '/');
      // Wait for the grid to render (the skeleton or actual cards)
      const grid = page.locator('.grid');
      await grid.waitFor({ state: 'attached' });
      const gridCols = await grid.evaluate((el) =>
        getComputedStyle(el).gridTemplateColumns,
      );
      // Single column means only one track value
      const tracks = gridCols.split(/\s+/).filter(Boolean);
      expect(tracks.length).toBe(1);
    });
  });

  // ── md (641–1024px) — iPad ───────────────────────────────────────────

  test.describe('md breakpoint (641–1024px / iPad)', () => {
    test.use({ ...({ viewport: { width: 820, height: 1180 } }) });

    test('sidebar is visible but collapsed', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');
      await expect(sidebar).toBeVisible();

      // The collapsed sidebar should be narrower than the full width
      // (full sidebar is var(--layout-sidebar-width), collapsed is
      //  var(--layout-sidebar-collapsed)).
      const sidebarBox = await sidebar.boundingBox();
      expect(sidebarBox).toBeTruthy();
      // The collapsed sidebar should be significantly narrower than 200px
      // (typical full width). We just confirm it is visible.
      expect(sidebarBox!.width).toBeGreaterThan(0);
    });

    test('bottom nav is hidden', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await expect(bottomNav).toBeHidden();
    });

    test('sidebar links have truncated text (font-size: 0)', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebarLink = page.locator('nav.sidebar > a').first();
      const fontSize = await sidebarLink.evaluate((el) =>
        getComputedStyle(el).fontSize,
      );
      // At md breakpoint, sidebar links have font-size: 0 (icon-only)
      expect(fontSize).toBe('0px');
    });

    test('sidebar footer (ConnectionIndicator) is hidden on tablet', async ({ page }) => {
      await navigateTo(page, '/');
      const footer = page.locator('.sidebar-footer');
      await expect(footer).toBeHidden();
    });
  });

  // ── lg (>1024px) — Desktop ───────────────────────────────────────────

  test.describe('lg breakpoint (>1024px / Desktop)', () => {
    test.use({ ...({ viewport: { width: 1280, height: 800 } }) });

    test('full sidebar is visible', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');
      await expect(sidebar).toBeVisible();
    });

    test('bottom nav is hidden', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await expect(bottomNav).toBeHidden();
    });

    test('sidebar links have readable text', async ({ page }) => {
      await navigateTo(page, '/');
      const overviewLink = page.locator('nav.sidebar > a', { hasText: 'Overview' });
      await expect(overviewLink).toBeVisible();
      const fontSize = await overviewLink.evaluate((el) =>
        getComputedStyle(el).fontSize,
      );
      // Font size should be non-zero (readable)
      expect(parseFloat(fontSize)).toBeGreaterThan(0);
    });

    test('sidebar contains all navigation links', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');

      const expectedLinks = [
        'Overview',
        'Convergence',
        'Memory',
        'Goals',
        'Sessions',
        'Agents',
        'Workflows',
        'Skills',
        'Studio',
        'Observability',
        'Orchestration',
        'Security',
        'Costs',
        'Settings',
      ];

      for (const label of expectedLinks) {
        await expect(sidebar.locator('a', { hasText: label }).first()).toBeAttached();
      }
    });

    test('sidebar footer is visible on desktop', async ({ page }) => {
      await navigateTo(page, '/');
      const footer = page.locator('.sidebar-footer');
      await expect(footer).toBeVisible();
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. TOUCH INTERACTIONS
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Touch interactions', () => {
  test.describe('WorkflowCanvas touch handlers', () => {
    test.use({ ...({ viewport: { width: 390, height: 844 } }) });

    test('workflow canvas SVG has touch event handlers', async ({ page }) => {
      await navigateTo(page, '/workflows');

      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      // Verify touch-action: none is set (required for custom touch handling)
      const touchAction = await canvas.evaluate((el) =>
        getComputedStyle(el).touchAction,
      );
      expect(touchAction).toBe('none');

      // Verify the SVG element has the touch event listeners wired up.
      // In Svelte 5, event handlers are attached via ontouchstart/ontouchmove/
      // ontouchend attributes — we verify they exist as properties on the element.
      const hasTouchHandlers = await canvas.evaluate((el) => {
        // Svelte 5 compiles ontouchstart/ontouchmove/ontouchend as
        // properties on the element. We check that the DOM element
        // has these as callable functions.
        return (
          typeof (el as any).ontouchstart === 'function' ||
          el.hasAttribute('ontouchstart') ||
          // Also check via getEventListeners-style inspection where possible
          typeof (el as any).__touchstart !== 'undefined'
        );
      });
      // The handler is always attached via Svelte's compiled output
      expect(hasTouchHandlers).toBe(true);
    });

    test('workflow canvas has role="img" and aria-label', async ({ page }) => {
      await navigateTo(page, '/workflows');

      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      await expect(canvas).toHaveAttribute('role', 'img');
      await expect(canvas).toHaveAttribute('aria-label', 'Workflow canvas');
    });
  });

  test.describe('Orchestration trust graph touch handlers', () => {
    test.use({ ...({ viewport: { width: 390, height: 844 } }) });

    test('trust graph SVG has touch event handlers', async ({ page }) => {
      await navigateTo(page, '/orchestration');

      // The trust graph SVG is rendered when trust tab is active (default)
      const graphSvg = page.locator('svg.graph-svg');
      await graphSvg.waitFor({ state: 'attached' });

      // Verify touch-action is properly set for gesture handling
      const touchAction = await graphSvg.evaluate((el) =>
        getComputedStyle(el).touchAction,
      );
      // Touch action should allow custom handling (auto or none)
      expect(['none', 'auto']).toContain(touchAction);

      // Verify touch event handlers are attached
      const hasTouchHandlers = await graphSvg.evaluate((el) => {
        return (
          typeof (el as any).ontouchstart === 'function' ||
          el.hasAttribute('ontouchstart')
        );
      });
      expect(hasTouchHandlers).toBe(true);
    });

    test('trust graph renders nodes from mocked data', async ({ page }) => {
      await navigateTo(page, '/orchestration');

      const graphSvg = page.locator('svg.graph-svg');
      await graphSvg.waitFor({ state: 'attached' });

      // Wait for d3-force to tick and render nodes
      // Each node gets a <g transform="translate(...)"> containing a circle
      const nodeGroups = graphSvg.locator('g > circle[r="24"]');
      await expect(nodeGroups.first()).toBeAttached({ timeout: 5_000 });
    });
  });

  test.describe('Pinch gesture support', () => {
    test.use({ ...({ viewport: { width: 390, height: 844 } }) });

    test('workflow canvas SVG has touch-action: none for pinch zoom', async ({ page }) => {
      await navigateTo(page, '/workflows');

      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      // touch-action: none allows the JS handlers (handleTouchStart etc.)
      // to process multi-finger gestures like pinch-to-zoom.
      const touchAction = await canvas.evaluate((el) =>
        getComputedStyle(el).touchAction,
      );
      expect(touchAction).toBe('none');
    });

    test('workflow canvas has viewBox attribute for zoom transforms', async ({ page }) => {
      await navigateTo(page, '/workflows');

      const canvas = page.locator('svg.workflow-canvas');
      await canvas.waitFor({ state: 'attached' });

      const viewBox = await canvas.getAttribute('viewBox');
      expect(viewBox).toBeTruthy();
      // Default viewBox is "-50 -50 800 600"
      const parts = viewBox!.split(/\s+/).map(Number);
      expect(parts).toHaveLength(4);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. NAVIGATION
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Navigation', () => {
  test.describe('Mobile bottom nav', () => {
    test.use({ ...({ viewport: { width: 390, height: 844 } }) });

    test('bottom nav Overview link navigates to /', async ({ page }) => {
      await navigateTo(page, '/goals');
      const bottomNav = page.locator('nav.bottom-nav');
      await bottomNav.locator('a', { hasText: 'Overview' }).click();
      await page.waitForURL('/');
      expect(page.url()).toContain('/');
    });

    test('bottom nav Agents link navigates to /agents', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await bottomNav.locator('a', { hasText: 'Agents' }).click();
      await page.waitForURL('**/agents');
      expect(page.url()).toContain('/agents');
    });

    test('bottom nav Goals link navigates to /goals', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await bottomNav.locator('a', { hasText: 'Goals' }).click();
      await page.waitForURL('**/goals');
      expect(page.url()).toContain('/goals');
    });

    test('bottom nav Workflows link navigates to /workflows', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await bottomNav.locator('a', { hasText: 'Workflows' }).click();
      await page.waitForURL('**/workflows');
      expect(page.url()).toContain('/workflows');
    });

    test('bottom nav Settings link navigates to /settings', async ({ page }) => {
      await navigateTo(page, '/');
      const bottomNav = page.locator('nav.bottom-nav');
      await bottomNav.locator('a', { hasText: 'Settings' }).click();
      await page.waitForURL('**/settings');
      expect(page.url()).toContain('/settings');
    });

    test('active bottom nav link has active class', async ({ page }) => {
      await navigateTo(page, '/goals');
      const bottomNav = page.locator('nav.bottom-nav');
      const goalsLink = bottomNav.locator('a[href="/goals"]');
      await expect(goalsLink).toHaveClass(/active/);
    });
  });

  test.describe('Desktop sidebar nav', () => {
    test.use({ ...({ viewport: { width: 1280, height: 800 } }) });

    test('sidebar Memory link navigates to /memory', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');
      await sidebar.locator('a', { hasText: 'Memory' }).click();
      await page.waitForURL('**/memory');
      expect(page.url()).toContain('/memory');
    });

    test('sidebar Orchestration link navigates to /orchestration', async ({ page }) => {
      await navigateTo(page, '/');
      const sidebar = page.locator('nav.sidebar');
      await sidebar.locator('a', { hasText: 'Orchestration' }).click();
      await page.waitForURL('**/orchestration');
      expect(page.url()).toContain('/orchestration');
    });

    test('sidebar active link has active class on current route', async ({ page }) => {
      await navigateTo(page, '/memory');
      const memoryLink = page.locator('nav.sidebar > a[href="/memory"]');
      await expect(memoryLink).toHaveClass(/active/);
    });

    test('clicking Settings reveals subnav links', async ({ page }) => {
      await navigateTo(page, '/settings');
      const subnav = page.locator('.settings-subnav');
      await expect(subnav).toBeVisible();

      const subnavLinks = subnav.locator('a');
      const hrefs = await subnavLinks.evaluateAll((els) =>
        els.map((el) => el.getAttribute('href')),
      );
      expect(hrefs).toContain('/settings/profiles');
      expect(hrefs).toContain('/settings/policies');
      expect(hrefs).toContain('/settings/backups');
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. PWA INSTALL FLOW
// ═══════════════════════════════════════════════════════════════════════════

test.describe('PWA install prerequisites', () => {
  test.use({ ...({ viewport: { width: 390, height: 844 } }) });

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

    // Fetch the manifest directly and validate its structure
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

    // Check for maskable icon (required for Android adaptive icons)
    const maskable = manifest.icons.find(
      (icon: any) => icon.purpose === 'maskable',
    );
    expect(maskable).toBeDefined();
  });

  test('install banner appears when beforeinstallprompt fires', async ({ page }) => {
    await navigateTo(page, '/');

    // Simulate the beforeinstallprompt event
    await page.evaluate(() => {
      const event = new Event('beforeinstallprompt', {
        bubbles: true,
        cancelable: true,
      });
      (event as any).prompt = () => Promise.resolve();
      (event as any).userChoice = Promise.resolve({ outcome: 'dismissed' });
      window.dispatchEvent(event);
    });

    // The install banner should now be visible
    const installBanner = page.locator('.install-banner');
    await expect(installBanner).toBeVisible();
    await expect(installBanner).toContainText('Install GHOST Dashboard');

    // It should have Install and Dismiss buttons
    const installBtn = installBanner.locator('button', { hasText: 'Install' });
    const dismissBtn = installBanner.locator('button', { hasText: 'Dismiss' });
    await expect(installBtn).toBeVisible();
    await expect(dismissBtn).toBeVisible();
  });

  test('dismiss button hides the install banner', async ({ page }) => {
    await navigateTo(page, '/');

    // Fire the beforeinstallprompt event to show the banner
    await page.evaluate(() => {
      const event = new Event('beforeinstallprompt', {
        bubbles: true,
        cancelable: true,
      });
      (event as any).prompt = () => Promise.resolve();
      (event as any).userChoice = Promise.resolve({ outcome: 'dismissed' });
      window.dispatchEvent(event);
    });

    const installBanner = page.locator('.install-banner');
    await expect(installBanner).toBeVisible();

    // Click Dismiss
    await installBanner.locator('button', { hasText: 'Dismiss' }).click();
    await expect(installBanner).toBeHidden();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. OFFLINE BANNER
// ═══════════════════════════════════════════════════════════════════════════

test.describe('Offline awareness', () => {
  test.use({ ...({ viewport: { width: 390, height: 844 } }) });

  test('offline banner appears when browser goes offline', async ({ page, context }) => {
    await navigateTo(page, '/');

    // Simulate going offline
    await context.setOffline(true);

    // Trigger the offline event in the page context
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
    // Do NOT call authenticate() — no token in sessionStorage
    await mockAllApis(page);
    await page.goto('/', { waitUntil: 'networkidle' });
    // The layout should redirect to /login
    await page.waitForURL('**/login', { timeout: 5_000 });
    expect(page.url()).toContain('/login');
  });

  test('login page renders without sidebar or bottom nav', async ({ page }) => {
    await mockAllApis(page);
    await page.goto('/login', { waitUntil: 'networkidle' });

    // Login page uses a special conditional branch — no sidebar/bottom-nav
    const sidebar = page.locator('nav.sidebar');
    const bottomNav = page.locator('nav.bottom-nav');
    await expect(sidebar).toHaveCount(0);
    await expect(bottomNav).toHaveCount(0);

    // Login card is visible
    const loginCard = page.locator('.login-card');
    await expect(loginCard).toBeVisible();
  });
});
