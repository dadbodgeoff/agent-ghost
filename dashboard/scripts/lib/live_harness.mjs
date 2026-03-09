import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';

const IGNORED_BUILD_INPUT_DIRS = new Set([
  '.git',
  '.svelte-kit',
  'artifacts',
  'dashboard',
  'dist',
  'node_modules',
  'target',
]);

export function timestampLabel(date = new Date()) {
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

export function nowIso() {
  return new Date().toISOString();
}

export async function getFreePort() {
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

export function buildTempConfig(baseConfigText, gatewayPort, dbPath) {
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

export function createLoggedProcess(command, args, options) {
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

async function runLoggedInternal(command, args, options, captureOutput) {
  const logStream = createWriteStream(options.logPath, { flags: 'a' });
  const stdoutChunks = [];
  const stderrChunks = [];
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    if (captureOutput) {
      stdoutChunks.push(chunk.toString());
    }
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    if (captureOutput) {
      stderrChunks.push(chunk.toString());
    }
    logStream.write(chunk);
  });

  if (options.input != null) {
    child.stdin.end(options.input);
  } else {
    child.stdin.end();
  }

  const exitCode = await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });

  await new Promise((resolve) => logStream.end(resolve));

  const stdout = stdoutChunks.join('');
  const stderr = stderrChunks.join('');
  if (exitCode !== 0) {
    const output = captureOutput ? `\n${stdout}${stderr}` : '';
    throw new Error(`${command} ${args.join(' ')} exited with code ${exitCode}${output}`);
  }

  return { exitCode, stdout, stderr };
}

export async function runLoggedCommand(command, args, options) {
  await runLoggedInternal(command, args, options, false);
}

export async function runLoggedCaptureCommand(command, args, options) {
  return runLoggedInternal(command, args, options, true);
}

export async function waitForHttp(url, options = {}) {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const start = Date.now();

  while (Date.now() - start < timeoutMs) {
    if (options.process && options.process.exitCode !== null) {
      throw new Error(`${options.name ?? url} exited before becoming ready`);
    }

    try {
      const response = await fetch(url, { headers: options.headers });
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

export async function ensureGatewayBinary(repoRoot, logPath) {
  const binaryPath = path.join(
    repoRoot,
    'target',
    'debug',
    os.platform() === 'win32' ? 'ghost.exe' : 'ghost',
  );
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

export async function writeJson(filePath, value) {
  await fs.writeFile(filePath, JSON.stringify(value, null, 2));
}

export async function writeJsonLines(filePath, entries) {
  const body = entries.map((entry) => JSON.stringify(entry)).join('\n');
  await fs.writeFile(filePath, body ? `${body}\n` : '');
}

export async function fetchJson(url, options = {}) {
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
    headers: Object.fromEntries(response.headers.entries()),
  };
}

export function attachBrowserEventCapture(page) {
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

  return browserEvents;
}

function connectionIndicator(page) {
  return page.locator('.sidebar-footer-row .indicator .label').first();
}

export async function waitForConnectionState(page, expectedText, timeoutMs) {
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

export async function loginViaDashboard(page, options, browserEvents) {
  await page.goto(`${options.dashboardUrl}/login`, { waitUntil: 'domcontentloaded' });
  await page.evaluate((gatewayUrl) => {
    localStorage.clear();
    sessionStorage.clear();
    localStorage.setItem('ghost-gateway-url', gatewayUrl);
  }, options.gatewayUrl);

  await page.goto(`${options.dashboardUrl}/`, { waitUntil: 'networkidle' });
  await page.waitForURL((url) => url.pathname === '/login', { timeout: options.timeoutMs });

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

  await page.waitForURL((url) => url.pathname !== '/login', { timeout: options.timeoutMs });
  await page.locator('.page-title').first().waitFor({ state: 'visible', timeout: options.timeoutMs });
  await waitForConnectionState(page, 'Connected', options.timeoutMs);

  return {
    accessToken: loginPayload.access_token,
    checks: {
      login_request_succeeded: loginResponse.status() === 200,
      login_redirects_to_dashboard:
        page.url() === `${options.dashboardUrl}/` || page.url().endsWith('/'),
      ws_connected_after_login: true,
      gateway_ws_opened: browserEvents.wsFrames.some(
        (frame) =>
          typeof frame.url === 'string' &&
          frame.url.startsWith(options.gatewayUrl.replace(/^http/, 'ws')) &&
          frame.url.includes('/api/ws'),
      ),
    },
  };
}

export async function persistBrowserArtifacts(runDir, prefix, browserResult, keepArtifacts) {
  const screenshotPath = path.join(runDir, `${prefix}-page.png`);
  const htmlPath = path.join(runDir, `${prefix}-page.html`);
  const tracePath = path.join(runDir, `${prefix}-playwright-trace.zip`);

  await browserResult.page.screenshot({ path: screenshotPath, fullPage: true });
  await fs.writeFile(htmlPath, await browserResult.page.content());
  await browserResult.context.tracing.stop(keepArtifacts ? { path: tracePath } : undefined);
}
