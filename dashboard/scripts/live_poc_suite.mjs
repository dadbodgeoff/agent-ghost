#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_JOURNEYS = [
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
    timeoutMs: DEFAULT_TIMEOUT_MS,
    journeys: DEFAULT_JOURNEYS,
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
      case '--journeys':
        options.journeys = (argv[index + 1] ?? '')
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

  const unsupportedJourneys = options.journeys.filter(
    (journey) => !DEFAULT_JOURNEYS.includes(journey),
  );
  if (unsupportedJourneys.length > 0) {
    throw new Error(`Unsupported journeys: ${unsupportedJourneys.join(', ')}`);
  }
  if (options.journeys.length === 0) {
    throw new Error('At least one journey is required');
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live POC suite

Usage:
  pnpm audit:poc-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                      [--timeout-ms 45000]
                      [--journeys ${DEFAULT_JOURNEYS.join(',')}]

What it does:
  1. Runs the named live journeys one by one
  2. Preserves each child run summary and logs while the suite runs
  3. Produces a single suite summary with pass/fail status
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

function relayOutput(targetStream, journey, chunk, logStream, buffer) {
  const text = chunk.toString();
  logStream.write(text);
  buffer.push(text);

  for (const line of text.split(/\r?\n/)) {
    if (line.length > 0) {
      targetStream.write(`[${journey}] ${line}\n`);
    }
  }
}

async function runJourney(repoRoot, journey, options, suiteDir) {
  const logPath = path.join(suiteDir, `${journey}.log`);
  const logStream = createWriteStream(logPath, { flags: 'a' });
  const output = [];
  const startedAt = nowIso();
  const startedMs = Date.now();

  const childArgs = [
    'dashboard/scripts/live_studio_audit.mjs',
    '--journey',
    journey,
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
    relayOutput(process.stdout, journey, chunk, logStream, output);
  });
  child.stderr.on('data', (chunk) => {
    relayOutput(process.stderr, journey, chunk, logStream, output);
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
    journey,
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
    'live-poc-suites',
    timestampLabel(),
  );

  await fs.mkdir(suiteDir, { recursive: true });

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    journeys: options.journeys,
    artifact_dir: suiteDir,
    status: 'running',
    runs: [],
    checks: {},
    warnings: [],
  };

  try {
    for (const journey of options.journeys) {
      const result = await runJourney(repoRoot, journey, options, suiteDir);
      const childSummary = result.summary;
      summary.runs.push({
        journey,
        status: childSummary?.status ?? result.status,
        started_at: childSummary?.started_at ?? result.started_at,
        finished_at: childSummary?.finished_at ?? result.finished_at,
        duration_ms: result.duration_ms,
        artifact_dir: result.artifact_dir,
        log_path: result.log_path,
        checks: childSummary?.checks ?? {},
        warnings: childSummary?.warnings ?? [],
        error: childSummary?.error ?? (result.exit_code === 0 ? null : `Exited with code ${result.exit_code}`),
      });
    }

    summary.checks.all_journeys_passed = summary.runs.every((run) => run.status === 'passed');
    summary.checks.expected_journey_count = summary.runs.length === options.journeys.length;
    summary.passed_count = summary.runs.filter((run) => run.status === 'passed').length;
    summary.failed_count = summary.runs.filter((run) => run.status !== 'passed').length;

    if (!summary.checks.all_journeys_passed || !summary.checks.expected_journey_count) {
      throw new Error('One or more live journeys failed');
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.stack ?? error.message : String(error);
  } finally {
    summary.finished_at = nowIso();
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await Promise.all(summary.runs.map((run) => removeIfPresent(run.artifact_dir)));
    await fs.rm(suiteDir, { recursive: true, force: true });
    process.stdout.write(`Live POC suite passed
Journeys: ${options.journeys.join(', ')}
Artifacts: not kept (use --keep-artifacts to preserve them)
`);
    return;
  }

  process.stdout.write(`Live POC suite ${summary.status}
Journeys: ${options.journeys.join(', ')}
Artifacts: ${suiteDir}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
