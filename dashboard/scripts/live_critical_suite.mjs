#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_COMPONENTS = ['poc', 'infra', 'runtime', 'knowledge', 'io', 'convergence', 'database'];

function parseArgs(argv) {
  const options = {
    mode: 'dev',
    headed: false,
    keepArtifacts: false,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    components: DEFAULT_COMPONENTS,
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
      case '--components':
        options.components = (argv[index + 1] ?? '')
          .split(',')
          .map((value) => value.trim())
          .filter(Boolean);
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

  const unsupportedComponents = options.components.filter(
    (component) => !DEFAULT_COMPONENTS.includes(component),
  );
  if (unsupportedComponents.length > 0) {
    throw new Error(`Unsupported components: ${unsupportedComponents.join(', ')}`);
  }
  if (options.components.length === 0) {
    throw new Error('At least one component is required');
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live critical suite

Usage:
  pnpm audit:critical-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                           [--timeout-ms 45000]
                           [--components ${DEFAULT_COMPONENTS.join(',')}]

What it does:
  1. Runs the current critical live verification components one by one
  2. Starts with the full Studio POC suite
  3. Continues into the infra/auth/bootstrap audit
  4. Produces a single suite summary with child artifact references
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

async function writeJson(filePath, value) {
  await fs.writeFile(filePath, JSON.stringify(value, null, 2));
}

async function readJsonIfExists(filePath) {
  try {
    return JSON.parse(await fs.readFile(filePath, 'utf8'));
  } catch {
    return null;
  }
}

function relayOutput(targetStream, component, chunk, logStream, buffer) {
  const text = chunk.toString();
  logStream.write(text);
  buffer.push(text);

  for (const line of text.split(/\r?\n/)) {
    if (line.length > 0) {
      targetStream.write(`[${component}] ${line}\n`);
    }
  }
}

function componentCommand(component) {
  if (component === 'poc') {
    return ['dashboard/scripts/live_poc_suite.mjs'];
  }
  if (component === 'infra') {
    return ['dashboard/scripts/live_infra_audit.mjs'];
  }
  if (component === 'runtime') {
    return ['dashboard/scripts/live_runtime_audit.mjs'];
  }
  if (component === 'knowledge') {
    return ['dashboard/scripts/live_knowledge_audit.mjs'];
  }
  if (component === 'io') {
    return ['dashboard/scripts/live_io_audit.mjs'];
  }
  if (component === 'convergence') {
    return ['dashboard/scripts/live_convergence_audit.mjs'];
  }
  if (component === 'database') {
    return ['dashboard/scripts/live_database_audit.mjs'];
  }
  throw new Error(`Unsupported component: ${component}`);
}

async function runComponent(repoRoot, component, options, suiteDir) {
  const logPath = path.join(suiteDir, `${component}.log`);
  const logStream = createWriteStream(logPath, { flags: 'a' });
  const output = [];
  const startedAt = nowIso();
  const startedMs = Date.now();

  const childArgs = [
    ...componentCommand(component),
    '--mode',
    options.mode,
    '--timeout-ms',
    String(options.timeoutMs),
    '--keep-artifacts',
  ];

  if (options.headed) {
    childArgs.push('--headed');
  }

  const child = spawn('node', childArgs, {
    cwd: repoRoot,
    env: process.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    relayOutput(process.stdout, component, chunk, logStream, output);
  });
  child.stderr.on('data', (chunk) => {
    relayOutput(process.stderr, component, chunk, logStream, output);
  });

  const exitCode = await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });
  await new Promise((resolve) => logStream.end(resolve));

  const combinedOutput = output.join('');
  const artifactDirMatch = combinedOutput.match(/^Artifacts:\s+(.+)$/m);
  const childArtifactDir = artifactDirMatch?.[1]?.trim() ?? null;
  const childSummaryPath = childArtifactDir ? path.join(childArtifactDir, 'summary.json') : null;
  const childSummary = childSummaryPath ? await readJsonIfExists(childSummaryPath) : null;
  const finishedAt = nowIso();

  return {
    component,
    status: exitCode === 0 ? 'passed' : 'failed',
    started_at: startedAt,
    finished_at: finishedAt,
    duration_ms: Date.now() - startedMs,
    exit_code: exitCode,
    artifact_dir: childArtifactDir,
    log_path: logPath,
    summary: childSummary,
  };
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
  const suiteDir = path.join(
    repoRoot,
    'artifacts',
    'live-critical-suites',
    timestampLabel(),
  );

  await fs.mkdir(suiteDir, { recursive: true });

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    components: options.components,
    artifact_dir: suiteDir,
    results: [],
    status: 'running',
  };

  try {
    for (const component of options.components) {
      const result = await runComponent(repoRoot, component, options, suiteDir);
      summary.results.push(result);
      await writeJson(path.join(suiteDir, 'summary.json'), summary);

      if (result.status !== 'passed') {
        throw new Error(`${component} component failed`);
      }
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.message : String(error);
  } finally {
    summary.finished_at = nowIso();
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    for (const result of summary.results) {
      await removeIfPresent(result.artifact_dir);
    }
    await removeIfPresent(suiteDir);
    process.stdout.write(`Live critical suite passed
Components: ${options.components.join(', ')}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live critical suite ${summary.status}
Components: ${options.components.join(', ')}
Artifacts: ${suiteDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
