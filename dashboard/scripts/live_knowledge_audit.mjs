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
  runLoggedCaptureCommand,
  runLoggedCommand,
  timestampLabel,
  waitForHttp,
  writeJson,
  writeJsonLines,
} from './lib/live_harness.mjs';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JWT_SECRET = 'ghost-knowledge-live-jwt-secret';
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
  process.stdout.write(`Live knowledge audit

Usage:
  pnpm audit:knowledge-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                            [--timeout-ms 45000]

What it does:
  1. Boots a fresh auth-enabled gateway and dashboard
  2. Writes real memories through /api/memory and verifies memory search/graph/archive flows
  3. Seeds a runtime session and verifies bookmarks, branching, unified search, audit, and admin export
  4. Runs CLI export and migration smoke checks against the built ghost binary
  5. Opens the real dashboard pages for memory, search, sessions, ITP, and security
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

async function fetchText(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  return {
    ok: response.ok,
    status: response.status,
    text,
    headers: Object.fromEntries(response.headers.entries()),
  };
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
    tags: [marker, 'knowledge-live', sharedTag, label],
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
    content: `${marker} seeded session follow up`,
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
  20,
  x'${secondHash}',
  x'${firstHash}',
  ${sqlString(secondAttributes)}
);
COMMIT;
`;

  await runSqlite(dbPath, sql, repoRoot, env, logPath);
  return {
    session_id: sessionId,
    sender,
    seeded_event_ids: [`${sessionId}-evt-1`, `${sessionId}-evt-2`],
  };
}

async function waitForPageWithoutError(page, timeoutMs) {
  await page.locator('.page-title, h1').first().waitFor({ state: 'visible', timeout: timeoutMs });
  return (await page.locator('.error-state').count()) === 0;
}

async function buildExportFixture(tempDir, marker) {
  const exportPath = path.join(tempDir, 'chatgpt-export.json');
  const fixture = JSON.stringify([
    {
      id: 'conv-1',
      mapping: {
        'node-1': {
          message: {
            author: { role: 'user' },
            content: { parts: [`${marker} hello`] },
            create_time: 1_700_000_000,
          },
        },
        'node-2': {
          message: {
            author: { role: 'assistant' },
            content: { parts: [`${marker} response`] },
            create_time: 1_700_000_060,
          },
        },
      },
    },
  ]);
  await fs.writeFile(exportPath, fixture);
  return exportPath;
}

async function buildMigrationFixture(tempDir, marker) {
  const sourceDir = path.join(tempDir, 'openclaw-source');
  await fs.mkdir(path.join(sourceDir, 'memories'), { recursive: true });
  await fs.mkdir(path.join(sourceDir, 'skills'), { recursive: true });

  await fs.writeFile(
    path.join(sourceDir, 'SOUL.md'),
    `# Migrated Soul\n\n${marker}\n\n<!-- AGENT-MUTABLE -->\nremove me\n<!-- /AGENT-MUTABLE -->\n`,
  );
  await fs.writeFile(
    path.join(sourceDir, 'memories', 'customer-note.md'),
    `${marker} imported memory`,
  );
  await fs.writeFile(
    path.join(sourceDir, 'skills', 'unsigned-skill.md'),
    `# Unsigned Skill\n\n${marker}\n`,
  );
  await fs.writeFile(
    path.join(sourceDir, 'config.yml'),
    'agent:\n  name: migrated-knowledge-agent\n',
  );

  return sourceDir;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-knowledge-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const tempHome = path.join(tempDir, 'home');

  await fs.mkdir(tempHome, { recursive: true });

  const gatewayPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const configPath = path.join(tempDir, 'ghost-live.yml');
  const dbPath = path.join(tempDir, 'ghost-live.db');
  const marker = `knowledge-live-${runLabel.toLowerCase()}`;
  const sharedTag = `shared-${runLabel.toLowerCase()}`;
  const actorId = `${marker}-actor`;
  const memoryAlphaId = `${marker}-alpha`;
  const memoryBetaId = `${marker}-beta`;
  const seededSessionId = `${marker}-session`;
  const seededSender = `${marker}-agent`;

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_url: gatewayUrl,
    dashboard_url: dashboardUrl,
    artifact_dir: runDir,
    marker,
    memory: {},
    sessions: {},
    search: {},
    audit: {},
    cli: {},
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
    const cliLogPath = path.join(runDir, 'cli.log');
    const gatewayBinary = await ensureGatewayBinary(repoRoot, buildLogPath);

    const gatewayEnv = {
      ...process.env,
      HOME: tempHome,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_export=warn,ghost_migrate=warn',
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
      headers: mutationHeaders(accessToken, `knowledge-memory-alpha-${runLabel}`),
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
      headers: mutationHeaders(accessToken, `knowledge-memory-beta-${runLabel}`),
      body: JSON.stringify({
        memory_id: memoryBetaId,
        event_type: 'memory_upsert',
        delta: `${marker} beta delta`,
        actor_id: actorId,
        snapshot: betaSnapshot,
      }),
    });

    const memoryList = await fetchJson(`${gatewayUrl}/api/memory?page_size=20`, {
      headers: authHeaders(accessToken),
    });
    const memoryDetail = await fetchJson(`${gatewayUrl}/api/memory/${memoryAlphaId}`, {
      headers: authHeaders(accessToken),
    });
    const memorySearch = await fetchJson(
      `${gatewayUrl}/api/memory/search?q=${encodeURIComponent(marker)}&limit=20`,
      { headers: authHeaders(accessToken) },
    );
    const memoryGraph = await fetchJson(`${gatewayUrl}/api/memory/graph?limit=20`, {
      headers: authHeaders(accessToken),
    });

    const archiveResponse = await fetchJson(`${gatewayUrl}/api/memory/${memoryAlphaId}/archive`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `knowledge-archive-${runLabel}`),
      body: JSON.stringify({
        reason: `${marker} archive`,
        decayed_confidence: 0.12,
        original_confidence: 0.93,
      }),
    });
    const archivedList = await fetchJson(`${gatewayUrl}/api/memory/archived`, {
      headers: authHeaders(accessToken),
    });
    const archivedSearch = await fetchJson(
      `${gatewayUrl}/api/memory/search?q=${encodeURIComponent(marker)}&limit=20`,
      { headers: authHeaders(accessToken) },
    );
    const unarchiveResponse = await fetchJson(
      `${gatewayUrl}/api/memory/${memoryAlphaId}/unarchive`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `knowledge-unarchive-${runLabel}`),
      },
    );
    const unarchivedSearch = await fetchJson(
      `${gatewayUrl}/api/memory/search?q=${encodeURIComponent(marker)}&limit=20`,
      { headers: authHeaders(accessToken) },
    );

    summary.memory = {
      memory_ids: [memoryAlphaId, memoryBetaId],
      write_alpha: alphaWrite.body,
      write_beta: betaWrite.body,
      list: memoryList.body,
      detail: memoryDetail.body,
      search: memorySearch.body,
      graph: memoryGraph.body,
      archive: archiveResponse.body,
      archived: archivedList.body,
      archived_search: archivedSearch.body,
      unarchive: unarchiveResponse.body,
      unarchived_search: unarchivedSearch.body,
    };

    Object.assign(summary.checks, {
      memory_write_alpha_succeeded: alphaWrite.status === 201,
      memory_write_beta_succeeded: betaWrite.status === 201,
      memory_list_contains_written:
        memoryList.status === 200 &&
        (memoryList.body?.memories ?? []).some((entry) => entry.memory_id === memoryAlphaId) &&
        (memoryList.body?.memories ?? []).some((entry) => entry.memory_id === memoryBetaId),
      memory_detail_contains_alpha:
        memoryDetail.status === 200 && memoryDetail.body?.memory_id === memoryAlphaId,
      memory_search_contains_both:
        memorySearch.status === 200 &&
        (memorySearch.body?.results ?? []).some((entry) => entry.memory_id === memoryAlphaId) &&
        (memorySearch.body?.results ?? []).some((entry) => entry.memory_id === memoryBetaId),
      memory_graph_has_edge:
        memoryGraph.status === 200 &&
        (memoryGraph.body?.nodes?.length ?? 0) >= 2 &&
        (memoryGraph.body?.edges?.length ?? 0) >= 1,
      memory_archive_succeeded:
        archiveResponse.status === 200 && archiveResponse.body?.status === 'archived',
      archived_list_contains_alpha:
        archivedList.status === 200 &&
        (archivedList.body?.archived ?? []).some((entry) => entry.memory_id === memoryAlphaId),
      archived_memory_hidden_from_search:
        archivedSearch.status === 200 &&
        !(archivedSearch.body?.results ?? []).some((entry) => entry.memory_id === memoryAlphaId),
      memory_unarchive_succeeded:
        unarchiveResponse.status === 200 && unarchiveResponse.body?.status === 'unarchived',
      unarchived_memory_search_visible:
        unarchivedSearch.status === 200 &&
        (unarchivedSearch.body?.results ?? []).some((entry) => entry.memory_id === memoryAlphaId),
    });

    summary.sessions.seeded = await seedRuntimeSession(
      dbPath,
      seededSessionId,
      seededSender,
      marker,
      repoRoot,
      gatewayEnv,
      sqliteLogPath,
    );

    const sessionEvents = await fetchJson(
      `${gatewayUrl}/api/sessions/${seededSessionId}/events?limit=20`,
      { headers: authHeaders(accessToken) },
    );
    const createBookmark = await fetchJson(
      `${gatewayUrl}/api/sessions/${seededSessionId}/bookmarks`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `knowledge-bookmark-${runLabel}`),
        body: JSON.stringify({
          sequence_number: 1,
          label: `${marker} checkpoint`,
        }),
      },
    );
    const bookmarkList = await fetchJson(
      `${gatewayUrl}/api/sessions/${seededSessionId}/bookmarks`,
      { headers: authHeaders(accessToken) },
    );
    const branchResponse = await fetchJson(
      `${gatewayUrl}/api/sessions/${seededSessionId}/branch`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `knowledge-branch-${runLabel}`),
        body: JSON.stringify({
          from_sequence_number: 1,
        }),
      },
    );
    const createdBookmarkId = createBookmark.body?.bookmark?.id ?? null;
    const branchedSessionId = branchResponse.body?.session?.session_id ?? null;
    const branchEvents = branchedSessionId
      ? await fetchJson(`${gatewayUrl}/api/sessions/${branchedSessionId}/events?limit=20`, {
        headers: authHeaders(accessToken),
      })
      : { status: 0, body: null };

    summary.sessions.events = sessionEvents.body;
    summary.sessions.bookmark = createBookmark.body;
    summary.sessions.bookmarks = bookmarkList.body;
    summary.sessions.branch = branchResponse.body;
    summary.sessions.branch_events = branchEvents.body;

    Object.assign(summary.checks, {
      session_events_visible: sessionEvents.status === 200,
      session_events_nonempty: (sessionEvents.body?.events?.length ?? 0) >= 2,
      session_chain_valid: sessionEvents.body?.chain_valid === true,
      bookmark_create_succeeded:
        createBookmark.status === 201 && typeof createdBookmarkId === 'string',
      bookmark_list_contains_created:
        bookmarkList.status === 200 &&
        (bookmarkList.body?.bookmarks ?? []).some(
          (bookmark) => bookmark.id === createdBookmarkId,
        ),
      branch_session_succeeded:
        branchResponse.status === 201 && typeof branchedSessionId === 'string',
      branch_session_events_visible:
        branchEvents.status === 200 &&
        branchEvents.body?.session_id === branchedSessionId &&
        (branchEvents.body?.events?.length ?? 0) >= 1,
    });

    const searchMemories = await fetchJson(
      `${gatewayUrl}/api/search?q=${encodeURIComponent(marker)}&types=memories&limit=20`,
      { headers: authHeaders(accessToken) },
    );
    const searchSessions = await fetchJson(
      `${gatewayUrl}/api/search?q=${encodeURIComponent(seededSessionId)}&types=sessions&limit=20`,
      { headers: authHeaders(accessToken) },
    );
    const searchAudit = await fetchJson(
      `${gatewayUrl}/api/search?q=${encodeURIComponent(marker)}&types=audit&limit=20`,
      { headers: authHeaders(accessToken) },
    );

    summary.search = {
      memories: searchMemories.body,
      sessions: searchSessions.body,
      audit: searchAudit.body,
    };
    Object.assign(summary.checks, {
      search_memories_returns_marker:
        searchMemories.status === 200 &&
        (searchMemories.body?.results ?? []).some(
          (entry) => entry.result_type === 'memory' && String(entry.snippet ?? '').includes(marker),
        ),
      search_sessions_returns_seeded_session:
        searchSessions.status === 200 &&
        (searchSessions.body?.results ?? []).some(
          (entry) => entry.result_type === 'session' && entry.id === seededSessionId,
        ),
      search_audit_returns_marker:
        searchAudit.status === 200 &&
        (searchAudit.body?.results ?? []).some(
          (entry) => entry.result_type === 'audit' && String(entry.snippet ?? '').includes(marker),
        ),
    });

    const auditQuery = await fetchJson(
      `${gatewayUrl}/api/audit?search=${encodeURIComponent(marker)}&page_size=100`,
      { headers: authHeaders(accessToken) },
    );
    const auditAggregation = await fetchJson(`${gatewayUrl}/api/audit/aggregation`, {
      headers: authHeaders(accessToken) },
    );
    const auditExport = await fetchText(`${gatewayUrl}/api/audit/export?format=jsonl`, {
      headers: authHeaders(accessToken),
    });
    const adminExport = await fetchJson(`${gatewayUrl}/api/admin/export?format=json`, {
      headers: authHeaders(accessToken),
    });

    summary.audit = {
      query: auditQuery.body,
      aggregation: auditAggregation.body,
      export_preview: auditExport.text.split('\n').slice(0, 5),
      admin_export: adminExport.body,
    };

    Object.assign(summary.checks, {
      audit_query_contains_marker:
        auditQuery.status === 200 &&
        (auditQuery.body?.entries ?? []).some((entry) =>
          JSON.stringify(entry).includes(marker),
        ),
      audit_aggregation_nonempty:
        auditAggregation.status === 200 &&
        Number(auditAggregation.body?.total_entries ?? 0) > 0,
      audit_export_jsonl_nonempty:
        auditExport.status === 200 && auditExport.text.includes('memory_write'),
      admin_export_json_includes_memories:
        adminExport.status === 200 &&
        Array.isArray(adminExport.body?.entities) &&
        adminExport.body.entities.some((entity) =>
          entity.entity_type === 'memories' && JSON.stringify(entity.data).includes(marker),
        ),
    });

    const exportFixturePath = await buildExportFixture(tempDir, marker);
    const exportResult = await runLoggedCaptureCommand(gatewayBinary, ['export', exportFixturePath], {
      cwd: repoRoot,
      env: gatewayEnv,
      logPath: cliLogPath,
    });

    const migrateSourceDir = await buildMigrationFixture(tempDir, marker);
    const migrateTargetDir = path.join(tempDir, 'migrate-target');
    const migrateResult = await runLoggedCaptureCommand(
      gatewayBinary,
      ['migrate', '--source', migrateSourceDir],
      {
        cwd: repoRoot,
        env: {
          ...gatewayEnv,
          GHOST_DIR: migrateTargetDir,
        },
        logPath: cliLogPath,
      },
    );

    const migratedSoul = await fs.readFile(path.join(migrateTargetDir, 'SOUL.md'), 'utf8');
    const migratedMemory = await fs.readFile(
      path.join(migrateTargetDir, 'memories', 'customer-note.md'),
      'utf8',
    );
    const quarantinedSkill = await fs.readFile(
      path.join(migrateTargetDir, 'skills_quarantine', 'unsigned-skill.md'),
      'utf8',
    );
    const migratedConfig = await fs.readFile(path.join(migrateTargetDir, 'ghost.yml'), 'utf8');

    summary.cli = {
      export_stdout: exportResult.stdout,
      migrate_stdout: migrateResult.stdout,
      migrated_files: {
        soul: migratedSoul,
        memory: migratedMemory,
        quarantined_skill: quarantinedSkill,
        config: migratedConfig,
      },
    };

    Object.assign(summary.checks, {
      cli_export_succeeded: exportResult.stdout.includes('Export Analysis Results'),
      cli_export_detected_chatgpt:
        exportResult.stdout.includes('Format:     ChatGPT') &&
        exportResult.stdout.includes('Messages:   2'),
      cli_migrate_succeeded: migrateResult.stdout.includes('Migration Complete'),
      cli_migrate_imported_soul:
        migratedSoul.includes('# Migrated Soul') && !migratedSoul.includes('remove me'),
      cli_migrate_imported_memory:
        migratedMemory.includes('importance: Low') && migratedMemory.includes(marker),
      cli_migrate_quarantined_skill: quarantinedSkill.includes('# Unsigned Skill'),
      cli_migrate_imported_config: migratedConfig.includes('gateway:'),
    });

    await page.goto('/memory', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Memory' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.getByLabel('Search memories').fill(marker, { timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Search' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      (expected) => document.body.textContent?.includes(expected),
      marker,
      { timeout: options.timeoutMs },
    );
    summary.pages.memory = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      marker_visible: await page.locator('.memory-card', { hasText: marker }).count() > 0,
    };

    await page.goto(`/search?q=${encodeURIComponent(marker)}`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Search' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    await page.waitForFunction(
      (expected) => document.body.textContent?.includes(expected),
      marker,
      { timeout: options.timeoutMs },
    );
    summary.pages.search = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      marker_visible: await page.locator('.result-item', { hasText: marker }).count() > 0,
    };

    const memorySearchResult = page.locator('.result-group', { hasText: 'Memories' }).locator('.result-link').first();
    if (await memorySearchResult.count()) {
      await memorySearchResult.click({ timeout: options.timeoutMs });
      await page.getByRole('heading', { name: 'Memory' }).waitFor({
        state: 'visible',
        timeout: options.timeoutMs,
      });
      summary.pages.search.memory_click = {
        landed: page.url().includes('/memory'),
        focused_marker_visible: await page.locator('.memory-card.focused', { hasText: marker }).count() > 0,
      };
    } else {
      summary.pages.search.memory_click = {
        landed: false,
        focused_marker_visible: false,
      };
    }

    await page.goto(`/search?q=${encodeURIComponent(marker)}`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    const auditSearchResult = page.locator('.result-group', { hasText: 'Audit Log' }).locator('.result-link').first();
    if (await auditSearchResult.count()) {
      await auditSearchResult.click({ timeout: options.timeoutMs });
      await page.getByRole('heading', { name: 'Security' }).waitFor({
        state: 'visible',
        timeout: options.timeoutMs,
      });
      summary.pages.search.audit_click = {
        landed: page.url().includes('/security'),
        focused_marker_visible: await page.locator('.timeline-entry.focused', { hasText: marker }).count() > 0,
      };
    } else {
      summary.pages.search.audit_click = {
        landed: false,
        focused_marker_visible: false,
      };
    }

    await page.goto('/sessions', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    summary.pages.sessions = {
      loaded_without_error: await waitForPageWithoutError(page, options.timeoutMs),
      session_visible:
        await page.locator('tbody tr', { hasText: seededSessionId.slice(0, 8) }).count() > 0,
    };

    await page.goto('/itp', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'ITP Events' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.itp = {
      loaded_without_error: (await page.locator('.error-banner').count()) === 0,
      session_visible:
        await page.locator('.event-row', { hasText: seededSessionId.slice(0, 10) }).count() > 0,
    };

    await page.goto('/security', { waitUntil: 'networkidle', timeout: options.timeoutMs });
    await page.getByRole('heading', { name: 'Security' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    summary.pages.security = {
      loaded_without_error: (await page.locator('.error-state').count()) === 0,
      audit_entries_visible: (await page.locator('.timeline-entry').count()) > 0,
    };

    Object.assign(summary.checks, {
      memory_page_loaded: summary.pages.memory.loaded_without_error,
      memory_page_shows_marker: summary.pages.memory.marker_visible,
      search_page_loaded: summary.pages.search.loaded_without_error,
      search_page_shows_marker: summary.pages.search.marker_visible,
      search_page_memory_click_lands: summary.pages.search.memory_click.landed,
      search_page_memory_click_focuses_result:
        summary.pages.search.memory_click.focused_marker_visible,
      search_page_audit_click_lands: summary.pages.search.audit_click.landed,
      search_page_audit_click_focuses_result:
        summary.pages.search.audit_click.focused_marker_visible,
      sessions_page_loaded: summary.pages.sessions.loaded_without_error,
      sessions_page_shows_seeded_session: summary.pages.sessions.session_visible,
      itp_page_loaded: summary.pages.itp.loaded_without_error,
      itp_page_shows_session: summary.pages.itp.session_visible,
      security_page_loaded: summary.pages.security.loaded_without_error,
      security_page_has_audit_entries: summary.pages.security.audit_entries_visible,
    });

    await persistBrowserArtifacts(runDir, 'knowledge', { context, page }, options.keepArtifacts);
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
      throw new Error(`Live knowledge audit failed checks: ${failedChecks.join(', ')}`);
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);

    if (context && page) {
      await persistBrowserArtifacts(runDir, 'knowledge', { context, page }, true).catch(() => {});
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
    process.stdout.write(`Live knowledge audit passed
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live knowledge audit ${summary.status}
Gateway: ${gatewayUrl}
Dashboard: ${dashboardUrl}
Artifacts: ${runDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
