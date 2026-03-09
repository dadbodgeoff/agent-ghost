#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as delay } from 'node:timers/promises';
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
  writeJsonLines,
} from './lib/live_harness.mjs';

const DEFAULT_TIMEOUT_MS = 60_000;
const DEFAULT_JWT_SECRET = 'ghost-live-jwt-secret';
const DASHBOARD_CLIENT_NAME = 'dashboard';
const DASHBOARD_CLIENT_VERSION = '0.1.0';
const BLOCKING_PROMPT =
  'Use the read_file tool on README.md and answer with the project name only. Do not answer from memory.';
const STREAMING_PROMPT =
  'Use the read_file tool on README.md and answer with exactly the project name only. Do not answer from memory or add punctuation.';

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
  process.stdout.write(`Live runtime audit

Usage:
  pnpm audit:runtime-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                          [--timeout-ms 60000]

What it does:
  1. Boots a fresh auth-enabled gateway and dashboard
  2. Logs into the real dashboard and keeps the websocket connected
  3. Creates a convergence profile through the real UI
  4. Creates a runtime agent through the real agent wizard
  5. Exercises blocking and streaming /api/agent/chat
  6. Verifies live execution, sessions, costs, safety, goals, traces, and page surfaces
  7. Preserves logs/traces/screenshots only when the run fails
`);
}

function authHeaders(accessToken, extra = {}) {
  return {
    Authorization: `Bearer ${accessToken}`,
    ...extra,
  };
}

function jsonHeaders(accessToken, extra = {}) {
  return authHeaders(accessToken, {
    'content-type': 'application/json',
    ...extra,
  });
}

function mutationHeaders(accessToken, mutationKey, extra = {}) {
  const requestId = randomUUID();
  return jsonHeaders(accessToken, {
    'x-request-id': requestId,
    'x-ghost-operation-id': randomUUID(),
    'idempotency-key': `${mutationKey}-${requestId}`,
    'x-ghost-client-name': DASHBOARD_CLIENT_NAME,
    'x-ghost-client-version': DASHBOARD_CLIENT_VERSION,
    ...extra,
  });
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

async function querySqliteScalar(dbPath, sql, repoRoot, env, logPath) {
  const result = await runLoggedCaptureCommand('sqlite3', [dbPath, sql], {
    cwd: repoRoot,
    env,
    logPath,
  });
  return result.stdout.trim();
}

async function createProfileViaUi(page, profileName, timeoutMs) {
  await page.goto('/settings/profiles', { waitUntil: 'networkidle', timeout: timeoutMs });
  await page.getByRole('heading', { name: 'Convergence Profiles' }).waitFor({
    state: 'visible',
    timeout: timeoutMs,
  });
  await page.getByPlaceholder('New profile name').fill(profileName, { timeout: timeoutMs });
  await page.getByRole('button', { name: 'Create' }).click({ timeout: timeoutMs });
  await page.waitForFunction(
    (expectedName) =>
      Array.from(document.querySelectorAll('.profile-btn')).some(
        (node) => (node.textContent ?? '').includes(expectedName),
      ),
    profileName,
    { timeout: timeoutMs },
  );

  return {
    profileCreated: true,
    profileVisible: true,
  };
}

async function createAgentViaWizard(page, agentName, timeoutMs) {
  await page.goto('/agents/new', { waitUntil: 'networkidle', timeout: timeoutMs });
  await page.getByRole('heading', { name: 'Create Agent' }).waitFor({
    state: 'visible',
    timeout: timeoutMs,
  });
  await page.locator('#name').fill(agentName, { timeout: timeoutMs });

  for (let step = 2; step <= 7; step += 1) {
    await page.getByRole('button', { name: 'Next' }).click({ timeout: timeoutMs });
    await page.waitForFunction(
      (expectedStep) => {
        const active = document.querySelector('.progress-step.active .step-number');
        return (active?.textContent ?? '').trim() === String(expectedStep);
      },
      step,
      { timeout: timeoutMs },
    );

    if (step === 4) {
      const fileReadCheckbox = page.getByRole('checkbox', { name: 'File Read' });
      await fileReadCheckbox.waitFor({ state: 'visible', timeout: timeoutMs });
      await fileReadCheckbox.check({ timeout: timeoutMs });
    }
  }

  await page.getByRole('button', { name: 'Create Agent' }).click({ timeout: timeoutMs });
  await page.waitForURL(
    (url) =>
      /^\/agents\/[^/]+$/.test(url.pathname) &&
      url.pathname !== '/agents/new',
    { timeout: timeoutMs },
  );

  const url = new URL(page.url());
  const agentId = url.pathname.split('/').filter(Boolean).at(-1);
  if (!agentId) {
    throw new Error('Agent wizard did not navigate to a detail page');
  }

  await page.waitForFunction(
    (expectedName) => {
      const heading = document.querySelector('.header-row h1');
      return (heading?.textContent ?? '').trim() === expectedName;
    },
    agentName,
    { timeout: timeoutMs },
  );

  return {
    agentId,
    detailLoaded: true,
  };
}

async function queryLiveExecutionId(dbPath, operationId, repoRoot, env, logPath) {
  return querySqliteScalar(
    dbPath,
    `SELECT id FROM live_execution_records WHERE operation_id = ${sqlString(operationId)} LIMIT 1;`,
    repoRoot,
    env,
    logPath,
  );
}

async function queryAssignedProfile(dbPath, agentId, repoRoot, env, logPath) {
  return querySqliteScalar(
    dbPath,
    `SELECT profile_name FROM agent_profile_assignments WHERE agent_id = ${sqlString(agentId)} LIMIT 1;`,
    repoRoot,
    env,
    logPath,
  );
}

async function seedTraceSpan(dbPath, sessionId, repoRoot, env, logPath) {
  const traceId = `trace-${sessionId}`;
  const spanId = `span-${sessionId}`;
  const now = nowIso();
  const sql = `
BEGIN;
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
  'runtime_live.blocking_turn',
  ${sqlString(now)},
  ${sqlString(now)},
  ${sqlString(JSON.stringify({ seeded_by: 'live_runtime_audit', session_id: sessionId }))},
  'ok',
  ${sqlString(sessionId)}
);
COMMIT;`;
  await runSqlite(dbPath, sql, repoRoot, env, logPath);
  return { traceId, spanId };
}

function goalSeedPayload(agentId, sessionId, suffix, kind) {
  const proposalId = `runtime-goal-${kind}-${suffix}`;
  const lineageId = `runtime-lineage-${kind}-${suffix}`;
  const subjectKey = `runtime-subject-${kind}-${suffix}`;
  const reviewedRevision = `runtime-rev-${kind}-1`;
  const createdAt = nowIso();
  const content = {
    title: `Runtime ${kind}`,
    subject_key: subjectKey,
    lineage_id: lineageId,
    reviewed_revision: reviewedRevision,
    requested_action: kind,
  };

  return {
    proposalId,
    lineageId,
    subjectKey,
    reviewedRevision,
    createdAt,
    content,
    eventHash: sha256Hex(`goal:${proposalId}`),
    previousHash: sha256Hex(`goal-prev:${proposalId}`),
    operation: `runtime-${kind}`,
  };
}

async function seedPendingGoals(dbPath, agentId, sessionId, repoRoot, env, logPath) {
  const suffix = timestampLabel().replace('-', '').toLowerCase();
  const approveGoal = goalSeedPayload(agentId, sessionId, suffix, 'approve');
  const rejectGoal = goalSeedPayload(agentId, sessionId, suffix, 'reject');
  const transitionIdA = `runtime-transition-a-${suffix}`;
  const transitionIdB = `runtime-transition-b-${suffix}`;

  const statements = [approveGoal, rejectGoal].flatMap((goal, index) => {
    const transitionId = index === 0 ? transitionIdA : transitionIdB;
    return [
      `INSERT INTO goal_proposals (
        id, agent_id, session_id, proposer_type, operation, target_type,
        content, cited_memory_ids, decision, resolved_at, resolver, flags,
        dimension_scores, denial_reason, event_hash, previous_hash, created_at
      ) VALUES (
        ${sqlString(goal.proposalId)},
        ${sqlString(agentId)},
        ${sqlString(sessionId)},
        'agent',
        ${sqlString(goal.operation)},
        'task',
        ${sqlString(JSON.stringify(goal.content))},
        '[]',
        NULL,
        NULL,
        NULL,
        '["runtime_live"]',
        '{"alignment":0.91,"safety":0.95}',
        NULL,
        x'${goal.eventHash}',
        x'${goal.previousHash}',
        ${sqlString(goal.createdAt)}
      );`,
      `INSERT INTO goal_proposals_v2 (
        id, lineage_id, subject_type, subject_key, reviewed_revision, proposer_type,
        proposer_id, agent_id, session_id, operation, target_type, content,
        cited_memory_ids, validation_disposition, validation_flags, validation_scores,
        denial_reason, supersedes_proposal_id, operation_id, request_id, created_at,
        event_hash, previous_hash
      ) VALUES (
        ${sqlString(goal.proposalId)},
        ${sqlString(goal.lineageId)},
        'runtime_task',
        ${sqlString(goal.subjectKey)},
        ${sqlString(goal.reviewedRevision)},
        'agent',
        ${sqlString(agentId)},
        ${sqlString(agentId)},
        ${sqlString(sessionId)},
        ${sqlString(goal.operation)},
        'task',
        ${sqlString(JSON.stringify(goal.content))},
        '[]',
        'HumanReviewRequired',
        '["runtime_live"]',
        '{"alignment":0.91,"safety":0.95}',
        NULL,
        NULL,
        NULL,
        NULL,
        ${sqlString(goal.createdAt)},
        x'${goal.eventHash}',
        x'${goal.previousHash}'
      );`,
      `INSERT INTO goal_proposal_transitions (
        id, proposal_id, lineage_id, from_state, to_state, actor_type, actor_id,
        reason_code, rationale, expected_state, expected_revision, operation_id,
        request_id, idempotency_key, created_at
      ) VALUES (
        ${sqlString(transitionId)},
        ${sqlString(goal.proposalId)},
        ${sqlString(goal.lineageId)},
        NULL,
        'pending_review',
        'system',
        NULL,
        'runtime_live_seed',
        NULL,
        NULL,
        ${sqlString(goal.reviewedRevision)},
        NULL,
        NULL,
        NULL,
        ${sqlString(goal.createdAt)}
      );`,
      `INSERT INTO goal_lineage_heads (
        subject_type, subject_key, lineage_id, head_proposal_id, head_state,
        current_revision, updated_at
      ) VALUES (
        'runtime_task',
        ${sqlString(goal.subjectKey)},
        ${sqlString(goal.lineageId)},
        ${sqlString(goal.proposalId)},
        'pending_review',
        ${sqlString(goal.reviewedRevision)},
        ${sqlString(goal.createdAt)}
      );`,
    ];
  });

  const sql = `BEGIN;\n${statements.join('\n')}\nCOMMIT;`;
  await runSqlite(dbPath, sql, repoRoot, env, logPath);

  return {
    approveProposalId: approveGoal.proposalId,
    rejectProposalId: rejectGoal.proposalId,
    approveOperation: approveGoal.operation,
    rejectOperation: rejectGoal.operation,
  };
}

function parseSseChunk(rawChunk) {
  const lines = rawChunk.split(/\r?\n/);
  const event = { event: 'message', data: '', id: null };

  for (const line of lines) {
    if (line.startsWith('event:')) {
      event.event = line.slice(6).trim();
      continue;
    }
    if (line.startsWith('data:')) {
      const next = line.slice(5).trim();
      event.data = event.data ? `${event.data}\n${next}` : next;
      continue;
    }
    if (line.startsWith('id:')) {
      event.id = line.slice(3).trim();
    }
  }

  let parsed = null;
  if (event.data) {
    try {
      parsed = JSON.parse(event.data);
    } catch {
      parsed = { raw: event.data };
    }
  }

  return {
    ...event,
    parsed,
  };
}

function parseSseTranscript(rawBody) {
  const events = [];
  let text = '';

  for (const chunk of rawBody.replace(/\r\n/g, '\n').split('\n\n')) {
    const trimmed = chunk.trim();
    if (!trimmed) {
      continue;
    }

    const event = parseSseChunk(trimmed);
    events.push(event);

    if (event.event === 'text_delta') {
      const delta = event.parsed?.content ?? event.parsed?.delta ?? event.parsed?.text ?? '';
      if (typeof delta === 'string') {
        text += delta;
      }
    }
    if (event.event === 'error') {
      throw new Error(`Streaming call emitted error: ${event.data}`);
    }
  }

  return {
    events,
    text: text.trim(),
    rawBody,
  };
}

async function requestSse(url, { method = 'GET', headers = {}, body }, timeoutMs) {
  const target = new URL(url);
  const transport = target.protocol === 'https:' ? https : http;

  return new Promise((resolve, reject) => {
    const request = transport.request(
      target,
      {
        method,
        headers,
      },
      (response) => {
        const chunks = [];
        response.setEncoding('utf8');
        response.on('data', (chunk) => {
          chunks.push(chunk);
        });
        response.on('end', () => {
          try {
            const rawBody = chunks.join('');
            const status = response.statusCode ?? 0;
            const headerValue = (name) => {
              const value = response.headers[name.toLowerCase()];
              return Array.isArray(value) ? value.join(', ') : value ?? null;
            };

            if (status < 200 || status >= 300) {
              reject(new Error(`Streaming call failed with ${status}: ${rawBody}`));
              return;
            }

            resolve({
              status,
              headers: {
                'x-ghost-operation-id': headerValue('x-ghost-operation-id'),
                'content-type': headerValue('content-type'),
              },
              ...parseSseTranscript(rawBody),
            });
          } catch (error) {
            reject(error);
          }
        });
      },
    );

    request.on('error', reject);
    request.setTimeout(timeoutMs, () => {
      request.destroy(new Error(`Streaming call did not complete within ${timeoutMs}ms`));
    });

    if (body) {
      request.write(body);
    }
    request.end();
  });
}

async function waitForWsFrame(browserEvents, predicate, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (browserEvents.wsFrames.some(predicate)) {
      return true;
    }
    await delay(200);
  }
  return false;
}

async function waitForPageWithoutError(page, timeoutMs) {
  await page.locator('.page-title, h1').first().waitFor({ state: 'visible', timeout: timeoutMs });
  return await page.locator('.error-state').count() === 0;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-runtime-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const tempHome = path.join(tempDir, 'home');

  await fs.mkdir(tempHome, { recursive: true });

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
    agent: {},
    profile: {},
    blocking_chat: {},
    streaming_chat: {},
    goals: {},
    traces: {},
    pages: {},
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
    const baseConfigPath = path.join(repoRoot, 'ghost.yml');
    const baseConfigText = await fs.readFile(baseConfigPath, 'utf8');
    await fs.writeFile(configPath, buildTempConfig(baseConfigText, gatewayPort, dbPath));

    const buildLogPath = path.join(runDir, 'gateway-build.log');
    const migrateLogPath = path.join(runDir, 'gateway-migrate.log');
    const gatewayLogPath = path.join(runDir, 'gateway.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');
    const sqliteLogPath = path.join(runDir, 'sqlite.log');
    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);

    const gatewayEnv = {
      ...process.env,
      HOME: tempHome,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_agent_loop=info,ghost_llm=warn',
    };

    await runLoggedCommand(gatewayBinary, ['-c', configPath, 'db', 'migrate'], {
      cwd: repoRoot,
      env: gatewayEnv,
      logPath: migrateLogPath,
    });

    gatewayProcess = createLoggedProcess(
      gatewayBinary,
      ['-c', configPath, 'serve'],
      { cwd: repoRoot, env: gatewayEnv, logPath: gatewayLogPath },
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

    const profileName = `runtime-live-${runLabel.toLowerCase()}`;
    const profileUi = await createProfileViaUi(page, profileName, options.timeoutMs);
    summary.profile.name = profileName;
    Object.assign(summary.checks, {
      profile_created_via_ui: profileUi.profileCreated,
      profile_visible_in_ui: profileUi.profileVisible,
    });

    const agentName = `runtime-agent-${runLabel.toLowerCase()}`;
    const agentUi = await createAgentViaWizard(page, agentName, options.timeoutMs);
    summary.agent.id = agentUi.agentId;
    summary.agent.name = agentName;
    Object.assign(summary.checks, {
      agent_created_via_ui: true,
      agent_detail_loaded: agentUi.detailLoaded,
    });

    summary.checks.ws_agent_state_change = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"AgentStateChange"') &&
        frame.payload.includes(agentUi.agentId),
      options.timeoutMs,
    );

    const assignProfile = await fetchJson(`${gatewayUrl}/api/agents/${agentUi.agentId}/profile`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `runtime-assign-profile-${runLabel}`),
      body: JSON.stringify({ profile_name: profileName }),
    });
    summary.checks.assign_profile_succeeded = assignProfile.status === 200;
    const assignedProfile = await queryAssignedProfile(
      dbPath,
      agentUi.agentId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    summary.checks.assigned_profile_persisted = assignedProfile === profileName;

    const blockingResponse = await fetch(`${gatewayUrl}/api/agent/chat`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `runtime-blocking-${runLabel}`),
      body: JSON.stringify({
        agent_id: agentUi.agentId,
        message: BLOCKING_PROMPT,
      }),
    });
    const blockingText = await blockingResponse.text();
    const blockingBody = JSON.parse(blockingText);
    const blockingOperationId = blockingResponse.headers.get('x-ghost-operation-id');
    if (!blockingOperationId) {
      throw new Error('Blocking agent chat did not return x-ghost-operation-id');
    }

    summary.blocking_chat = {
      operation_id: blockingOperationId,
      body: blockingBody,
    };
    Object.assign(summary.checks, {
      blocking_chat_succeeded: blockingResponse.status === 200,
      blocking_chat_used_tool: Number(blockingBody.tool_calls_made) > 0,
      blocking_chat_mentions_project: /ghost/i.test(blockingBody.content ?? ''),
      blocking_session_present: typeof blockingBody.session_id === 'string' && blockingBody.session_id.length > 10,
    });

    const blockingExecutionId = await queryLiveExecutionId(
      dbPath,
      blockingOperationId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    if (!blockingExecutionId) {
      throw new Error('Blocking live execution record was not persisted');
    }

    const blockingExecution = await fetchJson(
      `${gatewayUrl}/api/live-executions/${blockingExecutionId}`,
      { headers: authHeaders(accessToken) },
    );
    summary.blocking_chat.execution = blockingExecution.body;
    Object.assign(summary.checks, {
      blocking_execution_visible: blockingExecution.status === 200,
      blocking_execution_completed: blockingExecution.body?.status === 'completed',
    });

    const blockingSessionId = blockingBody.session_id;
    const heartbeat = await fetch(`${gatewayUrl}/api/sessions/${blockingSessionId}/heartbeat`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `runtime-heartbeat-${runLabel}`),
    });
    summary.checks.runtime_session_heartbeat_ok = heartbeat.status === 204;

    const sessionEvents = await fetchJson(
      `${gatewayUrl}/api/sessions/${blockingSessionId}/events?limit=50`,
      { headers: authHeaders(accessToken) },
    );
    summary.blocking_chat.session_events = sessionEvents.body;
    Object.assign(summary.checks, {
      runtime_session_events_visible: sessionEvents.status === 200,
      runtime_session_events_nonempty: (sessionEvents.body?.events?.length ?? 0) > 0,
      runtime_session_chain_valid: sessionEvents.body?.chain_valid === true,
    });

    const traceSeed = await seedTraceSpan(dbPath, blockingSessionId, repoRoot, gatewayEnv, sqliteLogPath);
    const traces = await fetchJson(`${gatewayUrl}/api/traces/${blockingSessionId}`, {
      headers: authHeaders(accessToken),
    });
    summary.traces = {
      ...traceSeed,
      body: traces.body,
    };
    Object.assign(summary.checks, {
      traces_visible: traces.status === 200,
      traces_seeded_span_visible: traces.body?.total_spans === 1,
    });

    const streamed = await requestSse(
      `${gatewayUrl}/api/agent/chat/stream`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `runtime-stream-${runLabel}`, {
          Accept: 'text/event-stream',
        }),
        body: JSON.stringify({
          agent_id: agentUi.agentId,
          message: STREAMING_PROMPT,
        }),
      },
      options.timeoutMs,
    );
    const streamOperationId = streamed.headers['x-ghost-operation-id'];
    if (!streamOperationId) {
      throw new Error('Streaming agent chat did not return x-ghost-operation-id');
    }
    const streamStart = streamed.events.find((event) => event.event === 'stream_start')?.parsed;
    summary.streaming_chat = {
      operation_id: streamOperationId,
      events: streamed.events,
      text: streamed.text,
    };
    Object.assign(summary.checks, {
      stream_started: !!streamStart?.session_id,
      stream_produced_text: streamed.text.length > 0,
      stream_mentions_project: /ghost/i.test(streamed.text),
      stream_emitted_tool_use: streamed.events.some((event) => event.event === 'tool_use'),
      stream_emitted_tool_result: streamed.events.some((event) => event.event === 'tool_result'),
      stream_ended: streamed.events.some((event) => event.event === 'stream_end'),
    });

    const streamExecutionId = await queryLiveExecutionId(
      dbPath,
      streamOperationId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    if (!streamExecutionId) {
      throw new Error('Streaming live execution record was not persisted');
    }

    const streamExecution = await fetchJson(
      `${gatewayUrl}/api/live-executions/${streamExecutionId}`,
      { headers: authHeaders(accessToken) },
    );
    summary.streaming_chat.execution = streamExecution.body;
    Object.assign(summary.checks, {
      streaming_execution_visible: streamExecution.status === 200,
      streaming_execution_completed: streamExecution.body?.status === 'completed',
    });

    summary.checks.ws_runtime_tool_use = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"SessionEvent"') &&
        frame.payload.includes('"event_type":"tool_use:read_file"'),
      options.timeoutMs,
    );
    summary.checks.ws_runtime_tool_result = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"SessionEvent"') &&
        frame.payload.includes('"event_type":"tool_result:read_file"'),
      options.timeoutMs,
    );

    const costs = await fetchJson(`${gatewayUrl}/api/costs`, {
      headers: authHeaders(accessToken),
    });
    summary.agent.costs = costs.body;
    const costRecord = Array.isArray(costs.body)
      ? costs.body.find((entry) => entry.agent_id === agentUi.agentId)
      : null;
    Object.assign(summary.checks, {
      costs_visible: costs.status === 200,
      cost_record_for_agent: !!costRecord,
      cost_record_positive: Number(costRecord?.daily_total ?? 0) > 0,
    });

    const pauseResponse = await fetchJson(`${gatewayUrl}/api/safety/pause/${agentUi.agentId}`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `runtime-pause-${runLabel}`),
      body: JSON.stringify({ reason: 'runtime live pause' }),
    });
    const pausedStatus = await fetchJson(`${gatewayUrl}/api/safety/status`, {
      headers: authHeaders(accessToken),
    });
    const pausedLevel = pausedStatus.body?.per_agent?.[agentUi.agentId]?.level ?? '';

    const resumeAfterPause = await fetchJson(`${gatewayUrl}/api/safety/resume/${agentUi.agentId}`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `runtime-resume-pause-${runLabel}`),
      body: JSON.stringify({}),
    });
    const resumedStatus = await fetchJson(`${gatewayUrl}/api/safety/status`, {
      headers: authHeaders(accessToken),
    });

    const quarantineResponse = await fetchJson(
      `${gatewayUrl}/api/safety/quarantine/${agentUi.agentId}`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `runtime-quarantine-${runLabel}`),
        body: JSON.stringify({ reason: 'runtime live quarantine' }),
      },
    );
    const quarantinedStatus = await fetchJson(`${gatewayUrl}/api/safety/status`, {
      headers: authHeaders(accessToken),
    });
    const quarantinedLevel = quarantinedStatus.body?.per_agent?.[agentUi.agentId]?.level ?? '';

    const resumeAfterQuarantine = await fetchJson(
      `${gatewayUrl}/api/safety/resume/${agentUi.agentId}`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `runtime-resume-quarantine-${runLabel}`),
        body: JSON.stringify({
          forensic_reviewed: true,
          second_confirmation: true,
        }),
      },
    );
    const finalSafetyStatus = await fetchJson(`${gatewayUrl}/api/safety/status`, {
      headers: authHeaders(accessToken),
    });
    summary.agent.safety = {
      pause_response: pauseResponse.body,
      pause_status: pausedStatus.body,
      resume_after_pause: resumeAfterPause.body,
      quarantine_response: quarantineResponse.body,
      quarantine_status: quarantinedStatus.body,
      resume_after_quarantine: resumeAfterQuarantine.body,
      final_status: finalSafetyStatus.body,
    };
    Object.assign(summary.checks, {
      pause_succeeded: pauseResponse.status === 200,
      pause_visible_in_status: String(pausedLevel).includes('Pause'),
      resume_after_pause_succeeded: resumeAfterPause.status === 200,
      pause_cleared_after_resume: !resumedStatus.body?.per_agent?.[agentUi.agentId],
      quarantine_succeeded: quarantineResponse.status === 200,
      quarantine_visible_in_status: String(quarantinedLevel).includes('Quarantine'),
      resume_after_quarantine_succeeded: resumeAfterQuarantine.status === 200,
      quarantine_cleared_after_resume: !finalSafetyStatus.body?.per_agent?.[agentUi.agentId],
    });

    summary.checks.ws_kill_switch_pause = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"KillSwitchActivation"') &&
        frame.payload.includes('"level":"PAUSE"') &&
        frame.payload.includes(agentUi.agentId),
      options.timeoutMs,
    );
    summary.checks.ws_kill_switch_quarantine = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"KillSwitchActivation"') &&
        frame.payload.includes('"level":"QUARANTINE"') &&
        frame.payload.includes(agentUi.agentId),
      options.timeoutMs,
    );

    const seededGoals = await seedPendingGoals(
      dbPath,
      agentUi.agentId,
      blockingSessionId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    summary.goals.seeded = seededGoals;

    await page.goto('/goals', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Goals' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.waitForFunction(
      ({ approveOperation, rejectOperation }) =>
        Array.from(document.querySelectorAll('.proposal-row')).every((node) => node.textContent) &&
        Array.from(document.querySelectorAll('.proposal-row')).some((node) =>
          (node.textContent ?? '').includes(approveOperation),
        ) &&
        Array.from(document.querySelectorAll('.proposal-row')).some((node) =>
          (node.textContent ?? '').includes(rejectOperation),
        ),
      {
        approveOperation: seededGoals.approveOperation,
        rejectOperation: seededGoals.rejectOperation,
      },
      { timeout: options.timeoutMs },
    );

    const approveRow = page.locator('.proposal-row', {
      hasText: seededGoals.approveOperation,
    }).first();
    await approveRow.getByRole('button', { name: 'Approve' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      (operation) =>
        !Array.from(document.querySelectorAll('.proposal-row')).some((node) =>
          (node.textContent ?? '').includes(operation),
        ),
      seededGoals.approveOperation,
      { timeout: options.timeoutMs },
    );

    const rejectRow = page.locator('.proposal-row', {
      hasText: seededGoals.rejectOperation,
    }).first();
    await rejectRow.getByRole('button', { name: 'Reject' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      (operation) =>
        !Array.from(document.querySelectorAll('.proposal-row')).some((node) =>
          (node.textContent ?? '').includes(operation),
        ),
      seededGoals.rejectOperation,
      { timeout: options.timeoutMs },
    );

    const approvedGoal = await fetchJson(`${gatewayUrl}/api/goals/${seededGoals.approveProposalId}`, {
      headers: authHeaders(accessToken),
    });
    const rejectedGoal = await fetchJson(`${gatewayUrl}/api/goals/${seededGoals.rejectProposalId}`, {
      headers: authHeaders(accessToken),
    });
    summary.goals.approved = approvedGoal.body;
    summary.goals.rejected = rejectedGoal.body;
    Object.assign(summary.checks, {
      goal_approve_succeeded: approvedGoal.body?.decision === 'approved',
      goal_reject_succeeded: rejectedGoal.body?.decision === 'rejected',
    });
    summary.checks.ws_proposal_decision = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"ProposalDecision"') &&
        frame.payload.includes(agentUi.agentId),
      options.timeoutMs,
    );

    await page.goto('/agents', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    summary.pages.agents = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      card_visible: await page.locator('.agent-card', { hasText: agentName }).count() > 0,
    };

    await page.goto('/sessions', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    summary.pages.sessions = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      session_visible: await page.locator('tbody tr', { hasText: blockingSessionId.slice(0, 8) }).count() > 0,
    };

    await page.goto('/costs', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    summary.pages.costs = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      agent_visible: await page.locator('.cost-card', { hasText: agentName }).count() > 0,
    };

    await page.goto('/security', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Security' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.security = {
      loaded_without_error: await page.locator('.error-state').count() === 0,
      audit_entries_visible: await page.locator('.timeline-entry').count() > 0,
      normal_level_visible: await page.locator('.kill-level').innerText(),
    };

    await page.goto('/settings/profiles', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    summary.pages.profiles = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      profile_visible: await page.locator('.profile-btn', { hasText: profileName }).count() > 0,
    };

    summary.checks.agents_page_loaded = summary.pages.agents.loaded_without_error;
    summary.checks.agents_page_shows_agent = summary.pages.agents.card_visible;
    summary.checks.sessions_page_loaded = summary.pages.sessions.loaded_without_error;
    summary.checks.sessions_page_shows_session = summary.pages.sessions.session_visible;
    summary.checks.costs_page_loaded = summary.pages.costs.loaded_without_error;
    summary.checks.costs_page_shows_agent = summary.pages.costs.agent_visible;
    summary.checks.security_page_loaded = summary.pages.security.loaded_without_error;
    summary.checks.security_page_has_audit_entries = summary.pages.security.audit_entries_visible;
    summary.checks.profiles_page_loaded = summary.pages.profiles.loaded_without_error;
    summary.checks.profiles_page_shows_profile = summary.pages.profiles.profile_visible;

    const authSession = await fetchJson(`${gatewayUrl}/api/auth/session`, {
      headers: authHeaders(accessToken),
    });
    summary.auth_session = authSession.body;
    Object.assign(summary.checks, {
      auth_session_still_valid: authSession.status === 200 && authSession.body?.authenticated === true,
    });

    const keepBrowserArtifacts = options.keepArtifacts;
    await persistBrowserArtifacts(
      runDir,
      'runtime',
      { context, page },
      keepBrowserArtifacts,
    );
    await browser.close();
    browser = null;

    await writeJsonLines(path.join(runDir, 'browser-console.jsonl'), browserEvents.console);
    await writeJsonLines(path.join(runDir, 'browser-page-errors.jsonl'), browserEvents.pageErrors);
    await writeJsonLines(path.join(runDir, 'browser-requests.jsonl'), browserEvents.requests);
    await writeJsonLines(path.join(runDir, 'browser-responses.jsonl'), browserEvents.responses);
    await writeJsonLines(
      path.join(runDir, 'browser-request-failures.jsonl'),
      browserEvents.requestFailures,
    );
    await writeJsonLines(path.join(runDir, 'browser-ws-frames.jsonl'), browserEvents.wsFrames);

    summary.checks.page_errors = browserEvents.pageErrors.length === 0;
    if (browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-console.jsonl.');
    }

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);
    if (failedChecks.length > 0) {
      throw new Error(`Live runtime audit failed checks: ${failedChecks.join(', ')}`);
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);

    if (context && page) {
      const keepBrowserArtifacts = true;
      await persistBrowserArtifacts(runDir, 'runtime', { context, page }, keepBrowserArtifacts)
        .catch(() => {});
    }
  } finally {
    if (browser) {
      await browser.close().catch(() => {});
    }
    if (dashboardProcess) {
      await dashboardProcess.stop();
    }
    if (gatewayProcess) {
      await gatewayProcess.stop();
    }

    if (browserEvents) {
      await writeJsonLines(path.join(runDir, 'browser-console.jsonl'), browserEvents.console).catch(() => {});
      await writeJsonLines(path.join(runDir, 'browser-page-errors.jsonl'), browserEvents.pageErrors).catch(() => {});
      await writeJsonLines(path.join(runDir, 'browser-requests.jsonl'), browserEvents.requests).catch(() => {});
      await writeJsonLines(path.join(runDir, 'browser-responses.jsonl'), browserEvents.responses).catch(() => {});
      await writeJsonLines(
        path.join(runDir, 'browser-request-failures.jsonl'),
        browserEvents.requestFailures,
      ).catch(() => {});
      await writeJsonLines(path.join(runDir, 'browser-ws-frames.jsonl'), browserEvents.wsFrames).catch(() => {});
    }

    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await fs.rm(runDir, { recursive: true, force: true });
    process.stdout.write(`Live runtime audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live runtime audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
