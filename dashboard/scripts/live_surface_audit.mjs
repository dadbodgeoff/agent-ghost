#!/usr/bin/env node

import { chromium } from '@playwright/test';
import { createHash, randomUUID } from 'node:crypto';
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
  runLoggedCommand,
  timestampLabel,
  waitForHttp,
  writeJson,
} from './lib/live_harness.mjs';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JWT_SECRET = 'ghost-surface-live-jwt-secret';
const DASHBOARD_CLIENT_NAME = 'dashboard';
const DASHBOARD_CLIENT_VERSION = '0.1.0';
const ZERO_HASH_HEX = '0'.repeat(64);

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
  process.stdout.write(`Live surface audit

Usage:
  pnpm audit:surface-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                          [--timeout-ms 45000]

What it does:
  1. Boots a fresh auth-enabled gateway and dashboard
  2. Verifies the last uncovered gateway families: openapi, profiles, workflows
  3. Seeds enough runtime state for remaining detail pages and link flows
  4. Smokes the leftover dashboard surfaces, including detail pages and settings edges
  5. Preserves logs/traces/screenshots only when the run fails
`);
}

function authHeaders(accessToken, extra = {}) {
  return {
    Authorization: `Bearer ${accessToken}`,
    'x-ghost-client-name': DASHBOARD_CLIENT_NAME,
    'x-ghost-client-version': DASHBOARD_CLIENT_VERSION,
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

function requireCheck(summary, key, condition, message) {
  summary.checks[key] = Boolean(condition);
  if (!condition) {
    throw new Error(message);
  }
}

function pageHasNoError(page) {
  return page
    .locator('.error-state, .error-banner, .error-msg')
    .count()
    .then((count) => count === 0);
}

function buildMemorySnapshot({ marker, label, sharedTag }) {
  return JSON.stringify({
    id: randomUUID(),
    memory_type: 'Semantic',
    content: `${marker} ${label} content`,
    summary: `${marker} ${label} summary`,
    importance: 'High',
    confidence: 0.93,
    created_at: nowIso(),
    last_accessed: null,
    access_count: 0,
    tags: [marker, 'surface-live', sharedTag, label],
    archived: false,
  });
}

async function seedRuntimeSession(dbPath, sessionId, sender, marker, repoRoot, env, logPath) {
  const firstHash = sha256Hex(`${sessionId}:1:${marker}`);
  const secondHash = sha256Hex(`${sessionId}:2:${marker}`);
  const firstTimestamp = nowIso();
  const secondTimestamp = new Date(Date.now() + 1_000).toISOString();
  const firstAttributes = JSON.stringify({
    role: 'user',
    content: `${marker} seeded session start`,
  });
  const secondAttributes = JSON.stringify({
    role: 'assistant',
    content: `${marker} seeded session reply`,
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
  5,
  12,
  x'${firstHash}',
  x'${ZERO_HASH_HEX}',
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
  9,
  18,
  x'${secondHash}',
  x'${firstHash}',
  ${sqlString(secondAttributes)}
);
COMMIT;`;

  await runSqlite(dbPath, sql, repoRoot, env, logPath);
  return {
    session_id: sessionId,
    seeded_event_ids: [`${sessionId}-evt-1`, `${sessionId}-evt-2`],
  };
}

function goalSeedPayload(agentId, sessionId, suffix) {
  const proposalId = `surface-goal-${suffix}`;
  const lineageId = `surface-lineage-${suffix}`;
  const subjectKey = `surface-subject-${suffix}`;
  const reviewedRevision = `surface-rev-${suffix}`;
  const createdAt = nowIso();
  const operation = `surface-approve-${suffix}`;
  const content = {
    title: 'Surface approval',
    subject_key: subjectKey,
    lineage_id: lineageId,
    reviewed_revision: reviewedRevision,
    requested_action: 'approve',
  };

  return {
    agentId,
    sessionId,
    proposalId,
    lineageId,
    subjectKey,
    reviewedRevision,
    createdAt,
    operation,
    content,
    eventHash: sha256Hex(`surface-goal:${proposalId}`),
    previousHash: sha256Hex(`surface-goal-prev:${proposalId}`),
  };
}

async function seedPendingGoal(dbPath, agentId, sessionId, repoRoot, env, logPath) {
  const goal = goalSeedPayload(
    agentId,
    sessionId,
    timestampLabel().replace('-', '').toLowerCase(),
  );

  const sql = `
BEGIN;
INSERT INTO goal_proposals (
  id, agent_id, session_id, proposer_type, operation, target_type,
  content, cited_memory_ids, decision, resolved_at, resolver, flags,
  dimension_scores, denial_reason, event_hash, previous_hash, created_at
) VALUES (
  ${sqlString(goal.proposalId)},
  ${sqlString(goal.agentId)},
  ${sqlString(goal.sessionId)},
  'agent',
  ${sqlString(goal.operation)},
  'task',
  ${sqlString(JSON.stringify(goal.content))},
  '[]',
  NULL,
  NULL,
  NULL,
  '["surface_live"]',
  '{"alignment":0.93,"safety":0.97}',
  NULL,
  x'${goal.eventHash}',
  x'${goal.previousHash}',
  ${sqlString(goal.createdAt)}
);
INSERT INTO goal_proposals_v2 (
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
  ${sqlString(goal.agentId)},
  ${sqlString(goal.agentId)},
  ${sqlString(goal.sessionId)},
  ${sqlString(goal.operation)},
  'task',
  ${sqlString(JSON.stringify(goal.content))},
  '[]',
  'HumanReviewRequired',
  '["surface_live"]',
  '{"alignment":0.93,"safety":0.97}',
  NULL,
  NULL,
  NULL,
  NULL,
  ${sqlString(goal.createdAt)},
  x'${goal.eventHash}',
  x'${goal.previousHash}'
);
INSERT INTO goal_proposal_transitions (
  id, proposal_id, lineage_id, from_state, to_state, actor_type, actor_id,
  reason_code, rationale, expected_state, expected_revision, operation_id,
  request_id, idempotency_key, created_at
) VALUES (
  ${sqlString(`surface-transition-${goal.proposalId}`)},
  ${sqlString(goal.proposalId)},
  ${sqlString(goal.lineageId)},
  NULL,
  'pending_review',
  'system',
  NULL,
  'surface_live_seed',
  NULL,
  NULL,
  ${sqlString(goal.reviewedRevision)},
  NULL,
  NULL,
  NULL,
  ${sqlString(goal.createdAt)}
);
INSERT INTO goal_lineage_heads (
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
);
COMMIT;`;

  await runSqlite(dbPath, sql, repoRoot, env, logPath);
  return goal;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-surface-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const homeDir = path.join(tempDir, 'home');

  await fs.mkdir(homeDir, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const configPath = path.join(tempDir, 'ghost.yml');
  const dbPath = path.join(tempDir, 'ghost.db');
  const marker = `surface-live-${runLabel.toLowerCase()}`;
  const agentName = `${marker}-agent`;
  const workflowName = `${marker}-workflow`;
  const sharedTag = `${marker}-shared`;
  const actorId = `surface-actor-${runLabel.toLowerCase()}`;
  const memoryAlphaId = `surface-memory-a-${runLabel.toLowerCase()}`;
  const memoryBetaId = `surface-memory-b-${runLabel.toLowerCase()}`;
  const sessionId = `surface-session-${runLabel.toLowerCase()}`;

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    artifact_dir: runDir,
    marker,
    api: {},
    seed: {},
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
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info',
    };

    const configText = buildTempConfig(baseConfigText, gatewayPort, dbPath);
    await fs.writeFile(configPath, configText);

    await runLoggedCommand(gatewayBinary, ['-c', configPath, 'db', 'migrate'], {
      cwd: repoRoot,
      env: gatewayEnv,
      logPath: gatewayMigrateLog,
    });

    gatewayProcess = createLoggedProcess(gatewayBinary, ['-c', configPath, 'serve'], {
      cwd: repoRoot,
      env: gatewayEnv,
      logPath: gatewayLog,
    });
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
    summary.checks.gateway_ready = true;

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

    const openapi = await fetchJson(`${gatewayUrl}/api/openapi.json`);
    summary.api.openapi = openapi.body;
    requireCheck(
      summary,
      'openapi_spec_public',
      openapi.status === 200 &&
        typeof openapi.body?.openapi === 'string' &&
        typeof openapi.body?.paths?.['/api/workflows'] === 'object',
      'OpenAPI spec was not available on the public route',
    );

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

    const profiles = await fetchJson(`${gatewayUrl}/api/profiles`, {
      headers: authHeaders(accessToken),
    });
    summary.api.profiles = profiles.body;
    requireCheck(
      summary,
      'profiles_route_lists_presets',
      profiles.status === 200 &&
        (profiles.body?.profiles ?? []).some((profile) => profile.name === 'standard'),
      'Profiles route did not return the preset convergence profiles',
    );

    const createAgent = await fetchJson(`${gatewayUrl}/api/agents`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-create-agent`),
      body: JSON.stringify({
        name: agentName,
        spending_cap: 5,
        capabilities: ['read'],
        generate_keypair: false,
      }),
    });
    const agentId = createAgent.body?.id ?? null;
    summary.seed.agent = createAgent.body;
    requireCheck(
      summary,
      'surface_agent_created',
      createAgent.status === 201 && typeof agentId === 'string',
      'Surface audit could not create an agent for detail-page verification',
    );

    const alphaSnapshot = buildMemorySnapshot({
      marker,
      label: 'alpha',
      sharedTag,
    });
    const betaSnapshot = buildMemorySnapshot({
      marker,
      label: 'beta',
      sharedTag,
    });
    const alphaWrite = await fetchJson(`${gatewayUrl}/api/memory`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-memory-alpha`),
      body: JSON.stringify({
        memory_id: memoryAlphaId,
        event_type: 'memory_upsert',
        delta: `${marker} alpha delta`,
        actor_id: actorId,
        snapshot: alphaSnapshot,
      }),
    });
    const betaWrite = await fetchJson(`${gatewayUrl}/api/memory`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-memory-beta`),
      body: JSON.stringify({
        memory_id: memoryBetaId,
        event_type: 'memory_upsert',
        delta: `${marker} beta delta`,
        actor_id: actorId,
        snapshot: betaSnapshot,
      }),
    });
    const memoryGraph = await fetchJson(`${gatewayUrl}/api/memory/graph?limit=20`, {
      headers: authHeaders(accessToken),
    });
    summary.seed.memory = {
      alpha: alphaWrite.body,
      beta: betaWrite.body,
      graph: memoryGraph.body,
    };
    requireCheck(
      summary,
      'surface_memory_graph_seeded',
      alphaWrite.status === 201 &&
        betaWrite.status === 201 &&
        (memoryGraph.body?.nodes?.length ?? 0) >= 2 &&
        (memoryGraph.body?.edges?.length ?? 0) >= 1,
      'Memory graph did not contain seeded nodes and edges',
    );

    summary.seed.session = await seedRuntimeSession(
      dbPath,
      sessionId,
      agentName,
      marker,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    const seededGoal = await seedPendingGoal(
      dbPath,
      agentId,
      sessionId,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    summary.seed.goal = seededGoal;

    const createWorkflow = await fetchJson(`${gatewayUrl}/api/workflows`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-workflow-create`),
      body: JSON.stringify({
        name: workflowName,
        description: 'Surface suite workflow',
        nodes: [{ id: 'n1', type: 'transform', template: 'surface output' }],
        edges: [],
      }),
    });
    const workflowId = createWorkflow.body?.id ?? null;
    const listWorkflows = await fetchJson(`${gatewayUrl}/api/workflows?page_size=20`, {
      headers: authHeaders(accessToken),
    });
    const workflowDetail = workflowId
      ? await fetchJson(`${gatewayUrl}/api/workflows/${workflowId}`, {
        headers: authHeaders(accessToken),
      })
      : { status: 0, body: null };
    const executeWorkflow = workflowId
      ? await fetchJson(`${gatewayUrl}/api/workflows/${workflowId}/execute`, {
        method: 'POST',
        headers: mutationHeaders(accessToken, `${marker}-workflow-execute`),
        body: JSON.stringify({ input: { marker } }),
      })
      : { status: 0, body: null };
    const workflowExecutions = workflowId
      ? await fetchJson(`${gatewayUrl}/api/workflows/${workflowId}/executions`, {
        headers: authHeaders(accessToken),
      })
      : { status: 0, body: null };
    summary.api.workflows = {
      create: createWorkflow.body,
      list: listWorkflows.body,
      detail: workflowDetail.body,
      execute: executeWorkflow.body,
      executions: workflowExecutions.body,
    };
    requireCheck(
      summary,
      'workflow_route_create_execute_list',
      createWorkflow.status === 201 &&
        typeof workflowId === 'string' &&
        listWorkflows.status === 200 &&
        (listWorkflows.body?.workflows ?? []).some((workflow) => workflow.id === workflowId) &&
        workflowDetail.status === 200 &&
        workflowDetail.body?.id === workflowId &&
        executeWorkflow.status === 200 &&
        typeof executeWorkflow.body?.execution_id === 'string' &&
        workflowExecutions.status === 200 &&
        (workflowExecutions.body?.executions ?? []).some(
          (execution) => execution.execution_id === executeWorkflow.body?.execution_id,
        ),
      'Workflow routes did not complete the create/list/detail/execute flow',
    );

    await page.goto('/agents', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Agents' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    const agentCard = page.locator('.agent-card', { hasText: agentName }).first();
    const agentHref = await agentCard.getAttribute('href');
    summary.pages.agents = {
      loaded_without_error: await pageHasNoError(page),
      link_href: agentHref,
    };
    requireCheck(
      summary,
      'agents_list_links_to_detail',
      agentHref === `/agents/${agentId}`,
      'Agents list card href did not point to the real agent detail route',
    );

    await page.goto(`/agents/${agentId}`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.detail-header h1').waitFor({ state: 'visible', timeout: options.timeoutMs });
    summary.pages.agent_detail = {
      loaded_without_error: await pageHasNoError(page),
      agent_name: (await page.locator('.detail-header h1').textContent())?.trim() ?? '',
    };
    requireCheck(
      summary,
      'agent_detail_page_loaded',
      summary.pages.agent_detail.loaded_without_error &&
        summary.pages.agent_detail.agent_name === agentName,
      'Agent detail page did not load the seeded agent',
    );

    await page.goto('/goals', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Goals' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.waitForFunction(
      (expectedOperation) =>
        Array.from(document.querySelectorAll('.proposal-row')).some((node) =>
          (node.textContent ?? '').includes(expectedOperation),
        ),
      seededGoal.operation,
      { timeout: options.timeoutMs },
    );
    const goalLink = page.locator('.proposal-row', { hasText: seededGoal.operation }).first().locator('.detail-link');
    const goalHref = await goalLink.getAttribute('href');
    summary.pages.goals = {
      loaded_without_error: await pageHasNoError(page),
      link_href: goalHref,
    };
    requireCheck(
      summary,
      'goals_list_links_to_detail',
      goalHref === `/goals/${seededGoal.proposalId}`,
      'Goals list detail link did not point to the seeded proposal detail route',
    );

    await page.goto(`/goals/${seededGoal.proposalId}`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.detail-header h1').waitFor({ state: 'visible', timeout: options.timeoutMs });
    summary.pages.goal_detail = {
      loaded_without_error: await pageHasNoError(page),
      heading: (await page.locator('.detail-header h1').textContent())?.trim() ?? '',
    };
    requireCheck(
      summary,
      'goal_detail_page_loaded',
      summary.pages.goal_detail.loaded_without_error &&
        summary.pages.goal_detail.heading.includes(seededGoal.proposalId.slice(0, 8)),
      'Goal detail page did not load the seeded proposal',
    );

    await page.goto('/approvals', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.waitForURL((url) => url.pathname === '/goals', { timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Goals' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.approvals = {
      redirected_path: new URL(page.url()).pathname,
      loaded_without_error: await pageHasNoError(page),
      proposal_visible:
        (await page.locator('.proposal-card, .proposal-row', { hasText: seededGoal.operation }).count()) > 0,
    };
    requireCheck(
      summary,
      'approvals_alias_redirects_to_goals',
      summary.pages.approvals.redirected_path === '/goals' &&
        summary.pages.approvals.loaded_without_error &&
        summary.pages.approvals.proposal_visible,
      'Approvals alias route did not redirect into the goals surface correctly',
    );

    await page.goto('/sessions', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Sessions' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    const sessionLink = page.locator('.session-link', { hasText: sessionId.slice(0, 8) }).first();
    const sessionHref = await sessionLink.getAttribute('href');
    summary.pages.sessions = {
      loaded_without_error: await pageHasNoError(page),
      link_href: sessionHref,
    };
    requireCheck(
      summary,
      'sessions_list_links_to_detail',
      sessionHref === `/sessions/${sessionId}`,
      'Sessions list link did not point to the seeded session detail route',
    );

    await page.goto(`/sessions/${sessionId}`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.detail-header h1').waitFor({ state: 'visible', timeout: options.timeoutMs });
    const replayHref = await page.locator('.replay-link').getAttribute('href');
    summary.pages.session_detail = {
      loaded_without_error: await pageHasNoError(page),
      replay_href: replayHref,
    };
    requireCheck(
      summary,
      'session_detail_links_to_replay',
      replayHref === `/sessions/${sessionId}/replay`,
      'Session detail replay link did not point to the replay route for the seeded session',
    );

    await page.goto(`/sessions/${sessionId}/replay`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.replay-header h1').waitFor({ state: 'visible', timeout: options.timeoutMs });
    const backHref = await page.locator('.replay-header .back-link').getAttribute('href');
    summary.pages.session_replay = {
      loaded_without_error: await pageHasNoError(page),
      back_href: backHref,
    };
    requireCheck(
      summary,
      'session_replay_links_back_to_detail',
      backHref === `/sessions/${sessionId}`,
      'Session replay back-link did not point to the seeded session detail route',
    );

    await page.goto('/memory/graph', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.page-title', { hasText: 'Knowledge Graph' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.memory_graph = {
      loaded_without_error: await pageHasNoError(page),
      graph_rendered: (await page.locator('.graph-svg').count()) > 0,
      legend_text: (await page.locator('.legend-count').textContent())?.trim() ?? '',
    };
    requireCheck(
      summary,
      'memory_graph_page_loaded',
      summary.pages.memory_graph.loaded_without_error &&
        summary.pages.memory_graph.graph_rendered &&
        summary.pages.memory_graph.legend_text.includes('nodes'),
      'Memory graph page did not render the seeded graph surface',
    );

    await page.goto('/observability/ade', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'ADE Self-Observability' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.observability_ade = {
      loaded_without_error: await pageHasNoError(page),
      component_rows: await page.locator('tbody tr').count(),
    };
    requireCheck(
      summary,
      'observability_ade_page_loaded',
      summary.pages.observability_ade.loaded_without_error &&
        summary.pages.observability_ade.component_rows >= 1,
      'ADE self-observability page did not render health rows',
    );

    await page.goto('/settings/channels', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Channels' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.settings_channels = {
      loaded_without_error: await pageHasNoError(page),
      agent_visible: (await page.locator('tbody tr', { hasText: agentName }).count()) > 0,
    };
    requireCheck(
      summary,
      'settings_channels_page_loaded',
      summary.pages.settings_channels.loaded_without_error &&
        summary.pages.settings_channels.agent_visible,
      'Settings channels page did not render the created agent channel row',
    );

    await page.goto('/settings/oauth', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'OAuth Connections' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.settings_oauth = {
      loaded_without_error: await pageHasNoError(page),
      empty_state_visible:
        (await page.locator('.empty-state', { hasText: 'No OAuth providers configured.' }).count()) > 0,
    };
    requireCheck(
      summary,
      'settings_oauth_page_loaded',
      summary.pages.settings_oauth.loaded_without_error &&
        summary.pages.settings_oauth.empty_state_visible,
      'Settings OAuth page did not render its configured empty state',
    );

    await page.goto('/settings/policies', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Safety Policies' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.settings_policies = {
      loaded_without_error: await pageHasNoError(page),
      policy_cards: await page.locator('.policy-card').count(),
    };
    requireCheck(
      summary,
      'settings_policies_page_loaded',
      summary.pages.settings_policies.loaded_without_error &&
        summary.pages.settings_policies.policy_cards >= 1,
      'Settings policies page did not render the policy cards',
    );

    await page.goto('/studio/sandbox', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Simulation Sandbox' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.studio_sandbox = {
      loaded_without_error: await pageHasNoError(page),
      run_button_disabled:
        await page.getByRole('button', { name: 'Run Simulation' }).isDisabled(),
    };
    requireCheck(
      summary,
      'studio_sandbox_page_loaded',
      summary.pages.studio_sandbox.loaded_without_error &&
        summary.pages.studio_sandbox.run_button_disabled,
      'Studio sandbox page did not render in the expected empty state',
    );

    await page.goto('/workflows', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.locator('.wf-sidebar h2').waitFor({ state: 'visible', timeout: options.timeoutMs });
    summary.pages.workflows = {
      loaded_without_error: await pageHasNoError(page),
      workflow_visible: (await page.locator('.wf-item', { hasText: workflowName }).count()) > 0,
    };
    requireCheck(
      summary,
      'workflows_page_loaded',
      summary.pages.workflows.loaded_without_error &&
        summary.pages.workflows.workflow_visible,
      'Workflows page did not render the created workflow in the list',
    );

    summary.checks.browser_page_errors_empty = browserEvents.pageErrors.length === 0;
    if (browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-events.json.');
    }

    summary.status = 'passed';
    await writeJson(path.join(runDir, 'browser-events.json'), browserEvents);
    await persistBrowserArtifacts(runDir, 'surface', { context, page }, options.keepArtifacts);
    await writeJson(path.join(runDir, 'summary.json'), summary);

    process.stdout.write(`Artifacts: ${runDir}\n`);
    process.stdout.write(
      `Summary: surface live audit passed with ${Object.values(summary.checks).filter(Boolean).length}/${Object.keys(summary.checks).length} checks\n`,
    );
  } catch (error) {
    summary.status = 'failed';
    summary.failed_at = nowIso();
    summary.error = error instanceof Error ? error.message : String(error);

    if (browserEvents) {
      await writeJson(path.join(runDir, 'browser-events.json'), browserEvents).catch(() => {});
    }
    if (context && page) {
      await persistBrowserArtifacts(runDir, 'surface', { context, page }, true).catch(() => {});
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
