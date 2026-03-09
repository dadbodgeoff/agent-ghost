#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JWT_SECRET = 'ghost-live-jwt-secret';
const IGNORED_BUILD_INPUT_DIRS = new Set([
  '.git',
  '.svelte-kit',
  'artifacts',
  'dashboard',
  'dist',
  'node_modules',
  'target',
]);

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
  process.stdout.write(`Live infra audit

Usage:
  pnpm audit:infra-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                        [--timeout-ms 45000]

What it does:
  1. Builds the gateway binary if needed or stale
  2. Validates config against a fresh temp gateway config
  3. Runs db migrate, db status, and db verify on a fresh temp DB
  4. Boots a real auth-enabled gateway
  5. Verifies /api/health, /api/ready, and /api/compatibility
  6. Drives the real dashboard login/logout flow with JWT auth
  7. Preserves logs/traces/screenshots only when the run fails
`);
}

function timestampLabel(date = new Date()) {
  const parts = [
    date.getUTCFullYear(),
    String(date.getUTCMonth() + 1).padStart(2, '0'),
    String(date.getUTCDate()).padStart(2, '0'),
    '-',
    String(date.getUTCHours()).padStart(2, '0'),
    String(date.getUTCMinutes()).padStart(2, '0'),
    String(date.getUTCSeconds()).padStart(2, '0'),
  ];
  return parts.join('');
}

function nowIso() {
  return new Date().toISOString();
}

async function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') {
        reject(new Error('Failed to allocate a free TCP port'));
        return;
      }
      const { port } = address;
      server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(port);
      });
    });
    server.on('error', reject);
  });
}

function isTopLevelYamlKey(line) {
  return /^\S[^:]*:\s*(#.*)?$/.test(line);
}

function buildTempConfig(baseConfigText, gatewayPort, dbPath) {
  const normalizedDbPath = dbPath.replace(/\\/g, '/');
  const lines = baseConfigText.split(/\r?\n/);
  let inGateway = false;

  return lines
    .map((line) => {
      if (isTopLevelYamlKey(line)) {
        inGateway = line.trim() === 'gateway:';
        return line;
      }

      if (inGateway && /^\s{2}port:/.test(line)) {
        return `  port: ${gatewayPort}`;
      }

      if (inGateway && /^\s{2}db_path:/.test(line)) {
        return `  db_path: "${normalizedDbPath}"`;
      }

      return line;
    })
    .join('\n');
}

function createLoggedProcess(command, args, options) {
  const logStream = createWriteStream(options.logPath, { flags: 'a' });
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    logStream.write(chunk);
  });

  child.on('error', (error) => {
    logStream.write(`\n[process-error] ${error.stack ?? error.message}\n`);
  });

  return {
    child,
    async stop() {
      if (child.exitCode !== null) {
        await new Promise((resolve) => logStream.end(resolve));
        return;
      }

      child.kill('SIGINT');
      const exited = await Promise.race([
        new Promise((resolve) => child.once('exit', resolve)),
        delay(5_000).then(() => false),
      ]);

      if (exited === false && child.exitCode === null) {
        child.kill('SIGTERM');
        await Promise.race([
          new Promise((resolve) => child.once('exit', resolve)),
          delay(5_000),
        ]);
      }

      await new Promise((resolve) => logStream.end(resolve));
    },
  };
}

function createGatewayProcess(gatewayBinary, repoRoot, configPath, gatewayEnv, gatewayLogPath) {
  return createLoggedProcess(
    gatewayBinary,
    ['-c', configPath, 'serve'],
    { cwd: repoRoot, env: gatewayEnv, logPath: gatewayLogPath },
  );
}

async function newestGatewayInputMtimeMs(targetPath) {
  let stats;
  try {
    stats = await fs.stat(targetPath);
  } catch {
    return 0;
  }

  if (stats.isFile()) {
    const base = path.basename(targetPath);
    const ext = path.extname(targetPath);
    if (ext === '.rs' || base === 'Cargo.toml' || base === 'Cargo.lock') {
      return stats.mtimeMs;
    }
    return 0;
  }

  if (!stats.isDirectory()) {
    return 0;
  }

  const base = path.basename(targetPath);
  if (IGNORED_BUILD_INPUT_DIRS.has(base)) {
    return 0;
  }

  let newest = 0;
  const entries = await fs.readdir(targetPath, { withFileTypes: true });
  for (const entry of entries) {
    const childPath = path.join(targetPath, entry.name);
    const childNewest = await newestGatewayInputMtimeMs(childPath);
    if (childNewest > newest) {
      newest = childNewest;
    }
  }
  return newest;
}

async function runLoggedCommand(command, args, options) {
  const logStream = createWriteStream(options.logPath, { flags: 'a' });
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    logStream.write(chunk);
  });

  const exitCode = await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });

  await new Promise((resolve) => logStream.end(resolve));

  if (exitCode !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited with code ${exitCode}`);
  }
}

async function runLoggedCaptureCommand(command, args, options) {
  const logStream = createWriteStream(options.logPath, { flags: 'a' });
  const stdoutChunks = [];
  const stderrChunks = [];
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    const text = chunk.toString();
    stdoutChunks.push(text);
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    const text = chunk.toString();
    stderrChunks.push(text);
    logStream.write(chunk);
  });

  const exitCode = await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });

  await new Promise((resolve) => logStream.end(resolve));

  const stdout = stdoutChunks.join('');
  const stderr = stderrChunks.join('');
  if (exitCode !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited with code ${exitCode}\n${stdout}${stderr}`);
  }

  return { stdout, stderr };
}

async function waitForHttp(url, options = {}) {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const start = Date.now();

  while (Date.now() - start < timeoutMs) {
    if (options.process && options.process.exitCode !== null) {
      throw new Error(`${options.name ?? url} exited before becoming ready`);
    }

    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      // Retry until timeout.
    }

    await delay(250);
  }

  throw new Error(`${options.name ?? url} did not become ready within ${timeoutMs}ms`);
}

async function ensureGatewayBinary(repoRoot, logPath) {
  const binaryPath = path.join(repoRoot, 'target', 'debug', os.platform() === 'win32' ? 'ghost.exe' : 'ghost');
  const binaryMtimeMs = await fs.stat(binaryPath)
    .then((stats) => stats.mtimeMs)
    .catch(() => 0);
  const newestInputMtimeMs = Math.max(
    await newestGatewayInputMtimeMs(path.join(repoRoot, 'Cargo.toml')),
    await newestGatewayInputMtimeMs(path.join(repoRoot, 'Cargo.lock')),
    await newestGatewayInputMtimeMs(path.join(repoRoot, 'crates')),
  );

  if (binaryMtimeMs === 0 || newestInputMtimeMs > binaryMtimeMs) {
    await runLoggedCommand('cargo', ['build', '-p', 'ghost-gateway'], {
      cwd: repoRoot,
      env: process.env,
      logPath,
    });
  }

  return binaryPath;
}

async function writeJson(filePath, value) {
  await fs.writeFile(filePath, JSON.stringify(value, null, 2));
}

async function writeJsonLines(filePath, entries) {
  const body = entries.map((entry) => JSON.stringify(entry)).join('\n');
  await fs.writeFile(filePath, body ? `${body}\n` : '');
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  let body = null;
  if (text.trim().length > 0) {
    try {
      body = JSON.parse(text);
    } catch {
      body = { raw: text };
    }
  }

  return {
    ok: response.ok,
    status: response.status,
    body,
  };
}

function connectionIndicator(page) {
  return page.locator('.sidebar-footer-row .indicator .label').first();
}

async function waitForConnectionState(page, expectedText, timeoutMs) {
  const label = connectionIndicator(page);
  await label.waitFor({ state: 'visible', timeout: timeoutMs });
  await page.waitForFunction(
    (expected) => {
      const node = document.querySelector('.sidebar-footer-row .indicator .label');
      return (node?.textContent ?? '').trim() === expected;
    },
    expectedText,
    { timeout: timeoutMs },
  );
}

async function runBrowserInfraJourney(options) {
  const browser = await chromium.launch({ headless: !options.headed });
  const context = await browser.newContext();
  await context.tracing.start({ screenshots: true, snapshots: true });

  const page = await context.newPage();
  const browserEvents = {
    console: [],
    pageErrors: [],
    requests: [],
    responses: [],
    requestFailures: [],
    wsFrames: [],
  };

  page.on('console', (message) => {
    browserEvents.console.push({
      captured_at: nowIso(),
      type: message.type(),
      text: message.text(),
    });
  });

  page.on('pageerror', (error) => {
    browserEvents.pageErrors.push({
      captured_at: nowIso(),
      message: error.stack ?? error.message,
    });
  });

  page.on('request', (request) => {
    browserEvents.requests.push({
      captured_at: nowIso(),
      method: request.method(),
      url: request.url(),
      resource_type: request.resourceType(),
    });
  });

  page.on('response', (response) => {
    browserEvents.responses.push({
      captured_at: nowIso(),
      method: response.request().method(),
      url: response.url(),
      status: response.status(),
    });
  });

  page.on('requestfailed', (request) => {
    browserEvents.requestFailures.push({
      captured_at: nowIso(),
      method: request.method(),
      url: request.url(),
      error_text: request.failure()?.errorText ?? 'unknown',
    });
  });

  page.on('websocket', (socket) => {
    browserEvents.wsFrames.push({
      captured_at: nowIso(),
      direction: 'open',
      url: socket.url(),
    });

    socket.on('framereceived', (event) => {
      browserEvents.wsFrames.push({
        captured_at: nowIso(),
        direction: 'received',
        url: socket.url(),
        payload: event.payload,
      });
    });

    socket.on('framesent', (event) => {
      browserEvents.wsFrames.push({
        captured_at: nowIso(),
        direction: 'sent',
        url: socket.url(),
        payload: event.payload,
      });
    });

    socket.on('close', () => {
      browserEvents.wsFrames.push({
        captured_at: nowIso(),
        direction: 'close',
        url: socket.url(),
      });
    });
  });

  const journey = {
    dashboardTitle: '',
    accessTokenIssued: false,
    sessionBeforeLogout: null,
    checks: {},
  };

  try {
    await page.goto(`${options.dashboardUrl}/login`, { waitUntil: 'domcontentloaded' });
    await page.evaluate((value) => {
      localStorage.clear();
      sessionStorage.clear();
      localStorage.setItem('ghost-gateway-url', value);
    }, options.gatewayUrl);
    await page.goto(`${options.dashboardUrl}/`, { waitUntil: 'networkidle' });
    await page.waitForURL((url) => url.pathname === '/login', { timeout: options.timeoutMs });
    journey.checks.dashboard_redirects_to_login = page.url().endsWith('/login');

    const loginResponsePromise = page.waitForResponse(
      (response) =>
        response.url() === `${options.gatewayUrl}/api/auth/login` &&
        response.request().method() === 'POST',
      { timeout: options.timeoutMs },
    );

    await page.locator('#token-input').fill(options.jwtSecret, { timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Login' }).click({ timeout: options.timeoutMs });

    const loginResponse = await loginResponsePromise;
    const loginPayload = await loginResponse.json();
    journey.accessTokenIssued =
      typeof loginPayload.access_token === 'string' && loginPayload.access_token.length > 20;
    journey.checks.login_request_succeeded = loginResponse.status() === 200;
    journey.checks.login_issues_access_token = journey.accessTokenIssued;

    await page.waitForURL((url) => url.pathname !== '/login', { timeout: options.timeoutMs });
    await page.locator('.page-title').first().waitFor({ state: 'visible', timeout: options.timeoutMs });
    await waitForConnectionState(page, 'Connected', options.timeoutMs);

    journey.dashboardTitle = await page.locator('.page-title').first().innerText();
    journey.checks.login_redirects_to_dashboard = page.url().endsWith('/') || page.url() === `${options.dashboardUrl}/`;
    journey.checks.dashboard_loaded_after_login = journey.dashboardTitle.trim() === 'Dashboard';
    journey.checks.dashboard_loaded_without_error = await page.locator('.error-state').count() === 0;
    journey.checks.ws_connected_after_login = true;
    journey.checks.gateway_ws_opened = browserEvents.wsFrames.some(
      (frame) =>
        typeof frame.url === 'string' &&
        frame.url.startsWith(options.gatewayUrl.replace(/^http/, 'ws')) &&
        frame.url.includes('/api/ws'),
    );

    journey.sessionBeforeLogout = await fetchJson(`${options.gatewayUrl}/api/auth/session`, {
      headers: {
        Authorization: `Bearer ${loginPayload.access_token}`,
      },
    });
    journey.checks.auth_session_authenticated = journey.sessionBeforeLogout.status === 200 &&
      journey.sessionBeforeLogout.body?.authenticated === true;
    journey.checks.auth_session_mode_jwt = journey.sessionBeforeLogout.body?.mode === 'jwt';
    journey.checks.auth_session_role_admin = journey.sessionBeforeLogout.body?.role === 'admin';

    await page.goto(`${options.dashboardUrl}/settings`, { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: 'Logout' }).click({ timeout: options.timeoutMs });
    await page.waitForURL((url) => url.pathname === '/login', { timeout: options.timeoutMs });
    journey.checks.logout_redirects_to_login = page.url().endsWith('/login');

    const revokedResponse = await fetchJson(`${options.gatewayUrl}/api/auth/session`, {
      headers: {
        Authorization: `Bearer ${loginPayload.access_token}`,
      },
    });
    journey.checks.revoked_access_token_rejected = revokedResponse.status === 401;

    return { browser, context, page, browserEvents, journey };
  } catch (error) {
    return { browser, context, page, browserEvents, journey, error };
  }
}

async function persistBrowserArtifacts(runDir, browserResult, keepArtifacts) {
  const screenshotPath = path.join(runDir, 'infra-page.png');
  const htmlPath = path.join(runDir, 'infra-page.html');
  const tracePath = path.join(runDir, 'playwright-trace.zip');

  await browserResult.page.screenshot({ path: screenshotPath, fullPage: true });
  await fs.writeFile(htmlPath, await browserResult.page.content());
  await browserResult.context.tracing.stop(keepArtifacts ? { path: tracePath } : undefined);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-infra-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');

  await fs.mkdir(tempDir, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const configPath = path.join(tempDir, 'ghost-live.yml');
  const dbPath = path.join(tempDir, 'ghost-live.db');

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    artifact_dir: runDir,
    checks: {},
    warnings: [],
    status: 'running',
  };

  let gatewayProcess = null;
  let dashboardProcess = null;
  let browserResult = null;

  try {
    const baseConfigPath = path.join(repoRoot, 'ghost.yml');
    const baseConfigText = await fs.readFile(baseConfigPath, 'utf8');
    await fs.writeFile(configPath, buildTempConfig(baseConfigText, gatewayPort, dbPath));

    const buildLogPath = path.join(runDir, 'gateway-build.log');
    const configValidateLogPath = path.join(runDir, 'config-validate.log');
    const migrateLogPath = path.join(runDir, 'gateway-migrate.log');
    const dbStatusLogPath = path.join(runDir, 'db-status.log');
    const dbVerifyLogPath = path.join(runDir, 'db-verify.log');
    const gatewayLogPath = path.join(runDir, 'gateway.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');
    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);

    const gatewayEnv = {
      ...process.env,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_agent_loop=info,ghost_llm=warn',
    };

    const configValidate = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--output', 'json', 'config', 'validate'],
      { cwd: repoRoot, env: gatewayEnv, logPath: configValidateLogPath },
    );
    const configValidateJson = JSON.parse(configValidate.stdout);
    await writeJson(path.join(runDir, 'config-validate.json'), configValidateJson);
    summary.checks.config_valid = configValidateJson.valid === true;

    await runLoggedCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'migrate'],
      { cwd: repoRoot, env: gatewayEnv, logPath: migrateLogPath },
    );
    summary.checks.gateway_db_migrated = true;

    const dbStatus = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--output', 'json', 'db', 'status'],
      { cwd: repoRoot, env: gatewayEnv, logPath: dbStatusLogPath },
    );
    const dbStatusJson = JSON.parse(dbStatus.stdout);
    await writeJson(path.join(runDir, 'db-status.json'), dbStatusJson);
    summary.checks.db_status_up_to_date = dbStatusJson.up_to_date === true;
    summary.checks.db_status_matches_latest = dbStatusJson.current_version === dbStatusJson.latest_version;

    const dbVerify = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'verify'],
      { cwd: repoRoot, env: gatewayEnv, logPath: dbVerifyLogPath },
    );
    summary.checks.db_verify_clean = dbVerify.stdout.includes('No breaks found');

    gatewayProcess = createGatewayProcess(
      gatewayBinary,
      repoRoot,
      configPath,
      gatewayEnv,
      gatewayLogPath,
    );

    await waitForHttp(`${gatewayUrl}/api/health`, {
      timeoutMs: 30_000,
      process: gatewayProcess.child,
      name: 'gateway',
    });
    await waitForHttp(`${gatewayUrl}/api/ready`, {
      timeoutMs: 30_000,
      process: gatewayProcess.child,
      name: 'gateway readiness',
    });

    const health = await fetchJson(`${gatewayUrl}/api/health`);
    const ready = await fetchJson(`${gatewayUrl}/api/ready`);
    const compatibility = await fetchJson(`${gatewayUrl}/api/compatibility`);
    await writeJson(path.join(runDir, 'health.json'), health.body);
    await writeJson(path.join(runDir, 'ready.json'), ready.body);
    await writeJson(path.join(runDir, 'compatibility.json'), compatibility.body);

    summary.checks.gateway_healthy = health.status === 200 && health.body?.status === 'alive';
    summary.checks.gateway_ready = ready.status === 200 && ready.body?.status === 'ready';
    summary.checks.compatibility_reports_dashboard = compatibility.status === 200 &&
      Array.isArray(compatibility.body?.supported_clients) &&
      compatibility.body.supported_clients.some((client) => client.client_name === 'dashboard');

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

    browserResult = await runBrowserInfraJourney({
      dashboardUrl,
      gatewayUrl,
      jwtSecret: DEFAULT_JWT_SECRET,
      timeoutMs: options.timeoutMs,
      headed: options.headed,
    });

    const keepBrowserArtifacts = options.keepArtifacts || Boolean(browserResult.error);
    await persistBrowserArtifacts(runDir, browserResult, keepBrowserArtifacts);
    await browserResult.browser.close();

    await writeJsonLines(path.join(runDir, 'browser-console.jsonl'), browserResult.browserEvents.console);
    await writeJsonLines(path.join(runDir, 'browser-page-errors.jsonl'), browserResult.browserEvents.pageErrors);
    await writeJsonLines(path.join(runDir, 'browser-requests.jsonl'), browserResult.browserEvents.requests);
    await writeJsonLines(path.join(runDir, 'browser-responses.jsonl'), browserResult.browserEvents.responses);
    await writeJsonLines(
      path.join(runDir, 'browser-request-failures.jsonl'),
      browserResult.browserEvents.requestFailures,
    );
    await writeJsonLines(path.join(runDir, 'browser-ws-frames.jsonl'), browserResult.browserEvents.wsFrames);

    if (browserResult.error) {
      throw browserResult.error;
    }

    summary.checks.page_errors = browserResult.browserEvents.pageErrors.length === 0;
    Object.assign(summary.checks, browserResult.journey.checks);

    if (browserResult.browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-console.jsonl.');
    }

    summary.dashboard_title = browserResult.journey.dashboardTitle;
    summary.auth_session = browserResult.journey.sessionBeforeLogout?.body ?? null;

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);

    if (failedChecks.length > 0) {
      throw new Error(`Live infra audit failed checks: ${failedChecks.join(', ')}`);
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);
  } finally {
    if (dashboardProcess) {
      await dashboardProcess.stop();
    }
    if (gatewayProcess) {
      await gatewayProcess.stop();
    }

    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await fs.rm(runDir, { recursive: true, force: true });
    process.stdout.write(`Live infra audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live infra audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
