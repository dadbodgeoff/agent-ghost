#!/usr/bin/env node

import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { nowIso, timestampLabel, writeJson } from './lib/live_harness.mjs';
import { generateLiveReport } from './lib/live_reporting.mjs';
import { removeIfPresent, runChildSuite } from './lib/live_suite.mjs';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_SUITES = ['preflight', 'critical'];

function parseArgs(argv) {
  const options = {
    mode: 'dev',
    headed: false,
    keepArtifacts: false,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    suites: DEFAULT_SUITES,
    pruneOldArtifacts: false,
    keepRecentRuns: 20,
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
      case '--suites':
        options.suites = (argv[index + 1] ?? '')
          .split(',')
          .map((value) => value.trim())
          .filter(Boolean);
        index += 1;
        break;
      case '--prune-old-artifacts':
        options.pruneOldArtifacts = true;
        break;
      case '--keep-recent-runs':
        options.keepRecentRuns =
          Number.parseInt(argv[index + 1] ?? '', 10) || options.keepRecentRuns;
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

  const unsupportedSuites = options.suites.filter((suite) => !DEFAULT_SUITES.includes(suite));
  if (unsupportedSuites.length > 0) {
    throw new Error(`Unsupported suites: ${unsupportedSuites.join(', ')}`);
  }
  if (options.suites.length === 0) {
    throw new Error('At least one suite is required');
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live repo suite

Usage:
  pnpm audit:repo-live [-- --mode dev|preview] [--headed] [--keep-artifacts]
                       [--timeout-ms 45000]
                       [--suites ${DEFAULT_SUITES.join(',')}]
                       [--prune-old-artifacts]
                       [--keep-recent-runs 20]

What it does:
  1. Runs preflight checks for local toolchain, disk, ports, browser, and gateway build
  2. Runs the critical live suite on the current repo
  3. Produces a single top-level summary with child suite references
`);
}

function suiteArgs(suite, options) {
  if (suite === 'preflight') {
    const args = ['dashboard/scripts/live_preflight_audit.mjs', '--keep-artifacts'];
    if (options.pruneOldArtifacts) {
      args.push('--prune-old-artifacts', '--keep-recent-runs', String(options.keepRecentRuns));
    }
    return args;
  }

  if (suite === 'critical') {
    const args = [
      'dashboard/scripts/live_critical_suite.mjs',
      '--mode',
      options.mode,
      '--timeout-ms',
      String(options.timeoutMs),
      '--keep-artifacts',
    ];
    if (options.headed) {
      args.push('--headed');
    }
    return args;
  }

  throw new Error(`Unsupported suite: ${suite}`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const suiteDir = path.join(
    repoRoot,
    'artifacts',
    'live-repo-suites',
    timestampLabel(),
  );

  await fs.mkdir(suiteDir, { recursive: true });

  const summary = {
    started_at: nowIso(),
    mode: options.mode,
    suites: options.suites,
    artifact_dir: suiteDir,
    results: [],
    reporting: null,
    status: 'running',
  };

  try {
    for (const suite of options.suites) {
      const result = await runChildSuite({
        repoRoot,
        suiteDir,
        label: suite,
        args: suiteArgs(suite, options),
      });
      summary.results.push(result);
      await writeJson(path.join(suiteDir, 'summary.json'), summary);

      if (result.status !== 'passed') {
        throw new Error(`${suite} suite failed`);
      }
    }

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.message : String(error);
  } finally {
    summary.finished_at = nowIso();
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
    summary.reporting = await generateLiveReport(repoRoot);
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    for (const result of summary.results) {
      await removeIfPresent(result.artifact_dir);
    }
    await removeIfPresent(suiteDir);
    process.stdout.write(`Live repo suite passed
Suites: ${options.suites.join(', ')}
Artifacts: not kept (use --keep-artifacts to preserve them)
Report: ${summary.reporting?.report_path ?? 'not available'}
`);
    return;
  }

  process.stdout.write(`Live repo suite ${summary.status}
Suites: ${options.suites.join(', ')}
Artifacts: ${suiteDir}
Report: ${summary.reporting?.report_path ?? 'not available'}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
