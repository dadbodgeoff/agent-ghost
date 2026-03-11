# Authz Compatibility Deprecation Checklist

Status: route and middleware cutover completed on March 9, 2026.

## Implemented

- [x] typed authz core exists in `api/authz.rs`
- [x] pure policy registry exists in `api/authz_policy.rs`
- [x] gateway-issued JWTs include `authz_v = 1`
- [x] gateway-issued JWTs include canonical `iss = ghost-gateway`
- [x] non-public routes bind through `RouteAuthorizationSpec`
- [x] live execution visibility uses typed owner-aware middleware
- [x] raw handler-local role auth checks are removed from safety, admin, and provider-key flows
- [x] legacy-vs-typed shadow scaffolding has been deleted after cutover
- [x] the typed authorizer is now the only runtime authorizer
- [x] legacy role-only claims remain accepted
- [x] legacy claims with non-empty capabilities fail closed

## Historical Rollout Evidence

- [x] canonical backup/admin equivalence was covered before cutover
- [x] `security_reviewer` compatibility was covered before cutover
- [x] owner-visible live execution equivalence was covered before cutover
- [x] the shadow diff artifact is closed

## Remaining Compatibility Cleanup

- [ ] stop accepting role-only JWTs with missing `authz_v` after token migration evidence exists
- [ ] remove compatibility alias `security_reviewer` after claim issuance audit proves it is gone
