#!/usr/bin/env node

import { chromium } from '@playwright/test';
import http from 'node:http';
import { randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
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

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JWT_SECRET = 'ghost-io-live-jwt-secret';
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
  process.stdout.write(`Live I/O audit

Usage:
  pnpm audit:io-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                     [--timeout-ms 45000]

What it does:
  1. Boots a fresh auth-enabled gateway and dashboard
  2. Exercises real skills, provider key, channel, webhook, push, and OAuth list surfaces
  3. Seeds external skill catalog rows to verify quarantine/resolve/reverify behavior
  4. Uses a local webhook receiver to validate delivery and signatures
  5. Opens the real dashboard pages for skills, channels, providers, webhooks, OAuth, and notifications
  6. Preserves logs/traces/screenshots only when the run fails
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

function explicitMutationHeaders(accessToken, operationId, idempotencyKey, extra = {}) {
  return jsonHeaders(accessToken, {
    'x-request-id': randomUUID(),
    'x-ghost-operation-id': operationId,
    'idempotency-key': idempotencyKey,
    'x-ghost-client-name': DASHBOARD_CLIENT_NAME,
    'x-ghost-client-version': DASHBOARD_CLIENT_VERSION,
    ...extra,
  });
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function buildIoTempConfig(
  baseConfigText,
  gatewayPort,
  dbPath,
  managedStoragePath,
  oauthProviderConfig = null,
) {
  const config = buildTempConfig(baseConfigText, gatewayPort, dbPath);
  const normalizedManagedPath = managedStoragePath.replace(/\\/g, '/');
  const oauthBlock = oauthProviderConfig
    ? `

oauth:
  providers:
    - name: "${oauthProviderConfig.name}"
      client_id: "${oauthProviderConfig.clientId}"
      client_secret_env: "${oauthProviderConfig.clientSecretEnv}"
      auth_url: "${oauthProviderConfig.authUrl}"
      token_url: "${oauthProviderConfig.tokenUrl}"
      revoke_url: "${oauthProviderConfig.revokeUrl}"
`
    : '';

  const externalSkillsBlock = /^\s*external_skills:\s*$/m.test(config)
    ? ''
    : `

external_skills:
  enabled: true
  execution_enabled: true
  managed_storage_path: "${normalizedManagedPath}"`;

  return `${config}${externalSkillsBlock}${oauthBlock}`;
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

async function waitForWsFrame(browserEvents, predicate, timeoutMs) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (browserEvents.wsFrames.some(predicate)) {
      return true;
    }
    await delay(200);
  }
  return false;
}

async function waitForPageWithoutError(page, timeoutMs) {
  await page.locator('.page-title, h1').first().waitFor({ state: 'visible', timeout: timeoutMs });
  return (await page.locator('.error-state, .error-banner[role="alert"]').count()) === 0;
}

async function createWebhookReceiver(port) {
  const deliveries = [];
  const server = http.createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) {
      chunks.push(Buffer.from(chunk));
    }
    const body = Buffer.concat(chunks).toString('utf8');
    deliveries.push({
      captured_at: nowIso(),
      method: request.method ?? 'GET',
      url: request.url ?? '/',
      headers: request.headers,
      body,
    });
    response.statusCode = 204;
    response.end();
  });

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve());
  });

  return {
    deliveries,
    server,
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
    },
  };
}

async function createOAuthMockServer(port) {
  const captures = {
    authorize: [],
    token: [],
    revoke: [],
    resource: [],
  };

  const server = http.createServer(async (request, response) => {
    const baseUrl = `http://127.0.0.1:${port}`;
    const requestUrl = new URL(request.url ?? '/', baseUrl);
    const chunks = [];
    for await (const chunk of request) {
      chunks.push(Buffer.from(chunk));
    }
    const rawBody = Buffer.concat(chunks).toString('utf8');

    if (requestUrl.pathname === '/authorize') {
      const redirectUri = requestUrl.searchParams.get('redirect_uri') ?? '';
      const state = requestUrl.searchParams.get('state') ?? '';
      const code = `oauth-live-${randomUUID()}`;
      captures.authorize.push({
        captured_at: nowIso(),
        method: request.method ?? 'GET',
        url: requestUrl.toString(),
        state,
        redirect_uri: redirectUri,
      });

      const callbackUrl = new URL(redirectUri);
      callbackUrl.searchParams.set('code', code);
      callbackUrl.searchParams.set('state', state);
      response.statusCode = 302;
      response.setHeader('location', callbackUrl.toString());
      response.end();
      return;
    }

    if (requestUrl.pathname === '/token') {
      const params = new URLSearchParams(rawBody);
      const grantType = params.get('grant_type') ?? '';
      captures.token.push({
        captured_at: nowIso(),
        method: request.method ?? 'POST',
        url: requestUrl.toString(),
        grant_type: grantType,
        code: params.get('code'),
        refresh_token: params.get('refresh_token'),
      });

      const payload =
        grantType === 'refresh_token'
          ? {
              access_token: `refreshed-${randomUUID()}`,
              refresh_token: `refresh-${randomUUID()}`,
              expires_in: 3600,
              scope: 'profile.read',
              token_type: 'Bearer',
            }
          : {
              access_token: `access-${randomUUID()}`,
              refresh_token: `refresh-${randomUUID()}`,
              expires_in: 3600,
              scope: 'profile.read',
              token_type: 'Bearer',
            };

      response.statusCode = 200;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify(payload));
      return;
    }

    if (requestUrl.pathname === '/revoke') {
      const params = new URLSearchParams(rawBody);
      captures.revoke.push({
        captured_at: nowIso(),
        method: request.method ?? 'POST',
        url: requestUrl.toString(),
        token: params.get('token'),
      });
      response.statusCode = 200;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({ revoked: true }));
      return;
    }

    if (requestUrl.pathname === '/resource') {
      const authorization = String(request.headers.authorization ?? '');
      captures.resource.push({
        captured_at: nowIso(),
        method: request.method ?? 'GET',
        url: requestUrl.toString(),
        authorization,
        suite_marker: request.headers['x-suite-marker'] ?? null,
      });
      response.statusCode = authorization.startsWith('Bearer ') ? 200 : 401;
      response.setHeader('content-type', 'application/json');
      response.end(
        JSON.stringify({
          ok: authorization.startsWith('Bearer '),
          method: request.method ?? 'GET',
          authorization,
        }),
      );
      return;
    }

    response.statusCode = 404;
    response.end('not found');
  });

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve());
  });

  return {
    captures,
    server,
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
    },
  };
}

async function seedExternalSkill(dbPath, seed, repoRoot, env, logPath) {
  const installSql = seed.install_state
    ? `
INSERT INTO external_skill_install_state (
  artifact_digest, skill_name, skill_version, state, updated_by
) VALUES (
  ${sqlString(seed.digest)},
  ${sqlString(seed.name)},
  '1.0.0',
  ${sqlString(seed.install_state)},
  'io-live'
);`
    : '';

  const sql = `
BEGIN;
INSERT INTO external_skill_artifacts (
  artifact_digest,
  artifact_schema_version,
  skill_name,
  skill_version,
  publisher,
  description,
  source_kind,
  execution_mode,
  entrypoint,
  source_uri,
  managed_artifact_path,
  managed_entrypoint_path,
  manifest_json,
  requested_capabilities,
  declared_privileges,
  signer_key_id,
  artifact_size_bytes
) VALUES (
  ${sqlString(seed.digest)},
  1,
  ${sqlString(seed.name)},
  '1.0.0',
  'ghost-live',
  ${sqlString(seed.description ?? 'live external skill')},
  'workspace',
  'wasm',
  'module.wasm',
  ${sqlString(`/source/${seed.name}.ghostskill`)},
  ${sqlString(seed.managed_artifact_path)},
  ${sqlString(seed.managed_entrypoint_path ?? seed.managed_artifact_path)},
  '{}',
  '[]',
  '["Pure WASM computation"]',
  'key-live',
  256
);
INSERT INTO external_skill_verifications (
  artifact_digest, status, signer_key_id, signer_publisher, details_json
) VALUES (
  ${sqlString(seed.digest)},
  ${sqlString(seed.verification_status ?? 'verified')},
  'key-live',
  'ghost-live',
  '{}'
);
INSERT INTO external_skill_quarantine (
  artifact_digest, state, reason_code, reason_detail, updated_by
) VALUES (
  ${sqlString(seed.digest)},
  ${sqlString(seed.quarantine_state ?? 'clear')},
  ${seed.reason_code ? sqlString(seed.reason_code) : 'NULL'},
  ${seed.reason_detail ? sqlString(seed.reason_detail) : 'NULL'},
  'io-live'
);
${installSql}
COMMIT;
`;

  await runSqlite(dbPath, sql, repoRoot, env, logPath);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-io-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const tempHome = path.join(tempDir, 'home');

  await fs.mkdir(tempHome, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const webhookPort = await getFreePort();
  const oauthMockPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const webhookUrl = `http://127.0.0.1:${webhookPort}/hook`;
  const oauthMockUrl = `http://127.0.0.1:${oauthMockPort}`;
  const configPath = path.join(tempDir, 'ghost-live.yml');
  const dbPath = path.join(tempDir, 'ghost-live.db');
  const managedSkillsPath = path.join(tempDir, 'managed-skills');
  const marker = `io-live-${runLabel.toLowerCase()}`;
  const agentName = `${marker}-agent`;
  const agentSessionId = randomUUID();
  const oauthProviderName = 'local-mock';
  const oauthClientSecretEnv = 'GHOST_IO_LIVE_OAUTH_CLIENT_SECRET';

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    webhook_url: webhookUrl,
    oauth_mock_url: oauthMockUrl,
    artifact_dir: runDir,
    marker,
    agent: {},
    skills: {},
    channels: {},
    provider_keys: {},
    webhooks: {},
    push: {},
    oauth: {},
    pages: {},
    checks: {},
    warnings: [],
    status: 'running',
  };

  let gatewayProcess = null;
  let dashboardProcess = null;
  let webhookReceiver = null;
  let oauthMockServer = null;
  let browser = null;
  let context = null;
  let page = null;
  let browserEvents = null;

  try {
    const baseConfigPath = path.join(repoRoot, 'ghost.yml');
    const baseConfigText = await fs.readFile(baseConfigPath, 'utf8');
    await fs.writeFile(
      configPath,
      buildIoTempConfig(baseConfigText, gatewayPort, dbPath, managedSkillsPath, {
        name: oauthProviderName,
        clientId: 'ghost-io-live-client',
        clientSecretEnv: oauthClientSecretEnv,
        authUrl: `${oauthMockUrl}/authorize`,
        tokenUrl: `${oauthMockUrl}/token`,
        revokeUrl: `${oauthMockUrl}/revoke`,
      }),
    );

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
      [oauthClientSecretEnv]: 'ghost-io-live-client-secret',
      GHOST_WEBHOOK_ALLOWED_HOSTS: '127.0.0.1,localhost',
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info',
    };
    delete gatewayEnv.XAI_API_KEY;
    delete gatewayEnv.OPENAI_API_KEY;
    delete gatewayEnv.ANTHROPIC_API_KEY;
    delete gatewayEnv.GEMINI_API_KEY;

    oauthMockServer = await createOAuthMockServer(oauthMockPort);

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

    webhookReceiver = await createWebhookReceiver(webhookPort);

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

    const createAgent = await fetchJson(`${gatewayUrl}/api/agents`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-agent-create`),
      body: JSON.stringify({
        name: agentName,
        skills: ['note_take', 'json_transform'],
      }),
    });
    const agentId = createAgent.body?.id ?? null;
    const agentList = await fetchJson(`${gatewayUrl}/api/agents`, {
      headers: authHeaders(accessToken),
    });

    summary.agent = {
      create: createAgent.body,
      list: agentList.body,
    };

    Object.assign(summary.checks, {
      agent_create_succeeded:
        createAgent.status === 201 && typeof agentId === 'string' && agentId.length > 0,
      agent_list_contains_created:
        agentList.status === 200 &&
        Array.isArray(agentList.body) &&
        agentList.body.some((agent) => agent.id === agentId && agent.name === agentName),
    });

    const skillsList = await fetchJson(`${gatewayUrl}/api/skills`, {
      headers: authHeaders(accessToken),
    });
    const installedSkills = skillsList.body?.installed ?? [];
    const availableSkills = skillsList.body?.available ?? [];

    const jsonTransformExecute = await fetchJson(`${gatewayUrl}/api/skills/json_transform/execute`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-skills-json-transform`),
      body: JSON.stringify({
        agent_id: agentId,
        session_id: agentSessionId,
        input: {
          action: 'get',
          data: {
            alpha: {
              beta: marker,
            },
          },
          path: 'alpha.beta',
        },
      }),
    });

    const uninstallOpId = randomUUID();
    const uninstallKey = `${marker}-skills-uninstall-note-take`;
    const uninstallNoteTake = await fetchJson(`${gatewayUrl}/api/skills/note_take/uninstall`, {
      method: 'POST',
      headers: explicitMutationHeaders(accessToken, uninstallOpId, uninstallKey),
    });
    const afterUninstallList = await fetchJson(`${gatewayUrl}/api/skills`, {
      headers: authHeaders(accessToken),
    });

    const installOpId = randomUUID();
    const installKey = `${marker}-skills-install-note-take`;
    const installNoteTake = await fetchJson(`${gatewayUrl}/api/skills/note_take/install`, {
      method: 'POST',
      headers: explicitMutationHeaders(accessToken, installOpId, installKey),
    });

    const noteExecuteOperationId = randomUUID();
    const noteExecuteKey = `${marker}-skills-note-create`;
    const noteRequestBody = {
      agent_id: agentId,
      session_id: agentSessionId,
      input: {
        action: 'create',
        title: `${marker} note`,
        content: `${marker} content`,
      },
    };
    const executeNoteTake = await fetchJson(`${gatewayUrl}/api/skills/note_take/execute`, {
      method: 'POST',
      headers: explicitMutationHeaders(accessToken, noteExecuteOperationId, noteExecuteKey),
      body: JSON.stringify(noteRequestBody),
    });
    const replayNoteTake = await fetchJson(`${gatewayUrl}/api/skills/note_take/execute`, {
      method: 'POST',
      headers: explicitMutationHeaders(accessToken, noteExecuteOperationId, noteExecuteKey),
      body: JSON.stringify(noteRequestBody),
    });
    const noteCount = await querySqliteScalar(
      dbPath,
      `SELECT COUNT(*) FROM agent_notes WHERE agent_id = ${sqlString(agentId)};`,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );

    const manualExternalDigest = `${marker}-manual-external`;
    await seedExternalSkill(
      dbPath,
      {
        digest: manualExternalDigest,
        name: `${marker}_manual_echo`,
        managed_artifact_path: path.join(tempDir, 'missing-manual.ghostskill'),
      },
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );

    const externalBeforeQuarantine = await fetchJson(`${gatewayUrl}/api/skills`, {
      headers: authHeaders(accessToken),
    });
    const quarantineExternal = await fetchJson(
      `${gatewayUrl}/api/skills/${encodeURIComponent(manualExternalDigest)}/quarantine`,
      {
        method: 'POST',
        headers: explicitMutationHeaders(
          accessToken,
          randomUUID(),
          `${marker}-skills-quarantine-external`,
        ),
        body: JSON.stringify({
          reason: `${marker} manual review`,
        }),
      },
    );
    const resolveExternal = await fetchJson(
      `${gatewayUrl}/api/skills/${encodeURIComponent(manualExternalDigest)}/quarantine/resolve`,
      {
        method: 'POST',
        headers: explicitMutationHeaders(
          accessToken,
          randomUUID(),
          `${marker}-skills-resolve-external`,
        ),
        body: JSON.stringify({
          expected_quarantine_revision: quarantineExternal.body?.quarantine_revision ?? -1,
        }),
      },
    );

    const reverifyExternalDigest = `${marker}-reverify-external`;
    await seedExternalSkill(
      dbPath,
      {
        digest: reverifyExternalDigest,
        name: `${marker}_reverify_echo`,
        install_state: 'installed',
        managed_artifact_path: path.join(tempDir, 'missing-reverify.ghostskill'),
      },
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );
    const reverifyExternal = await fetchJson(
      `${gatewayUrl}/api/skills/${encodeURIComponent(reverifyExternalDigest)}/reverify`,
      {
        method: 'POST',
        headers: explicitMutationHeaders(
          accessToken,
          randomUUID(),
          `${marker}-skills-reverify-external`,
        ),
      },
    );

    const wsSkillChangeSeen = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"SkillChange"') &&
        frame.payload.includes('note_take'),
      options.timeoutMs,
    );

    summary.skills = {
      initial_list: skillsList.body,
      json_transform_execute: jsonTransformExecute.body,
      uninstall_note_take: uninstallNoteTake.body,
      after_uninstall_list: afterUninstallList.body,
      install_note_take: installNoteTake.body,
      execute_note_take: executeNoteTake.body,
      replay_note_take: replayNoteTake.body,
      external_before_quarantine: externalBeforeQuarantine.body,
      quarantine_external: quarantineExternal.body,
      resolve_external: resolveExternal.body,
      reverify_external: reverifyExternal.body,
      note_count: noteCount,
    };

    Object.assign(summary.checks, {
      skills_list_loaded:
        skillsList.status === 200 &&
        installedSkills.some((skill) => skill.name === 'note_take') &&
        installedSkills.some((skill) => skill.name === 'json_transform'),
      json_transform_executes:
        jsonTransformExecute.status === 200 &&
        jsonTransformExecute.body?.skill === 'json_transform' &&
        jsonTransformExecute.body?.result?.value === marker,
      note_take_uninstall_succeeded:
        uninstallNoteTake.status === 200 && uninstallNoteTake.body?.state === 'available',
      note_take_visible_as_available_after_uninstall:
        afterUninstallList.status === 200 &&
        (afterUninstallList.body?.available ?? []).some((skill) => skill.name === 'note_take'),
      note_take_install_succeeded:
        installNoteTake.status === 200 && installNoteTake.body?.state === 'installed',
      note_take_executes_transactionally:
        executeNoteTake.status === 200 &&
        executeNoteTake.body?.skill === 'note_take' &&
        executeNoteTake.body?.result?.status === 'created',
      note_take_replay_dedupes_side_effect:
        replayNoteTake.status === 200 &&
        executeNoteTake.body?.result?.note_id === replayNoteTake.body?.result?.note_id &&
        replayNoteTake.headers['x-ghost-idempotency-status'] === 'replayed' &&
        noteCount === '1',
      external_skill_seed_visible:
        externalBeforeQuarantine.status === 200 &&
        (externalBeforeQuarantine.body?.available ?? []).some(
          (skill) => skill.id === manualExternalDigest,
        ),
      external_skill_quarantine_succeeds:
        quarantineExternal.status === 200 &&
        quarantineExternal.body?.quarantine_state === 'quarantined',
      external_skill_resolve_succeeds:
        resolveExternal.status === 200 &&
        resolveExternal.body?.quarantine_state === 'clear',
      external_skill_reverify_fails_closed:
        reverifyExternal.status === 200 &&
        reverifyExternal.body?.verification_status === 'validation_failed' &&
        reverifyExternal.body?.quarantine_state === 'quarantined',
      skill_change_ws_seen: wsSkillChangeSeen,
    });

    const initialChannels = await fetchJson(`${gatewayUrl}/api/channels`, {
      headers: authHeaders(accessToken),
    });
    const createChannel = await fetchJson(`${gatewayUrl}/api/channels`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-channel-create`),
      body: JSON.stringify({
        channel_type: 'cli',
        agent_id: agentId,
      }),
    });
    const createdChannelId = createChannel.body?.id ?? null;
    const channelsAfterCreate = await fetchJson(`${gatewayUrl}/api/channels`, {
      headers: authHeaders(accessToken),
    });
    const reconnectChannel = await fetchJson(
      `${gatewayUrl}/api/channels/${encodeURIComponent(createdChannelId)}/reconnect`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `${marker}-channel-reconnect`),
        body: JSON.stringify({}),
      },
    );
    const injectChannel = await fetchJson(`${gatewayUrl}/api/channels/cli/inject`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-channel-inject`),
      body: JSON.stringify({
        content: `${marker} channel message`,
        sender: 'io-live',
        agent_id: agentId,
      }),
    });
    const wsChannelInjectSeen = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"AgentStateChange"') &&
        frame.payload.includes(String(agentId)) &&
        frame.payload.includes('channel_inject:cli'),
      options.timeoutMs,
    );

    await page.goto('/channels', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Channels' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.channels = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      cli_visible: (await page.locator('.channel-card', { hasText: 'cli' }).count()) > 0,
    };

    const deleteChannel = await fetchJson(
      `${gatewayUrl}/api/channels/${encodeURIComponent(createdChannelId)}`,
      {
        method: 'DELETE',
        headers: mutationHeaders(accessToken, `${marker}-channel-delete`),
      },
    );
    const channelsAfterDelete = await fetchJson(`${gatewayUrl}/api/channels`, {
      headers: authHeaders(accessToken),
    });

    summary.channels = {
      initial_list: initialChannels.body,
      create: createChannel.body,
      after_create: channelsAfterCreate.body,
      reconnect: reconnectChannel.body,
      inject: injectChannel.body,
      delete: deleteChannel.body,
      after_delete: channelsAfterDelete.body,
    };

    Object.assign(summary.checks, {
      channels_list_loaded: initialChannels.status === 200,
      channel_create_succeeded:
        createChannel.status === 201 && typeof createdChannelId === 'string',
      channel_list_contains_created:
        channelsAfterCreate.status === 200 &&
        (channelsAfterCreate.body?.channels ?? []).some((channel) => channel.id === createdChannelId),
      channel_reconnect_succeeded:
        reconnectChannel.status === 200 && reconnectChannel.body?.status === 'reconnected',
      channel_inject_succeeded:
        injectChannel.status === 202 &&
        injectChannel.body?.routed === true &&
        injectChannel.body?.agent_id === agentId,
      channel_inject_ws_seen: wsChannelInjectSeen,
      channel_delete_succeeded:
        deleteChannel.status === 200 && deleteChannel.body?.status === 'deleted',
      channel_deleted_from_list:
        channelsAfterDelete.status === 200 &&
        !(channelsAfterDelete.body?.channels ?? []).some((channel) => channel.id === createdChannelId),
    });

    const providerKeysBefore = await fetchJson(`${gatewayUrl}/api/admin/provider-keys`, {
      headers: authHeaders(accessToken),
    });
    const configurableProvider = (providerKeysBefore.body?.providers ?? []).find(
      (provider) => provider.env_name && provider.provider_name !== 'ollama',
    );
    const providerEnvName = configurableProvider?.env_name ?? null;
    const providerValue = `${marker}-api-key-1234`;
    const setProviderKey = providerEnvName
      ? await fetchJson(`${gatewayUrl}/api/admin/provider-keys`, {
        method: 'PUT',
        headers: mutationHeaders(accessToken, `${marker}-provider-key-set`),
        body: JSON.stringify({
          env_name: providerEnvName,
          value: providerValue,
        }),
      })
      : { status: 0, body: null };
    const providerKeysAfterSet = await fetchJson(`${gatewayUrl}/api/admin/provider-keys`, {
      headers: authHeaders(accessToken),
    });

    await page.goto('/settings/providers', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'LLM Providers' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.providers = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      provider_visible: providerEnvName
        ? (await page.locator('.provider-row', { hasText: providerEnvName }).count()) > 0
        : (await page.locator('.empty-state').count()) > 0,
    };

    const deleteProviderKey = providerEnvName
      ? await fetchJson(
        `${gatewayUrl}/api/admin/provider-keys/${encodeURIComponent(providerEnvName)}`,
        {
          method: 'DELETE',
          headers: mutationHeaders(accessToken, `${marker}-provider-key-delete`),
        },
      )
      : { status: 0, body: null };
    const providerKeysAfterDelete = await fetchJson(`${gatewayUrl}/api/admin/provider-keys`, {
      headers: authHeaders(accessToken),
    });

    summary.provider_keys = {
      before: providerKeysBefore.body,
      set: setProviderKey.body,
      after_set: providerKeysAfterSet.body,
      delete: deleteProviderKey.body,
      after_delete: providerKeysAfterDelete.body,
    };

    Object.assign(summary.checks, {
      provider_keys_list_loaded: providerKeysBefore.status === 200,
      provider_key_target_found: typeof providerEnvName === 'string' && providerEnvName.length > 0,
      provider_key_set_succeeded:
        setProviderKey.status === 200 &&
        setProviderKey.body?.env_name === providerEnvName &&
        String(setProviderKey.body?.preview ?? '').startsWith(providerValue.slice(0, 4)),
      provider_key_preview_visible_after_set:
        providerKeysAfterSet.status === 200 &&
        (providerKeysAfterSet.body?.providers ?? []).some(
          (provider) => provider.env_name === providerEnvName && provider.is_set === true,
        ),
      provider_key_delete_succeeded:
        deleteProviderKey.status === 200 && deleteProviderKey.body?.env_name === providerEnvName,
      provider_key_cleared_after_delete:
        providerKeysAfterDelete.status === 200 &&
        (providerKeysAfterDelete.body?.providers ?? []).some(
          (provider) => provider.env_name === providerEnvName && provider.is_set === false,
        ),
      providers_page_loaded: summary.pages.providers.loaded_without_error,
      providers_page_shows_provider: summary.pages.providers.provider_visible,
    });

    const webhooksInitial = await fetchJson(`${gatewayUrl}/api/webhooks`, {
      headers: authHeaders(accessToken),
    });
    const createWebhook = await fetchJson(`${gatewayUrl}/api/webhooks`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-webhook-create`),
      body: JSON.stringify({
        name: `${marker} webhook`,
        url: webhookUrl,
        secret: `${marker}-secret`,
        events: ['backup_complete'],
        headers: {
          'x-suite-marker': marker,
        },
      }),
    });
    const webhookId = createWebhook.body?.id ?? null;
    const webhooksAfterCreate = await fetchJson(`${gatewayUrl}/api/webhooks`, {
      headers: authHeaders(accessToken),
    });
    const testWebhook = await fetchJson(
      `${gatewayUrl}/api/webhooks/${encodeURIComponent(webhookId)}/test`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `${marker}-webhook-test`),
        body: JSON.stringify({}),
      },
    );
    const wsWebhookSeen = await waitForWsFrame(
      browserEvents,
      (frame) =>
        typeof frame.payload === 'string' &&
        frame.payload.includes('"type":"WebhookFired"') &&
        frame.payload.includes(String(webhookId)),
      options.timeoutMs,
    );
    await delay(500);
    const updateWebhook = await fetchJson(
      `${gatewayUrl}/api/webhooks/${encodeURIComponent(webhookId)}`,
      {
        method: 'PUT',
        headers: mutationHeaders(accessToken, `${marker}-webhook-update`),
        body: JSON.stringify({
          active: false,
        }),
      },
    );

    await page.goto('/settings/webhooks', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Webhooks' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.webhooks = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      webhook_visible:
        (await page.locator('.webhook-row', { hasText: `${marker} webhook` }).count()) > 0,
    };

    const deleteWebhook = await fetchJson(
      `${gatewayUrl}/api/webhooks/${encodeURIComponent(webhookId)}`,
      {
        method: 'DELETE',
        headers: mutationHeaders(accessToken, `${marker}-webhook-delete`),
      },
    );
    const webhooksAfterDelete = await fetchJson(`${gatewayUrl}/api/webhooks`, {
      headers: authHeaders(accessToken),
    });
    const webhookDelivery = webhookReceiver.deliveries.at(-1) ?? null;
    let webhookBody = null;
    try {
      webhookBody = webhookDelivery?.body ? JSON.parse(webhookDelivery.body) : null;
    } catch {
      webhookBody = null;
    }

    summary.webhooks = {
      initial: webhooksInitial.body,
      create: createWebhook.body,
      after_create: webhooksAfterCreate.body,
      test: testWebhook.body,
      receiver_delivery: webhookDelivery,
      receiver_body: webhookBody,
      update: updateWebhook.body,
      delete: deleteWebhook.body,
      after_delete: webhooksAfterDelete.body,
    };

    Object.assign(summary.checks, {
      webhooks_list_loaded: webhooksInitial.status === 200,
      webhook_create_succeeded:
        createWebhook.status === 201 && typeof webhookId === 'string',
      webhook_list_contains_created:
        webhooksAfterCreate.status === 200 &&
        (webhooksAfterCreate.body?.webhooks ?? []).some((webhook) => webhook.id === webhookId),
      webhook_test_succeeded:
        testWebhook.status === 200 && testWebhook.body?.success === true,
      webhook_receiver_captured_delivery:
        webhookDelivery?.method === 'POST' &&
        webhookDelivery?.headers?.['x-ghost-webhook-signature']?.startsWith('sha256=') === true &&
        webhookDelivery?.headers?.['x-suite-marker'] === marker &&
        webhookBody?.event === 'test',
      webhook_ws_seen: wsWebhookSeen,
      webhook_update_succeeded:
        updateWebhook.status === 200 && updateWebhook.body?.updated === webhookId,
      webhook_delete_succeeded:
        deleteWebhook.status === 200 && deleteWebhook.body?.deleted === webhookId,
      webhook_deleted_from_list:
        webhooksAfterDelete.status === 200 &&
        !(webhooksAfterDelete.body?.webhooks ?? []).some((webhook) => webhook.id === webhookId),
      webhooks_page_loaded: summary.pages.webhooks.loaded_without_error,
      webhooks_page_shows_webhook: summary.pages.webhooks.webhook_visible,
    });

    const pushVapidKey = await fetchJson(`${gatewayUrl}/api/push/vapid-key`, {
      headers: authHeaders(accessToken),
    });
    const pushPayload = {
      endpoint: `https://example.invalid/${marker}`,
      keys: {
        p256dh: 'p256dh',
        auth: 'auth',
      },
    };
    const pushSubscribe = await fetchJson(`${gatewayUrl}/api/push/subscribe`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-push-subscribe`),
      body: JSON.stringify(pushPayload),
    });
    const pushUnsubscribe = await fetchJson(`${gatewayUrl}/api/push/unsubscribe`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `${marker}-push-unsubscribe`),
      body: JSON.stringify(pushPayload),
    });

    summary.push = {
      vapid_key: pushVapidKey.body,
      subscribe_status: pushSubscribe.status,
      unsubscribe_status: pushUnsubscribe.status,
    };
    Object.assign(summary.checks, {
      push_vapid_key_available:
        pushVapidKey.status === 200 && String(pushVapidKey.body?.key ?? '').length > 10,
      push_subscribe_succeeds: pushSubscribe.status === 204,
      push_unsubscribe_succeeds: pushUnsubscribe.status === 204,
    });

    const oauthProviders = await fetchJson(`${gatewayUrl}/api/oauth/providers`, {
      headers: authHeaders(accessToken),
    });
    const oauthConnections = await fetchJson(`${gatewayUrl}/api/oauth/connections`, {
      headers: authHeaders(accessToken),
    });
    const oauthProviderRegistered =
      oauthProviders.status === 200 &&
      (oauthProviders.body ?? []).some((provider) => provider.name === oauthProviderName);
    const initialOAuthPagePath = '/settings/oauth';

    summary.oauth = {
      provider_name: oauthProviderName,
      providers: oauthProviders.body,
      initial_connections: oauthConnections.body,
      mock_captures: oauthMockServer?.captures ?? null,
    };
    Object.assign(summary.checks, {
      oauth_providers_list_loaded: oauthProviders.status === 200,
      oauth_connections_list_loaded: oauthConnections.status === 200,
      oauth_provider_registered: oauthProviderRegistered,
    });

    await page.goto('/skills', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Skills' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.skills = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      note_take_visible:
        (await page.locator('.skill-grid, .skill-card', { hasText: 'note_take' }).count()) > 0 ||
        (await page.locator('body', { hasText: 'note_take' }).count()) > 0,
    };

    await page.goto(initialOAuthPagePath, { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'OAuth Connections' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.oauth = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      provider_visible:
        (await page.locator('.provider-card', { hasText: oauthProviderName }).count()) > 0,
      connect_button_visible:
        (await page.getByRole('button', { name: `Connect ${oauthProviderName}` }).count()) > 0,
      callback_connected: false,
      connection_visible: false,
      connect_button_visible_after_disconnect: false,
    };
    const oauthConnectButton = page.getByRole('button', { name: `Connect ${oauthProviderName}` });
    await oauthConnectButton.waitFor({ state: 'visible', timeout: options.timeoutMs });
    await oauthConnectButton.click();
    await page.waitForURL(/\/api\/oauth\/callback\?/, { timeout: options.timeoutMs });
    await page.waitForLoadState('networkidle', { timeout: options.timeoutMs });
    const oauthCallbackText = (await page.locator('body').textContent())?.trim() ?? '';
    summary.oauth.browser_callback = {
      url: page.url(),
      body: oauthCallbackText,
    };
    summary.pages.oauth.callback_connected =
      oauthCallbackText.includes('"status":"connected"') ||
      oauthCallbackText.includes('"status": "connected"');

    const oauthConnectionsAfterConnect = await fetchJson(`${gatewayUrl}/api/oauth/connections`, {
      headers: authHeaders(accessToken),
    });
    const oauthConnectionsAfterConnectList = asArray(oauthConnectionsAfterConnect.body);
    const oauthConnection = oauthConnectionsAfterConnectList.find(
      (connection) =>
        connection.provider === oauthProviderName && connection.status === 'connected',
    );

    let oauthExecute = { status: 0, body: null };
    let oauthExecuteBody = null;
    if (oauthConnection?.ref_id) {
      oauthExecute = await fetchJson(`${gatewayUrl}/api/oauth/execute`, {
        method: 'POST',
        headers: mutationHeaders(accessToken, `${marker}-oauth-execute`),
        body: JSON.stringify({
          ref_id: oauthConnection.ref_id,
          api_request: {
            method: 'GET',
            url: `${oauthMockUrl}/resource`,
            headers: {
              'x-suite-marker': marker,
            },
            body: null,
          },
        }),
      });
      try {
        oauthExecuteBody = JSON.parse(String(oauthExecute.body?.body ?? 'null'));
      } catch {
        oauthExecuteBody = null;
      }
    }

    await page.goto(initialOAuthPagePath, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'OAuth Connections' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.oauth.connection_visible =
      (await page.locator('.connection-card', { hasText: oauthProviderName }).count()) > 0;

    const oauthDisconnectButton = page.getByRole('button', {
      name: `Disconnect ${oauthProviderName}`,
    });
    if (await oauthDisconnectButton.count()) {
      await oauthDisconnectButton.click();
      await page
        .getByRole('button', { name: `Connect ${oauthProviderName}` })
        .waitFor({ state: 'visible', timeout: options.timeoutMs });
    }

    const oauthConnectionsAfterDisconnect = await fetchJson(`${gatewayUrl}/api/oauth/connections`, {
      headers: authHeaders(accessToken),
    });
    const oauthConnectionsAfterDisconnectList = asArray(oauthConnectionsAfterDisconnect.body);
    summary.pages.oauth.connect_button_visible_after_disconnect =
      (await page.getByRole('button', { name: `Connect ${oauthProviderName}` }).count()) > 0;
    summary.oauth = {
      ...summary.oauth,
      after_connect_connections_status: oauthConnectionsAfterConnect.status,
      after_connect_connections: oauthConnectionsAfterConnect.body,
      execute: oauthExecute.body,
      execute_resource_body: oauthExecuteBody,
      after_disconnect_connections_status: oauthConnectionsAfterDisconnect.status,
      after_disconnect_connections: oauthConnectionsAfterDisconnect.body,
      mock_captures: oauthMockServer?.captures ?? null,
    };

    Object.assign(summary.checks, {
      oauth_page_loaded: summary.pages.oauth.loaded_without_error,
      oauth_page_shows_provider: summary.pages.oauth.provider_visible,
      oauth_connect_button_visible: summary.pages.oauth.connect_button_visible,
      oauth_browser_callback_reports_connected: summary.pages.oauth.callback_connected,
      oauth_connection_created: typeof oauthConnection?.ref_id === 'string',
      oauth_execute_succeeds:
        oauthExecute.status === 200 &&
        oauthExecute.body?.status === 200 &&
        oauthExecuteBody?.ok === true,
      oauth_page_shows_connection: summary.pages.oauth.connection_visible,
      oauth_disconnect_clears_connection:
        oauthConnectionsAfterDisconnect.status === 200 &&
        !oauthConnectionsAfterDisconnectList.some(
          (connection) => connection.provider === oauthProviderName,
        ),
      oauth_page_shows_connect_after_disconnect:
        summary.pages.oauth.connect_button_visible_after_disconnect,
      oauth_authorize_hit_mock: (oauthMockServer?.captures.authorize.length ?? 0) > 0,
      oauth_token_exchange_hit_mock: (oauthMockServer?.captures.token.length ?? 0) > 0,
      oauth_execute_hit_mock_resource:
        (oauthMockServer?.captures.resource ?? []).some(
          (capture) =>
            capture.authorization.startsWith('Bearer ') &&
            capture.suite_marker === marker,
        ),
      oauth_revoke_hit_mock: (oauthMockServer?.captures.revoke.length ?? 0) > 0,
    });

    await page.goto('/settings/notifications', {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Notifications' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.notifications = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      content_visible:
        (await page.locator('.unsupported, .section, .toggle-row').count()) > 0,
    };

    Object.assign(summary.checks, {
      skills_page_loaded: summary.pages.skills.loaded_without_error,
      skills_page_shows_note_take: summary.pages.skills.note_take_visible,
      channels_page_loaded: summary.pages.channels.loaded_without_error,
      channels_page_shows_cli: summary.pages.channels.cli_visible,
      notifications_page_loaded: summary.pages.notifications.loaded_without_error,
      notifications_page_has_content: summary.pages.notifications.content_visible,
    });

    await persistBrowserArtifacts(runDir, 'io', { context, page }, options.keepArtifacts);
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
    await writeJson(path.join(runDir, 'webhook-deliveries.json'), webhookReceiver.deliveries);
    await writeJson(path.join(runDir, 'oauth-mock-captures.json'), oauthMockServer?.captures ?? {});

    summary.checks.page_errors = browserEvents.pageErrors.length === 0;
    if (browserEvents.console.some((event) => event.type === 'error')) {
      summary.warnings.push('Browser console emitted error-level messages. See browser-console.jsonl.');
    }

    const failedChecks = Object.entries(summary.checks)
      .filter(([, value]) => value === false)
      .map(([key]) => key);
    if (failedChecks.length > 0) {
      throw new Error(`Live I/O audit failed checks: ${failedChecks.join(', ')}`);
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);

    if (context && page) {
      await persistBrowserArtifacts(runDir, 'io', { context, page }, true).catch(() => {});
    }
  } finally {
    if (browser) {
      await browser.close().catch(() => {});
    }
    if (webhookReceiver) {
      await webhookReceiver.stop().catch(() => {});
    }
    if (oauthMockServer) {
      await oauthMockServer.stop().catch(() => {});
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
    if (webhookReceiver) {
      await writeJson(path.join(runDir, 'webhook-deliveries.json'), webhookReceiver.deliveries).catch(() => {});
    }
    if (oauthMockServer) {
      await writeJson(path.join(runDir, 'oauth-mock-captures.json'), oauthMockServer.captures).catch(() => {});
    }

    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await fs.rm(runDir, { recursive: true, force: true });
    process.stdout.write(`Live I/O audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live I/O audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
