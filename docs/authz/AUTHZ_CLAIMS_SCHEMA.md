# Authz Claims Schema

Status: active runtime schema on March 9, 2026.

This file defines the privilege wire contract for gateway-issued JWTs and the
compatibility rules for legacy role-only claims.

## Current Runtime Shape

The live `Claims` struct now carries the following fields:

```json
{
  "sub": "string",
  "role": "viewer|operator|admin|superadmin|dev",
  "capabilities": ["string"],
  "authz_v": 1,
  "exp": 0,
  "iat": 0,
  "jti": "string",
  "iss": "string|null"
}
```

Field rules:

- `sub`: subject identifier.
- `role`: base role string for backward compatibility. `dev` remains local
  no-auth compatibility only and normalizes to `BaseRole::Operator`.
- `capabilities`: optional capability names. Empty for role-only principals.
- `authz_v`: optional claim version marker. `1` is the first typed schema.
- `exp`: unix expiry.
- `iat`: unix issued-at.
- `jti`: token identifier.
- `iss`: optional issuer. Gateway-issued typed JWTs use `ghost-gateway`.

## Typed V1 Contract

Gateway-issued JWTs now emit `AuthzClaimsV1` semantics:

```json
{
  "sub": "admin",
  "role": "admin",
  "capabilities": [],
  "authz_v": 1,
  "exp": 1760000000,
  "iat": 1759999100,
  "jti": "018f...",
  "iss": "ghost-gateway"
}
```

Rules:

- `authz_v = 1` means the token is parsed through the typed authz model.
- Capability names are closed-enum values.
- `security_reviewer` is accepted only as a compatibility alias and normalizes
  to canonical capability `safety_review`.
- Unknown capability names are authorization failures.
- Unknown roles are authorization failures.

## Legacy Compatibility Window

The gateway still accepts legacy role-only claims with no `authz_v`:

```json
{
  "sub": "legacy-token-user",
  "role": "admin",
  "exp": 18446744073709551615,
  "iat": 0,
  "jti": ""
}
```

Compatibility rules:

- missing `authz_v` means legacy normalization
- legacy claims must not carry non-empty `capabilities`
- legacy claims normalize to `Principal.authz_version = 0`
- no-auth fallback claims also remain legacy and normalize to
  `AuthMode::NoAuthDev`

## Issuance Rules

- JWT login issues typed v1 claims with empty capabilities.
- JWT refresh preserves typed capabilities and issuer if present.
- JWT refresh upgrades old role-only JWTs to typed v1 with empty capabilities.
- legacy bearer token mode does not mint JWTs and still injects compatibility
  fallback claims in middleware.
- no-auth dev mode does not mint JWTs and still injects compatibility fallback
  claims in middleware.

## Current Capability Set

| Capability | Meaning | Notes |
| --- | --- | --- |
| `safety_review` | narrow authority for quarantine review and related safety inspection flows | canonical name |

## Compatibility Constraints

- `authz_v` is required before non-empty capabilities may be trusted.
- typed authz parsing is fail-closed.
- unsupported future versions must deny until an explicit parser exists.
