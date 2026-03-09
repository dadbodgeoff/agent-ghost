import { promises as fs } from 'node:fs';
import path from 'node:path';

function parseIsoTimestampMs(value) {
  if (!value || typeof value !== 'string') {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function inferDurationMs(summary) {
  if (!summary || typeof summary !== 'object') {
    return null;
  }
  if (typeof summary.duration_ms === 'number' && Number.isFinite(summary.duration_ms)) {
    return summary.duration_ms;
  }

  const startedMs = parseIsoTimestampMs(summary.started_at);
  const finishedMs = parseIsoTimestampMs(summary.finished_at ?? summary.failed_at);
  if (startedMs === null || finishedMs === null || finishedMs < startedMs) {
    return null;
  }

  return finishedMs - startedMs;
}

function normalizeFailureClassification(classification, reason = null) {
  if (!classification || typeof classification !== 'string') {
    return null;
  }

  const normalized = classification.trim().toLowerCase().replace(/[\s-]+/g, '_');
  switch (normalized) {
    case 'product':
    case 'product_bug':
    case 'product_regression':
    case 'regression':
      return {
        classification: 'product_regression',
        reason,
      };
    case 'harness':
    case 'harness_bug':
    case 'harness_defect':
    case 'selector_bug':
      return {
        classification: 'harness_defect',
        reason,
      };
    case 'environment':
    case 'environment_failure':
    case 'environment_local':
      return {
        classification: 'environment_failure',
        reason,
      };
    case 'environment_transient':
    case 'provider_transient':
    case 'transient':
    case 'transient_external':
    case 'transient_external_provider_failure':
      return {
        classification: 'transient_external_provider_failure',
        reason,
      };
    default:
      return {
        classification: 'product_regression',
        reason: reason ?? `Unmapped failure classification: ${classification}`,
      };
  }
}

function classifyFailureFromText(summary) {
  const haystack = [
    summary?.failure_reason,
    summary?.error,
    summary?.stderr,
    summary?.stdout,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  if (haystack.length === 0) {
    return null;
  }

  if (
    /(playwright|locator|selector|navigation timeout|expect\(|strict mode violation|element is not attached)/.test(
      haystack,
    )
  ) {
    return {
      classification: 'harness_defect',
      reason: summary?.failure_reason ?? summary?.error ?? 'Browser harness failure pattern matched',
    };
  }

  if (
    /(econn|socket hang up|fetch failed|connection reset|temporar|transient|provider timeout|rate limit|network error)/.test(
      haystack,
    )
  ) {
    return {
      classification: 'transient_external_provider_failure',
      reason:
        summary?.failure_reason ??
        summary?.error ??
        'Transient environment or provider failure pattern matched',
    };
  }

  if (
    /(toolchain unavailable|chromium|free disk|port|sqlite|gateway binary|command not found|no such file or directory)/.test(
      haystack,
    )
  ) {
    return {
      classification: 'environment_failure',
      reason:
        summary?.failure_reason ?? summary?.error ?? 'Local environment failure pattern matched',
    };
  }

  return {
    classification: 'product_regression',
    reason: summary?.failure_reason ?? summary?.error ?? 'Failed checks or assertions',
  };
}

function findNestedFailure(summary) {
  if (!summary || typeof summary !== 'object') {
    return null;
  }

  const declared = normalizeFailureClassification(
    summary.failure_classification ?? summary.failureClassification ?? null,
    summary.failure_reason ?? summary.error ?? null,
  );
  if (declared) {
    return declared;
  }

  const collections = [summary.results, summary.runs, summary.attempts];
  for (const collection of collections) {
    if (!Array.isArray(collection)) {
      continue;
    }

    for (const entry of collection) {
      if (!entry || typeof entry !== 'object') {
        continue;
      }
      if (entry.status === 'passed') {
        continue;
      }

      const nestedDeclared = normalizeFailureClassification(
        entry.failure_classification ?? entry.failureClassification ?? null,
        entry.failure_reason ?? entry.error ?? null,
      );
      if (nestedDeclared) {
        return nestedDeclared;
      }

      const nestedSummaryFailure = findNestedFailure(entry.summary ?? null);
      if (nestedSummaryFailure) {
        return nestedSummaryFailure;
      }

      const textFailure = classifyFailureFromText(entry);
      if (textFailure) {
        return textFailure;
      }
    }
  }

  return null;
}

export function classifyRunFailure(summary, status = 'failed') {
  if (status === 'passed') {
    return {
      failure_classification: null,
      failure_reason: null,
    };
  }

  const nestedFailure = findNestedFailure(summary);
  if (nestedFailure) {
    return {
      failure_classification: nestedFailure.classification,
      failure_reason: nestedFailure.reason,
    };
  }

  const textFailure = classifyFailureFromText(summary);
  if (textFailure) {
    return {
      failure_classification: textFailure.classification,
      failure_reason: textFailure.reason,
    };
  }

  return {
    failure_classification: 'product_regression',
    failure_reason: 'Run failed without a more specific classifier',
  };
}

export async function readJsonIfExists(filePath) {
  try {
    return JSON.parse(await fs.readFile(filePath, 'utf8'));
  } catch {
    return null;
  }
}

export async function listLiveArtifactFamilies(artifactsRoot, options = {}) {
  const entries = await fs.readdir(artifactsRoot, { withFileTypes: true }).catch(() => []);
  return entries
    .filter((entry) => entry.isDirectory() && entry.name.startsWith('live-'))
    .filter((entry) => options.includeReporting || entry.name !== 'live-reporting')
    .map((entry) => entry.name)
    .sort();
}

export async function readArtifactRunMetadata(runDir, runId = path.basename(runDir)) {
  const summaryPath = path.join(runDir, 'summary.json');
  const summary = await readJsonIfExists(summaryPath);
  const status = summary?.status ?? 'unknown';
  const failure = classifyRunFailure(summary, status);

  return {
    run_id: runId,
    artifact_dir: runDir,
    summary_path: summary ? summaryPath : null,
    status,
    started_at: summary?.started_at ?? null,
    finished_at: summary?.finished_at ?? summary?.failed_at ?? null,
    duration_ms: inferDurationMs(summary),
    failure_classification: failure.failure_classification,
    failure_reason: failure.failure_reason,
    summary,
  };
}

export async function collectArtifactFamilyRuns(artifactsRoot, family) {
  const familyDir = path.join(artifactsRoot, family);
  const runEntries = await fs.readdir(familyDir, { withFileTypes: true }).catch(() => []);
  const runDirectories = runEntries
    .filter((entry) => entry.isDirectory())
    .sort((left, right) => right.name.localeCompare(left.name));

  const runs = [];
  for (const entry of runDirectories) {
    runs.push(await readArtifactRunMetadata(path.join(familyDir, entry.name), entry.name));
  }
  return runs;
}

export function summarizeDurationStats(runs) {
  const durations = runs
    .map((run) => run.duration_ms)
    .filter((value) => typeof value === 'number' && Number.isFinite(value));

  if (durations.length === 0) {
    return {
      count: 0,
      min_ms: null,
      max_ms: null,
      average_ms: null,
    };
  }

  const total = durations.reduce((sum, value) => sum + value, 0);
  return {
    count: durations.length,
    min_ms: Math.min(...durations),
    max_ms: Math.max(...durations),
    average_ms: Math.round(total / durations.length),
  };
}

export async function pruneArtifactRuns({
  artifactsRoot,
  families = null,
  keepSuccessRuns = 5,
  keepFailureRuns = 10,
  keepUnknownRuns = 2,
  dryRun = false,
}) {
  const targetFamilies = families ?? (await listLiveArtifactFamilies(artifactsRoot));
  const pruned = [];
  const retained = {};

  for (const family of targetFamilies) {
    const runs = await collectArtifactFamilyRuns(artifactsRoot, family);
    const buckets = {
      passed: [],
      failed: [],
      other: [],
    };

    for (const run of runs) {
      if (run.status === 'passed') {
        buckets.passed.push(run);
      } else if (run.status === 'failed') {
        buckets.failed.push(run);
      } else {
        buckets.other.push(run);
      }
    }

    const retainedRuns = [
      ...buckets.passed.slice(0, keepSuccessRuns),
      ...buckets.failed.slice(0, keepFailureRuns),
      ...buckets.other.slice(0, keepUnknownRuns),
    ];
    const retainedPaths = new Set(retainedRuns.map((run) => run.artifact_dir));
    const prunableRuns = runs.filter((run) => !retainedPaths.has(run.artifact_dir));

    retained[family] = {
      passed: buckets.passed.slice(0, keepSuccessRuns).map((run) => run.run_id),
      failed: buckets.failed.slice(0, keepFailureRuns).map((run) => run.run_id),
      other: buckets.other.slice(0, keepUnknownRuns).map((run) => run.run_id),
    };

    for (const run of prunableRuns) {
      if (!dryRun) {
        await fs.rm(run.artifact_dir, { recursive: true, force: true });
      }
      pruned.push({
        family,
        run_id: run.run_id,
        artifact_dir: run.artifact_dir,
        status: run.status,
        failure_classification: run.failure_classification,
      });
    }
  }

  return {
    pruned,
    retained,
    counts: {
      pruned_runs: pruned.length,
      families: targetFamilies.length,
    },
  };
}
