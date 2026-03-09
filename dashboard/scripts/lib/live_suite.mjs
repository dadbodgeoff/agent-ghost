import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { promises as fs } from 'node:fs';
import path from 'node:path';

import { nowIso } from './live_harness.mjs';

export async function readJsonIfExists(filePath) {
  try {
    return JSON.parse(await fs.readFile(filePath, 'utf8'));
  } catch {
    return null;
  }
}

export async function removeIfPresent(targetPath) {
  if (!targetPath) {
    return;
  }
  await fs.rm(targetPath, { recursive: true, force: true });
}

function relayOutput(targetStream, label, chunk, logStream, buffer) {
  const text = chunk.toString();
  logStream.write(text);
  buffer.push(text);

  for (const line of text.split(/\r?\n/)) {
    if (line.length > 0) {
      targetStream.write(`[${label}] ${line}\n`);
    }
  }
}

export async function runChildSuite({
  repoRoot,
  suiteDir,
  label,
  command = 'node',
  args,
  env = process.env,
}) {
  const logPath = path.join(suiteDir, `${label}.log`);
  const logStream = createWriteStream(logPath, { flags: 'a' });
  const output = [];
  const startedAt = nowIso();
  const startedMs = Date.now();

  const child = spawn(command, args, {
    cwd: repoRoot,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  child.stdout.on('data', (chunk) => {
    relayOutput(process.stdout, label, chunk, logStream, output);
  });
  child.stderr.on('data', (chunk) => {
    relayOutput(process.stderr, label, chunk, logStream, output);
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

  return {
    label,
    status: exitCode === 0 ? 'passed' : 'failed',
    started_at: startedAt,
    finished_at: nowIso(),
    duration_ms: Date.now() - startedMs,
    exit_code: exitCode,
    artifact_dir: childArtifactDir,
    log_path: logPath,
    summary: childSummary,
  };
}
