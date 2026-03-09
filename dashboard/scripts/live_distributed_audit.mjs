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
  runLoggedCommand,
  timestampLabel,
  waitForHttp,
  writeJson,
} from './lib/live_harness.mjs';

const DEFAULT_TIMEOUT_MS = 60_000;
const DEFAULT_JWT_SECRET = 'ghost-live-jwt-secret';
const DASHBOARD_CLIENT_NAME = 'dashboard';
const DASHBOARD_CLIENT_VERSION = '0.1.0';
const LOCAL_CLUSTER_ALLOWED_HOSTS = '127.0.0.1,localhost';

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
  process.stdout.write(`Live distributed audit

Usage:
  pnpm audit:distributed-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                              [--timeout-ms 60000]

What it does:
  1. Boots two real mesh-enabled gateways on isolated temp homes
  2. Verifies mesh identity persistence across a gateway restart
  3. Confirms signed agent card discovery and verified A2A dispatch between gateways
  4. Drives the real /orchestration dashboard page on gateway A
  5. Exercises marketplace agent/skill/contract/review flows through the API
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

function yamlString(value) {
  return JSON.stringify(value);
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

function buildDistributedConfig(baseConfigText, gatewayPort, dbPath, knownAgents) {
  const baseConfig = buildTempConfig(baseConfigText, gatewayPort, dbPath);
  const meshLines = ['mesh:', '  enabled: true'];

  if (knownAgents.length === 0) {
    meshLines.push('  known_agents: []');
  } else {
    meshLines.push('  known_agents:');
    for (const agent of knownAgents) {
      meshLines.push(`    - name: ${yamlString(agent.name)}`);
      meshLines.push(`      endpoint: ${yamlString(agent.endpoint)}`);
      meshLines.push(`      public_key: ${yamlString(agent.public_key)}`);
    }
  }

  return upsertTopLevelSection(baseConfig, 'mesh', meshLines);
}

function knownAgent(name, endpoint, publicKeyBytes) {
  return {
    name,
    endpoint,
    public_key: Buffer.from(publicKeyBytes).toString('base64'),
  };
}

function meshPublicKeyPath(homeDir) {
  return path.join(homeDir, '.ghost', 'agents', 'platform', 'keys', 'agent.pub');
}

async function waitForFile(filePath, timeoutMs) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    try {
      return await fs.readFile(filePath);
    } catch {
      // Retry until timeout.
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`File did not appear within ${timeoutMs}ms: ${filePath}`);
}

async function writeGatewayConfig(configPath, baseConfigText, gatewayPort, dbPath, knownAgents) {
  const configText = buildDistributedConfig(baseConfigText, gatewayPort, dbPath, knownAgents);
  await fs.writeFile(configPath, configText);
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

function cardLooksSigned(card) {
  return (
    Array.isArray(card?.public_key) &&
    card.public_key.length === 32 &&
    Array.isArray(card?.signature) &&
    card.signature.length === 64
  );
}

function requireCheck(summary, key, condition, message) {
  summary.checks[key] = Boolean(condition);
  if (!condition) {
    throw new Error(message);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const runLabel = timestampLabel();
  const runDir = path.join(repoRoot, 'artifacts', 'live-distributed-audits', runLabel);
  const tempDir = path.join(runDir, 'temp');
  const homeA = path.join(tempDir, 'home-a');
  const homeB = path.join(tempDir, 'home-b');

  await fs.mkdir(homeA, { recursive: true });
  await fs.mkdir(homeB, { recursive: true });

  const gatewayAPort = await getFreePort();
  const gatewayBPort = await getFreePort();
  const dashboardPort = await getFreePort();
  const gatewayAUrl = `http://127.0.0.1:${gatewayAPort}`;
  const gatewayBUrl = `http://127.0.0.1:${gatewayBPort}`;
  const dashboardUrl = `http://127.0.0.1:${dashboardPort}`;
  const configAPath = path.join(tempDir, 'ghost-a.yml');
  const configBPath = path.join(tempDir, 'ghost-b.yml');
  const dbAPath = path.join(tempDir, 'ghost-a.db');
  const dbBPath = path.join(tempDir, 'ghost-b.db');

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    gateway_a_url: gatewayAUrl,
    gateway_b_url: gatewayBUrl,
    dashboard_url: dashboardUrl,
    artifact_dir: runDir,
    mesh: {},
    orchestration: {},
    marketplace: {},
    checks: {},
    warnings: [],
    status: 'running',
  };

  let gatewayAProcess = null;
  let gatewayBProcess = null;
  let dashboardProcess = null;
  let browser = null;
  let context = null;
  let page = null;
  let browserEvents = null;

  try {
    const baseConfigText = await fs.readFile(path.join(repoRoot, 'ghost.yml'), 'utf8');
    const gatewayBuildLog = path.join(runDir, 'gateway-build.log');
    const gatewayAMigrateLog = path.join(runDir, 'gateway-a-migrate.log');
    const gatewayABootstrapLog = path.join(runDir, 'gateway-a-bootstrap.log');
    const gatewayAFinalLog = path.join(runDir, 'gateway-a.log');
    const gatewayBMigrateLog = path.join(runDir, 'gateway-b-migrate.log');
    const gatewayBLog = path.join(runDir, 'gateway-b.log');
    const dashboardLogPath = path.join(runDir, 'dashboard.log');
    const gatewayBinary = await ensureGatewayBinary(repoRoot, gatewayBuildLog);

    const gatewayEnvA = {
      ...process.env,
      HOME: homeA,
      GHOST_CORS_ORIGINS: `${dashboardUrl},http://localhost:${dashboardPort}`,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      GHOST_SSRF_ALLOWED_HOSTS: [
        process.env.GHOST_SSRF_ALLOWED_HOSTS,
        LOCAL_CLUSTER_ALLOWED_HOSTS,
      ]
        .filter(Boolean)
        .join(','),
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_mesh=info',
    };
    const gatewayEnvB = {
      ...process.env,
      HOME: homeB,
      GHOST_JWT_SECRET: DEFAULT_JWT_SECRET,
      GHOST_SSRF_ALLOWED_HOSTS: [
        process.env.GHOST_SSRF_ALLOWED_HOSTS,
        LOCAL_CLUSTER_ALLOWED_HOSTS,
      ]
        .filter(Boolean)
        .join(','),
      RUST_LOG: process.env.RUST_LOG ?? 'ghost_gateway=info,ghost_mesh=info',
    };

    await writeGatewayConfig(configAPath, baseConfigText, gatewayAPort, dbAPath, []);
    await runGatewayMigration(gatewayBinary, repoRoot, gatewayEnvA, configAPath, gatewayAMigrateLog);
    gatewayAProcess = await startGatewayInstance(
      gatewayBinary,
      repoRoot,
      gatewayEnvA,
      configAPath,
      gatewayAUrl,
      gatewayABootstrapLog,
    );

    const gatewayAPublicKeyBefore = await waitForFile(
      meshPublicKeyPath(homeA),
      options.timeoutMs,
    );
    summary.mesh.gateway_a_public_key_before_restart = Buffer.from(gatewayAPublicKeyBefore).toString(
      'base64',
    );

    const bootstrapCardA = await fetchJson(`${gatewayAUrl}/.well-known/agent.json`);
    summary.mesh.gateway_a_bootstrap_card = bootstrapCardA.body;
    requireCheck(
      summary,
      'gateway_a_bootstrap_card_signed',
      bootstrapCardA.status === 200 && cardLooksSigned(bootstrapCardA.body),
      'Gateway A bootstrap card was not signed',
    );

    await gatewayAProcess.stop();
    gatewayAProcess = null;

    await writeGatewayConfig(
      configBPath,
      baseConfigText,
      gatewayBPort,
      dbBPath,
      [knownAgent('gateway-a', gatewayAUrl, gatewayAPublicKeyBefore)],
    );
    await runGatewayMigration(gatewayBinary, repoRoot, gatewayEnvB, configBPath, gatewayBMigrateLog);
    gatewayBProcess = await startGatewayInstance(
      gatewayBinary,
      repoRoot,
      gatewayEnvB,
      configBPath,
      gatewayBUrl,
      gatewayBLog,
    );

    const gatewayBPublicKey = await waitForFile(meshPublicKeyPath(homeB), options.timeoutMs);
    summary.mesh.gateway_b_public_key = Buffer.from(gatewayBPublicKey).toString('base64');

    const cardB = await fetchJson(`${gatewayBUrl}/.well-known/agent.json`);
    summary.mesh.gateway_b_card = cardB.body;
    requireCheck(
      summary,
      'gateway_b_card_signed',
      cardB.status === 200 && cardLooksSigned(cardB.body),
      'Gateway B card was not signed',
    );

    await writeGatewayConfig(
      configAPath,
      baseConfigText,
      gatewayAPort,
      dbAPath,
      [knownAgent('gateway-b', gatewayBUrl, gatewayBPublicKey)],
    );
    gatewayAProcess = await startGatewayInstance(
      gatewayBinary,
      repoRoot,
      gatewayEnvA,
      configAPath,
      gatewayAUrl,
      gatewayAFinalLog,
    );

    const gatewayAPublicKeyAfter = await waitForFile(meshPublicKeyPath(homeA), options.timeoutMs);
    summary.mesh.gateway_a_public_key_after_restart = Buffer.from(gatewayAPublicKeyAfter).toString(
      'base64',
    );
    requireCheck(
      summary,
      'mesh_identity_persisted_across_restart',
      Buffer.compare(gatewayAPublicKeyBefore, gatewayAPublicKeyAfter) === 0,
      'Gateway A mesh identity changed across restart',
    );

    const cardA = await fetchJson(`${gatewayAUrl}/.well-known/agent.json`);
    summary.mesh.gateway_a_card = cardA.body;
    requireCheck(
      summary,
      'gateway_a_card_signed',
      cardA.status === 200 && cardLooksSigned(cardA.body),
      'Gateway A final card was not signed',
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
        gatewayUrl: gatewayAUrl,
        jwtSecret: DEFAULT_JWT_SECRET,
        timeoutMs: options.timeoutMs,
      },
      browserEvents,
    );
    Object.assign(summary.checks, login.checks);
    const accessToken = login.accessToken;

    const discoveryA = await fetchJson(`${gatewayAUrl}/api/a2a/discover`, {
      headers: authHeaders(accessToken),
    });
    const discoveryB = await fetchJson(`${gatewayBUrl}/api/a2a/discover`, {
      headers: authHeaders(accessToken),
    });
    summary.mesh.discovery_a = discoveryA.body;
    summary.mesh.discovery_b = discoveryB.body;
    requireCheck(
      summary,
      'gateway_a_discovers_verified_gateway_b',
      discoveryA.status === 200 &&
        (discoveryA.body?.agents ?? []).some(
          (agent) => agent.endpoint_url === gatewayBUrl && agent.trust_score === 1 && agent.reachable,
        ),
      'Gateway A did not discover a verified reachable gateway B',
    );
    requireCheck(
      summary,
      'gateway_b_discovers_verified_gateway_a',
      discoveryB.status === 200 &&
        (discoveryB.body?.agents ?? []).some(
          (agent) => agent.endpoint_url === gatewayAUrl && agent.trust_score === 1 && agent.reachable,
        ),
      'Gateway B did not discover a verified reachable gateway A',
    );

    const trustGraph = await fetchJson(`${gatewayAUrl}/api/mesh/trust-graph`, {
      headers: authHeaders(accessToken),
    });
    const consensus = await fetchJson(`${gatewayAUrl}/api/mesh/consensus`, {
      headers: authHeaders(accessToken),
    });
    const delegations = await fetchJson(`${gatewayAUrl}/api/mesh/delegations`, {
      headers: authHeaders(accessToken),
    });
    summary.mesh.trust_graph = trustGraph.body;
    summary.mesh.consensus = consensus.body;
    summary.mesh.delegations = delegations.body;
    requireCheck(summary, 'mesh_trust_graph_visible', trustGraph.status === 200, 'Mesh trust graph failed');
    requireCheck(summary, 'mesh_consensus_visible', consensus.status === 200, 'Mesh consensus failed');
    requireCheck(summary, 'mesh_delegations_visible', delegations.status === 200, 'Mesh delegations failed');

    await page.goto(`${dashboardUrl}/orchestration`, {
      waitUntil: 'networkidle',
      timeout: options.timeoutMs,
    });
    await page.getByRole('heading', { name: 'Orchestration' }).waitFor({
      state: 'visible',
      timeout: options.timeoutMs,
    });
    requireCheck(
      summary,
      'orchestration_page_loaded',
      (await page.locator('.error-msg').count()) === 0,
      'Orchestration page rendered an error state',
    );

    await page.getByRole('tab', { name: 'A2A Discovery' }).click({ timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Discover Agents' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      (expectedEndpoint) =>
        Array.from(document.querySelectorAll('.agent-url')).some((node) =>
          (node.textContent ?? '').includes(expectedEndpoint.replace(/^https?:\/\//, '')),
        ),
      gatewayBUrl,
      { timeout: options.timeoutMs },
    );
    requireCheck(
      summary,
      'orchestration_discovery_renders_gateway_b',
      true,
      'Orchestration page did not render discovered gateway B',
    );

    await page.locator('.send-input').fill(gatewayBUrl, { timeout: options.timeoutMs });
    await page
      .locator('.send-textarea')
      .fill(JSON.stringify({ task: 'distributed live ping' }), { timeout: options.timeoutMs });
    await page.getByRole('button', { name: 'Send Task' }).click({ timeout: options.timeoutMs });
    await page.waitForFunction(
      () => {
        const headings = Array.from(document.querySelectorAll('h2'));
        const taskHeading = headings.find((node) =>
          (node.textContent ?? '').includes('In-Flight Tasks'),
        );
        return /\((\d+)\)/.test(taskHeading?.textContent ?? '') &&
          Number((taskHeading?.textContent ?? '').match(/\((\d+)\)/)?.[1] ?? '0') >= 1;
      },
      { timeout: options.timeoutMs },
    );

    requireCheck(
      summary,
      'ws_a2a_task_update_received',
      await waitForWsFrame(
        browserEvents,
        (frame) =>
          typeof frame.payload === 'string' &&
          frame.payload.includes('"type":"A2ATaskUpdate"'),
        options.timeoutMs,
      ),
      'Dashboard websocket did not receive A2ATaskUpdate',
    );

    const tasks = await fetchJson(`${gatewayAUrl}/api/a2a/tasks`, {
      headers: authHeaders(accessToken),
    });
    summary.mesh.tasks = tasks.body;
    const submittedTask = (tasks.body?.tasks ?? []).find((task) => task.target_url === gatewayBUrl);
    requireCheck(
      summary,
      'a2a_task_persisted',
      tasks.status === 200 && !!submittedTask,
      'A2A task was not persisted on gateway A',
    );
    requireCheck(
      summary,
      'a2a_task_submitted',
      submittedTask?.status === 'submitted',
      'A2A task did not reach submitted status',
    );
    summary.mesh.task = submittedTask ?? null;

    const agentHirer = `distributed-hirer-${runLabel.toLowerCase()}`;
    const agentWorker = `distributed-worker-${runLabel.toLowerCase()}`;
    const skillName = `distributed-skill-${runLabel.toLowerCase()}`;

    const walletSeedHirer = await fetchJson(`${gatewayAUrl}/api/marketplace/wallet/seed`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-wallet-hirer-${runLabel}`),
      body: JSON.stringify({ agent_id: agentHirer, amount: 10_000 }),
    });
    const walletSeedWorker = await fetchJson(`${gatewayAUrl}/api/marketplace/wallet/seed`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-wallet-worker-${runLabel}`),
      body: JSON.stringify({ agent_id: agentWorker, amount: 5_000 }),
    });
    requireCheck(
      summary,
      'marketplace_wallets_seeded',
      walletSeedHirer.status === 200 && walletSeedWorker.status === 200,
      'Marketplace wallet seeding failed',
    );

    const workerAgent = await fetchJson(`${gatewayAUrl}/api/marketplace/agents`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-agent-worker-${runLabel}`),
      body: JSON.stringify({
        agent_id: agentWorker,
        description: 'Distributed live worker',
        capabilities: ['analysis', 'delegation'],
        pricing_model: 'per_task',
        base_price: 250,
        endpoint_url: gatewayBUrl,
        public_key: summary.mesh.gateway_b_public_key,
      }),
    });
    const hirerAgent = await fetchJson(`${gatewayAUrl}/api/marketplace/agents`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-agent-hirer-${runLabel}`),
      body: JSON.stringify({
        agent_id: agentHirer,
        description: 'Distributed live hirer',
        capabilities: ['orchestration'],
        pricing_model: 'per_task',
        base_price: 150,
        endpoint_url: gatewayAUrl,
        public_key: summary.mesh.gateway_a_public_key_after_restart,
      }),
    });
    requireCheck(
      summary,
      'marketplace_agents_registered',
      workerAgent.status === 200 && hirerAgent.status === 200,
      'Marketplace agent registration failed',
    );

    const publishedSkill = await fetchJson(`${gatewayAUrl}/api/marketplace/skills`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-skill-${runLabel}`),
      body: JSON.stringify({
        skill_name: skillName,
        version: '0.1.0',
        author_agent_id: agentWorker,
        description: 'Distributed live skill listing',
        price_credits: 25,
      }),
    });
    requireCheck(
      summary,
      'marketplace_skill_published',
      publishedSkill.status === 200,
      'Marketplace skill publish failed',
    );

    const listedAgents = await fetchJson(`${gatewayAUrl}/api/marketplace/agents`, {
      headers: authHeaders(accessToken),
    });
    const listedSkills = await fetchJson(`${gatewayAUrl}/api/marketplace/skills`, {
      headers: authHeaders(accessToken),
    });
    const discoveredWorkers = await fetchJson(`${gatewayAUrl}/api/marketplace/discover`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-discover-${runLabel}`),
      body: JSON.stringify({
        capabilities: ['analysis'],
        max_price: 500,
        limit: 10,
      }),
    });
    requireCheck(
      summary,
      'marketplace_listings_visible',
      listedAgents.status === 200 &&
        listedSkills.status === 200 &&
        (listedAgents.body?.agents ?? []).length >= 2 &&
        (listedSkills.body?.skills ?? []).length >= 1,
      'Marketplace listings did not become visible',
    );
    requireCheck(
      summary,
      'marketplace_discovery_finds_worker',
      discoveredWorkers.status === 200 &&
        (discoveredWorkers.body?.agents ?? []).some((agent) => agent.agent_id === agentWorker),
      'Marketplace discovery did not return the worker agent',
    );

    const proposedContract = await fetchJson(`${gatewayAUrl}/api/marketplace/contracts`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-contract-${runLabel}`),
      body: JSON.stringify({
        hirer_agent_id: agentHirer,
        worker_agent_id: agentWorker,
        task_description: 'Distributed live contract',
        agreed_price: 250,
      }),
    });
    const contractId = proposedContract.body?.id;
    requireCheck(
      summary,
      'marketplace_contract_proposed',
      proposedContract.status === 200 && typeof contractId === 'string',
      'Marketplace contract proposal failed',
    );

    const acceptContract = await fetchJson(
      `${gatewayAUrl}/api/marketplace/contracts/${contractId}/accept`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `distributed-contract-accept-${runLabel}`),
        body: JSON.stringify({}),
      },
    );
    const startContract = await fetchJson(
      `${gatewayAUrl}/api/marketplace/contracts/${contractId}/start`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `distributed-contract-start-${runLabel}`),
        body: JSON.stringify({}),
      },
    );
    const completeContract = await fetchJson(
      `${gatewayAUrl}/api/marketplace/contracts/${contractId}/complete`,
      {
        method: 'POST',
        headers: mutationHeaders(accessToken, `distributed-contract-complete-${runLabel}`),
        body: JSON.stringify({ result: 'distributed live suite complete' }),
      },
    );
    requireCheck(
      summary,
      'marketplace_contract_lifecycle',
      acceptContract.status === 200 &&
        startContract.status === 200 &&
        completeContract.status === 200,
      'Marketplace contract lifecycle failed',
    );

    const contractDetail = await fetchJson(`${gatewayAUrl}/api/marketplace/contracts/${contractId}`, {
      headers: authHeaders(accessToken),
    });
    requireCheck(
      summary,
      'marketplace_contract_completed',
      contractDetail.status === 200 && contractDetail.body?.state === 'completed',
      'Marketplace contract did not finish in completed state',
    );

    const review = await fetchJson(`${gatewayAUrl}/api/marketplace/reviews`, {
      method: 'POST',
      headers: mutationHeaders(accessToken, `distributed-review-${runLabel}`),
      body: JSON.stringify({
        contract_id: contractId,
        reviewer_agent_id: agentHirer,
        reviewee_agent_id: agentWorker,
        rating: 5,
        comment: 'distributed live suite review',
      }),
    });
    const workerReviews = await fetchJson(`${gatewayAUrl}/api/marketplace/reviews/${agentWorker}`, {
      headers: authHeaders(accessToken),
    });
    const workerWallet = await fetchJson(`${gatewayAUrl}/api/marketplace/wallet?agent_id=${encodeURIComponent(agentWorker)}`, {
      headers: authHeaders(accessToken),
    });
    const hirerTransactions = await fetchJson(
      `${gatewayAUrl}/api/marketplace/wallet/transactions?agent_id=${encodeURIComponent(agentHirer)}`,
      { headers: authHeaders(accessToken) },
    );
    requireCheck(
      summary,
      'marketplace_review_recorded',
      review.status === 200 &&
        workerReviews.status === 200 &&
        (workerReviews.body?.reviews ?? []).some((entry) => entry.contract_id === contractId),
      'Marketplace review was not recorded',
    );
    requireCheck(
      summary,
      'marketplace_wallet_updated',
      workerWallet.status === 200 && (workerWallet.body?.balance ?? 0) > 5_000,
      'Marketplace worker wallet did not increase after completion',
    );
    requireCheck(
      summary,
      'marketplace_transactions_visible',
      hirerTransactions.status === 200 && (hirerTransactions.body?.transactions ?? []).length > 0,
      'Marketplace transactions were not visible',
    );

    summary.marketplace = {
      agents: listedAgents.body,
      skills: listedSkills.body,
      discovery: discoveredWorkers.body,
      contract: contractDetail.body,
      worker_reviews: workerReviews.body,
      worker_wallet: workerWallet.body,
      hirer_transactions: hirerTransactions.body,
    };
    summary.orchestration = {
      page_loaded: true,
      discovered_gateway_b: true,
      browser_page_errors: browserEvents.pageErrors,
      browser_console: browserEvents.console,
    };
    summary.checks.browser_page_errors_empty = browserEvents.pageErrors.length === 0;
    summary.status = 'passed';

    await writeJson(path.join(runDir, 'browser-events.json'), browserEvents);
    await persistBrowserArtifacts(runDir, 'distributed', { context, page }, options.keepArtifacts);
    await writeJson(path.join(runDir, 'summary.json'), summary);

    process.stdout.write(`Artifacts: ${runDir}\n`);
    process.stdout.write(
      `Summary: distributed live audit passed with ${Object.values(summary.checks).filter(Boolean).length}/${Object.keys(summary.checks).length} checks\n`,
    );
  } catch (error) {
    summary.status = 'failed';
    summary.failed_at = nowIso();
    summary.error = error instanceof Error ? error.message : String(error);

    if (browserEvents) {
      await writeJson(path.join(runDir, 'browser-events.json'), browserEvents).catch(() => {});
    }
    if (context && page) {
      await persistBrowserArtifacts(runDir, 'distributed', { context, page }, true).catch(() => {});
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
    if (gatewayAProcess) {
      await gatewayAProcess.stop().catch(() => {});
    }
    if (gatewayBProcess) {
      await gatewayBProcess.stop().catch(() => {});
    }
  }
}

await main();
