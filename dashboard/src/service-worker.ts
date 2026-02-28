/// <reference lib="webworker" />

/**
 * GHOST Dashboard Service Worker — PWA offline support + push notifications.
 *
 * Caching strategy:
 *   - Static assets (HTML, CSS, JS): cache-first
 *   - API calls: network-first with fallback to cached last-known state
 *
 * Push events: convergence alerts, kill switch activations, proposal approvals.
 */

declare const self: ServiceWorkerGlobalScope;

import { build, files, version } from '$service-worker';

const CACHE_NAME = `ghost-cache-${version}`;

// Static assets to pre-cache (shell).
const PRECACHE_ASSETS = [...build, ...files];

// ── Install ─────────────────────────────────────────────────────────────

self.addEventListener('install', (event: ExtendableEvent) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE_ASSETS))
  );
  // Activate immediately without waiting for existing clients to close.
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
  // Claim all open clients so the new SW takes effect immediately.
  self.clients.claim();
});

// ── Fetch ───────────────────────────────────────────────────────────────

self.addEventListener('fetch', (event: FetchEvent) => {
  const url = new URL(event.request.url);

  // API calls: network-first, fallback to cache.
  if (url.pathname.startsWith('/api/')) {
    event.respondWith(networkFirstWithCache(event.request));
    return;
  }

  // Static assets: cache-first, fallback to network.
  if (event.request.method === 'GET') {
    event.respondWith(cacheFirstWithNetwork(event.request));
    return;
  }
});

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
    // Offline fallback: return the app shell.
    const fallback = await caches.match('/');
    return fallback ?? new Response('Offline', { status: 503 });
  }
}

async function networkFirstWithCache(request: Request): Promise<Response> {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE_NAME);
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const cached = await caches.match(request);
    return cached ?? new Response(JSON.stringify({ error: 'offline' }), {
      status: 503,
      headers: { 'Content-Type': 'application/json' },
    });
  }
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
