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

const DEFAULT_PROMPT =
  'Use the read_file tool on README.md and answer with the project name only. Do not answer from memory.';
const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JOURNEY = 'studio-tool';
const CANCELLATION_PROMPT =
  'Write exactly 400 numbered lines about why streaming interfaces need cancellation controls. Start immediately with line 1 and do not add a conclusion.';
const RELOAD_RECOVER_PROMPT =
  'Start your answer with exactly STREAM-RECOVER-ALPHA on its own line, then write exactly 250 numbered lines about resilient streaming UIs. No conclusion.';
const SUPPORTED_JOURNEYS = [
  'studio-plain',
  'studio-tool',
  'studio-persist',
  'studio-cancel',
  'studio-reload-recover',
  'studio-session-switch',
  'studio-ws-reconnect',
  'skills',
];

function parseArgs(argv) {
  const options = {
    mode: 'dev',
    headed: false,
    keepArtifacts: false,
    prompt: DEFAULT_PROMPT,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    journey: DEFAULT_JOURNEY,
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
      case '--prompt':
        options.prompt = argv[index + 1] ?? options.prompt;
        index += 1;
        break;
      case '--timeout-ms':
        options.timeoutMs = Number.parseInt(argv[index + 1] ?? '', 10) || options.timeoutMs;
        index += 1;
        break;
      case '--journey':
        options.journey = argv[index + 1] ?? options.journey;
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
  if (!SUPPORTED_JOURNEYS.includes(options.journey)) {
    throw new Error(`Unsupported journey: ${options.journey}`);
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live Studio audit harness

Usage:
  pnpm audit:studio-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                         [--prompt "..."] [--timeout-ms 45000]
                         [--journey ${SUPPORTED_JOURNEYS.join('|')}]

What it does:
  1. Builds the gateway binary if needed
  2. Creates a fresh temp config + DB
  3. Boots a real gateway
  4. Boots the dashboard
  5. Drives the real /studio UI in Chromium
  6. Verifies tool-use, websocket events, and stream recovery
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

const IGNORED_BUILD_INPUT_DIRS = new Set([
  '.git',
  '.svelte-kit',
  'artifacts',
  'dashboard',
  'dist',
  'node_modules',
  'target',
]);

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
  const newestInputMtimeMs = await Math.max(
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

function nowIso() {
  return new Date().toISOString();
}

async function collectGatewayState(gatewayUrl) {
  const sessionsResponse = await fetch(`${gatewayUrl}/api/studio/sessions?limit=10`);
  const sessions = await sessionsResponse.json();
  const latestSession = sessions.sessions?.[0] ?? null;

  let sessionDetail = null;
  let recover = null;
  let latestAssistantMessage = null;

  if (latestSession?.id) {
    const detailResponse = await fetch(
      `${gatewayUrl}/api/studio/sessions/${encodeURIComponent(latestSession.id)}`,
    );
    sessionDetail = await detailResponse.json();
    latestAssistantMessage =
      sessionDetail.messages?.filter((message) => message.role === 'assistant').at(-1) ?? null;

    if (latestAssistantMessage?.id) {
      const recoverResponse = await fetch(
        `${gatewayUrl}/api/studio/sessions/${encodeURIComponent(latestSession.id)}/stream/recover?message_id=${encodeURIComponent(latestAssistantMessage.id)}&after_seq=0`,
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

async function openFreshStudio(page, dashboardUrl, gatewayUrl) {
  await page.addInitScript(({ value }) => {
    localStorage.clear();
    sessionStorage.clear();
    localStorage.setItem('ghost-gateway-url', value);
  }, { value: gatewayUrl });

  await page.goto(`${dashboardUrl}/studio`, { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: '+ New' }).click();
  return page.getByRole('textbox', { name: 'Message input' });
}

async function sendStudioMessage(page, prompt, timeoutMs, expectToolActivity) {
  const input = page.getByRole('textbox', { name: 'Message input' });
  await input.click({ timeout: timeoutMs });

  const assistantCountBefore = await page.locator('.assistant-text').count();
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
    toolEntries: await page.locator('.tool-call-entry, .tool-indicator').allInnerTexts(),
  };
}

async function waitForLastAssistantTextContains(page, expectedText, timeoutMs) {
  await page.waitForFunction(
    (needle) => {
      const nodes = Array.from(document.querySelectorAll('.assistant-text'));
      if (nodes.length === 0) return false;
      return (nodes[nodes.length - 1].textContent ?? '').includes(needle);
    },
    expectedText,
    { timeout: timeoutMs },
  );
}

async function clickSessionByIndex(page, index, timeoutMs) {
  const button = page.locator('.session-list .session-btn').nth(index);
  await button.waitFor({ state: 'visible', timeout: timeoutMs });
  await button.click({ timeout: timeoutMs });
}

function connectionIndicator(page) {
  return page.locator('.sidebar-footer-row .indicator .label').first();
}

async function createSessionAndWait(page, timeoutMs) {
  const sessionItems = page.locator('.session-list .session-item');
  const countBefore = await sessionItems.count();
  await page.getByRole('button', { name: '+ New' }).click({ timeout: timeoutMs });
  await page.waitForFunction(
    (count) => document.querySelectorAll('.session-list .session-item').length > count,
    countBefore,
    { timeout: timeoutMs },
  );
  await page.waitForFunction(
    () => document.querySelectorAll('.session-item.active').length === 1,
    undefined,
    { timeout: timeoutMs },
  );
  await page.getByRole('textbox', { name: 'Message input' }).waitFor({
    state: 'visible',
    timeout: timeoutMs,
  });
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

async function runStudioPlainJourney(page, options) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);
  const prompt = options.prompt === DEFAULT_PROMPT
    ? 'Reply with exactly OK and nothing else.'
    : options.prompt;
  const result = await sendStudioMessage(page, prompt, options.timeoutMs, false);

  return {
    assistantText: result.assistantText,
    toolEntries: result.toolEntries,
    expectToolActivity: false,
    verifyRecovery: true,
    checks: {
      assistant_rendered: result.assistantText.trim().length > 0,
      assistant_mentions_expected_output: /\bOK\b/.test(result.assistantText),
    },
  };
}

async function runStudioToolJourney(page, options, browserEvents) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);
  const result = await sendStudioMessage(page, options.prompt, options.timeoutMs, true);

  return {
    assistantText: result.assistantText,
    toolEntries: result.toolEntries,
    expectToolActivity: true,
    verifyRecovery: true,
    checks: {
      assistant_rendered: result.assistantText.trim().length > 0,
      assistant_mentions_expected_output: /ghost/i.test(result.assistantText),
      ws_tool_use: browserEvents.wsFrames.some(
        (frame) =>
          typeof frame.payload === 'string' && frame.payload.includes('"tool_use:read_file"'),
      ),
      ws_tool_result: browserEvents.wsFrames.some(
        (frame) =>
          typeof frame.payload === 'string' && frame.payload.includes('"tool_result:read_file"'),
      ),
    },
  };
}

async function runStudioPersistJourney(page, options, browserEvents) {
  const toolJourney = await runStudioToolJourney(page, options, browserEvents);
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForFunction(
    () => document.querySelectorAll('.assistant-text').length > 0,
    undefined,
    { timeout: options.timeoutMs },
  );

  const persistedText = await page.locator('.assistant-text').last().innerText();
  return {
    ...toolJourney,
    persistedText,
    verifyRecovery: true,
    checks: {
      ...toolJourney.checks,
      persisted_after_reload: persistedText.trim().length > 0 && /ghost/i.test(persistedText),
    },
  };
}

async function runStudioCancelJourney(page, options) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);
  const prompt = options.prompt === DEFAULT_PROMPT ? CANCELLATION_PROMPT : options.prompt;
  const input = page.getByRole('textbox', { name: 'Message input' });
  await input.click({ timeout: options.timeoutMs });
  await page.keyboard.type(prompt);
  await page.getByRole('button', { name: 'Send' }).click({ timeout: options.timeoutMs });

  const stopButton = page.locator('.btn-stop');
  await stopButton.waitFor({ state: 'visible', timeout: options.timeoutMs });
  await page.waitForFunction(
    () => {
      const nodes = Array.from(document.querySelectorAll('.assistant-text'));
      if (nodes.length === 0) return false;
      return ((nodes[nodes.length - 1].textContent ?? '').trim().length) >= 20;
    },
    undefined,
    { timeout: options.timeoutMs },
  );

  await stopButton.click({ timeout: options.timeoutMs });
  await page.locator('.btn-primary').waitFor({ state: 'visible', timeout: options.timeoutMs });

  const assistantText = await page.locator('.assistant-text').last().innerText();
  return {
    assistantText,
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: false,
    checks: {
      cancelled_rendered: assistantText.includes('(cancelled)'),
      cancelled_has_partial_output: assistantText.replace(/\*\(cancelled\)\*/g, '').trim().length > 0,
      streaming_stopped_after_cancel: await page.locator('.btn-stop').count() === 0,
    },
  };
}

async function runStudioReloadRecoverJourney(page, options) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);
  const prompt = options.prompt === DEFAULT_PROMPT ? RELOAD_RECOVER_PROMPT : options.prompt;
  const input = page.getByRole('textbox', { name: 'Message input' });
  await input.click({ timeout: options.timeoutMs });
  await page.keyboard.type(prompt);
  await page.getByRole('button', { name: 'Send' }).click({ timeout: options.timeoutMs });

  const stopButton = page.locator('.btn-stop');
  await stopButton.waitFor({ state: 'visible', timeout: options.timeoutMs });
  await page.waitForFunction(
    () => {
      const nodes = Array.from(document.querySelectorAll('.assistant-text'));
      if (nodes.length === 0) return false;
      return (nodes[nodes.length - 1].textContent ?? '').trim().length >= 80;
    },
    undefined,
    { timeout: options.timeoutMs },
  );

  const partialText = await page.locator('.assistant-text').last().innerText();
  const partialLength = partialText.trim().length;

  await page.reload({ waitUntil: 'networkidle' });
  await page.getByRole('textbox', { name: 'Message input' }).waitFor({
    state: 'visible',
    timeout: options.timeoutMs,
  });

  await page.waitForFunction(
    ({ minLength }) => {
      const nodes = Array.from(document.querySelectorAll('.assistant-text'));
      if (nodes.length === 0) return false;
      const text = (nodes[nodes.length - 1].textContent ?? '').trim();
      return text.includes('STREAM-RECOVER-ALPHA') && text.length > minLength + 100;
    },
    { minLength: partialLength },
    { timeout: options.timeoutMs },
  );

  const recoveredText = await page.locator('.assistant-text').last().innerText();
  return {
    assistantText: recoveredText,
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: true,
    checks: {
      recovered_after_reload: recoveredText.includes('STREAM-RECOVER-ALPHA'),
      recovered_content_grew: recoveredText.trim().length > partialLength + 100,
      recovered_session_visible: await page.locator('.session-item.active').count() === 1,
    },
  };
}

async function runStudioSessionSwitchJourney(page, options) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);

  const firstPrompt = 'Reply with exactly ALPHA and nothing else.';
  const firstResult = await sendStudioMessage(page, firstPrompt, options.timeoutMs, false);

  await createSessionAndWait(page, options.timeoutMs);
  const secondPrompt = 'Reply with exactly BETA and nothing else.';
  const secondResult = await sendStudioMessage(page, secondPrompt, options.timeoutMs, false);

  const sessionButtons = page.locator('.session-list .session-btn');
  await page.waitForFunction(
    () => document.querySelectorAll('.session-list .session-btn').length >= 2,
    undefined,
    { timeout: options.timeoutMs },
  );

  await clickSessionByIndex(page, 1, options.timeoutMs);
  await waitForLastAssistantTextContains(page, 'ALPHA', options.timeoutMs);
  const switchedToOlderText = await page.locator('.assistant-text').last().innerText();

  await clickSessionByIndex(page, 0, options.timeoutMs);
  await waitForLastAssistantTextContains(page, 'BETA', options.timeoutMs);
  const switchedBackText = await page.locator('.assistant-text').last().innerText();

  return {
    assistantText: switchedBackText,
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: true,
    checks: {
      first_session_rendered: /\bALPHA\b/.test(firstResult.assistantText),
      second_session_rendered: /\bBETA\b/.test(secondResult.assistantText),
      session_list_has_multiple_sessions: await sessionButtons.count() >= 2,
      switch_to_previous_session: /\bALPHA\b/.test(switchedToOlderText),
      switch_back_to_latest_session: /\bBETA\b/.test(switchedBackText),
      active_session_indicator_present: await page.locator('.session-item.active').count() === 1,
    },
  };
}

async function runStudioWsReconnectJourney(page, options) {
  await openFreshStudio(page, options.dashboardUrl, options.gatewayUrl);
  await waitForConnectionState(page, 'Connected', options.timeoutMs);

  const prompt = 'Reply with exactly RECONNECT-OK and nothing else.';
  const result = await sendStudioMessage(page, prompt, options.timeoutMs, false);

  await options.controls.stopGateway();
  await page.waitForFunction(
    () => {
      const label = document.querySelector('.sidebar-footer-row .indicator .label');
      const text = (label?.textContent ?? '').trim();
      return text === 'Reconnecting' || text === 'Disconnected';
    },
    undefined,
    { timeout: options.timeoutMs },
  );

  await options.controls.startGateway();
  await waitForConnectionState(page, 'Connected', options.timeoutMs);
  await waitForLastAssistantTextContains(page, 'RECONNECT-OK', options.timeoutMs);

  const assistantText = await page.locator('.assistant-text').last().innerText();
  return {
    assistantText,
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: true,
    checks: {
      initial_session_rendered: result.assistantText.trim() === 'RECONNECT-OK',
      ws_reconnected: true,
      ws_resync_received: options.browserEvents.wsFrames.some(
        (frame) =>
          typeof frame.payload === 'string' && frame.payload.includes('"type":"Resync"'),
      ),
      session_survived_gateway_restart: assistantText.trim() === 'RECONNECT-OK',
    },
  };
}

async function runSkillsJourney(page, options) {
  await page.addInitScript(({ value }) => {
    localStorage.clear();
    sessionStorage.clear();
    localStorage.setItem('ghost-gateway-url', value);
  }, { value: options.gatewayUrl });

  await page.goto(`${options.dashboardUrl}/skills`, { waitUntil: 'networkidle' });
  const bodyText = await page.locator('body').innerText();

  return {
    assistantText: '',
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: false,
    checks: {
      skills_page_loaded: bodyText.includes('Skills'),
      skills_rendered: /convergence_check|attachment_monitor|note_take|git_status/.test(bodyText),
    },
  };
}

async function runBrowserJourney(options) {
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

  page.on('response', async (response) => {
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

  let journey = {
    assistantText: '',
    toolEntries: [],
    expectToolActivity: false,
    verifyRecovery: true,
    checks: {},
  };

  try {
    if (options.journey === 'studio-plain') {
      journey = await runStudioPlainJourney(page, options);
    } else if (options.journey === 'studio-tool') {
      journey = await runStudioToolJourney(page, options, browserEvents);
    } else if (options.journey === 'studio-persist') {
      journey = await runStudioPersistJourney(page, options, browserEvents);
    } else if (options.journey === 'studio-cancel') {
      journey = await runStudioCancelJourney(page, options);
    } else if (options.journey === 'studio-reload-recover') {
      journey = await runStudioReloadRecoverJourney(page, options);
    } else if (options.journey === 'studio-session-switch') {
      journey = await runStudioSessionSwitchJourney(page, options);
    } else if (options.journey === 'studio-ws-reconnect') {
      journey = await runStudioWsReconnectJourney(page, { ...options, browserEvents });
    } else {
      journey = await runSkillsJourney(page, options);
    }

    return { journey, browserEvents, page, context, browser };
  } catch (error) {
    return { journey, browserEvents, page, context, browser, error };
  }
}

async function persistBrowserArtifacts(runDir, browserResult, keepArtifacts) {
  const screenshotPath = path.join(runDir, 'studio-page.png');
  const htmlPath = path.join(runDir, 'studio-page.html');
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
  const runDir = path.join(repoRoot, 'artifacts', 'live-audits', runLabel);
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
    journey: options.journey,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    prompt: options.prompt,
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
    const migrateLogPath = path.join(runDir, 'gateway-migrate.log');
    const gatewayLogPath = path.join(runDir, 'gateway.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');

    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);

    const gatewayEnv = {
      ...process.env,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_agent_loop=info,ghost_llm=warn',
    };

    await runLoggedCommand(
      gatewayBinary,
      ['-c', configPath, 'db', 'migrate'],
      { cwd: repoRoot, env: gatewayEnv, logPath: migrateLogPath },
    );
    summary.checks.gateway_db_migrated = true;

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
    summary.checks.gateway_healthy = true;

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

    await waitForHttp(`${dashboardUrl}/studio`, {
      timeoutMs: 45_000,
      process: dashboardProcess.child,
      name: 'dashboard',
    });
    summary.checks.dashboard_ready = true;

    browserResult = await runBrowserJourney({
      gatewayUrl,
      dashboardUrl,
      prompt: options.prompt,
      timeoutMs: options.timeoutMs,
      headed: options.headed,
      journey: options.journey,
      controls: {
        stopGateway: async () => {
          if (!gatewayProcess) return;
          await gatewayProcess.stop();
          gatewayProcess = null;
        },
        startGateway: async () => {
          if (gatewayProcess) return;
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
        },
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

    summary.checks.page_errors = browserResult.browserEvents.pageErrors.length === 0;
    Object.assign(summary.checks, browserResult.journey.checks);

    if (browserResult.browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-console.jsonl.');
    }

    if (options.journey.startsWith('studio')) {
      const gatewayState = await collectGatewayState(gatewayUrl);
      await writeJson(path.join(runDir, 'sessions.json'), gatewayState.sessions);
      await writeJson(path.join(runDir, 'session-detail.json'), gatewayState.sessionDetail);
      await writeJson(path.join(runDir, 'stream-recover.json'), gatewayState.recover);

      if (browserResult.journey.verifyRecovery !== false) {
        const recoverEvents = gatewayState.recover?.events ?? [];
        summary.checks.recover_has_events = recoverEvents.length > 0;
        summary.checks.recover_has_stream_end = recoverEvents.some(
          (event) => event.event_type === 'stream_end',
        );

        if (browserResult.journey.expectToolActivity) {
          summary.checks.recover_has_tool_use = recoverEvents.some(
            (event) => event.event_type === 'tool_use',
          );
          summary.checks.recover_has_tool_result = recoverEvents.some(
            (event) => event.event_type === 'tool_result',
          );
        }
      }

      summary.session_id = gatewayState.latestSession?.id ?? null;
      summary.assistant_message_id = gatewayState.latestAssistantMessage?.id ?? null;
    }

    summary.assistant_text = browserResult.journey.assistantText;
    summary.tool_entries = browserResult.journey.toolEntries;
    if (browserResult.journey.persistedText) {
      summary.persisted_text = browserResult.journey.persistedText;
    }

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);

    if (failedChecks.length > 0) {
      throw new Error(`Live audit failed checks: ${failedChecks.join(', ')}`);
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
    process.stdout.write(`Live Studio audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live Studio audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
