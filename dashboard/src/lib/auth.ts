/**
 * Transitional auth facade.
 *
 * New code should prefer the platform runtime directly, but this module remains
 * as the compatibility boundary while the dashboard is still migrating off
 * `$lib/api` and route-local auth handling.
 */

import { invalidateGhostClient } from '$lib/ghost-client';
import { getRuntime } from '$lib/platform/runtime';

let cachedToken: string | null = null;

export async function getToken(): Promise<string | null> {
  const runtime = await getRuntime();
  cachedToken = await runtime.getToken();
  return cachedToken;
}

export async function setToken(token: string): Promise<void> {
  const runtime = await getRuntime();
  await runtime.setToken(token);
  cachedToken = token;
  invalidateGhostClient();
}

export async function clearToken(): Promise<void> {
  const runtime = await getRuntime();
  await runtime.clearToken();
  cachedToken = null;
  invalidateGhostClient();
}

export function isAuthenticated(): boolean {
  return cachedToken !== null;
}
