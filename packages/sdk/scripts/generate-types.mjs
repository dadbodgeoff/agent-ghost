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

import { execSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SDK_ROOT = resolve(__dirname, '..');
const TYPES_OUTPUT = resolve(SDK_ROOT, 'src', 'generated-types.ts');

// Step 1: Dump OpenAPI spec from the ghost binary
console.log('Dumping OpenAPI spec from ghost binary...');
let spec;
try {
  spec = execSync('cargo run --bin ghost -- openapi-dump 2>/dev/null', {
    cwd: resolve(SDK_ROOT, '..', '..'),
    encoding: 'utf-8',
    maxBuffer: 10 * 1024 * 1024,
  });
} catch (err) {
  console.error('Failed to dump OpenAPI spec. Is the ghost binary built?');
  console.error(err.message);
  process.exit(1);
}

// Step 2: Write temp spec file (openapi-typescript needs a file or URL)
const tempSpec = resolve(SDK_ROOT, '.openapi-spec.json');
writeFileSync(tempSpec, spec);

// Step 3: Generate TypeScript types
console.log('Generating TypeScript types...');
try {
  execSync(
    `npx openapi-typescript ${tempSpec} -o ${TYPES_OUTPUT}`,
    { cwd: SDK_ROOT, stdio: 'inherit' },
  );
} catch (err) {
  console.error('Failed to generate TypeScript types.');
  process.exit(1);
}

// Step 4: Clean up temp file
try {
  execSync(`rm ${tempSpec}`);
} catch {
  // ignore
}

console.log(`Types generated at ${TYPES_OUTPUT}`);
