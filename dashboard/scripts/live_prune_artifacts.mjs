#!/usr/bin/env node

import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { pruneArtifactRuns } from './lib/live_artifacts.mjs';
import { nowIso, timestampLabel, writeJson } from './lib/live_harness.mjs';
import { generateLiveReport } from './lib/live_reporting.mjs';
import { removeIfPresent } from './lib/live_suite.mjs';

function parseArgs(argv) {
  const options = {
    keepSuccessRuns: 5,
    keepFailureRuns: 10,
    keepUnknownRuns: 2,
    dryRun: false,
    keepArtifacts: false,
    families: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    switch (arg) {
      case '--keep-success-runs':
        options.keepSuccessRuns =
          Number.parseInt(argv[index + 1] ?? '', 10) || options.keepSuccessRuns;
        index += 1;
        break;
      case '--keep-failure-runs':
        options.keepFailureRuns =
          Number.parseInt(argv[index + 1] ?? '', 10) || options.keepFailureRuns;
        index += 1;
        break;
      case '--keep-unknown-runs':
        options.keepUnknownRuns =
          Number.parseInt(argv[index + 1] ?? '', 10) || options.keepUnknownRuns;
        index += 1;
        break;
      case '--families':
        options.families = (argv[index + 1] ?? '')
          .split(',')
          .map((value) => value.trim())
          .filter(Boolean);
        index += 1;
        break;
      case '--dry-run':
        options.dryRun = true;
        break;
      case '--keep-artifacts':
        options.keepArtifacts = true;
        break;
      case '--help':
      case '-h':
        printHelp();
        process.exit(0);
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return options;
}

function printHelp() {
  process.stdout.write(`Live artifact prune

Usage:
  pnpm audit:prune-live [-- --keep-success-runs 5]
                        [--keep-failure-runs 10]
                        [--keep-unknown-runs 2]
                        [--families live-critical-suites,live-repo-suites]
                        [--dry-run]
                        [--keep-artifacts]

What it does:
  1. Prunes old live artifact runs while preserving recent successes and failures
  2. Refreshes the artifact index and live report after pruning
  3. Writes a prune summary when --keep-artifacts is used or when pruning fails
`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const artifactsRoot = path.join(repoRoot, 'artifacts');

  const summary = {
    started_at: nowIso(),
    dry_run: options.dryRun,
    keep_success_runs: options.keepSuccessRuns,
    keep_failure_runs: options.keepFailureRuns,
    keep_unknown_runs: options.keepUnknownRuns,
    families: options.families,
    reporting: null,
    result: null,
    status: 'running',
  };

  let runDir = null;

  try {
    summary.result = await pruneArtifactRuns({
      artifactsRoot,
      families: options.families,
      keepSuccessRuns: options.keepSuccessRuns,
      keepFailureRuns: options.keepFailureRuns,
      keepUnknownRuns: options.keepUnknownRuns,
      dryRun: options.dryRun,
    });
    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.message : String(error);
  } finally {
    runDir = path.join(artifactsRoot, 'live-prune-audits', timestampLabel());
    await fs.mkdir(runDir, { recursive: true });
    summary.artifact_dir = runDir;
    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
    summary.reporting = await generateLiveReport(repoRoot);
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await removeIfPresent(runDir);
    process.stdout.write(`Live artifact prune passed
Dry run: ${options.dryRun ? 'yes' : 'no'}
Pruned runs: ${summary.result?.counts?.pruned_runs ?? 0}
Artifacts: not kept (use --keep-artifacts to preserve them)
Report: ${summary.reporting?.report_path ?? 'not available'}
`);
    return;
  }

  process.stdout.write(`Live artifact prune ${summary.status}
Dry run: ${options.dryRun ? 'yes' : 'no'}
Pruned runs: ${summary.result?.counts?.pruned_runs ?? 0}
Artifacts: ${runDir}
Report: ${summary.reporting?.report_path ?? 'not available'}
`);

  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
