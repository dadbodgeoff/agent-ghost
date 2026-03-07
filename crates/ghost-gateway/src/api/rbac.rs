//! Role-based access control middleware (Task 1.12).
//!
//! Roles form a hierarchy: Viewer < Operator < Admin < SuperAdmin.
//! The special "dev" role (assigned in no-auth mode) maps to Operator
//! for backward compatibility — it can read and write, but cannot
//! access safety or admin endpoints.
//!
//! Each route group requires a minimum role level. The RBAC middleware
//! reads the role from JWT `Claims` stored in request extensions by the
//! auth middleware.
//!
//! Usage in router construction:
//! ```ignore
//! let admin_routes = Router::new()
//!     .route("/api/admin/backup", post(admin::create_backup))
//!     .route_layer(axum::middleware::from_fn(rbac::admin));
//! ```

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};

use crate::api::error::ApiError;

/// Role hierarchy -- higher ordinal = more privilege.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer = 0,
    Operator = 1,
    Admin = 2,
    SuperAdmin = 3,
}

impl Role {
    /// Parse a role string (as stored in JWT claims) into a `Role`.
    ///
    /// Recognized values: `"viewer"`, `"operator"`, `"admin"`, `"superadmin"`.
    /// The special `"dev"` role (used in no-auth dev mode) maps to `Operator`
    /// so that unauthenticated local development can still create agents and
    /// run sessions, but cannot access safety or admin endpoints.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "viewer" => Some(Role::Viewer),
            "operator" => Some(Role::Operator),
            "dev" => Some(Role::Operator),
            "admin" => Some(Role::Admin),
            "superadmin" => Some(Role::SuperAdmin),
            _ => None,
        }
    }
}

/// Core RBAC enforcement. Reads the `Claims` from request extensions
/// (set by the auth middleware) and checks that the user's role meets
/// or exceeds the required minimum.
async fn require_role(
    minimum: Role,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Get claims from request extensions (set by auth middleware).
    let claims = req.extensions().get::<crate::api::auth::Claims>();

    let user_role = match claims {
        Some(c) => match Role::from_str(&c.role) {
            Some(role) => role,
            None => {
                tracing::warn!(
                    role = %c.role,
                    "Unrecognized role in JWT claims — defaulting to Viewer"
                );
                Role::Viewer
            }
        },
        None => {
            // No claims means the auth middleware didn't inject them.
            // This should only happen if auth is completely disabled AND
            // the request somehow bypassed the auth middleware. Treat as
            // unauthorized in all cases.
            return Err(ApiError::Unauthorized(
                "No authentication credentials provided".into(),
            ));
        }
    };

    if user_role < minimum {
        tracing::debug!(
            required = ?minimum,
            actual = ?user_role,
            "RBAC check failed"
        );
        return Err(ApiError::Forbidden(
            "Insufficient permissions for this operation".into(),
        ));
    }

    Ok(next.run(req).await)
}

// ---- Convenience middleware functions for use with `from_fn` ----
//
// Usage: `.route_layer(axum::middleware::from_fn(rbac::operator))`

/// Middleware that requires at least `Viewer` role.
/// Effectively just checks that the user is authenticated.
pub async fn viewer(req: Request, next: Next) -> Result<Response, ApiError> {
    require_role(Role::Viewer, req, next).await
}

/// Middleware that requires at least `Operator` role.
/// Use for write operations: creating agents, running sessions, etc.
pub async fn operator(req: Request, next: Next) -> Result<Response, ApiError> {
    require_role(Role::Operator, req, next).await
}

/// Middleware that requires at least `Admin` role.
/// Use for safety endpoints, provider key management, admin operations.
pub async fn admin(req: Request, next: Next) -> Result<Response, ApiError> {
    require_role(Role::Admin, req, next).await
}

/// Middleware that requires `SuperAdmin` role.
/// Use for the most destructive operations (kill-all, data restore).
pub async fn superadmin(req: Request, next: Next) -> Result<Response, ApiError> {
    require_role(Role::SuperAdmin, req, next).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering() {
        assert!(Role::Viewer < Role::Operator);
        assert!(Role::Operator < Role::Admin);
        assert!(Role::Admin < Role::SuperAdmin);
    }

    #[test]
    fn role_from_str_known() {
        assert_eq!(Role::from_str("viewer"), Some(Role::Viewer));
        assert_eq!(Role::from_str("operator"), Some(Role::Operator));
        assert_eq!(Role::from_str("admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("superadmin"), Some(Role::SuperAdmin));
    }

    #[test]
    fn role_from_str_dev_maps_to_operator() {
        assert_eq!(Role::from_str("dev"), Some(Role::Operator));
    }

    #[test]
    fn role_from_str_unknown() {
        assert_eq!(Role::from_str("unknown"), None);
        assert_eq!(Role::from_str(""), None);
    }

    #[test]
    fn admin_passes_admin_check() {
        let admin = Role::Admin;
        let required = Role::Admin;
        assert!(admin >= required);
    }

    #[test]
    fn operator_fails_admin_check() {
        let op = Role::Operator;
        let required = Role::Admin;
        assert!(op < required);
    }

    #[test]
    fn superadmin_passes_all() {
        let sa = Role::SuperAdmin;
        assert!(sa >= Role::Viewer);
        assert!(sa >= Role::Operator);
        assert!(sa >= Role::Admin);
        assert!(sa >= Role::SuperAdmin);
    }
}
