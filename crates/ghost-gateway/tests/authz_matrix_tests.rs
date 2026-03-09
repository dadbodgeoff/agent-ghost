use axum::http::Method;
use ghost_gateway::api::auth::Claims;
use ghost_gateway::api::authz::{
    Action, AuthMode, AuthorizationContext, BaseRole, Principal, ResourceContext, RouteId,
    AUTHZ_CLAIMS_VERSION_V1, INTERNAL_JWT_ISSUER,
};
use ghost_gateway::api::authz_policy::{
    admin_actions, authorize, authorize_claims, operator_actions, route_spec_for,
    superadmin_actions, viewer_actions, RouteAuthorizationKind,
};

fn typed_claims(role: &str, capabilities: Vec<&str>) -> Claims {
    Claims {
        sub: format!("{role}-subject"),
        role: role.into(),
        capabilities: capabilities.into_iter().map(str::to_string).collect(),
        authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
        exp: 42,
        iat: 21,
        jti: format!("{role}-jwt"),
        iss: Some(INTERNAL_JWT_ISSUER.into()),
    }
}

fn legacy_claims(role: &str) -> Claims {
    Claims {
        sub: format!("{role}-subject"),
        role: role.into(),
        capabilities: Vec::new(),
        authz_v: None,
        exp: 42,
        iat: 21,
        jti: format!("{role}-legacy"),
        iss: None,
    }
}

fn principal(role: BaseRole) -> Principal {
    Principal {
        subject: format!("{:?}", role).to_lowercase(),
        base_role: role,
        capabilities: Default::default(),
        auth_mode: AuthMode::Jwt,
        token_id: Some("jti-1".into()),
        authz_version: AUTHZ_CLAIMS_VERSION_V1,
        issuer: Some(INTERNAL_JWT_ISSUER.into()),
    }
}

#[test]
fn viewer_actions_allow_all_authenticated_roles() {
    for action in viewer_actions() {
        let context = AuthorizationContext::new(*action, RouteId::Unknown);
        for role in ["viewer", "operator", "admin", "superadmin"] {
            let decision =
                authorize_claims(Some(&typed_claims(role, Vec::new())), &context).expect("allow");
            assert!(
                decision.1.allowed,
                "expected {action:?} to allow {role} principal"
            );
        }
    }
}

#[test]
fn operator_actions_deny_viewer_but_allow_operator_and_higher() {
    for action in operator_actions() {
        let context = AuthorizationContext::new(*action, RouteId::Unknown);
        assert!(
            authorize_claims(Some(&typed_claims("viewer", Vec::new())), &context).is_err(),
            "expected {action:?} to deny viewer"
        );
        for role in ["operator", "admin", "superadmin"] {
            let decision =
                authorize_claims(Some(&typed_claims(role, Vec::new())), &context).expect("allow");
            assert!(
                decision.1.allowed,
                "expected {action:?} to allow {role} principal"
            );
        }
    }
}

#[test]
fn admin_actions_require_admin_or_higher_and_deny_dev_operator() {
    for action in admin_actions() {
        let context = AuthorizationContext::new(*action, RouteId::Unknown);
        for denied in ["viewer", "operator", "dev"] {
            assert!(
                authorize_claims(Some(&legacy_claims(denied)), &context).is_err(),
                "expected {action:?} to deny {denied} principal"
            );
        }
        for allowed in ["admin", "superadmin"] {
            let decision = authorize_claims(Some(&typed_claims(allowed, Vec::new())), &context)
                .expect("allow");
            assert!(
                decision.1.allowed,
                "expected {action:?} to allow {allowed} principal"
            );
        }
    }
}

#[test]
fn superadmin_actions_require_superadmin() {
    for action in superadmin_actions() {
        let context = AuthorizationContext::new(*action, RouteId::Unknown);
        for denied in ["viewer", "operator", "admin", "dev"] {
            assert!(
                authorize_claims(Some(&legacy_claims(denied)), &context).is_err(),
                "expected {action:?} to deny {denied} principal"
            );
        }
        let decision = authorize_claims(Some(&typed_claims("superadmin", Vec::new())), &context)
            .expect("allow");
        assert!(
            decision.1.allowed,
            "expected {action:?} to allow superadmin"
        );
    }
}

#[test]
fn safety_resume_requires_capability_or_admin() {
    let context = AuthorizationContext::new(Action::SafetyResumeAgent, RouteId::SafetyResumeAgent);

    assert!(authorize_claims(Some(&typed_claims("operator", Vec::new())), &context).is_err());
    assert!(authorize_claims(
        Some(&typed_claims("operator", vec!["safety_review"])),
        &context
    )
    .is_ok());
    assert!(authorize_claims(Some(&typed_claims("admin", Vec::new())), &context).is_ok());
    assert!(
        authorize_claims(Some(&legacy_claims("security_reviewer")), &context).is_ok(),
        "legacy security reviewer compatibility should remain narrow and explicit"
    );
}

#[test]
fn live_execution_read_allows_owner_or_admin_only() {
    let owner_context =
        AuthorizationContext::new(Action::LiveExecutionRead, RouteId::LiveExecutionById)
            .with_resource(ResourceContext::LiveExecution {
                execution_id: "exec-1",
                owner_subject: Some("owner"),
            });
    let mut owner = principal(BaseRole::Operator);
    owner.subject = "owner".into();
    assert!(authorize(&owner, &owner_context).allowed);

    let mut other_operator = principal(BaseRole::Operator);
    other_operator.subject = "other".into();
    assert!(!authorize(&other_operator, &owner_context).allowed);

    let mut admin = principal(BaseRole::Admin);
    admin.subject = "admin".into();
    assert!(authorize(&admin, &owner_context).allowed);
}

#[test]
fn malformed_claims_fail_closed() {
    let context = AuthorizationContext::new(Action::AdminBackupCreate, RouteId::AdminBackupCreate);

    assert!(authorize_claims(Some(&legacy_claims("mystery")), &context).is_err());
    assert!(authorize_claims(
        Some(&typed_claims("operator", vec!["unknown_capability"])),
        &context
    )
    .is_err());

    let mut legacy_with_caps = legacy_claims("operator");
    legacy_with_caps.capabilities = vec!["safety_review".into()];
    assert!(authorize_claims(Some(&legacy_with_caps), &context).is_err());
}

#[test]
fn route_spec_covers_live_execution_as_owner_or_admin() {
    let spec = route_spec_for(RouteId::LiveExecutionById, &Method::GET).expect("route spec");

    assert_eq!(spec.action, Action::LiveExecutionRead);
    assert_eq!(
        spec.authorization_kind,
        RouteAuthorizationKind::OwnerOrAdmin
    );
}

#[test]
fn route_spec_covers_safety_resume_as_capability_bound() {
    let spec = route_spec_for(RouteId::SafetyResumeAgent, &Method::POST).expect("route spec");

    assert_eq!(spec.action, Action::SafetyResumeAgent);
    assert_eq!(
        spec.authorization_kind,
        RouteAuthorizationKind::SafetyReview
    );
}
