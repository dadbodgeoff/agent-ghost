#!/usr/bin/env node
/**
 * Generate TypeScript types from the Ghost OpenAPI specification.
 *
 * Usage:
 *   node scripts/generate-types.mjs
 *
 * Prerequisites:
 *   - `ghost` binary must be built: cargo build --bin ghost
 *   - openapi-typescript installed: pnpm add -D openapi-typescript
 */

import { execFileSync } from 'node:child_process';
import { existsSync, unlinkSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SDK_ROOT = resolve(__dirname, '..');
const REPO_ROOT = resolve(SDK_ROOT, '..', '..');
const GHOST_BINARY = resolve(REPO_ROOT, 'target', 'debug', 'ghost');
const TYPES_OUTPUT = resolve(SDK_ROOT, 'src', 'generated-types.ts');
const TEMP_SPEC = resolve(SDK_ROOT, '.openapi-spec.json');

function ensureGhostBinary() {
  if (existsSync(GHOST_BINARY)) {
    return;
  }

  console.log('Building ghost binary...');
  try {
    execFileSync('cargo', ['build', '--bin', 'ghost'], {
      cwd: REPO_ROOT,
      stdio: 'inherit',
    });
  } catch (err) {
    console.error('Failed to build the ghost binary.');
    console.error(err.message);
    process.exit(1);
  }
}

function dumpOpenApiSpec() {
  console.log('Dumping OpenAPI spec from ghost binary...');
  try {
    return execFileSync(GHOST_BINARY, ['openapi-dump'], {
      cwd: REPO_ROOT,
      encoding: 'utf-8',
      maxBuffer: 10 * 1024 * 1024,
    });
  } catch (err) {
    console.error('Failed to dump OpenAPI spec from the ghost binary.');
    console.error(err.message);
    process.exit(1);
  }
}

ensureGhostBinary();
const spec = dumpOpenApiSpec();
writeFileSync(TEMP_SPEC, spec);

try {
  console.log('Generating TypeScript types...');
  execFileSync('npx', ['openapi-typescript', TEMP_SPEC, '-o', TYPES_OUTPUT], {
    cwd: SDK_ROOT,
    stdio: 'inherit',
  });
} catch (err) {
  console.error('Failed to generate TypeScript types.');
  process.exit(1);
} finally {
  try {
    unlinkSync(TEMP_SPEC);
  } catch {
    // ignore
  }
}

console.log(`Types generated at ${TYPES_OUTPUT}`);
