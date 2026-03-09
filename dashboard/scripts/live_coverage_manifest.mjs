#!/usr/bin/env node

import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { generateCoverageManifest } from './lib/live_reporting.mjs';

function parseArgs(argv) {
  const options = {
    outputPath: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    switch (arg) {
      case '--output':
        options.outputPath = argv[index + 1] ?? null;
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
  process.stdout.write(`Live coverage manifest

Usage:
  pnpm audit:coverage-live [-- --output /abs/path/coverage-manifest.json]
`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const repoRoot = path.resolve(scriptDir, '..', '..');
  const result = await generateCoverageManifest(repoRoot, options.outputPath);

  process.stdout.write(`Live coverage manifest generated\nArtifacts: ${result.path}\n`);
}

await main();
