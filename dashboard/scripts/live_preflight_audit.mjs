#!/usr/bin/env node

import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import {
  ensureGatewayBinary,
  getFreePort,
  nowIso,
  runLoggedCaptureCommand,
  timestampLabel,
  writeJson,
} from './lib/live_harness.mjs';
import { pruneArtifactRuns } from './lib/live_artifacts.mjs';
import { removeIfPresent } from './lib/live_suite.mjs';

const DEFAULT_MIN_FREE_GB = 6;
const DEFAULT_WARN_FREE_GB = 12;
const DEFAULT_KEEP_RECENT_RUNS = 20;

function parseArgs(argv) {
  const options = {
    keepArtifacts: false,
    minFreeGb: DEFAULT_MIN_FREE_GB,
    warnFreeGb: DEFAULT_WARN_FREE_GB,
    pruneOldArtifacts: false,
    keepRecentRuns: DEFAULT_KEEP_RECENT_RUNS,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    switch (arg) {
      case '--keep-artifacts':
        options.keepArtifacts = true;
        break;
      case '--min-free-gb':
        options.minFreeGb = Number.parseInt(argv[index + 1] ?? '', 10) || options.minFreeGb;
        index += 1;
        break;
      case '--warn-free-gb':
        options.warnFreeGb = Number.parseInt(argv[index + 1] ?? '', 10) || options.warnFreeGb;
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

  return options;
}

function printHelp() {
  process.stdout.write(`Live preflight audit

Usage:
  pnpm audit:preflight-live [-- --keep-artifacts]
                            [--min-free-gb 6]
                            [--warn-free-gb 12]
                            [--prune-old-artifacts]
                            [--keep-recent-runs 20]

What it checks:
  1. Required local files and toolchain binaries exist
  2. Enough free disk remains for live runs
  3. Free localhost ports can be allocated
  4. Playwright Chromium launches successfully
  5. The gateway binary can be built or reused cleanly
`);
}

function bytesToGiB(bytes) {
  return Number(bytes) / (1024 ** 3);
}

function requireCheck(summary, name, condition, message, details = undefined) {
  summary.checks[name] = Boolean(condition);
  if (!condition) {
    throw new Error(message);
  }
  if (details !== undefined) {
    summary.details[name] = details;
  }
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function pruneOldArtifactRuns(artifactsRoot, keepRecentRuns) {
  const result = await pruneArtifactRuns({
    artifactsRoot,
    keepSuccessRuns: keepRecentRuns,
    keepFailureRuns: keepRecentRuns,
    keepUnknownRuns: Math.min(keepRecentRuns, 5),
  });
  return result.pruned.map((entry) => entry.artifact_dir);
}

async function verifyCommand(command, args, cwd, logPath) {
  const result = await runLoggedCaptureCommand(command, args, {
    cwd,
    env: process.env,
    logPath,
  });
  return result.stdout.trim() || result.stderr.trim();
}

async function verifyPlaywrightBrowser(dashboardDir, logPath) {
  const script = [
    "import { chromium } from '@playwright/test';",
    'const browser = await chromium.launch({ headless: true });',
    'await browser.close();',
    "console.log('chromium-ok');",
  ].join(' ');

  const result = await runLoggedCaptureCommand(
    'node',
    ['--input-type=module', '-e', script],
    {
      cwd: dashboardDir,
      env: process.env,
      logPath,
    },
  );
  return result.stdout.trim();
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const dashboardDir = path.resolve(scriptDir, '..');
  const repoRoot = path.resolve(dashboardDir, '..');
  const artifactsRoot = path.join(repoRoot, 'artifacts');
  const runDir = path.join(artifactsRoot, 'live-preflight-audits', timestampLabel());

  await fs.mkdir(runDir, { recursive: true });

  const summary = {
    started_at: nowIso(),
    artifact_dir: runDir,
    checks: {},
    details: {},
    warnings: [],
    pruned_artifacts: [],
    status: 'running',
  };

  try {
    if (options.pruneOldArtifacts) {
      summary.pruned_artifacts = await pruneOldArtifactRuns(
        artifactsRoot,
        options.keepRecentRuns,
      );
    }

    requireCheck(
      summary,
      'repo_config_present',
      await pathExists(path.join(repoRoot, 'ghost.yml')),
      'ghost.yml was not found at the repo root',
    );
    requireCheck(
      summary,
      'dashboard_package_present',
      await pathExists(path.join(dashboardDir, 'package.json')),
      'dashboard/package.json was not found',
    );

    summary.details.commands = {
      node: await verifyCommand('node', ['--version'], repoRoot, path.join(runDir, 'node.log')),
      pnpm: await verifyCommand('pnpm', ['--version'], repoRoot, path.join(runDir, 'pnpm.log')),
      cargo: await verifyCommand('cargo', ['--version'], repoRoot, path.join(runDir, 'cargo.log')),
      sqlite3: await verifyCommand(
        'sqlite3',
        ['--version'],
        repoRoot,
        path.join(runDir, 'sqlite3.log'),
      ),
    };
    requireCheck(summary, 'toolchain_available', true, 'toolchain unavailable', summary.details.commands);

    const stat = await fs.statfs(repoRoot);
    const freeBytes = Number(stat.bavail) * Number(stat.bsize);
    const freeGiB = bytesToGiB(freeBytes);
    summary.details.disk = {
      free_bytes: freeBytes,
      free_gib: Number(freeGiB.toFixed(2)),
      min_free_gib: options.minFreeGb,
      warn_free_gib: options.warnFreeGb,
    };
    requireCheck(
      summary,
      'disk_space_ok',
      freeGiB >= options.minFreeGb,
      `Free disk space ${freeGiB.toFixed(2)} GiB is below the minimum ${options.minFreeGb} GiB`,
      summary.details.disk,
    );
    if (freeGiB < options.warnFreeGb) {
      summary.warnings.push(
        `Free disk space is ${freeGiB.toFixed(2)} GiB; consider pruning old artifacts before long runs`,
      );
    }

    const ports = await Promise.all(Array.from({ length: 4 }, () => getFreePort()));
    summary.details.ports = ports;
    requireCheck(
      summary,
      'free_ports_allocated',
      new Set(ports).size === ports.length,
      'Failed to allocate distinct free localhost ports',
      ports,
    );

    summary.details.playwright = {
      chromium: await verifyPlaywrightBrowser(
        dashboardDir,
        path.join(runDir, 'playwright.log'),
      ),
    };
    requireCheck(
      summary,
      'playwright_browser_launches',
      summary.details.playwright.chromium.includes('chromium-ok'),
      'Playwright Chromium failed to launch',
      summary.details.playwright,
    );

    const gatewayBinary = await ensureGatewayBinary(
      repoRoot,
      path.join(runDir, 'gateway-build.log'),
    );
    summary.details.gateway_binary = gatewayBinary;
    requireCheck(
      summary,
      'gateway_binary_ready',
      await pathExists(gatewayBinary),
      'Gateway binary was not available after preflight build',
      gatewayBinary,
    );

    summary.status = 'passed';
  } catch (error) {
    summary.status = 'failed';
    summary.error = error instanceof Error ? error.message : String(error);
  } finally {
    summary.finished_at = nowIso();
    await writeJson(path.join(runDir, 'summary.json'), summary);
  }

  if (summary.status === 'passed' && !options.keepArtifacts) {
    await removeIfPresent(runDir);
    process.stdout.write('Live preflight audit passed\nArtifacts: not kept (use --keep-artifacts to preserve them)\n');
    return;
  }

  process.stdout.write(`Live preflight audit ${summary.status}\nArtifacts: ${runDir}\n`);
  if (summary.status !== 'passed') {
    process.exitCode = 1;
  }
}

await main();
