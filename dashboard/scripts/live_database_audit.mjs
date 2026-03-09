#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JWT_SECRET = 'ghost-db-live-jwt-secret';
const DEFAULT_BACKUP_PASSPHRASE = 'ghost-db-live-backup-passphrase';
const DEFAULT_PROMPT =
  'Use the read_file tool on README.md and answer with the project name only. Do not answer from memory.';
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
  process.stdout.write(`Live database audit

Usage:
  pnpm audit:database-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                           [--timeout-ms 45000]

What it does:
  1. Boots an auth-enabled gateway on a fresh temp GHOST dir
  2. Creates real Studio data through the dashboard
  3. Creates a backup through the real backups UI
  4. Verifies a populated DB, restores the backup into a fresh target, and compacts the source DB
  5. Restarts the gateway and verifies persisted backup/session state
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

async function runLoggedCommand(command, args, options) {
  const result = await runLoggedCaptureCommand(command, args, options);
  return result;
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

async function runLoggedMaybeFailCommand(command, args, options) {
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

  return {
    exitCode,
    stdout: stdoutChunks.join(''),
    stderr: stderrChunks.join(''),
  };
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

function stableHex(label) {
  return createHash('sha256').update(label).digest('hex').toUpperCase();
}

function sha256Hex(value) {
  return createHash('sha256').update(value).digest('hex').toUpperCase();
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

async function ensureBlake3HexHelper(repoRoot, helperDir, logPath) {
  const helperPath = path.join(helperDir, 'blake3_hex_helper');
  const sourcePath = path.join(helperDir, 'blake3_hex_helper.rs');

  if (await fs.stat(helperPath).then(() => true).catch(() => false)) {
    return helperPath;
  }

  const depsDir = path.join(repoRoot, 'target', 'debug', 'deps');
  const blake3Rlib = (await fs.readdir(depsDir))
    .filter((entry) => /^libblake3-.*\.rlib$/.test(entry))
    .sort()[0];

  if (!blake3Rlib) {
    throw new Error('Could not locate a compiled blake3 rlib in target/debug/deps');
  }

  const helperSource = `use std::env;

fn decode_hex(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "hex input must have even length");
    let mut out = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16).expect("invalid hex");
        let lo = (pair[1] as char).to_digit(16).expect("invalid hex");
        out.push(((hi << 4) | lo) as u8);
    }
    out
}

fn main() {
    let hex = env::args().nth(1).expect("missing hex arg");
    let bytes = decode_hex(&hex);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&bytes);
    for byte in hasher.finalize().as_bytes() {
        print!("{:02X}", byte);
    }
    println!();
}
`;

  await fs.writeFile(sourcePath, helperSource);
  await runLoggedCommand(
    'rustc',
    [
      sourcePath,
      '--edition=2021',
      '-L',
      `dependency=${depsDir}`,
      '--extern',
      `blake3=${path.join(depsDir, blake3Rlib)}`,
      '-o',
      helperPath,
    ],
    {
      cwd: repoRoot,
      env: process.env,
      logPath,
    },
  );

  return helperPath;
}

async function computeBlake3Hex(helperPath, inputHex, repoRoot, env, logPath) {
  const result = await runLoggedCaptureCommand(helperPath, [inputHex.toUpperCase()], {
    cwd: repoRoot,
    env,
    logPath,
  });
  return result.stdout.trim().toUpperCase();
}

async function captureDbCounts({ dbPath, repoRoot, env, logPath }) {
  const query = [
    "SELECT",
    "(SELECT COUNT(*) FROM itp_events),",
    "(SELECT COUNT(*) FROM memory_events),",
    "(SELECT COUNT(*) FROM memory_snapshots),",
    "(SELECT COUNT(*) FROM compaction_runs),",
    "(SELECT COUNT(*) FROM compaction_event_ranges);",
  ].join(' ');
  const result = await runLoggedCaptureCommand('sqlite3', [dbPath, query], {
    cwd: repoRoot,
    env,
    logPath,
  });
  const [itpEvents, memoryEvents, memorySnapshots, compactionRuns, compactionRanges] = result.stdout
    .trim()
    .split('|')
    .map((value) => Number.parseInt(value, 10) || 0);

  return {
    itp_events: itpEvents,
    memory_events: memoryEvents,
    memory_snapshots: memorySnapshots,
    compaction_runs: compactionRuns,
    compaction_event_ranges: compactionRanges,
  };
}

async function seedItpVerificationChain({
  repoRoot,
  env,
  dbPath,
  sqliteLogPath,
  blake3HelperPath,
  blake3LogPath,
  agentId,
  sessionId,
}) {
  const zeroHash = '00'.repeat(32);
  const events = [
    {
      id: `${sessionId}-0`,
      eventType: 'InteractionMessage',
      timestamp: '2026-03-09T01:00:00Z',
      content: 'database audit seeded event 0',
    },
    {
      id: `${sessionId}-1`,
      eventType: 'ToolCall',
      timestamp: '2026-03-09T01:00:05Z',
      content: 'database audit seeded event 1',
    },
    {
      id: `${sessionId}-2`,
      eventType: 'ToolResult',
      timestamp: '2026-03-09T01:00:10Z',
      content: 'database audit seeded event 2',
    },
  ];

  let previousHash = zeroHash;
  const statements = ['BEGIN TRANSACTION;'];

  for (let index = 0; index < events.length; index += 1) {
    const event = events[index];
    const contentHash = sha256Hex(event.content);
    const eventHash = await computeBlake3Hex(
      blake3HelperPath,
      `${contentHash}${previousHash}`,
      repoRoot,
      env,
      blake3LogPath,
    );

    statements.push(`
INSERT INTO itp_events (
  id, session_id, event_type, sender, timestamp, sequence_number, content_hash,
  content_length, privacy_level, event_hash, previous_hash, attributes
) VALUES (
  ${sqlString(event.id)},
  ${sqlString(sessionId)},
  ${sqlString(event.eventType)},
  ${sqlString(agentId)},
  ${sqlString(event.timestamp)},
  ${index},
  ${sqlString(contentHash.toLowerCase())},
  ${event.content.length},
  'standard',
  x'${eventHash}',
  x'${previousHash}',
  ${sqlString(JSON.stringify({ seeded_by: 'live_database_audit', content: event.content }))}
);`);

    previousHash = eventHash;
  }

  statements.push('COMMIT;');
  await runSqlite(dbPath, statements.join('\n'), repoRoot, env, sqliteLogPath);

  return {
    session_id: sessionId,
    event_count: events.length,
  };
}

async function seedCompactionCandidate({
  repoRoot,
  env,
  dbPath,
  sqliteLogPath,
  agentId,
  memoryId,
}) {
  const zeroHash = '00'.repeat(32);
  const statements = ['BEGIN TRANSACTION;'];
  let previousHash = zeroHash;

  for (let index = 1; index <= 60; index += 1) {
    const eventHash = stableHex(`db-audit-memory:${memoryId}:${index}`);
    const recordedAt = `2026-03-09T02:${String(Math.floor(index / 2)).padStart(2, '0')}:${String((index % 2) * 30).padStart(2, '0')}Z`;
    statements.push(`
INSERT INTO memory_events (
  memory_id, event_type, delta, actor_id, recorded_at, event_hash, previous_hash
) VALUES (
  ${sqlString(memoryId)},
  'MemoryPatched',
  ${sqlString(JSON.stringify({ revision: index, note: `compaction-${index}` }))},
  ${sqlString(agentId)},
  ${sqlString(recordedAt)},
  x'${eventHash}',
  x'${previousHash}'
);`);
    previousHash = eventHash;
  }

  statements.push('COMMIT;');
  await runSqlite(dbPath, statements.join('\n'), repoRoot, env, sqliteLogPath);
}

function parseVerifyOutput(stdout) {
  const countMatch = stdout.match(/Hash chain verification \([^)]+\):\s+(\d+)\s+events checked/i);
  return {
    checkedEvents: countMatch ? Number.parseInt(countMatch[1], 10) : 0,
    clean: stdout.includes('No breaks found'),
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

async function loginViaDashboard(page, options, browserEvents) {
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
      login_redirects_to_dashboard: page.url() === `${options.dashboardUrl}/` || page.url().endsWith('/'),
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

async function openFreshStudio(page, dashboardUrl, timeoutMs) {
  await page.goto(`${dashboardUrl}/studio`, { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: '+ New' }).click({ timeout: timeoutMs });
  await page.getByRole('textbox', { name: 'Message input' }).waitFor({
    state: 'visible',
    timeout: timeoutMs,
  });
}

async function sendStudioMessage(page, prompt, timeoutMs, expectToolActivity) {
  const input = page.getByRole('textbox', { name: 'Message input' });
  const assistantCountBefore = await page.locator('.assistant-text').count();

  await input.click({ timeout: timeoutMs });
  await page.keyboard.type(prompt);
  await page.getByRole('button', { name: 'Send' }).click({ timeout: timeoutMs });

  if (expectToolActivity) {
    await page.waitForSelector('.tool-call-entry, .tool-indicator', { timeout: timeoutMs });
  }

  await page.waitForFunction(
    (count) => document.querySelectorAll('.assistant-text').length > count,
    assistantCountBefore,
    { timeout: timeoutMs },
  );
  await page.waitForFunction(
    (count) => {
      const nodes = Array.from(document.querySelectorAll('.assistant-text'));
      if (nodes.length <= count) return false;
      return (nodes[nodes.length - 1].textContent ?? '').trim().length > 0;
    },
    assistantCountBefore,
    { timeout: timeoutMs },
  );
  await page.waitForFunction(
    () => document.querySelector('.btn-stop') === null,
    undefined,
    { timeout: timeoutMs },
  );

  return {
    assistantText: await page.locator('.assistant-text').last().innerText(),
  };
}

async function collectStudioState(gatewayUrl, token) {
  const headers = {
    Authorization: `Bearer ${token}`,
  };
  const sessionsResponse = await fetch(`${gatewayUrl}/api/studio/sessions?limit=10`, { headers });
  const sessions = await sessionsResponse.json();
  const latestSession = sessions.sessions?.[0] ?? null;

  let sessionDetail = null;
  let recover = null;
  let latestAssistantMessage = null;

  if (latestSession?.id) {
    const detailResponse = await fetch(
      `${gatewayUrl}/api/studio/sessions/${encodeURIComponent(latestSession.id)}`,
      { headers },
    );
    sessionDetail = await detailResponse.json();
    latestAssistantMessage =
      sessionDetail.messages?.filter((message) => message.role === 'assistant').at(-1) ?? null;

    if (latestAssistantMessage?.id) {
      const recoverResponse = await fetch(
        `${gatewayUrl}/api/studio/sessions/${encodeURIComponent(latestSession.id)}/stream/recover?message_id=${encodeURIComponent(latestAssistantMessage.id)}&after_seq=0`,
        { headers },
      );
      recover = await recoverResponse.json();
    }
  }

  return {
    sessions,
    latestSession,
    sessionDetail,
    latestAssistantMessage,
    recover,
  };
}

function createBrowserEvents() {
  return {
    console: [],
    pageErrors: [],
    requests: [],
    responses: [],
    requestFailures: [],
    wsFrames: [],
  };
}

function attachBrowserEventCapture(page, browserEvents) {
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
}

async function runPhaseOneBrowserJourney(options) {
  const browser = await chromium.launch({ headless: !options.headed });
  const context = await browser.newContext();
  await context.tracing.start({ screenshots: true, snapshots: true });
  const page = await context.newPage();
  const browserEvents = createBrowserEvents();
  attachBrowserEventCapture(page, browserEvents);

  const journey = {
    accessToken: null,
    studioAssistantText: null,
    backupResponse: null,
    checks: {},
  };

  try {
    const login = await loginViaDashboard(page, options, browserEvents);
    journey.accessToken = login.accessToken;
    Object.assign(journey.checks, login.checks);

    await openFreshStudio(page, options.dashboardUrl, options.timeoutMs);
    const studioResult = await sendStudioMessage(page, DEFAULT_PROMPT, options.timeoutMs, true);
    journey.studioAssistantText = studioResult.assistantText;
    journey.checks.studio_tool_session_created = /ghost/i.test(studioResult.assistantText);

    await options.onStudioDataReady(journey.accessToken);

    await page.goto(`${options.dashboardUrl}/settings/backups`, { waitUntil: 'networkidle' });
    await page.locator('h1').waitFor({ state: 'visible', timeout: options.timeoutMs });
    const rowsBefore = await page.locator('.data-table tbody tr').count();
    const backupResponsePromise = page.waitForResponse(
      (response) =>
        response.url() === `${options.gatewayUrl}/api/admin/backup` &&
        response.request().method() === 'POST',
      { timeout: options.timeoutMs },
    );

    await page.getByRole('button', { name: 'Backup Now' }).click({ timeout: options.timeoutMs });
    const backupResponse = await backupResponsePromise;
    const backupPayload = await backupResponse.json();
    journey.backupResponse = backupPayload;

    await page.waitForFunction(
      (count) => document.querySelectorAll('.data-table tbody tr').length > count,
      rowsBefore,
      { timeout: options.timeoutMs },
    );
    await page.waitForFunction(
      (idPrefix) => (document.body.textContent ?? '').includes(idPrefix),
      backupPayload.backup_id.slice(0, 8),
      { timeout: options.timeoutMs },
    );

    journey.checks.backup_created_via_ui = backupResponse.status() === 200;
    journey.checks.backup_row_rendered = true;
    journey.checks.ws_backup_complete_received = browserEvents.wsFrames.some(
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"BackupComplete"') &&
        frame.payload.includes(backupPayload.backup_id),
    );

    return { browser, context, page, browserEvents, journey };
  } catch (error) {
    return { browser, context, page, browserEvents, journey, error };
  }
}

async function runPhaseTwoBrowserJourney(options) {
  const browser = await chromium.launch({ headless: !options.headed });
  const context = await browser.newContext();
  await context.tracing.start({ screenshots: true, snapshots: true });
  const page = await context.newPage();
  const browserEvents = createBrowserEvents();
  attachBrowserEventCapture(page, browserEvents);

  const journey = {
    checks: {},
    restoredAssistantText: null,
  };

  try {
    const login = await loginViaDashboard(page, options, browserEvents);
    Object.assign(journey.checks, login.checks);

    await page.goto(`${options.dashboardUrl}/settings/backups`, { waitUntil: 'networkidle' });
    await page.waitForFunction(
      (idPrefix) => (document.body.textContent ?? '').includes(idPrefix),
      options.backupId.slice(0, 8),
      { timeout: options.timeoutMs },
    );
    journey.checks.backup_persisted_after_restart = true;

    await page.goto(`${options.dashboardUrl}/studio`, { waitUntil: 'networkidle' });
    const firstSession = page.locator('.session-list .session-btn').first();
    await firstSession.waitFor({ state: 'visible', timeout: options.timeoutMs });
    await firstSession.click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      (needle) => {
        const nodes = Array.from(document.querySelectorAll('.assistant-text'));
        if (nodes.length === 0) return false;
        return (nodes[nodes.length - 1].textContent ?? '').includes(needle);
      },
      options.expectedAssistantNeedle,
      { timeout: options.timeoutMs },
    );
    journey.restoredAssistantText = await page.locator('.assistant-text').last().innerText();
    journey.checks.studio_session_persisted_after_restart =
      journey.restoredAssistantText.includes(options.expectedAssistantNeedle);

    return { browser, context, page, browserEvents, journey };
  } catch (error) {
    return { browser, context, page, browserEvents, journey, error };
  }
}

async function persistBrowserArtifacts(runDir, prefix, browserResult, keepArtifacts) {
  const screenshotPath = path.join(runDir, `${prefix}-page.png`);
  const htmlPath = path.join(runDir, `${prefix}-page.html`);
  const tracePath = path.join(runDir, `${prefix}-trace.zip`);

  await browserResult.page.screenshot({ path: screenshotPath, fullPage: true });
  await fs.writeFile(htmlPath, await browserResult.page.content());
  await browserResult.context.tracing.stop(keepArtifacts ? { path: tracePath } : undefined);

  await writeJsonLines(path.join(runDir, `${prefix}-browser-console.jsonl`), browserResult.browserEvents.console);
  await writeJsonLines(path.join(runDir, `${prefix}-browser-page-errors.jsonl`), browserResult.browserEvents.pageErrors);
  await writeJsonLines(path.join(runDir, `${prefix}-browser-requests.jsonl`), browserResult.browserEvents.requests);
  await writeJsonLines(path.join(runDir, `${prefix}-browser-responses.jsonl`), browserResult.browserEvents.responses);
  await writeJsonLines(
    path.join(runDir, `${prefix}-browser-request-failures.jsonl`),
    browserResult.browserEvents.requestFailures,
  );
  await writeJsonLines(path.join(runDir, `${prefix}-browser-ws-frames.jsonl`), browserResult.browserEvents.wsFrames);
}

async function removeIfPresent(targetPath) {
  if (!targetPath) {
    return;
  }
  await fs.rm(targetPath, { recursive: true, force: true });
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runDir = path.join(repoRoot, 'artifacts', 'live-database-audits', timestampLabel());
  const tempDir = path.join(runDir, 'temp');
  const tempHome = path.join(tempDir, 'home');
  const ghostDir = path.join(tempHome, '.ghost');
  const backupDir = path.join(ghostDir, 'backups');
  const dataDir = path.join(ghostDir, 'data');
  const configDir = path.join(ghostDir, 'config');
  const agentsDir = path.join(ghostDir, 'agents');

  await fs.mkdir(runDir, { recursive: true });
  await fs.mkdir(dataDir, { recursive: true });
  await fs.mkdir(configDir, { recursive: true });
  await fs.mkdir(agentsDir, { recursive: true });
  await fs.mkdir(backupDir, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const dbPath = path.join(dataDir, 'ghost.db');
  const configPath = path.join(tempDir, 'ghost-live.yml');
  const restoredGhostDir = path.join(tempDir, 'ghost-restored');
  const restoredConfigPath = path.join(tempDir, 'ghost-restored.yml');
  const seededMemoryId = 'db-audit-memory';
  const seededItpSessionId = 'db-audit-itp-session';
  const agentId = '1d5ecfab-ec27-509b-8d30-179beea17f98';

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    ghost_dir: ghostDir,
    backup_dir: backupDir,
    artifact_dir: runDir,
    checks: {},
    warnings: [],
    status: 'running',
  };

  let gatewayProcess = null;
  let dashboardProcess = null;
  let phaseOneResult = null;
  let phaseTwoResult = null;

  const buildLogPath = path.join(runDir, 'gateway-build.log');
  const configValidateLogPath = path.join(runDir, 'config-validate.log');
  const migrateLogPath = path.join(runDir, 'db-migrate.log');
  const blake3HelperBuildLogPath = path.join(runDir, 'blake3-helper-build.log');
  const blake3HelperRunLogPath = path.join(runDir, 'blake3-helper.log');
  const sqliteLogPath = path.join(runDir, 'sqlite-seed.log');
  const originalCountsLogPath = path.join(runDir, 'db-counts-original.log');
  const originalDbStatusLogPath = path.join(runDir, 'db-status-original.log');
  const dbVerifyLogPath = path.join(runDir, 'db-verify.log');
  const dbCompactRunningLogPath = path.join(runDir, 'db-compact-running.log');
  const dbCompactDryRunLogPath = path.join(runDir, 'db-compact-dry-run.log');
  const restoreLogPath = path.join(runDir, 'restore.log');
  const restoredCountsLogPath = path.join(runDir, 'db-counts-restored.log');
  const restoredDbStatusLogPath = path.join(runDir, 'db-status-restored.log');
  const restoredDbVerifyLogPath = path.join(runDir, 'db-verify-restored.log');
  const dbCompactLogPath = path.join(runDir, 'db-compact.log');
  const compactionQueryLogPath = path.join(runDir, 'compaction-query.log');
  const gatewayLogPath = path.join(runDir, 'gateway.log');
  const dashboardLogPath = path.join(runDir, 'dashboard.log');

  try {
    const baseConfigPath = path.join(repoRoot, 'ghost.yml');
    const baseConfigText = await fs.readFile(baseConfigPath, 'utf8');
    const tempConfigText = buildTempConfig(baseConfigText, gatewayPort, dbPath);
    await fs.writeFile(configPath, tempConfigText);
    await fs.writeFile(path.join(configDir, 'ghost.yml'), tempConfigText);

    const childEnv = {
      ...process.env,
      HOME: tempHome,
      GHOST_DIR: ghostDir,
      GHOST_BACKUP_DIR: backupDir,
      GHOST_BACKUP_PASSPHRASE: DEFAULT_BACKUP_PASSPHRASE,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_agent_loop=info,ghost_llm=warn',
    };

    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);
    const blake3HelperPath = await ensureBlake3HexHelper(repoRoot, tempDir, blake3HelperBuildLogPath);

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

    gatewayProcess = createLoggedProcess(
      gatewayBinary,
      ['-c', configPath, 'serve'],
      { cwd: repoRoot, env: childEnv, logPath: gatewayLogPath },
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

    await waitForHttp(`${gatewayUrl}/api/health`, {
      timeoutMs: 30_000,
      process: gatewayProcess.child,
      name: 'gateway',
    });
    await waitForHttp(`${dashboardUrl}/login`, {
      timeoutMs: 45_000,
      process: dashboardProcess.child,
      name: 'dashboard',
    });

    const health = await fetchJson(`${gatewayUrl}/api/health`);
    await writeJson(path.join(runDir, 'health.json'), health.body);
    summary.checks.gateway_healthy = health.status === 200 && health.body?.status === 'alive';
    summary.checks.dashboard_ready = true;

    let originalDbCounts = null;
    let originalDbStatusJson = null;
    let seededItpChain = null;
    phaseOneResult = await runPhaseOneBrowserJourney({
      dashboardUrl,
      gatewayUrl,
      jwtSecret: DEFAULT_JWT_SECRET,
      timeoutMs: options.timeoutMs,
      headed: options.headed,
      onStudioDataReady: async (accessToken) => {
        seededItpChain = await seedItpVerificationChain({
          repoRoot,
          env: childEnv,
          dbPath,
          sqliteLogPath,
          blake3HelperPath,
          blake3LogPath: blake3HelperRunLogPath,
          agentId,
          sessionId: seededItpSessionId,
        });

        await seedCompactionCandidate({
          repoRoot,
          env: childEnv,
          dbPath,
          sqliteLogPath,
          agentId,
          memoryId: seededMemoryId,
        });

        originalDbCounts = await captureDbCounts({
          dbPath,
          repoRoot,
          env: childEnv,
          logPath: originalCountsLogPath,
        });
        await writeJson(path.join(runDir, 'db-counts-original.json'), originalDbCounts);

        const originalDbStatus = await runLoggedCaptureCommand(
          gatewayBinary,
          ['-c', configPath, '--output', 'json', 'db', 'status'],
          { cwd: repoRoot, env: childEnv, logPath: originalDbStatusLogPath },
        );
        originalDbStatusJson = JSON.parse(originalDbStatus.stdout);
        await writeJson(path.join(runDir, 'db-status-original.json'), originalDbStatusJson);
      },
    });

    const keepPhaseOneArtifacts = options.keepArtifacts || Boolean(phaseOneResult.error);
    await persistBrowserArtifacts(runDir, 'phase-one', phaseOneResult, keepPhaseOneArtifacts);
    await phaseOneResult.browser.close();

    if (phaseOneResult.error) {
      throw phaseOneResult.error;
    }

    if (!originalDbStatusJson) {
      throw new Error('Original DB status was not captured before backup');
    }
    if (!originalDbCounts) {
      throw new Error('Original direct DB counts were not captured before backup');
    }

    Object.assign(summary.checks, phaseOneResult.journey.checks);
    summary.checks.phase_one_page_errors = phaseOneResult.browserEvents.pageErrors.length === 0;
    summary.phase_one = {
      studio_assistant_text: phaseOneResult.journey.studioAssistantText,
      backup_response: phaseOneResult.journey.backupResponse,
      seeded_itp_chain: seededItpChain,
    };

    const studioState = await collectStudioState(gatewayUrl, phaseOneResult.journey.accessToken);
    await writeJson(path.join(runDir, 'studio-state-before-restart.json'), studioState);
    summary.checks.stream_recover_before_restart =
      Array.isArray(studioState.recover?.events) && studioState.recover.events.length > 0;

    summary.checks.populated_db_has_itp_events = originalDbCounts.itp_events > 0;
    summary.checks.populated_db_has_compaction_candidate = originalDbCounts.memory_events >= 60;

    const dbVerify = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'verify', '--full'],
      { cwd: repoRoot, env: childEnv, logPath: dbVerifyLogPath },
    );
    const dbVerifyResult = parseVerifyOutput(dbVerify.stdout);
    await writeJson(path.join(runDir, 'db-verify.json'), dbVerifyResult);
    summary.checks.db_verify_populated_nonzero = dbVerifyResult.checkedEvents > 0;
    summary.checks.db_verify_populated_clean = dbVerifyResult.clean === true;

    const compactWhileRunning = await runLoggedMaybeFailCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'compact', '--yes'],
      { cwd: repoRoot, env: childEnv, logPath: dbCompactRunningLogPath },
    );
    await writeJson(path.join(runDir, 'db-compact-running.json'), compactWhileRunning);
    summary.checks.db_compact_rejects_running_gateway =
      compactWhileRunning.exitCode !== 0 &&
      `${compactWhileRunning.stdout}\n${compactWhileRunning.stderr}`.includes('Gateway is running');

    const backupId = phaseOneResult.journey.backupResponse?.backup_id;
    if (!backupId) {
      throw new Error('Backup UI completed without returning a backup_id');
    }
    const backupPath = path.join(backupDir, `ghost-backup-${backupId}.ghost-backup`);
    summary.backup_path = backupPath;
    summary.checks.backup_archive_written = await fs.stat(backupPath).then(() => true).catch(() => false);

    const restoreResult = await runLoggedCaptureCommand(
      gatewayBinary,
      ['restore', '--input', backupPath, '--target', restoredGhostDir],
      { cwd: repoRoot, env: childEnv, logPath: restoreLogPath },
    );
    summary.checks.restore_created_target = restoreResult.stdout.includes('Backup restored into fresh target');
    summary.checks.restore_target_exists = await fs.stat(restoredGhostDir).then(() => true).catch(() => false);

    const restoredGatewayPort = await getFreePort();
    const restoredConfigText = buildTempConfig(
      baseConfigText,
      restoredGatewayPort,
      path.join(restoredGhostDir, 'data', 'ghost.db'),
    );
    await fs.writeFile(restoredConfigPath, restoredConfigText);

    const restoredStatus = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', restoredConfigPath, '--output', 'json', 'db', 'status'],
      { cwd: repoRoot, env: childEnv, logPath: restoredDbStatusLogPath },
    );
    const restoredStatusJson = JSON.parse(restoredStatus.stdout);
    await writeJson(path.join(runDir, 'db-status-restored.json'), restoredStatusJson);
    const restoredDbCounts = await captureDbCounts({
      dbPath: path.join(restoredGhostDir, 'data', 'ghost.db'),
      repoRoot,
      env: childEnv,
      logPath: restoredCountsLogPath,
    });
    await writeJson(path.join(runDir, 'db-counts-restored.json'), restoredDbCounts);
    summary.checks.restored_itp_count_matches_prebackup =
      restoredDbCounts.itp_events === originalDbCounts.itp_events;
    summary.checks.restored_memory_count_matches_prebackup =
      restoredDbCounts.memory_events === originalDbCounts.memory_events;

    const restoredVerify = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', restoredConfigPath, 'db', 'verify', '--full'],
      { cwd: repoRoot, env: childEnv, logPath: restoredDbVerifyLogPath },
    );
    const restoredVerifyResult = parseVerifyOutput(restoredVerify.stdout);
    await writeJson(path.join(runDir, 'db-verify-restored.json'), restoredVerifyResult);
    summary.checks.restored_db_verify_nonzero = restoredVerifyResult.checkedEvents > 0;
    summary.checks.restored_db_verify_clean = restoredVerifyResult.clean === true;

    const compactDryRun = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'compact', '--dry-run'],
      { cwd: repoRoot, env: childEnv, logPath: dbCompactDryRunLogPath },
    );
    await writeJson(path.join(runDir, 'db-compact-dry-run.json'), { stdout: compactDryRun.stdout });
    summary.checks.db_compact_dry_run_reports_candidate =
      compactDryRun.stdout.includes('eligible for compaction') &&
      compactDryRun.stdout.includes(seededMemoryId);

    await gatewayProcess.stop();
    gatewayProcess = null;

    const compactResult = await runLoggedCaptureCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'compact', '--yes'],
      { cwd: repoRoot, env: childEnv, logPath: dbCompactLogPath },
    );
    summary.checks.db_compact_completed = compactResult.stdout.includes('Database compacted');
    summary.checks.db_compact_compacted_memory = compactResult.stdout.includes('Memory compaction: 1 memories');

    const compactionCounts = await runLoggedCaptureCommand(
      'sqlite3',
      [dbPath, "SELECT (SELECT COUNT(*) FROM compaction_runs) || '|' || (SELECT COUNT(*) FROM compaction_event_ranges) || '|' || (SELECT COUNT(*) FROM memory_snapshots);"],
      { cwd: repoRoot, env: childEnv, logPath: compactionQueryLogPath },
    );
    const [compactionRuns, compactionRanges, snapshotCount] = compactionCounts.stdout.trim().split('|').map((value) => Number.parseInt(value, 10) || 0);
    await writeJson(path.join(runDir, 'compaction-counts.json'), {
      compaction_runs: compactionRuns,
      compaction_event_ranges: compactionRanges,
      memory_snapshots: snapshotCount,
    });
    summary.checks.compaction_rows_recorded = compactionRuns > 0 && compactionRanges > 0 && snapshotCount > 0;

    gatewayProcess = createLoggedProcess(
      gatewayBinary,
      ['-c', configPath, 'serve'],
      { cwd: repoRoot, env: childEnv, logPath: gatewayLogPath },
    );
    await waitForHttp(`${gatewayUrl}/api/health`, {
      timeoutMs: 30_000,
      process: gatewayProcess.child,
      name: 'gateway restart',
    });

    const restartedHealth = await fetchJson(`${gatewayUrl}/api/health`);
    await writeJson(path.join(runDir, 'health-after-restart.json'), restartedHealth.body);
    summary.checks.gateway_restart_healthy =
      restartedHealth.status === 200 && restartedHealth.body?.status === 'alive';

    phaseTwoResult = await runPhaseTwoBrowserJourney({
      dashboardUrl,
      gatewayUrl,
      jwtSecret: DEFAULT_JWT_SECRET,
      timeoutMs: options.timeoutMs,
      headed: options.headed,
      backupId,
      expectedAssistantNeedle: 'GHOST',
    });

    const keepPhaseTwoArtifacts = options.keepArtifacts || Boolean(phaseTwoResult.error);
    await persistBrowserArtifacts(runDir, 'phase-two', phaseTwoResult, keepPhaseTwoArtifacts);
    await phaseTwoResult.browser.close();

    if (phaseTwoResult.error) {
      throw phaseTwoResult.error;
    }

    Object.assign(summary.checks, phaseTwoResult.journey.checks);
    summary.checks.phase_two_page_errors = phaseTwoResult.browserEvents.pageErrors.length === 0;

    const studioStateAfterRestart = await collectStudioState(gatewayUrl, phaseOneResult.journey.accessToken);
    await writeJson(path.join(runDir, 'studio-state-after-restart.json'), studioStateAfterRestart);
    summary.checks.stream_recover_after_restart =
      Array.isArray(studioStateAfterRestart.recover?.events) &&
      studioStateAfterRestart.latestSession?.id === studioState.latestSession?.id &&
      studioStateAfterRestart.recover.events.length > 0;

    if (phaseOneResult.browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Phase one browser console emitted error-level messages.');
    }
    if (phaseTwoResult.browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Phase two browser console emitted error-level messages.');
    }

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);
    if (failedChecks.length > 0) {
      throw new Error(`Live database audit failed checks: ${failedChecks.join(', ')}`);
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);
  } finally {
    if (phaseTwoResult?.browser) {
      await phaseTwoResult.browser.close().catch(() => {});
    }
    if (phaseOneResult?.browser) {
      await phaseOneResult.browser.close().catch(() => {});
    }
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
    await removeIfPresent(runDir);
    process.stdout.write(`Live database audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live database audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
