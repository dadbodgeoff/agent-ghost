import { promises as fs } from 'node:fs';
import path from 'node:path';

import { nowIso, writeJson } from './live_harness.mjs';
import {
  collectArtifactFamilyRuns,
  summarizeDurationStats,
} from './live_artifacts.mjs';
import { LIVE_COMPONENT_METADATA, SUPPORT_ONLY_CRATES } from './live_coverage_map.mjs';

const REPORTING_DIR = path.join('artifacts', 'live-reporting');
const ROUTE_SETS_PATH = path.join('crates', 'ghost-gateway', 'src', 'route_sets.rs');
const DASHBOARD_ROUTES_DIR = path.join('dashboard', 'src', 'routes');
const CRATES_DIR = 'crates';

function normalizePathSlashes(value) {
  return value.replaceAll(path.sep, '/');
}

function uniqueSorted(values) {
  return [...new Set(values.filter(Boolean))].sort();
}

async function walkFiles(rootDir, predicate) {
  const results = [];

  async function walk(currentDir) {
    const entries = await fs.readdir(currentDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        await walk(fullPath);
        continue;
      }
      if (predicate(fullPath)) {
        results.push(fullPath);
      }
    }
  }

  await walk(rootDir);
  return results;
}

function normalizeRoute(route) {
  return route
    .replace(/\$\{[^}]+\}/g, ':param')
    .replace(/\?.*$/, '')
    .replace(/[),;]+$/, '')
    .replace(/\/+$/, '') || '/';
}

function routeFamily(route) {
  if (!route.startsWith('/api/')) {
    return route;
  }
  const segments = route.split('/').filter(Boolean);
  if (segments.length <= 2) {
    return `/${segments.join('/')}`;
  }
  return `/api/${segments[1]}`;
}

function normalizeDashboardPath(route) {
  const cleaned = route
    .replace(/\$\{[^}]+\}/g, ':param')
    .replace(/\[[^/\]]+\]/g, ':param')
    .replace(/\?.*$/, '')
    .replace(/[),;]+$/, '')
    .replace(/\/+$/, '');
  return cleaned === '' ? '/' : cleaned;
}

function dashboardCoverageKey(route) {
  return normalizeDashboardPath(route)
    .split('/')
    .map((segment) => (segment.startsWith(':') ? ':param' : segment))
    .join('/');
}

function dashboardRouteFromFile(repoRoot, filePath) {
  const relative = normalizePathSlashes(path.relative(path.join(repoRoot, DASHBOARD_ROUTES_DIR), filePath));
  const withoutSuffix = relative.replace(/\/\+page\.svelte$/, '').replace(/\+page\.svelte$/, '');
  if (withoutSuffix === '') {
    return '/';
  }
  return `/${withoutSuffix}`;
}

function extractGatewayRoutes(scriptSource) {
  return uniqueSorted(
    [...scriptSource.matchAll(/\/api\/[^"'`\s)]+/g)].map((match) => normalizeRoute(match[0])),
  );
}

function extractDashboardPages(scriptSource) {
  const pages = [];
  const routeConstants = new Map();

  function addPageTarget(target) {
    const normalized = normalizeExtractedDashboardTarget(target);
    if (normalized) {
      pages.push(normalized);
    }
  }

  for (const match of scriptSource.matchAll(
    /\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*['"]([^'"]+)['"]/g,
  )) {
    routeConstants.set(match[1], match[2]);
  }

  for (const match of scriptSource.matchAll(/page\.goto\(\s*`([^`]+)`/g)) {
    addPageTarget(match[1]);
  }
  for (const match of scriptSource.matchAll(/page\.goto\(\s*['"]([^'"]+)['"]/g)) {
    addPageTarget(match[1]);
  }
  for (const match of scriptSource.matchAll(/page\.goto\(\s*([A-Za-z_$][\w$]*)\s*[,)]/g)) {
    addPageTarget(routeConstants.get(match[1]) ?? null);
  }
  for (const match of scriptSource.matchAll(/pathname === ['"]([^'"]+)['"]/g)) {
    addPageTarget(match[1]);
  }

  return uniqueSorted(pages);
}

function normalizeExtractedDashboardTarget(target) {
  if (!target || typeof target !== 'string') {
    return null;
  }

  let value = target.trim();
  while (value.startsWith('${')) {
    value = value.replace(/^\$\{[^}]+\}/, '');
  }

  if (/^https?:\/\//.test(value)) {
    try {
      const url = new URL(value);
      value = `${url.pathname}${url.search}`;
    } catch {
      return null;
    }
  }

  if (!value.startsWith('/')) {
    return null;
  }

  return normalizeDashboardPath(value);
}

async function collectScriptCoverage(repoRoot, scriptPath) {
  const source = await fs.readFile(path.join(repoRoot, scriptPath), 'utf8');
  return {
    gateway_routes: extractGatewayRoutes(source),
    dashboard_pages: extractDashboardPages(source),
  };
}

async function collectWorkspaceCrates(repoRoot) {
  const files = await walkFiles(path.join(repoRoot, CRATES_DIR), (filePath) => path.basename(filePath) === 'Cargo.toml');
  return uniqueSorted(
    files.map((filePath) =>
      normalizePathSlashes(path.relative(repoRoot, path.dirname(filePath))),
    ),
  );
}

async function collectDashboardPages(repoRoot) {
  const files = await walkFiles(path.join(repoRoot, DASHBOARD_ROUTES_DIR), (filePath) =>
    filePath.endsWith('+page.svelte'),
  );
  return uniqueSorted(files.map((filePath) => dashboardRouteFromFile(repoRoot, filePath)));
}

async function collectGatewayRouteFamilies(repoRoot) {
  const source = await fs.readFile(path.join(repoRoot, ROUTE_SETS_PATH), 'utf8');
  const routes = uniqueSorted(
    [...source.matchAll(/"\/api\/[^"]+"/g)].map((match) =>
      normalizeRoute(match[0].slice(1, -1)),
    ),
  );
  return {
    routes,
    families: uniqueSorted(routes.map(routeFamily)),
  };
}

function appendToCoverageMap(map, key, value) {
  if (!map.has(key)) {
    map.set(key, new Set());
  }
  map.get(key).add(value);
}

function objectFromCoverageMap(map) {
  return Object.fromEntries(
    [...map.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, values]) => [key, [...values].sort()]),
  );
}

function summarizeChecks(summary) {
  if (!summary || typeof summary !== 'object') {
    return null;
  }
  if (summary.checks && typeof summary.checks === 'object') {
    const total = Object.keys(summary.checks).length;
    const passed = Object.values(summary.checks).filter(Boolean).length;
    return { passed, total };
  }
  if (Array.isArray(summary.results)) {
    const total = summary.results.length;
    const passed = summary.results.filter((result) => result.status === 'passed').length;
    return { passed, total };
  }
  return null;
}

export async function generateCoverageManifest(repoRoot, outputPath = null) {
  const actualCrates = await collectWorkspaceCrates(repoRoot);
  const actualPages = await collectDashboardPages(repoRoot);
  const gatewayRouteInventory = await collectGatewayRouteFamilies(repoRoot);
  const dashboardPageInventory = new Map();
  for (const page of actualPages) {
    const key = dashboardCoverageKey(page);
    if (!dashboardPageInventory.has(key)) {
      dashboardPageInventory.set(key, []);
    }
    dashboardPageInventory.get(key).push(page);
  }
  const crateSuiteMap = new Map();
  const pageSuiteMap = new Map();
  const familySuiteMap = new Map();
  const suiteCoverage = {};

  for (const [component, metadata] of Object.entries(LIVE_COMPONENT_METADATA)) {
    const gatewayRoutes = new Set();
    const dashboardPages = new Set();

    for (const scriptPath of metadata.scripts) {
      const extracted = await collectScriptCoverage(repoRoot, scriptPath);
      extracted.gateway_routes.forEach((route) => gatewayRoutes.add(route));
      extracted.dashboard_pages.forEach((page) => dashboardPages.add(page));
    }

    const routeFamilies = uniqueSorted([...gatewayRoutes].map(routeFamily));
    const crates = uniqueSorted(metadata.crates ?? []);
    const pages = uniqueSorted(
      [...dashboardPages].flatMap((pagePath) => dashboardPageInventory.get(dashboardCoverageKey(pagePath)) ?? []),
    );
    const routes = uniqueSorted([...gatewayRoutes]);

    suiteCoverage[component] = {
      aggregate: Boolean(metadata.aggregate),
      scripts: metadata.scripts,
      journeys: metadata.journeys ?? [],
      children: metadata.children ?? [],
      crates,
      dashboard_pages: pages,
      gateway_routes: routes,
      gateway_route_families: routeFamilies,
    };

    if (metadata.aggregate) {
      continue;
    }

    crates.forEach((cratePath) => appendToCoverageMap(crateSuiteMap, cratePath, component));
    pages.forEach((pagePath) => appendToCoverageMap(pageSuiteMap, pagePath, component));
    routeFamilies.forEach((family) => appendToCoverageMap(familySuiteMap, family, component));
  }

  const supportOnlyCrates = uniqueSorted(
    SUPPORT_ONLY_CRATES.filter((cratePath) => actualCrates.includes(cratePath)),
  );
  const coveredCrates = uniqueSorted([...crateSuiteMap.keys()]);
  const coveredPages = uniqueSorted([...pageSuiteMap.keys()]);
  const coveredFamilies = uniqueSorted([...familySuiteMap.keys()]);

  const manifest = {
    generated_at: nowIso(),
    workspace: {
      crate_count: actualCrates.length,
      dashboard_page_count: actualPages.length,
      gateway_route_count: gatewayRouteInventory.routes.length,
      gateway_route_family_count: gatewayRouteInventory.families.length,
    },
    suites: suiteCoverage,
    coverage: {
      crates: {
        covered: coveredCrates,
        uncovered: actualCrates.filter(
          (cratePath) =>
            !coveredCrates.includes(cratePath) && !supportOnlyCrates.includes(cratePath),
        ),
        support_only: supportOnlyCrates,
        suite_map: objectFromCoverageMap(crateSuiteMap),
      },
      dashboard_pages: {
        covered: coveredPages,
        uncovered: actualPages.filter((pagePath) => !coveredPages.includes(pagePath)),
        suite_map: objectFromCoverageMap(pageSuiteMap),
      },
      gateway_route_families: {
        covered: coveredFamilies,
        uncovered: gatewayRouteInventory.families.filter(
          (family) => !coveredFamilies.includes(family),
        ),
        suite_map: objectFromCoverageMap(familySuiteMap),
      },
    },
    inventories: {
      crates: actualCrates,
      dashboard_pages: actualPages,
      gateway_routes: gatewayRouteInventory.routes,
      gateway_route_families: gatewayRouteInventory.families,
    },
  };

  const targetPath = outputPath ?? path.join(repoRoot, REPORTING_DIR, 'coverage-manifest.json');
  await fs.mkdir(path.dirname(targetPath), { recursive: true });
  await writeJson(targetPath, manifest);
  return { path: targetPath, manifest };
}

export async function generateArtifactIndex(repoRoot, outputPath = null) {
  const artifactsRoot = path.join(repoRoot, 'artifacts');
  const targetPath = outputPath ?? path.join(repoRoot, REPORTING_DIR, 'artifact-index.json');
  const entries = await fs.readdir(artifactsRoot, { withFileTypes: true }).catch(() => []);
  const artifactFamilies = entries
    .filter((entry) => entry.isDirectory() && entry.name.startsWith('live-'))
    .map((entry) => entry.name)
    .sort();

  const families = {};
  for (const family of artifactFamilies) {
    const runs = await collectArtifactFamilyRuns(artifactsRoot, family);
    const serializedRuns = runs.map((run) => ({
      run_id: run.run_id,
      artifact_dir: run.artifact_dir,
      summary_path: run.summary_path,
      status: run.status,
      started_at: run.started_at,
      finished_at: run.finished_at,
      duration_ms: run.duration_ms,
      failure_classification: run.failure_classification,
      failure_reason: run.failure_reason,
      check_counts: summarizeChecks(run.summary),
    }));

    families[family] = {
      total_runs: serializedRuns.length,
      passed_runs: serializedRuns.filter((run) => run.status === 'passed').length,
      failed_runs: serializedRuns.filter((run) => run.status === 'failed').length,
      duration_summary: summarizeDurationStats(serializedRuns),
      latest: serializedRuns[0] ?? null,
      latest_success: serializedRuns.find((run) => run.status === 'passed') ?? null,
      latest_failure: serializedRuns.find((run) => run.status === 'failed') ?? null,
      runs: serializedRuns,
    };
  }

  const artifactIndex = {
    generated_at: nowIso(),
    artifact_root: artifactsRoot,
    families,
  };

  await fs.mkdir(path.dirname(targetPath), { recursive: true });
  await writeJson(targetPath, artifactIndex);
  return { path: targetPath, index: artifactIndex };
}

export async function generateLiveReport(repoRoot, outputDir = null) {
  const reportingDir = outputDir ?? path.join(repoRoot, REPORTING_DIR);
  await fs.mkdir(reportingDir, { recursive: true });

  const coverage = await generateCoverageManifest(
    repoRoot,
    path.join(reportingDir, 'coverage-manifest.json'),
  );
  const artifactIndex = await generateArtifactIndex(
    repoRoot,
    path.join(reportingDir, 'artifact-index.json'),
  );

  const report = {
    generated_at: nowIso(),
    coverage_manifest_path: coverage.path,
    artifact_index_path: artifactIndex.path,
    latest_repo_run:
      artifactIndex.index.families['live-repo-suites']?.latest ?? null,
    latest_critical_run:
      artifactIndex.index.families['live-critical-suites']?.latest ?? null,
    uncovered_counts: {
      crates: coverage.manifest.coverage.crates.uncovered.length,
      dashboard_pages: coverage.manifest.coverage.dashboard_pages.uncovered.length,
      gateway_route_families:
        coverage.manifest.coverage.gateway_route_families.uncovered.length,
    },
  };

  const reportPath = path.join(reportingDir, 'live-report.json');
  await writeJson(reportPath, report);

  return {
    report_path: reportPath,
    coverage_manifest_path: coverage.path,
    artifact_index_path: artifactIndex.path,
    report,
  };
}
