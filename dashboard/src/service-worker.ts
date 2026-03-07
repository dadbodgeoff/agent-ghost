/// <reference lib="webworker" />

/**
 * GHOST Dashboard Service Worker — PWA offline support + push notifications.
 *
 * Caching strategy (tiered — T-4.7.1):
 *   - Static assets (HTML, CSS, JS): cache-first (pre-cached shell)
 *   - Stale-while-revalidate: /api/agents, /api/convergence, /api/costs, /api/skills
 *   - Network-first: /api/audit, /api/sessions, /api/memory, /api/goals, /api/workflows
 *   - NEVER cached: /api/safety/* writes — returns 503 when offline (T-4.7.2)
 *
 * Push events: convergence alerts, kill switch activations, proposal approvals.
 * Background sync: queued non-safety actions replayed on reconnect (T-4.7.3).
 */

declare const self: ServiceWorkerGlobalScope;

import { build, files, version } from '$service-worker';

const CACHE_NAME = `ghost-cache-${version}`;

// Static assets to pre-cache (shell).
const PRECACHE_ASSETS = [...build, ...files];

// API paths eligible for stale-while-revalidate.
const STALE_REVALIDATE_PATHS = [
  '/api/agents',
  '/api/convergence',
  '/api/costs',
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
  if (
    event.data?.type === 'ghost-auth-cleared' ||
    event.data?.type === 'ghost-auth-changed'
  ) {
    event.waitUntil(clearAuthenticatedState());
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

async function clearPendingActions(): Promise<void> {
  const db = await openPendingActionsDB().catch(() => null);
  if (!db) return;

  await new Promise<void>((resolve) => {
    const tx = db.transaction('pending_actions', 'readwrite');
    tx.objectStore('pending_actions').clear();
    tx.oncomplete = () => resolve();
    tx.onabort = () => resolve();
    tx.onerror = () => resolve();
  });

  db.close();
}

async function clearAuthenticatedState(): Promise<void> {
  await Promise.all([clearAuthenticatedApiCache(), clearPendingActions()]);
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

// ── Background Sync (T-4.7.3) ──────────────────────────────────────────

self.addEventListener('sync' as any, (event: any) => {
  if (event.tag === 'ghost-pending-actions') {
    event.waitUntil(replayPendingActions());
  }
});

async function replayPendingActions(): Promise<void> {
  // Open IndexedDB to read queued actions.
  const db = await openPendingActionsDB();
  const tx = db.transaction('pending_actions', 'readonly');
  const store = tx.objectStore('pending_actions');
  const request = store.getAll();

  return new Promise((resolve) => {
    request.onsuccess = async () => {
      const actions: PendingAction[] = request.result ?? [];

      for (const action of actions) {
        // NEVER replay safety actions — hard rule.
        if (isSafetyPath(action.url)) continue;

        try {
          // WP9-N: Include session sequence for staleness detection.
          const headers: Record<string, string> = { ...action.headers };
          if (action.session_seq != null) {
            headers['X-Ghost-Expected-Seq'] = String(action.session_seq);
          }

          const resp = await fetch(action.url, {
            method: action.method,
            headers,
            body: action.body,
          });

          // WP9-N: 409 Conflict = session changed while offline — discard stale action.
          if (resp.status === 409) {
            const deleteTx = db.transaction('pending_actions', 'readwrite');
            deleteTx.objectStore('pending_actions').delete(action.id);
            // Notify user via postMessage to all clients.
            const clients = await self.clients.matchAll({ type: 'window' });
            for (const client of clients) {
              client.postMessage({
                type: 'ghost-sync-conflict',
                message: 'Message outdated — session changed while offline',
                actionId: action.id,
              });
            }
            continue;
          }

          // Remove from queue on success.
          const deleteTx = db.transaction('pending_actions', 'readwrite');
          deleteTx.objectStore('pending_actions').delete(action.id);
        } catch {
          // Will retry on next sync event.
          break;
        }
      }
      resolve();
    };
    request.onerror = () => resolve();
  });
}

function openPendingActionsDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('ghost-pending-actions', 1);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains('pending_actions')) {
        db.createObjectStore('pending_actions', { keyPath: 'id', autoIncrement: true });
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

interface PendingAction {
  id: number;
  url: string;
  method: string;
  headers: Record<string, string>;
  body: string | null;
  /** WP9-N: Session sequence at time of queuing — for staleness detection. */
  session_seq?: number;
}
