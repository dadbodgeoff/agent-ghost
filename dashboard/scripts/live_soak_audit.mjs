#!/usr/bin/env node

import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { classifyRunFailure } from './lib/live_artifacts.mjs';
import { nowIso, timestampLabel, writeJson } from './lib/live_harness.mjs';
import { generateLiveReport } from './lib/live_reporting.mjs';
import { removeIfPresent, runChildSuite } from './lib/live_suite.mjs';

const DEFAULT_TIMEOUT_MS = 45_000;
const DEFAULT_TARGET = 'critical';
const SUPPORTED_TARGETS = ['critical', 'repo'];

function parseArgs(argv) {
  const options = {
    target: DEFAULT_TARGET,
    runs: 3,
    mode: 'dev',
    headed: false,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    keepArtifacts: false,
    failFast: false,
    pruneOldArtifacts: false,
    keepRecentRuns: 20,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    switch (arg) {
      case '--target':
        options.target = argv[index + 1] ?? options.target;
        index += 1;
        break;
      case '--runs':
        options.runs = Number.parseInt(argv[index + 1] ?? '', 10) || options.runs;
        index += 1;
        break;
      case '--mode':
        options.mode = argv[index + 1] ?? options.mode;
        index += 1;
        break;
      case '--headed':
        options.headed = true;
        break;
      case '--timeout-ms':
        options.timeoutMs = Number.parseInt(argv[index + 1] ?? '', 10) || options.timeoutMs;
        index += 1;
        break;
      case '--keep-artifacts':
        options.keepArtifacts = true;
        break;
      case '--fail-fast':
        options.failFast = true;
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

  if (!SUPPORTED_TARGETS.includes(options.target)) {
    throw new Error(`Unsupported target: ${options.target}`);
  }
  if (!['dev', 'preview'].includes(options.mode)) {
    throw new Error(`Unsupported mode: ${options.mode}`);
  }
  if (!Number.isInteger(options.runs) || options.runs <= 0) {
    throw new Error(`--runs must be a positive integer, received: ${options.runs}`);
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live soak audit

Usage:
  pnpm audit:soak-live [-- --target critical|repo]
                       [--runs 3]
                       [--mode dev|preview]
                       [--timeout-ms 45000]
                       [--headed]
                       [--fail-fast]
                       [--prune-old-artifacts]
                       [--keep-recent-runs 20]
                       [--keep-artifacts]

What it does:
  1. Repeats either the critical or repo live suite multiple times on temp state
  2. Records duration, artifact paths, and failure classification for each pass
  3. Produces one aggregate soak summary and refreshes live reporting
`);
}

function targetArgs(target, options) {
  const args =
    target === 'repo'
      ? ['dashboard/scripts/live_repo_suite.mjs']
      : ['dashboard/scripts/live_critical_suite.mjs'];

  args.push('--mode', options.mode, '--timeout-ms', String(options.timeoutMs), '--keep-artifacts');
  if (options.headed) {
    args.push('--headed');
  }
  if (target === 'repo' && options.pruneOldArtifacts) {
    args.push('--prune-old-artifacts', '--keep-recent-runs', String(options.keepRecentRuns));
  }

  return args;
}

function summarizeDurations(runs) {
  const durations = runs
    .map((run) => run.duration_ms)
    .filter((value) => typeof value === 'number' && Number.isFinite(value));

  if (durations.length === 0) {
    return {
      min_ms: null,
      max_ms: null,
      average_ms: null,
      total_ms: 0,
    };
  }

  const totalMs = durations.reduce((sum, value) => sum + value, 0);
  return {
    min_ms: Math.min(...durations),
    max_ms: Math.max(...durations),
    average_ms: Math.round(totalMs / durations.length),
    total_ms: totalMs,
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const suiteDir = path.join(repoRoot, 'artifacts', 'live-soak-audits', timestampLabel());

  await fs.mkdir(suiteDir, { recursive: true });

  const summary = {
    started_at: nowIso(),
    target: options.target,
    planned_runs: options.runs,
    completed_runs: 0,
    mode: options.mode,
    artifact_dir: suiteDir,
    runs: [],
    checks: {},
    stats: {},
    reporting: null,
    status: 'running',
  };

  try {
    for (let iteration = 1; iteration <= options.runs; iteration += 1) {
      const result = await runChildSuite({
        repoRoot,
        suiteDir,
        label: `${options.target}-${String(iteration).padStart(2, '0')}`,
        args: targetArgs(options.target, options),
      });
      const failure = classifyRunFailure(result.summary, result.status);

      summary.runs.push({
        iteration,
        target: options.target,
        status: result.status,
        started_at: result.started_at,
        finished_at: result.finished_at,
        duration_ms: result.duration_ms,
        artifact_dir: result.artifact_dir,
        log_path: result.log_path,
        exit_code: result.exit_code,
        failure_classification: failure.failure_classification,
        failure_reason: failure.failure_reason,
        summary_path: result.artifact_dir ? path.join(result.artifact_dir, 'summary.json') : null,
      });
      summary.completed_runs = iteration;
      await writeJson(path.join(suiteDir, 'summary.json'), summary);

      if (result.status !== 'passed' && options.failFast) {
        throw new Error(`${options.target} run ${iteration} failed`);
      }
    }

    summary.checks.expected_run_count = summary.runs.length === options.runs;
    summary.checks.all_runs_passed = summary.runs.every((run) => run.status === 'passed');
    summary.checks.failed_runs_classified = summary.runs
      .filter((run) => run.status !== 'passed')
      .every((run) => Boolean(run.failure_classification));
    summary.stats = {
      pass_count: summary.runs.filter((run) => run.status === 'passed').length,
      fail_count: summary.runs.filter((run) => run.status !== 'passed').length,
      durations: summarizeDurations(summary.runs),
    };

    if (!summary.checks.expected_run_count || !summary.checks.all_runs_passed) {
      throw new Error('One or more soak runs failed');
    }

    summary.status = 'passed';
  } catch (error) {
    summary.checks.expected_run_count = summary.runs.length === options.runs;
    summary.checks.all_runs_passed = summary.runs.every((run) => run.status === 'passed');
    summary.checks.failed_runs_classified = summary.runs
      .filter((run) => run.status !== 'passed')
      .every((run) => Boolean(run.failure_classification));
    summary.stats = {
      pass_count: summary.runs.filter((run) => run.status === 'passed').length,
      fail_count: summary.runs.filter((run) => run.status !== 'passed').length,
      durations: summarizeDurations(summary.runs),
    };
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.message : String(error);
  } finally {
    summary.finished_at = nowIso();
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
    summary.reporting = await generateLiveReport(repoRoot);
    await writeJson(path.join(suiteDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    for (const run of summary.runs) {
      await removeIfPresent(run.artifact_dir);
    }
    await removeIfPresent(suiteDir);
    process.stdout.write(`Live soak audit passed
Target: ${options.target}
Runs: ${summary.completed_runs}/${summary.planned_runs}
Artifacts: not kept (use --keep-artifacts to preserve them)
Report: ${summary.reporting?.report_path ?? 'not available'}
`);
    return;
  }

  process.stdout.write(`Live soak audit ${summary.status}
Target: ${options.target}
Runs: ${summary.completed_runs}/${summary.planned_runs}
Artifacts: ${suiteDir}
Report: ${summary.reporting?.report_path ?? 'not available'}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
