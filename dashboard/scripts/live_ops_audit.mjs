#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import {
  attachBrowserEventCapture,
  buildTempConfig,
  createLoggedProcess,
  ensureGatewayBinary,
  fetchJson,
  getFreePort,
  loginViaDashboard,
  nowIso,
  persistBrowserArtifacts,
  runLoggedCaptureCommand,
  runLoggedCommand,
  timestampLabel,
  waitForHttp,
  writeJson,
} from './lib/live_harness.mjs';

const DEFAULT_TIMEOUT_MS = 60_000;
const DEFAULT_JWT_SECRET = 'ghost-live-jwt-secret';
const DASHBOARD_CLIENT_NAME = 'dashboard';
const DASHBOARD_CLIENT_VERSION = '0.1.0';

function parseArgs(argv) {
  const options = {
    mode: 'dev',
    headed: false,
    keepArtifacts: false,
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    switch (arg) {
      case '--mode':
        options.mode = argv[index + 1] ?? options.mode;
        index += 1;
        break;
      case '--headed':
        options.headed = true;
        break;
      case '--keep-artifacts':
        options.keepArtifacts = true;
        break;
      case '--timeout-ms':
        options.timeoutMs = Number.parseInt(argv[index + 1] ?? '', 10) || options.timeoutMs;
        index += 1;
        break;
      case '--help':
      case '-h':
        printHelp();
        process.exit(0);
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }

  if (!['dev', 'preview'].includes(options.mode)) {
    throw new Error(`Unsupported mode: ${options.mode}`);
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live ops audit

Usage:
  pnpm audit:ops-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                      [--timeout-ms 60000]

What it does:
  1. Boots a real gateway with PC control enabled on a temp config
  2. Seeds runtime session + trace data for the observability page
  3. Executes a safe PC-control skill path and verifies action logging
  4. Drives the real pc-control, observability, security, and settings pages
  5. Verifies logout and websocket disconnect at the end of the journey
`);
}

function authHeaders(accessToken, extra = {}) {
  return {
    Authorization: `Bearer ${accessToken}`,
    ...extra,
  };
}

function mutationHeaders(accessToken, mutationKey, extra = {}) {
  const requestId = randomUUID();
  return authHeaders(accessToken, {
    'content-type': 'application/json',
    'x-request-id': requestId,
    'x-ghost-operation-id': randomUUID(),
    'idempotency-key': `${mutationKey}-${requestId}`,
    'x-ghost-client-name': DASHBOARD_CLIENT_NAME,
    'x-ghost-client-version': DASHBOARD_CLIENT_VERSION,
    ...extra,
  });
}

function yamlString(value) {
  return JSON.stringify(value);
}

function sqlString(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

function upsertTopLevelSection(text, sectionName, replacementLines) {
  const lines = text.split(/\r?\n/);
  const next = [];
  let skipping = false;
  let replaced = false;

  for (const line of lines) {
    const isTopLevel = /^\S[^:]*:\s*(#.*)?$/.test(line);
    const key = isTopLevel ? line.replace(/:.*/, '').trim() : null;

    if (isTopLevel && key === sectionName) {
      if (!replaced) {
        next.push(...replacementLines);
        replaced = true;
      }
      skipping = true;
      continue;
    }

    if (skipping && isTopLevel) {
      skipping = false;
    }

    if (!skipping) {
      next.push(line);
    }
  }

  if (!replaced) {
    if (next.length > 0 && next.at(-1)?.trim() !== '') {
      next.push('');
    }
    next.push(...replacementLines);
  }

  return `${next.join('\n').replace(/\n+$/, '')}\n`;
}

function buildOpsConfig(baseConfigText, gatewayPort, dbPath) {
  const baseConfig = buildTempConfig(baseConfigText, gatewayPort, dbPath);
  const pcControlLines = [
    'pc_control:',
    '  enabled: true',
    '  allowed_apps: []',
    '  blocked_hotkeys: []',
  ];
  return upsertTopLevelSection(baseConfig, 'pc_control', pcControlLines);
}

async function runGatewayMigration(gatewayBinary, repoRoot, env, configPath, logPath) {
  await runLoggedCommand(gatewayBinary, ['-c', configPath, 'db', 'migrate'], {
    cwd: repoRoot,
    env,
    logPath,
  });
}

async function startGatewayInstance(gatewayBinary, repoRoot, env, configPath, gatewayUrl, logPath) {
  const gatewayProcess = createLoggedProcess(
    gatewayBinary,
    ['-c', configPath, 'serve'],
    { cwd: repoRoot, env, logPath },
  );

  await waitForHttp(`${gatewayUrl}/api/health`, {
    timeoutMs: 30_000,
    process: gatewayProcess.child,
    name: `gateway ${gatewayUrl}`,
  });
  await waitForHttp(`${gatewayUrl}/api/ready`, {
    timeoutMs: 30_000,
    process: gatewayProcess.child,
    name: `gateway readiness ${gatewayUrl}`,
  });

  return gatewayProcess;
}

async function runSqlite(dbPath, sql, repoRoot, env, logPath) {
  await runLoggedCommand('sqlite3', [dbPath], {
    cwd: repoRoot,
    env,
    input: sql,
    logPath,
  });
}

async function querySqliteScalar(dbPath, sql, repoRoot, env, logPath) {
  const result = await runLoggedCaptureCommand('sqlite3', [dbPath, sql], {
    cwd: repoRoot,
    env,
    logPath,
  });
  return result.stdout.trim();
}

async function waitForWsFrame(browserEvents, predicate, timeoutMs) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (browserEvents.wsFrames.some(predicate)) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  return false;
}

function requireCheck(summary, key, condition, message) {
  summary.checks[key] = Boolean(condition);
  if (!condition) {
    throw new Error(message);
  }
}

async function seedObservabilityData(dbPath, sessionId, repoRoot, env, logPath, runLabel) {
  const traceId = `ops-trace-${runLabel.toLowerCase()}`;
  const spanId = `ops-span-${runLabel.toLowerCase()}`;
  const hashSeed = runLabel.replace('-', '').toLowerCase();
  const firstHash = `${hashSeed}aa11`.slice(0, 32).padEnd(32, 'a');
  const secondHash = `${hashSeed}bb22`.slice(0, 32).padEnd(32, 'b');
  const firstTimestamp = '2026-03-09T04:10:00Z';
  const secondTimestamp = '2026-03-09T04:11:00Z';
  const spanStart = '2026-03-09T04:12:00Z';
  const spanEnd = '2026-03-09T04:12:01Z';
  const sender = 'ops-live';
  const firstAttributes = JSON.stringify({
    role: 'user',
    content: `ops session ${runLabel} seeded start`,
  });
  const secondAttributes = JSON.stringify({
    role: 'assistant',
    content: `ops session ${runLabel} seeded reply`,
  });

  const sql = `
BEGIN;
INSERT INTO itp_events (
  id, session_id, event_type, sender, timestamp, sequence_number,
  content_hash, content_length, privacy_level, latency_ms, token_count,
  event_hash, previous_hash, attributes
) VALUES (
  ${sqlString(`${sessionId}-evt-1`)},
  ${sqlString(sessionId)},
  'InteractionMessage',
  ${sqlString(sender)},
  ${sqlString(firstTimestamp)},
  1,
  ${sqlString(firstHash)},
  ${firstAttributes.length},
  'internal',
  4,
  10,
  x'${firstHash}',
  x'00000000000000000000000000000000',
  ${sqlString(firstAttributes)}
);
INSERT INTO itp_events (
  id, session_id, event_type, sender, timestamp, sequence_number,
  content_hash, content_length, privacy_level, latency_ms, token_count,
  event_hash, previous_hash, attributes
) VALUES (
  ${sqlString(`${sessionId}-evt-2`)},
  ${sqlString(sessionId)},
  'InteractionMessage',
  ${sqlString(sender)},
  ${sqlString(secondTimestamp)},
  2,
  ${sqlString(secondHash)},
  ${secondAttributes.length},
  'internal',
  7,
  18,
  x'${secondHash}',
  x'${firstHash}',
  ${sqlString(secondAttributes)}
);
INSERT INTO otel_spans (
  span_id,
  trace_id,
  parent_span_id,
  operation_name,
  start_time,
  end_time,
  attributes,
  status,
  session_id
) VALUES (
  ${sqlString(spanId)},
  ${sqlString(traceId)},
  NULL,
  'ops_live.seeded_trace',
  ${sqlString(spanStart)},
  ${sqlString(spanEnd)},
  ${sqlString(JSON.stringify({ seeded_by: 'live_ops_audit', session_id: sessionId }))},
  'ok',
  ${sqlString(sessionId)}
);
COMMIT;`;

  await runSqlite(dbPath, sql, repoRoot, env, logPath);
  return { sessionId, traceId, spanId };
}

function boxCenter(box, left, top) {
  return {
    x: box.x + left,
    y: box.y + top,
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-ops-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const homeDir = path.join(tempDir, 'home');

  await fs.mkdir(homeDir, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const configPath = path.join(tempDir, 'ghost.yml');
  const dbPath = path.join(tempDir, 'ghost.db');

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    artifact_dir: runDir,
    pc_control: {},
    observability: {},
    security: {},
    settings: {},
    checks: {},
    warnings: [],
    status: 'running',
  };

  let gatewayProcess = null;
  let dashboardProcess = null;
  let browser = null;
  let context = null;
  let page = null;
  let browserEvents = null;

  try {
    const baseConfigText = await fs.readFile(path.join(repoRoot, 'ghost.yml'), 'utf8');
    const gatewayBuildLog = path.join(runDir, 'gateway-build.log');
    const gatewayMigrateLog = path.join(runDir, 'gateway-migrate.log');
    const gatewayLog = path.join(runDir, 'gateway.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');
    const sqliteLogPath = path.join(runDir, 'sqlite.log');
    const gatewayBinary = await ensureGatewayBinary(repoRoot, gatewayBuildLog);

    const gatewayEnv = {
      ...process.env,
      HOME: homeDir,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_pc_control=info',
    };

    const configText = buildOpsConfig(baseConfigText, gatewayPort, dbPath);
    await fs.writeFile(configPath, configText);

    await runGatewayMigration(gatewayBinary, repoRoot, gatewayEnv, configPath, gatewayMigrateLog);
    gatewayProcess = await startGatewayInstance(
      gatewayBinary,
      repoRoot,
      gatewayEnv,
      configPath,
      gatewayUrl,
      gatewayLog,
    );

    if (options.mode === 'preview') {
      await runLoggedCommand('pnpm', ['build'], {
        cwd: dashboardDir,
        env: process.env,
        logPath: dashboardLogPath,
      });
      dashboardProcess = createLoggedProcess(
        'pnpm',
        ['exec', 'vite', 'preview', '--host', '127.0.0.1', '--port', String(dashboardPort)],
        { cwd: dashboardDir, env: process.env, logPath: dashboardLogPath },
      );
    } else {
      dashboardProcess = createLoggedProcess(
        'pnpm',
        ['exec', 'vite', 'dev', '--host', '127.0.0.1', '--port', String(dashboardPort)],
        { cwd: dashboardDir, env: process.env, logPath: dashboardLogPath },
      );
    }

    await waitForHttp(`${dashboardUrl}/login`, {
      timeoutMs: 45_000,
      process: dashboardProcess.child,
      name: 'dashboard',
    });
    summary.checks.dashboard_ready = true;

    browser = await chromium.launch({ headless: !options.headed });
    context = await browser.newContext({ baseURL: dashboardUrl });
    await context.tracing.start({ screenshots: true, snapshots: true });
    page = await context.newPage();
    browserEvents = attachBrowserEventCapture(page);

    const login = await loginViaDashboard(
      page,
      {
        dashboardUrl,
        gatewayUrl,
        jwtSecret: DEFAULT_JWT_SECRET,
        timeoutMs: options.timeoutMs,
      },
      browserEvents,
    );
    Object.assign(summary.checks, login.checks);
    const accessToken = login.accessToken;

    const seededSessionId = randomUUID();
    summary.observability.seed = await seedObservabilityData(
      dbPath,
      seededSessionId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
      runLabel,
    );

    const skills = await fetchJson(`${gatewayUrl}/api/skills`, {
      headers: authHeaders(accessToken),
    });
    summary.pc_control.skills = skills.body;
    requireCheck(
      summary,
      'pc_control_list_windows_installed',
      skills.status === 200 &&
        (skills.body?.installed ?? []).some((skill) => skill.name === 'list_windows'),
      'list_windows was not available for live PC-control execution',
    );

    const pcActionAgentId = randomUUID();
    const blockedPcAction = await fetchJson(`${gatewayUrl}/api/skills/mouse_move/execute`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `ops-mouse-move-${runLabel}`),
      body: JSON.stringify({
        agent_id: pcActionAgentId,
        session_id: randomUUID(),
        input: { x: 80, y: 80 },
      }),
    });
    const pcActionSessionId = randomUUID();
    const pcAction = await fetchJson(`${gatewayUrl}/api/skills/list_windows/execute`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `ops-list-windows-${runLabel}`),
      body: JSON.stringify({
        agent_id: pcActionAgentId,
        session_id: pcActionSessionId,
        input: {},
      }),
    });
    const pcActions = await fetchJson(`${gatewayUrl}/api/pc-control/actions?limit=20`, {
      headers: authHeaders(accessToken),
    });
    summary.pc_control.initial_execute = {
      blocked_response: blockedPcAction.body,
      response: pcAction.body,
      actions: pcActions.body,
    };
    requireCheck(
      summary,
      'pc_control_external_execute_blocked_on_canonical_route',
      blockedPcAction.status === 409 &&
        blockedPcAction.body?.error?.code === 'NON_IDEMPOTENT_SKILL_UNSUPPORTED',
      'External-side-effect PC-control skill was not blocked on the canonical execute route',
    );
    requireCheck(
      summary,
      'pc_control_read_only_action_executes',
      pcAction.status === 200 && pcAction.body?.result?.status === 'ok',
      'Read-only PC-control action did not execute successfully',
    );
    requireCheck(
      summary,
      'pc_control_action_logged',
      pcActions.status === 200 &&
        (pcActions.body?.actions ?? []).some((entry) => entry.action_type === 'list_windows'),
      'PC-control action log did not record the executed list_windows action',
    );

    await page.goto(`${dashboardUrl}/pc-control`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'PC Control' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    const actionLogSection = page.locator('section.card').filter({
      has: page.getByRole('heading', { name: 'Action Log' }),
    });
    await actionLogSection
      .locator('.log-row')
      .filter({ hasText: 'list_windows' })
      .first()
      .waitFor({ state: 'visible', timeout: options.timeoutMs });
    requireCheck(
      summary,
      'pc_control_page_loaded',
      (await page.locator('.error-banner').count()) === 0,
      'PC-control page rendered an error banner',
    );

    const appName = `OpsApp-${runLabel}`;
    await page.getByPlaceholder('Application name').fill(appName, { timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Add' }).first().click({ timeout: options.timeoutMs });
    await page.getByText(appName).waitFor({ state: 'visible', timeout: options.timeoutMs });

    const hotkey = 'Ctrl+Shift+Y';
    await page.getByPlaceholder('e.g. Cmd+Q').fill(hotkey, { timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Add' }).nth(1).click({ timeout: options.timeoutMs });
    await page.getByText(hotkey).waitFor({ state: 'visible', timeout: options.timeoutMs });

    const canvas = page.locator('svg.zone-canvas');
    const box = await canvas.boundingBox();
    if (!box) {
      throw new Error('Safe-zone canvas did not render');
    }
    const start = boxCenter(box, 40, 40);
    const end = boxCenter(box, 180, 120);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.mouse.move(end.x, end.y, { steps: 5 });
    await page.mouse.up();
    await page.getByText('Primary Safe Zone').waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });

    const pcToggleButton = page.getByRole('button', { name: 'Toggle PC control' });
    await pcToggleButton.click({ timeout: options.timeoutMs });
    await page.locator('button.toggle-btn').filter({ hasText: 'Disabled' }).first().waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });

    requireCheck(
      summary,
      'ws_pc_control_runtime_change_received',
      await waitForWsFrame(
        browserEvents,
        (frame) =>
          typeof frame.payload === 'string' &&
          frame.payload.includes('"type":"PcControlRuntimeChange"') &&
          frame.payload.includes('"pc_control"'),
        options.timeoutMs,
      ),
      'Dashboard websocket did not receive PcControlRuntimeChange after PC-control mutation',
    );

    const pcStatus = await fetchJson(`${gatewayUrl}/api/pc-control/status`, {
      headers: authHeaders(accessToken),
    });
    summary.pc_control.status = pcStatus.body;
    requireCheck(
      summary,
      'pc_control_status_persisted',
      pcStatus.status === 200 &&
        pcStatus.body?.enabled === false &&
        (pcStatus.body?.allowed_apps ?? []).includes(appName) &&
        (pcStatus.body?.blocked_hotkeys ?? []).includes(hotkey) &&
        (pcStatus.body?.safe_zones ?? []).length === 1,
      'PC-control status did not persist dashboard mutations',
    );

    const seededSessions = await fetchJson(`${gatewayUrl}/api/sessions?limit=20`, {
      headers: authHeaders(accessToken),
    });
    const seededTraces = await fetchJson(`${gatewayUrl}/api/traces/${seededSessionId}`, {
      headers: authHeaders(accessToken),
    });
    summary.observability.api = {
      sessions: seededSessions.body,
      traces: seededTraces.body,
    };
    requireCheck(
      summary,
      'observability_seed_visible_via_api',
      seededSessions.status === 200 &&
        (seededSessions.body?.data ?? []).some((session) => session.session_id === seededSessionId),
      'Seeded session was not visible via runtime sessions API',
    );
    requireCheck(
      summary,
      'observability_trace_visible_via_api',
      seededTraces.status === 200 && seededTraces.body?.total_spans === 1,
      'Seeded trace was not visible via traces API',
    );

    await page.goto(`${dashboardUrl}/observability`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Observability' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.getByRole('button').filter({ hasText: seededSessionId.slice(0, 8) }).click({
      timeout: options.timeoutMs,
    });
    await page.getByText('ops_live.seeded_trace').waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    requireCheck(
      summary,
      'observability_page_loaded',
      (await page.locator('.error-msg').count()) === 0,
      'Observability page rendered an error state',
    );
    requireCheck(
      summary,
      'observability_trace_waterfall_rendered',
      await page.locator('.span-count').textContent().then((value) => value?.includes('1 spans')),
      'Observability page did not render the seeded trace waterfall',
    );

    const auditQuery = await fetchJson(`${gatewayUrl}/api/audit?page_size=20`, {
      headers: authHeaders(accessToken),
    });
    summary.security.audit = auditQuery.body;
    requireCheck(
      summary,
      'security_audit_contains_pc_control_entries',
      auditQuery.status === 200 &&
        (auditQuery.body?.entries ?? []).some((entry) =>
          String(entry.event_type ?? '').startsWith('pc_control_'),
        ),
      'Audit query did not include PC-control mutations from the live ops run',
    );

    await page.goto(`${dashboardUrl}/security`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Security' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    const exportResponsePromise = page.waitForResponse(
      (response) =>
        response.url() === `${gatewayUrl}/api/audit/export?format=json` &&
        response.request().method() === 'GET',
      { timeout: options.timeoutMs },
    );
    await page.getByRole('button', { name: 'JSON', exact: true }).click({
      timeout: options.timeoutMs,
    });
    const exportResponse = await exportResponsePromise;
    summary.security.export = {
      status: exportResponse.status(),
    };
    requireCheck(
      summary,
      'security_page_loaded',
      (await page.locator('.error-state').count()) === 0,
      'Security page rendered an error state',
    );
    requireCheck(
      summary,
      'security_json_export_succeeded',
      exportResponse.status() === 200,
      'Security audit export did not return HTTP 200',
    );

    await page.goto(`${dashboardUrl}/settings`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Settings' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.getByRole('radio', { name: 'Light' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(() => document.documentElement.classList.contains('light'), {
      timeout: options.timeoutMs,
    });
    requireCheck(
      summary,
      'settings_theme_toggle_applied',
      await page.evaluate(() => document.documentElement.classList.contains('light')),
      'Settings theme toggle did not apply the light class',
    );

    const logoutResponsePromise = page.waitForResponse(
      (response) =>
        response.url() === `${gatewayUrl}/api/auth/logout` &&
        response.request().method() === 'POST',
      { timeout: options.timeoutMs },
    );
    await page.getByRole('button', { name: 'Logout' }).click({ timeout: options.timeoutMs });
    const logoutResponse = await logoutResponsePromise;
    await page.waitForURL((url) => url.pathname === '/login', { timeout: options.timeoutMs });

    summary.settings.logout = {
      status: logoutResponse.status(),
      browser_page_errors: browserEvents.pageErrors,
      browser_console: browserEvents.console,
    };
    requireCheck(
      summary,
      'settings_logout_succeeded',
      logoutResponse.status() === 200,
      'Logout endpoint did not return HTTP 200',
    );
    requireCheck(
      summary,
      'logout_redirected_to_login',
      new URL(page.url()).pathname === '/login',
      'Settings logout did not redirect back to /login',
    );
    requireCheck(
      summary,
      'ws_closed_after_logout',
      await waitForWsFrame(browserEvents, (frame) => frame.direction === 'close', options.timeoutMs),
      'Websocket did not close after logout',
    );

    requireCheck(
      summary,
      'browser_page_errors_empty',
      browserEvents.pageErrors.length === 0,
      'Browser page errors were captured during ops live audit',
    );
    summary.status = 'passed';

    await writeJson(path.join(runDir, 'browser-events.json'), browserEvents);
    await persistBrowserArtifacts(runDir, 'ops', { context, page }, options.keepArtifacts);
    await writeJson(path.join(runDir, 'summary.json'), summary);

    process.stdout.write(`Artifacts: ${runDir}\n`);
    process.stdout.write(
      `Summary: ops live audit passed with ${Object.values(summary.checks).filter(Boolean).length}/${Object.keys(summary.checks).length} checks\n`,
    );
  } catch (error) {
    summary.status = 'failed';
    summary.failed_at = nowIso();
    summary.error = error instanceof Error ? error.message : String(error);

    if (browserEvents) {
      await writeJson(path.join(runDir, 'browser-events.json'), browserEvents).catch(() => {});
    }
    if (context && page) {
      await persistBrowserArtifacts(runDir, 'ops', { context, page }, true).catch(() => {});
    }
    await writeJson(path.join(runDir, 'summary.json'), summary).catch(() => {});
    process.stderr.write(`${summary.error}\n`);
    process.stderr.write(`Artifacts: ${runDir}\n`);
    process.exitCode = 1;
  } finally {
    if (page) {
      await page.close().catch(() => {});
    }
    if (context) {
      await context.close().catch(() => {});
    }
    if (browser) {
      await browser.close().catch(() => {});
    }
    if (dashboardProcess) {
      await dashboardProcess.stop().catch(() => {});
    }
    if (gatewayProcess) {
      await gatewayProcess.stop().catch(() => {});
    }
  }
}

await main();
