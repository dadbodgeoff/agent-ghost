# Service Worker Cache Matrix

This matrix defines the dashboard service-worker policy that backs `RG-07`.

| Surface | Strategy | Cached | Queued Offline | Auth boundary behavior |
| --- | --- | --- | --- | --- |
| Static shell (`build`, `files`) | cache-first | yes | n/a | preserved |
| `/api/auth/*` | network-only | no | no | n/a |
| `/api/safety/*` `GET` | network-only | no | no | n/a |
| `/api/safety/*` non-`GET` | network-only with offline `503` guard | no | no | n/a |
| Stale-while-revalidate API paths (`/api/agents`, `/api/convergence`, `/api/costs`, `/api/skills`, `/api/health`, `/api/profiles`) | stale-while-revalidate | only unauthenticated `GET` | no | cleared on `ghost-auth-changed` and `ghost-auth-cleared` |
| Other API `GET` paths | network-first with cache fallback | only unauthenticated `GET` | no | cleared on `ghost-auth-changed` and `ghost-auth-cleared` |
| Pending offline actions (`ghost-pending-actions` IndexedDB) | replay on background sync | n/a | yes, non-safety only | cleared on `ghost-auth-changed` and `ghost-auth-cleared` |
| Replayed offline action with `session_seq` mismatch | replay attempt with `X-Ghost-Expected-Seq` | n/a | discarded on `409` | stale action removed and clients notified |

Notes:

- API responses with an `Authorization` header are never cached.
- Auth-boundary messages clear both API cache entries and queued offline actions.
- Static shell entries are intentionally preserved across auth boundaries.
