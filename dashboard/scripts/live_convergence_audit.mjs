#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer as createHttpServer } from 'node:http';
import { createServer as createTcpServer } from 'node:net';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const DEFAULT_TIMEOUT_MS = 45_000;
const AGENT_NAME = 'ghost';
const UUID_DNS_NAMESPACE = '6ba7b812-9dad-11d1-80b4-00c04fd430c8';
const SIGNAL_NAMES = [
  'session_duration',
  'inter_session_gap',
  'response_latency',
  'vocabulary_convergence',
  'goal_boundary_erosion',
  'initiative_balance',
  'disengagement_resistance',
  'behavioral_anomaly',
];
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
  process.stdout.write(`Live convergence audit

Usage:
  pnpm audit:convergence-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                              [--timeout-ms 45000]

What it does:
  1. Builds the gateway binary if needed or stale
  2. Creates a fresh temp config, DB, HOME, and mock convergence monitor
  3. Seeds real convergence, memory, and ITP data for the default agent
  4. Boots the real gateway and dashboard
  5. Verifies REST, CLI, dashboard UI, websocket score updates, and chain integrity
  6. Preserves logs/traces/screenshots only when the run fails
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
    const server = createTcpServer();
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

function buildTempConfig(baseConfigText, gatewayPort, dbPath, monitorAddress) {
  const normalizedDbPath = dbPath.replace(/\\/g, '/');
  const lines = baseConfigText.split(/\r?\n/);
  const output = [];
  let inGateway = false;
  let inConvergence = false;
  let sawMonitorBlock = false;

  const appendMonitorBlock = () => {
    if (inConvergence && !sawMonitorBlock) {
      output.push('  monitor:');
      output.push('    enabled: true');
      output.push('    block_on_degraded: false');
      output.push('    stale_after_secs: 300');
      output.push(`    address: "${monitorAddress}"`);
      sawMonitorBlock = true;
    }
  };

  for (const line of lines) {
    if (isTopLevelYamlKey(line)) {
      appendMonitorBlock();
      inGateway = line.trim() === 'gateway:';
      inConvergence = line.trim() === 'convergence:';
      output.push(line);
      continue;
    }

    if (inGateway && /^\s{2}port:/.test(line)) {
      output.push(`  port: ${gatewayPort}`);
      continue;
    }

    if (inGateway && /^\s{2}db_path:/.test(line)) {
      output.push(`  db_path: "${normalizedDbPath}"`);
      continue;
    }

    if (inConvergence && /^\s{2}monitor:\s*$/.test(line)) {
      sawMonitorBlock = true;
      output.push(line);
      continue;
    }

    if (inConvergence && sawMonitorBlock && /^\s{4}enabled:/.test(line)) {
      output.push('    enabled: true');
      continue;
    }

    if (inConvergence && sawMonitorBlock && /^\s{4}block_on_degraded:/.test(line)) {
      output.push('    block_on_degraded: false');
      continue;
    }

    if (inConvergence && sawMonitorBlock && /^\s{4}stale_after_secs:/.test(line)) {
      output.push('    stale_after_secs: 300');
      continue;
    }

    if (inConvergence && sawMonitorBlock && /^\s{4}address:/.test(line)) {
      output.push(`    address: "${monitorAddress}"`);
      continue;
    }

    output.push(line);
  }

  appendMonitorBlock();
  return output.join('\n');
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

async function runLoggedCommand(command, args, options) {
  const logStream = createWriteStream(options.logPath, { flags: 'a' });
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    logStream.write(chunk);
  });

  if (options.input !== undefined) {
    child.stdin.end(options.input);
  } else {
    child.stdin.end();
  }

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
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    stdoutChunks.push(chunk.toString());
    logStream.write(chunk);
  });
  child.stderr.on('data', (chunk) => {
    stderrChunks.push(chunk.toString());
    logStream.write(chunk);
  });

  if (options.input !== undefined) {
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

function bytesFromUuid(uuid) {
  const hex = uuid.replace(/-/g, '');
  return Buffer.from(hex, 'hex');
}

function formatUuidFromBytes(bytes) {
  const hex = Buffer.from(bytes).toString('hex');
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join('-');
}

function uuidV5(name, namespace) {
  const namespaceBytes = bytesFromUuid(namespace);
  const nameBytes = Buffer.from(name, 'utf8');
  const sha1 = createHash('sha1').update(namespaceBytes).update(nameBytes).digest();
  sha1[6] = (sha1[6] & 0x0f) | 0x50;
  sha1[8] = (sha1[8] & 0x3f) | 0x80;
  return formatUuidFromBytes(sha1.subarray(0, 16));
}

function stableHex(label) {
  return createHash('sha256').update(label).digest('hex').toUpperCase();
}

function sqlString(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

async function runSqlite(dbPath, sql, repoRoot, env, logPath) {
  await runLoggedCommand('sqlite3', [dbPath], {
    cwd: repoRoot,
    env,
    logPath,
    input: `${sql}\n`,
  });
}

async function createMockMonitorServer(port, logPath) {
  const requests = [];
  const logStream = createWriteStream(logPath, { flags: 'a' });

  const server = createHttpServer((request, response) => {
    requests.push({
      captured_at: nowIso(),
      method: request.method,
      url: request.url,
    });
    logStream.write(`[${nowIso()}] ${request.method} ${request.url}\n`);

    if (request.url === '/health') {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ status: 'ok', version: 'live-audit-monitor' }));
      return;
    }

    response.writeHead(404, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ error: 'not found' }));
  });

  await new Promise((resolve, reject) => {
    server.listen(port, '127.0.0.1', () => resolve());
    server.on('error', reject);
  });

  return {
    requests,
    async stop() {
      await new Promise((resolve, reject) => {
        server.close((error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      });
      await new Promise((resolve) => logStream.end(resolve));
    },
  };
}

function buildSignalObject(values) {
  return Object.fromEntries(SIGNAL_NAMES.map((name, index) => [name, values[index] ?? 0]));
}

async function seedInitialState({
  repoRoot,
  env,
  dbPath,
  sqliteLogPath,
  stateDir,
  agentId,
  memoryId,
  sessionId,
}) {
  const zeroHash = '00'.repeat(32);
  const memoryEventHash1 = stableHex(`memory:${agentId}:1`);
  const memoryEventHash2 = stableHex(`memory:${agentId}:2`);
  const itpEventHash1 = stableHex(`itp:${agentId}:1`);
  const itpEventHash2 = stableHex(`itp:${agentId}:2`);
  const convergenceEventHash1 = stableHex(`convergence:${agentId}:1`);
  const initialSignals = [0.14, 0.09, 0.18, 0.22, 0.12, 0.17, 0.08, 0.05];

  const initialScore = {
    id: `score-${agentId}-1`,
    score: 0.42,
    level: 1,
    profile: 'standard',
    computedAt: '2026-03-09T02:10:00Z',
    signalScores: buildSignalObject(initialSignals),
  };

  const sql = `
BEGIN TRANSACTION;
INSERT INTO memory_events (
  memory_id, event_type, delta, actor_id, recorded_at, event_hash, previous_hash
) VALUES (
  ${sqlString(memoryId)},
  'MemoryCreated',
  ${sqlString('{"summary":"initial memory created","revision":1}')},
  ${sqlString(agentId)},
  '2026-03-09T02:00:00Z',
  x'${memoryEventHash1}',
  x'${zeroHash}'
);
INSERT INTO memory_events (
  memory_id, event_type, delta, actor_id, recorded_at, event_hash, previous_hash
) VALUES (
  ${sqlString(memoryId)},
  'MemoryPatched',
  ${sqlString('{"summary":"memory patched","revision":2}')},
  ${sqlString(agentId)},
  '2026-03-09T02:01:00Z',
  x'${memoryEventHash2}',
  x'${memoryEventHash1}'
);
INSERT INTO itp_events (
  id, session_id, event_type, sender, timestamp, sequence_number, content_hash,
  content_length, privacy_level, event_hash, previous_hash, attributes
) VALUES (
  ${sqlString(`itp-${agentId}-1`)},
  ${sqlString(sessionId)},
  'InteractionMessage',
  ${sqlString(agentId)},
  '2026-03-09T02:02:00Z',
  1,
  ${sqlString(stableHex(`itp-content:${agentId}:1`).toLowerCase())},
  64,
  'standard',
  x'${itpEventHash1}',
  x'${zeroHash}',
  '{}'
);
INSERT INTO itp_events (
  id, session_id, event_type, sender, timestamp, sequence_number, content_hash,
  content_length, privacy_level, event_hash, previous_hash, attributes
) VALUES (
  ${sqlString(`itp-${agentId}-2`)},
  ${sqlString(sessionId)},
  'ToolResult',
  ${sqlString(agentId)},
  '2026-03-09T02:03:00Z',
  2,
  ${sqlString(stableHex(`itp-content:${agentId}:2`).toLowerCase())},
  72,
  'standard',
  x'${itpEventHash2}',
  x'${itpEventHash1}',
  '{}'
);
INSERT INTO convergence_scores (
  id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash, previous_hash
) VALUES (
  ${sqlString(initialScore.id)},
  ${sqlString(agentId)},
  ${sqlString(sessionId)},
  ${initialScore.score},
  ${sqlString(JSON.stringify(initialScore.signalScores))},
  ${initialScore.level},
  ${sqlString(initialScore.profile)},
  ${sqlString(initialScore.computedAt)},
  x'${convergenceEventHash1}',
  x'${zeroHash}'
);
COMMIT;
`;

  await runSqlite(dbPath, sql, repoRoot, env, sqliteLogPath);

  await fs.mkdir(stateDir, { recursive: true });
  await fs.writeFile(
    path.join(stateDir, `${agentId}.json`),
    JSON.stringify(
      {
        agent_id: agentId,
        score: initialScore.score,
        level: initialScore.level,
        signal_scores: initialSignals,
        consecutive_normal: 0,
        cooldown_until: null,
        ack_required: false,
        updated_at: initialScore.computedAt,
      },
      null,
      2,
    ),
  );

  return initialScore;
}

async function insertUpdatedConvergenceScore({
  repoRoot,
  env,
  dbPath,
  sqliteLogPath,
  agentId,
  sessionId,
}) {
  const updatedSignals = [0.61, 0.48, 0.66, 0.72, 0.58, 0.69, 0.52, 0.77];
  const updatedScore = {
    id: `score-${agentId}-2`,
    score: 0.81,
    level: 3,
    profile: 'standard',
    computedAt: '2026-03-09T02:12:00Z',
    signalScores: buildSignalObject(updatedSignals),
    eventHash: stableHex(`convergence:${agentId}:2`),
    previousHash: stableHex(`convergence:${agentId}:1`),
  };

  const sql = `
INSERT INTO convergence_scores (
  id, agent_id, session_id, composite_score, signal_scores, level, profile, computed_at, event_hash, previous_hash
) VALUES (
  ${sqlString(updatedScore.id)},
  ${sqlString(agentId)},
  ${sqlString(sessionId)},
  ${updatedScore.score},
  ${sqlString(JSON.stringify(updatedScore.signalScores))},
  ${updatedScore.level},
  ${sqlString(updatedScore.profile)},
  ${sqlString(updatedScore.computedAt)},
  x'${updatedScore.eventHash}',
  x'${updatedScore.previousHash}'
);
`;

  await runSqlite(dbPath, sql, repoRoot, env, sqliteLogPath);
  return updatedScore;
}

async function waitForScoreValue(page, expectedText, timeoutMs) {
  await page.locator('.score-value').first().waitFor({ state: 'visible', timeout: timeoutMs });
  await page.waitForFunction(
    (expected) => {
      const node = document.querySelector('.score-value');
      return (node?.textContent ?? '').trim() === expected;
    },
    expectedText,
    { timeout: timeoutMs },
  );
}

async function runBrowserConvergenceJourney(options) {
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
    initialScoreText: null,
    finalScoreText: null,
    signalRowCount: 0,
    degradedBannerVisible: false,
    checks: {},
  };

  try {
    await page.goto(`${options.dashboardUrl}/`, { waitUntil: 'domcontentloaded' });
    await page.evaluate((gatewayUrl) => {
      localStorage.clear();
      sessionStorage.clear();
      localStorage.setItem('ghost-gateway-url', gatewayUrl);
    }, options.gatewayUrl);

    await page.goto(`${options.dashboardUrl}/convergence`, { waitUntil: 'networkidle' });
    await page.locator('.page-title').first().waitFor({ state: 'visible', timeout: options.timeoutMs });

    journey.checks.convergence_page_loaded = (await page.locator('.page-title').first().innerText()).trim() === 'Convergence';
    await waitForScoreValue(page, options.initialScoreText, options.timeoutMs);

    journey.initialScoreText = (await page.locator('.score-value').first().innerText()).trim();
    journey.signalRowCount = await page.locator('.signal-row').count();
    journey.degradedBannerVisible = (await page.locator('.degraded-banner').count()) > 0;
    journey.checks.initial_score_rendered = journey.initialScoreText === options.initialScoreText;
    journey.checks.signal_rows_rendered = journey.signalRowCount === SIGNAL_NAMES.length;
    journey.checks.monitor_online_banner_hidden = journey.degradedBannerVisible === false;

    await options.insertUpdatedScore();
    await waitForScoreValue(page, options.updatedScoreText, options.timeoutMs + 15_000);

    const levelBadgeText = await page.locator('.gauge .level-badge').first().innerText();
    journey.finalScoreText = (await page.locator('.score-value').first().innerText()).trim();
    journey.checks.updated_score_rendered = journey.finalScoreText === options.updatedScoreText;
    journey.checks.updated_level_rendered = levelBadgeText.includes(`L${options.updatedLevel}`);

    return { browser, context, page, browserEvents, journey };
  } catch (error) {
    return { browser, context, page, browserEvents, journey, error };
  }
}

async function persistBrowserArtifacts(runDir, browserResult, keepArtifacts) {
  const screenshotPath = path.join(runDir, 'convergence-page.png');
  const htmlPath = path.join(runDir, 'convergence-page.html');
  const tracePath = path.join(runDir, 'playwright-trace.zip');

  await browserResult.page.screenshot({ path: screenshotPath, fullPage: true });
  await fs.writeFile(htmlPath, await browserResult.page.content());
  await browserResult.context.tracing.stop(keepArtifacts ? { path: tracePath } : undefined);
}

async function removeIfPresent(targetPath) {
  if (!targetPath) {
    return;
  }
  await fs.rm(targetPath, { recursive: true, force: true });
}

function parseWsEnvelopes(frames) {
  return frames
    .map((frame) => frame.payload)
    .filter((payload) => typeof payload === 'string')
    .map((payload) => {
      try {
        return JSON.parse(payload);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runDir = path.join(repoRoot, 'artifacts', 'live-convergence-audits', timestampLabel());
  const tempDir = path.join(runDir, 'temp');
  const tempHome = path.join(tempDir, 'home');

  await fs.mkdir(tempDir, { recursive: true });
  await fs.mkdir(tempHome, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const monitorPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const monitorAddress = `127.0.0.1:${monitorPort}`;
  const configPath = path.join(tempDir, 'ghost-live.yml');
  const dbPath = path.join(tempDir, 'ghost-live.db');
  const stateDir = path.join(tempHome, '.ghost', 'data', 'convergence_state');
  const agentId = uuidV5(AGENT_NAME, UUID_DNS_NAMESPACE);
  const memoryId = `memory-${agentId.slice(0, 8)}`;
  const sessionId = `session-${agentId.slice(0, 8)}`;

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    monitor_address: monitorAddress,
    agent_id: agentId,
    memory_id: memoryId,
    session_id: sessionId,
    artifact_dir: runDir,
    checks: {},
    warnings: [],
    status: 'running',
  };

  let monitorServer = null;
  let gatewayProcess = null;
  let dashboardProcess = null;
  let browserResult = null;

  try {
    const baseConfigPath = path.join(repoRoot, 'ghost.yml');
    const baseConfigText = await fs.readFile(baseConfigPath, 'utf8');
    await fs.writeFile(configPath, buildTempConfig(baseConfigText, gatewayPort, dbPath, monitorAddress));

    const buildLogPath = path.join(runDir, 'gateway-build.log');
    const configValidateLogPath = path.join(runDir, 'config-validate.log');
    const migrateLogPath = path.join(runDir, 'gateway-migrate.log');
    const sqliteLogPath = path.join(runDir, 'sqlite-seed.log');
    const gatewayLogPath = path.join(runDir, 'gateway.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');
    const monitorLogPath = path.join(runDir, 'monitor.log');
    const cliScoresLogPath = path.join(runDir, 'cli-convergence-scores.log');
    const cliHistoryHttpLogPath = path.join(runDir, 'cli-convergence-history-http.log');
    const cliHistoryDirectLogPath = path.join(runDir, 'cli-convergence-history-direct.log');

    const childEnv = {
      ...process.env,
      HOME: tempHome,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_agent_loop=info,ghost_llm=warn',
    };

    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);

    const configValidate = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--output', 'json', 'config', 'validate'],
      { cwd: repoRoot, env: childEnv, logPath: configValidateLogPath },
    );
    const configValidateJson = JSON.parse(configValidate.stdout);
    await writeJson(path.join(runDir, 'config-validate.json'), configValidateJson);
    summary.checks.config_valid = configValidateJson.valid === true;

    await runLoggedCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'migrate'],
      { cwd: repoRoot, env: childEnv, logPath: migrateLogPath },
    );
    summary.checks.gateway_db_migrated = true;

    const initialScore = await seedInitialState({
      repoRoot,
      env: childEnv,
      dbPath,
      sqliteLogPath,
      stateDir,
      agentId,
      memoryId,
      sessionId,
    });

    monitorServer = await createMockMonitorServer(monitorPort, monitorLogPath);

    gatewayProcess = createLoggedProcess(
      gatewayBinary,
      ['-c', configPath, 'serve'],
      { cwd: repoRoot, env: childEnv, logPath: gatewayLogPath },
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
    const convergenceScoresInitial = await fetchJson(`${gatewayUrl}/api/convergence/scores`);
    const crdtState = await fetchJson(
      `${gatewayUrl}/api/state/crdt/${encodeURIComponent(agentId)}?memory_id=${encodeURIComponent(memoryId)}&limit=20`,
    );
    const integrity = await fetchJson(
      `${gatewayUrl}/api/integrity/chain/${encodeURIComponent(agentId)}?chain=both`,
    );
    const itpEvents = await fetchJson(`${gatewayUrl}/api/itp/events?limit=10`);

    await writeJson(path.join(runDir, 'health.json'), health.body);
    await writeJson(path.join(runDir, 'ready.json'), ready.body);
    await writeJson(path.join(runDir, 'convergence-scores-initial.json'), convergenceScoresInitial.body);
    await writeJson(path.join(runDir, 'crdt-state.json'), crdtState.body);
    await writeJson(path.join(runDir, 'integrity.json'), integrity.body);
    await writeJson(path.join(runDir, 'itp-events.json'), itpEvents.body);

    const initialScoreRow = convergenceScoresInitial.body?.scores?.find((score) => score.agent_id === agentId);
    summary.checks.gateway_healthy = health.status === 200 && health.body?.status === 'alive';
    summary.checks.gateway_ready = ready.status === 200 && ready.body?.status === 'ready';
    summary.checks.monitor_connected = health.body?.convergence_monitor?.connected === true;
    summary.checks.convergence_protection_healthy_agent =
      health.body?.convergence_protection?.agents?.healthy === 1;
    summary.checks.rest_initial_score_seeded =
      initialScoreRow?.score === initialScore.score &&
      initialScoreRow?.level === initialScore.level &&
      initialScoreRow?.profile === initialScore.profile;
    summary.checks.crdt_chain_valid = crdtState.status === 200 && crdtState.body?.chain_valid === true;
    summary.checks.crdt_contains_two_seeded_deltas = crdtState.body?.deltas?.length === 2;
    summary.checks.integrity_memory_valid =
      integrity.status === 200 && integrity.body?.chains?.memory_events?.is_valid === true;
    summary.checks.integrity_itp_valid =
      integrity.status === 200 && integrity.body?.chains?.itp_events?.is_valid === true;
    summary.checks.itp_events_seeded =
      itpEvents.status === 200 &&
      itpEvents.body?.extension_connected === true &&
      Array.isArray(itpEvents.body?.events) &&
      itpEvents.body.events.some((event) => event.session_id === sessionId);

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

    await waitForHttp(`${dashboardUrl}/convergence`, {
      timeoutMs: 45_000,
      process: dashboardProcess.child,
      name: 'dashboard',
    });
    summary.checks.dashboard_ready = true;

    let updatedScore = null;
    browserResult = await runBrowserConvergenceJourney({
      dashboardUrl,
      gatewayUrl,
      headed: options.headed,
      timeoutMs: options.timeoutMs,
      initialScoreText: initialScore.score.toFixed(2),
      updatedScoreText: '0.81',
      updatedLevel: 3,
      insertUpdatedScore: async () => {
        updatedScore = await insertUpdatedConvergenceScore({
          repoRoot,
          env: childEnv,
          dbPath,
          sqliteLogPath,
          agentId,
          sessionId,
        });
      },
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

    const convergenceScoresUpdated = await fetchJson(`${gatewayUrl}/api/convergence/scores`);
    const convergenceHistoryUpdated = await fetchJson(
      `${gatewayUrl}/api/convergence/history/${encodeURIComponent(agentId)}?limit=24`,
    );
    const updatedScoreRow = convergenceScoresUpdated.body?.scores?.find((score) => score.agent_id === agentId);
    const cliScores = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--gateway-url', gatewayUrl, '--output', 'json', 'convergence', 'scores'],
      { cwd: repoRoot, env: childEnv, logPath: cliScoresLogPath },
    );
    const cliScoresJson = JSON.parse(cliScores.stdout);
    await writeJson(path.join(runDir, 'cli-convergence-scores.json'), cliScoresJson);

    const cliHistoryHttp = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--gateway-url', gatewayUrl, '--output', 'json', 'convergence', 'history', agentId],
      { cwd: repoRoot, env: childEnv, logPath: cliHistoryHttpLogPath },
    );
    const cliHistoryHttpJson = JSON.parse(cliHistoryHttp.stdout);
    await writeJson(path.join(runDir, 'cli-convergence-history-http.json'), cliHistoryHttpJson);

    const cliHistoryDirect = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, '--gateway-url', 'http://127.0.0.1:1', '--output', 'json', 'convergence', 'history', agentId],
      { cwd: repoRoot, env: childEnv, logPath: cliHistoryDirectLogPath },
    );
    const cliHistoryDirectJson = JSON.parse(cliHistoryDirect.stdout);
    await writeJson(path.join(runDir, 'cli-convergence-history-direct.json'), cliHistoryDirectJson);
    await writeJson(path.join(runDir, 'convergence-scores-updated.json'), convergenceScoresUpdated.body);
    await writeJson(path.join(runDir, 'convergence-history-updated.json'), convergenceHistoryUpdated.body);

    const cliScoreRow = cliScoresJson?.scores?.find((score) => score.agent_id === agentId);
    const wsEnvelopes = parseWsEnvelopes(browserResult.browserEvents.wsFrames);
    const scoreUpdateEnvelope = wsEnvelopes.find(
      (envelope) =>
        envelope?.event?.type === 'ScoreUpdate' &&
        envelope.event.agent_id === agentId &&
        envelope.event.level === updatedScore?.level,
    );
    const interventionEnvelope = wsEnvelopes.find(
      (envelope) =>
        envelope?.event?.type === 'InterventionChange' &&
        envelope.event.agent_id === agentId &&
        envelope.event.new_level === updatedScore?.level,
    );

    Object.assign(summary.checks, browserResult.journey.checks);
    summary.checks.page_errors = browserResult.browserEvents.pageErrors.length === 0;
    summary.checks.gateway_ws_opened = browserResult.browserEvents.wsFrames.some(
      (frame) =>
        typeof frame.url === 'string' &&
        frame.url.startsWith(gatewayUrl.replace(/^http/, 'ws')) &&
        frame.url.includes('/api/ws'),
    );
    summary.checks.ws_score_update_received = Boolean(scoreUpdateEnvelope);
    summary.checks.ws_score_update_includes_signals =
      Array.isArray(scoreUpdateEnvelope?.event?.signals) &&
      scoreUpdateEnvelope.event.signals.length === SIGNAL_NAMES.length;
    summary.checks.ws_intervention_change_received = Boolean(interventionEnvelope);
    summary.checks.rest_updated_score_visible =
      updatedScoreRow?.score === updatedScore?.score &&
      updatedScoreRow?.level === updatedScore?.level &&
      updatedScoreRow?.computed_at === updatedScore?.computedAt;
    summary.checks.rest_history_contains_two_entries =
      convergenceHistoryUpdated.status === 200 &&
      convergenceHistoryUpdated.body?.agent_id === agentId &&
      Array.isArray(convergenceHistoryUpdated.body?.entries) &&
      convergenceHistoryUpdated.body.entries.length === 2;
    summary.checks.cli_http_scores_matches_updated_score =
      cliScoreRow?.score === updatedScore?.score && cliScoreRow?.level === updatedScore?.level;
    summary.checks.cli_http_history_contains_two_entries =
      cliHistoryHttpJson?.agent_id === agentId &&
      Array.isArray(cliHistoryHttpJson?.entries) &&
      cliHistoryHttpJson.entries.length === 2;
    summary.checks.cli_direct_history_contains_two_entries =
      cliHistoryDirectJson?.agent_id === agentId &&
      Array.isArray(cliHistoryDirectJson?.entries) &&
      cliHistoryDirectJson.entries.length === 2;

    summary.initial_score = initialScore;
    summary.updated_score = updatedScore;
    summary.health = health.body ?? null;
    summary.crdt = crdtState.body ?? null;
    summary.integrity = integrity.body ?? null;
    summary.itp = itpEvents.body ?? null;
    summary.monitor_requests = monitorServer.requests;

    if (browserResult.browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-console.jsonl.');
    }

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);
    if (failedChecks.length > 0) {
      throw new Error(`Live convergence audit failed checks: ${failedChecks.join(', ')}`);
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
    if (monitorServer) {
      await monitorServer.stop();
    }

    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await removeIfPresent(runDir);
    process.stdout.write(`Live convergence audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live convergence audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
