import { GhostClient } from '@ghost/sdk';
import type { GhostClientOptions } from '@ghost/sdk';
import { getRuntime } from '$lib/platform/runtime';

let cachedClient: GhostClient | null = null;
let cachedKey = '';
let subscribedToRuntime = false;

async function resolveOptions(): Promise<GhostClientOptions> {
  const runtime = await getRuntime();

  return {
    baseUrl: await runtime.getBaseUrl(),
    token: (await runtime.getToken()) ?? undefined,
    clientName: runtime.kind === 'tauri' ? 'desktop' : 'dashboard',
    clientVersion: __APP_VERSION__,
  };
}

function optionsKey(options: GhostClientOptions): string {
  return [
    options.baseUrl ?? '',
    options.token ?? '',
    options.clientName ?? '',
    options.clientVersion ?? '',
  ].join('::');
}

async function ensureRuntimeSubscription() {
  if (subscribedToRuntime) return;

  const runtime = await getRuntime();
  runtime.subscribeTokenChange(() => {
    invalidateGhostClient();
  });
  subscribedToRuntime = true;
}

export function invalidateGhostClient() {
  cachedClient = null;
  cachedKey = '';
}

export async function getGhostClient(): Promise<GhostClient> {
  await ensureRuntimeSubscription();

  const options = await resolveOptions();
  const key = optionsKey(options);

  if (!cachedClient || cachedKey !== key) {
    cachedClient = new GhostClient(options);
    cachedKey = key;
  }

  return cachedClient;
}
