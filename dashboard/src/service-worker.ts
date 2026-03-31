/// <reference lib="webworker" />

/**
 * GHOST Dashboard Service Worker — PWA offline support + push notifications.
 *
 * Caching strategy (tiered — T-4.7.1):
 *   - Static assets (HTML, CSS, JS): cache-first (pre-cached shell)
 *   - Stale-while-revalidate: /api/agents, /api/convergence, /api/skills
 *   - Network-first: /api/audit, /api/sessions, /api/memory, /api/goals, /api/workflows
 *   - NEVER queued: /api/safety/* writes and proposal decisions — returns 503 when offline
 *
 * Push events: convergence alerts, kill switch activations, proposal approvals.
 * Background sync: queued non-safety actions replayed on reconnect (T-4.7.3).
 */

declare const self: ServiceWorkerGlobalScope;

import { build, files, version } from '$service-worker';

const CACHE_NAME = `ghost-cache-${version}`;
const PENDING_ACTIONS_DB = 'ghost-pending-actions';
const PENDING_ACTIONS_DB_VERSION = 2;
const PENDING_ACTIONS_STORE = 'pending_actions';
const AUTH_STATE_STORE = 'auth_state';
const AUTH_STATE_KEY = 'active';
let activeReplayAuthState: ReplayAuthState | null = null;

interface SyncCapableServiceWorkerRegistration extends ServiceWorkerRegistration {
  sync?: {
    register(tag: string): Promise<void>;
  };
}

interface SyncEvent extends ExtendableEvent {
  tag: string;
}

// Static assets to pre-cache (shell).
const PRECACHE_ASSETS = [...build, ...files];

// API paths eligible for stale-while-revalidate.
const STALE_REVALIDATE_PATHS = [
  '/api/agents',
  '/api/convergence',
  '/api/skills',
  '/api/health',
  '/api/profiles',
];

// Safety paths — NEVER cached, NEVER queued (hard rule T-4.7.2).
const SAFETY_PATHS = ['/api/safety/'];

// ── Install ─────────────────────────────────────────────────────────────

self.addEventListener('install', (event: ExtendableEvent) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE_ASSETS))
  );
  self.skipWaiting();
});

// ── Activate ────────────────────────────────────────────────────────────

self.addEventListener('activate', (event: ExtendableEvent) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key !== CACHE_NAME)
          .map((key) => caches.delete(key))
      )
    )
  );
  self.clients.claim();
});

self.addEventListener('message', (event: ExtendableMessageEvent) => {
  if (event.data?.type === 'ghost-auth-session' && event.data?.auth) {
    activeReplayAuthState = event.data.auth as ReplayAuthState;
    event.waitUntil(persistReplayAuthState(event.data.auth as ReplayAuthState));
    return;
  }
  if (event.data?.type === 'ghost-auth-changed') {
    if (event.data?.auth) {
      activeReplayAuthState = event.data.auth as ReplayAuthState;
      event.waitUntil(handleAuthChanged(event.data.auth as ReplayAuthState));
      return;
    }
    activeReplayAuthState = null;
    event.waitUntil(handleAuthCleared());
    return;
  }
  if (event.data?.type === 'ghost-auth-cleared') {
    activeReplayAuthState = null;
    event.waitUntil(handleAuthCleared());
    return;
  }
  if (event.data?.type === 'ghost-replay-pending-actions') {
    event.waitUntil(replayPendingActions());
  }
});

// ── Fetch ───────────────────────────────────────────────────────────────

self.addEventListener('fetch', (event: FetchEvent) => {
  const url = new URL(event.request.url);

  // Safety endpoints: NEVER cache writes, block offline mutations.
  if (isSafetyPath(url.pathname)) {
    if (event.request.method !== 'GET') {
      event.respondWith(safetyWriteGuard(event.request));
      return;
    }
    // GET safety status: network-first, no caching.
    event.respondWith(networkOnly(event.request));
    return;
  }

  // API calls: tiered strategy.
  if (url.pathname.startsWith('/api/')) {
    if (isAuthPath(url.pathname)) {
      event.respondWith(networkOnly(event.request));
      return;
    }
    if (isProposalDecisionPath(event.request.url)) {
      event.respondWith(proposalDecisionWriteGuard(event.request));
      return;
    }
    if (event.request.method !== 'GET' && event.request.method !== 'HEAD') {
      event.respondWith(networkWriteWithQueue(event.request));
      return;
    }
    if (isStaleRevalidatePath(url.pathname)) {
      event.respondWith(staleWhileRevalidate(event.request));
    } else {
      event.respondWith(networkFirstWithCache(event.request));
    }
    return;
  }

  // Static assets: cache-first.
  if (event.request.method === 'GET') {
    event.respondWith(cacheFirstWithNetwork(event.request));
    return;
  }
});

function isSafetyPath(pathname: string): boolean {
  return SAFETY_PATHS.some((p) => pathname.startsWith(p));
}

function isStaleRevalidatePath(pathname: string): boolean {
  return STALE_REVALIDATE_PATHS.some((p) => pathname.startsWith(p));
}

function isAuthPath(pathname: string): boolean {
  return pathname.startsWith('/api/auth/');
}

function isProposalDecisionPath(pathOrUrl: string): boolean {
  const normalized = pathOrUrl.toLowerCase().replace(/\/+$/, '');
  return (
    normalized.includes('/api/goals/') &&
    (/\/approve(?:[/?#]|$)/.test(normalized) || /\/reject(?:[/?#]|$)/.test(normalized))
  );
}

function isCacheableApiRequest(request: Request): boolean {
  return request.method === 'GET' && !request.headers.has('Authorization');
}

// ── Caching strategies ──────────────────────────────────────────────────

async function cacheFirstWithNetwork(request: Request): Promise<Response> {
  const cached = await caches.match(request);
  if (cached) return cached;

  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE_NAME);
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const fallback = await caches.match('/');
    return fallback ?? new Response('Offline', { status: 503 });
  }
}

async function networkFirstWithCache(request: Request): Promise<Response> {
  try {
    const response = await fetch(request);
    if (response.ok && isCacheableApiRequest(request)) {
      const cache = await caches.open(CACHE_NAME);
      // Add last-sync timestamp header to cached response.
      const headers = new Headers(response.headers);
      headers.set('X-Ghost-Last-Sync', new Date().toISOString());
      const cachedResponse = new Response(await response.clone().arrayBuffer(), {
        status: response.status,
        statusText: response.statusText,
        headers,
      });
      cache.put(request, cachedResponse);
    }
    return response;
  } catch {
    const cached = isCacheableApiRequest(request) ? await caches.match(request) : undefined;
    return cached ?? new Response(JSON.stringify({ error: 'offline' }), {
      status: 503,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}

async function staleWhileRevalidate(request: Request): Promise<Response> {
  const cache = await caches.open(CACHE_NAME);
  const cached = isCacheableApiRequest(request) ? await cache.match(request) : undefined;

  // Start network fetch in background.
  const networkFetch = fetch(request).then(async (response) => {
    if (response.ok && isCacheableApiRequest(request)) {
      const headers = new Headers(response.headers);
      headers.set('X-Ghost-Last-Sync', new Date().toISOString());
      const cachedResponse = new Response(await response.clone().arrayBuffer(), {
        status: response.status,
        statusText: response.statusText,
        headers,
      });
      cache.put(request, cachedResponse);
    }
    return response;
  }).catch(() => null);

  // If we have a cached version, return it immediately.
  if (cached) {
    // Fire and forget the revalidation.
    networkFetch;
    return cached;
  }

  // No cache — wait for network.
  const response = await networkFetch;
  return response ?? new Response(JSON.stringify({ error: 'offline' }), {
    status: 503,
    headers: { 'Content-Type': 'application/json' },
  });
}

async function clearAuthenticatedApiCache(): Promise<void> {
  const cache = await caches.open(CACHE_NAME);
  const keys = await cache.keys();

  await Promise.all(
    keys
      .filter((request) => new URL(request.url).pathname.startsWith('/api/'))
      .map((request) => cache.delete(request)),
  );
}

async function persistReplayAuthState(auth: ReplayAuthState): Promise<void> {
  activeReplayAuthState = auth;
  const db = await openPendingActionsDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(AUTH_STATE_STORE, 'readwrite');
    tx.objectStore(AUTH_STATE_STORE).put({ key: AUTH_STATE_KEY, ...auth });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
  db.close();
}

async function loadReplayAuthState(): Promise<ReplayAuthState | null> {
  if (activeReplayAuthState) return activeReplayAuthState;

  const db = await openPendingActionsDB();
  const auth = await new Promise<ReplayAuthState | null>((resolve, reject) => {
    const tx = db.transaction(AUTH_STATE_STORE, 'readonly');
    const req = tx.objectStore(AUTH_STATE_STORE).get(AUTH_STATE_KEY);
    req.onsuccess = () => {
      const value = req.result as (ReplayAuthState & { key: string }) | undefined;
      if (!value) {
        resolve(null);
        return;
      }
      resolve({
        client_id: value.client_id,
        session_epoch: value.session_epoch,
        token: value.token,
      });
    };
    req.onerror = () => reject(req.error);
  }).catch(() => null);
  db.close();
  activeReplayAuthState = auth;
  return auth;
}

async function clearAuthenticatedState(): Promise<void> {
  await clearAuthenticatedApiCache();
  await clearDurableReplayState();
}

async function handleAuthChanged(auth: ReplayAuthState): Promise<void> {
  await clearAuthenticatedApiCache();
  await replaceReplayAuthState(auth);
}

async function handleAuthCleared(): Promise<void> {
  await clearAuthenticatedState();
}

async function networkOnly(request: Request): Promise<Response> {
  try {
    return await fetch(request);
  } catch {
    return new Response(JSON.stringify({ error: 'offline' }), {
      status: 503,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}

/**
 * Safety write guard: blocks all non-GET safety requests when offline.
 * Safety actions (kill, pause, quarantine) MUST NEVER be queued or cached.
 */
async function safetyWriteGuard(request: Request): Promise<Response> {
  try {
    return await fetch(request);
  } catch {
    return new Response(
      JSON.stringify({
        error: 'Safety actions require network connection',
        offline: true,
        message: 'Cannot execute safety actions while offline. Please reconnect and try again.',
      }),
      {
        status: 503,
        headers: { 'Content-Type': 'application/json' },
      }
    );
  }
}

async function proposalDecisionWriteGuard(request: Request): Promise<Response> {
  try {
    return await fetch(request);
  } catch {
    return new Response(
      JSON.stringify({
        error: 'Proposal decisions require network connection',
        offline: true,
        message: 'Cannot approve or reject proposals while offline. Reconnect and retry the decision.',
      }),
      {
        status: 503,
        headers: { 'Content-Type': 'application/json' },
      }
    );
  }
}

async function networkWriteWithQueue(request: Request): Promise<Response> {
  if (isProposalDecisionPath(request.url)) {
    return proposalDecisionWriteGuard(request);
  }

  const replayRequest = request.clone();
  try {
    const response = await fetch(request);
    if (response.status !== 503) {
      return response;
    }

    const queued = await queuePendingAction(replayRequest);
    if (!queued) {
      return response;
    }

    try {
      await registerPendingActionSync();
    } catch {
      // SyncManager is optional; queued actions can still be replayed explicitly.
    }

    return new Response(
      JSON.stringify({
        queued: true,
        offline: true,
        message: 'Action queued for replay when connectivity returns.',
      }),
      {
        status: 202,
        headers: { 'Content-Type': 'application/json' },
      },
    );
  } catch {
    const queued = await queuePendingAction(replayRequest);
    if (!queued) {
      return new Response(JSON.stringify({ error: 'offline' }), {
        status: 503,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    try {
      await registerPendingActionSync();
    } catch {
      // SyncManager is optional; queued actions can still be replayed explicitly.
    }

    return new Response(
      JSON.stringify({
        queued: true,
        offline: true,
        message: 'Action queued for replay when connectivity returns.',
      }),
      {
        status: 202,
        headers: { 'Content-Type': 'application/json' },
      },
    );
  }
}

async function registerPendingActionSync(): Promise<void> {
  const registration = self.registration as SyncCapableServiceWorkerRegistration;
  await registration.sync?.register('ghost-pending-actions');
}

// ── Background Sync (T-4.7.3) ──────────────────────────────────────────

self.addEventListener('sync', (event: Event) => {
  const syncEvent = event as SyncEvent;
  if (syncEvent.tag === 'ghost-pending-actions') {
    syncEvent.waitUntil(replayPendingActions());
  }
});

async function replayPendingActions(): Promise<void> {
  const db = await openPendingActionsDB();
  const activeAuth = await loadReplayAuthState();
  if (!activeAuth) {
    db.close();
    return;
  }

  const tx = db.transaction(PENDING_ACTIONS_STORE, 'readonly');
  const store = tx.objectStore(PENDING_ACTIONS_STORE);
  const request = store.getAll();

  return new Promise((resolve) => {
    request.onsuccess = async () => {
      const actions: PendingAction[] = request.result ?? [];

      for (const action of actions) {
        if (isSafetyPath(action.url)) {
          await deletePendingAction(db, action.id);
          continue;
        }
        if (isProposalDecisionPath(action.url)) {
          await deletePendingAction(db, action.id);
          continue;
        }
        if (
          action.client_id !== activeAuth.client_id ||
          action.session_epoch !== activeAuth.session_epoch
        ) {
          await deletePendingAction(db, action.id);
          continue;
        }

        try {
          const headers: Record<string, string> = { ...action.headers };
          if (activeAuth.token) {
            headers.Authorization = `Bearer ${activeAuth.token}`;
          }
          if (action.operation_envelope.request_id) {
            headers['X-Request-ID'] = action.operation_envelope.request_id;
          }
          if (action.operation_envelope.operation_id) {
            headers['X-Ghost-Operation-ID'] = action.operation_envelope.operation_id;
          }
          if (action.operation_envelope.idempotency_key) {
            headers['Idempotency-Key'] = action.operation_envelope.idempotency_key;
          }

          const resp = await fetch(action.url, {
            method: action.method,
            headers,
            body: action.body,
          });

          if (resp.status === 409) {
            await deletePendingAction(db, action.id);
            await broadcastToClients({
              type: 'ghost-sync-conflict',
              message: 'Queued action is stale for the current session.',
              actionId: action.id,
            });
            continue;
          }

          if (resp.status === 401 || resp.status === 403) {
            await clearAuthenticatedState();
            await broadcastToClients({
              type: 'ghost-sync-auth-revoked',
              message: 'Queued actions were dropped because the current session is no longer valid.',
            });
            continue;
          }

          if (resp.ok) {
            await deletePendingAction(db, action.id);
            continue;
          }

          if (resp.status >= 400 && resp.status < 500) {
            await deletePendingAction(db, action.id);
            continue;
          }
        } catch {
          // Will retry on next sync event.
          break;
        }
      }
      db.close();
      resolve();
    };
    request.onerror = () => {
      db.close();
      resolve();
    };
  });
}

async function queuePendingAction(request: Request): Promise<boolean> {
  if (isProposalDecisionPath(request.url)) return false;

  const auth = await loadReplayAuthState();
  if (!auth?.token) return false;

  const db = await openPendingActionsDB();
  const action: Omit<PendingAction, 'id'> = {
    url: request.url,
    method: request.method,
    headers: cloneReplayHeaders(request.headers),
    body: await readReplayBody(request),
    client_id: auth.client_id,
    session_epoch: auth.session_epoch,
    operation_envelope: buildOperationEnvelope(request),
  };

  let stored = true;
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(PENDING_ACTIONS_STORE, 'readwrite');
    tx.objectStore(PENDING_ACTIONS_STORE).add(action);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  }).catch(() => {
    stored = false;
  });

  db.close();
  return stored;
}

async function readReplayBody(request: Request): Promise<string | null> {
  if (request.method === 'GET' || request.method === 'HEAD') {
    return null;
  }

  try {
    return await request.text();
  } catch {
    return null;
  }
}

function cloneReplayHeaders(headers: Headers): Record<string, string> {
  const replayHeaders: Record<string, string> = {};
  for (const [key, value] of headers.entries()) {
    const lower = key.toLowerCase();
    if (
      lower === 'authorization' ||
      lower === 'x-request-id' ||
      lower === 'x-ghost-operation-id' ||
      lower === 'idempotency-key'
    ) {
      continue;
    }
    replayHeaders[key] = value;
  }
  return replayHeaders;
}

function buildOperationEnvelope(request: Request): OperationEnvelope {
  const requestId = request.headers.get('X-Request-ID') ?? crypto.randomUUID();
  const operationId = request.headers.get('X-Ghost-Operation-ID') ?? crypto.randomUUID();
  const idempotencyKey = request.headers.get('Idempotency-Key') ?? crypto.randomUUID();

  return {
    request_id: requestId,
    operation_id: operationId,
    idempotency_key: idempotencyKey,
  };
}

async function deletePendingAction(db: IDBDatabase, id: number): Promise<void> {
  await new Promise<void>((resolve) => {
    const tx = db.transaction(PENDING_ACTIONS_STORE, 'readwrite');
    tx.objectStore(PENDING_ACTIONS_STORE).delete(id);
    tx.oncomplete = () => resolve();
    tx.onabort = () => resolve();
    tx.onerror = () => resolve();
  });
}

async function broadcastToClients(message: Record<string, unknown>): Promise<void> {
  const clients = await self.clients.matchAll({ type: 'window' });
  for (const client of clients) {
    client.postMessage(message);
  }
}

async function replaceReplayAuthState(auth: ReplayAuthState): Promise<void> {
  activeReplayAuthState = auth;
  const db = await openPendingActionsDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction([PENDING_ACTIONS_STORE, AUTH_STATE_STORE], 'readwrite');
    tx.objectStore(PENDING_ACTIONS_STORE).clear();
    tx.objectStore(AUTH_STATE_STORE).put({ key: AUTH_STATE_KEY, ...auth });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
  db.close();
}

async function clearDurableReplayState(): Promise<void> {
  activeReplayAuthState = null;
  const db = await openPendingActionsDB().catch(() => null);
  if (!db) return;

  await new Promise<void>((resolve) => {
    const tx = db.transaction([PENDING_ACTIONS_STORE, AUTH_STATE_STORE], 'readwrite');
    tx.objectStore(PENDING_ACTIONS_STORE).clear();
    tx.objectStore(AUTH_STATE_STORE).delete(AUTH_STATE_KEY);
    tx.oncomplete = () => resolve();
    tx.onabort = () => resolve();
    tx.onerror = () => resolve();
  });

  db.close();
}

function openPendingActionsDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(PENDING_ACTIONS_DB, PENDING_ACTIONS_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(PENDING_ACTIONS_STORE)) {
        db.createObjectStore(PENDING_ACTIONS_STORE, { keyPath: 'id', autoIncrement: true });
      }
      if (!db.objectStoreNames.contains(AUTH_STATE_STORE)) {
        db.createObjectStore(AUTH_STATE_STORE, { keyPath: 'key' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

// ── Push Notifications ──────────────────────────────────────────────────

self.addEventListener('push', (event: PushEvent) => {
  if (!event.data) return;

  const data = event.data.json() as PushPayload;

  const options: NotificationOptions = {
    body: data.body,
    icon: '/icons/ghost-192.png',
    badge: '/icons/ghost-192.png',
    tag: data.tag ?? 'ghost-notification',
    data: { url: data.url ?? '/' },
  };

  event.waitUntil(self.registration.showNotification(data.title, options));
});

self.addEventListener('notificationclick', (event: NotificationEvent) => {
  event.notification.close();
  const url = (event.notification.data as { url?: string })?.url ?? '/';
  event.waitUntil(self.clients.openWindow(url));
});

// ── Types ───────────────────────────────────────────────────────────────

interface PushPayload {
  title: string;
  body: string;
  tag?: string;
  url?: string;
}

interface SyncEvent extends ExtendableEvent {
  tag: string;
}

interface ReplayAuthState {
  client_id: string;
  session_epoch: number;
  token: string | null;
}

interface OperationEnvelope {
  request_id: string;
  operation_id: string;
  idempotency_key: string;
}

interface PendingAction {
  id: number;
  url: string;
  method: string;
  headers: Record<string, string>;
  body: string | null;
  client_id: string;
  session_epoch: number;
  operation_envelope: OperationEnvelope;
}
