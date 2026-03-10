//! Typed authorization model and claim normalization scaffolding.
//!
//! This module is intentionally pure and compile-safe. It does not change
//! live route enforcement yet; it provides the shared types the gateway will
//! use during the authz remediation cutover.

use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::api::auth::Claims;

pub const AUTHZ_CLAIMS_VERSION_V1: u16 = 1;
pub const INTERNAL_JWT_ISSUER: &str = "ghost-gateway";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseRole {
    Viewer,
    Operator,
    Admin,
    SuperAdmin,
}

impl BaseRole {
    pub fn parse_wire_role(role: &str) -> Result<Self, AuthzError> {
        match role {
            "viewer" => Ok(Self::Viewer),
            "operator" => Ok(Self::Operator),
            "admin" => Ok(Self::Admin),
            "superadmin" => Ok(Self::SuperAdmin),
            // Legacy no-auth compatibility.
            "dev" => Ok(Self::Operator),
            _ => Err(AuthzError::UnknownBaseRole(role.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
            Self::SuperAdmin => "superadmin",
        }
    }

    pub fn satisfies(self, minimum: Self) -> bool {
        self >= minimum
    }
}

impl FromStr for BaseRole {
    type Err = AuthzError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_wire_role(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    SafetyReview,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafetyReview => "safety_review",
        }
    }
}

impl FromStr for Capability {
    type Err = AuthzError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "safety_review" | "security_reviewer" => Ok(Self::SafetyReview),
            _ => Err(AuthzError::UnknownCapability(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Jwt,
    LegacyToken,
    NoAuthDev,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Authenticated read actions.
    AuthSessionRead,
    AgentReadList,
    AuditReadQuery,
    AuditAggregationRead,
    AuditExportRead,
    ConvergenceScoreRead,
    GoalReadList,
    GoalReadItem,
    SessionReadList,
    SessionEventRead,
    SessionBookmarkRead,
    MemoryReadList,
    MemoryGraphRead,
    MemorySearch,
    MemoryArchivedRead,
    MemoryItemRead,
    LiveExecutionRead,
    LiveExecutionCancel,
    StateCrdtRead,
    IntegrityChainRead,
    WorkflowReadList,
    WorkflowReadItem,
    WorkflowExecutionRead,
    AutonomyReadStatus,
    AutonomyReadJobs,
    AutonomyReadRuns,
    StudioSessionReadList,
    StudioSessionReadItem,
    StudioSessionRecoverStream,
    TraceRead,
    MeshTrustGraphRead,
    MeshConsensusRead,
    MeshDelegationRead,
    ProfileReadList,
    SearchRead,
    SkillReadList,
    A2aTaskReadList,
    A2aTaskReadItem,
    A2aTaskStreamRead,
    A2aDiscoverRead,
    ChannelReadList,
    CostRead,
    ItpEventRead,
    WebSocketConnect,
    WebSocketTicketIssue,
    OAuthProviderRead,
    OAuthCallbackReceive,
    OAuthConnectionRead,
    MarketplaceAgentReadList,
    MarketplaceAgentReadItem,
    MarketplaceSkillReadList,
    MarketplaceSkillReadItem,
    MarketplaceContractReadList,
    MarketplaceContractReadItem,
    MarketplaceWalletRead,
    MarketplaceWalletTransactionRead,
    MarketplaceReviewReadList,
    // Operator actions.
    SafetyStatusRead,
    AgentCreate,
    AgentDelete,
    GoalApprove,
    GoalReject,
    MemoryWrite,
    MemoryArchive,
    MemoryUnarchive,
    WorkflowCreate,
    WorkflowUpdate,
    WorkflowExecute,
    WorkflowExecutionResume,
    AutonomyPolicyWrite,
    AutonomySuppressionWrite,
    AutonomyRunApprove,
    SessionBookmarkCreate,
    SessionBookmarkDelete,
    SessionBranch,
    SessionHeartbeat,
    StudioRunPrompt,
    StudioSessionCreate,
    StudioSessionDelete,
    StudioSessionMessageSend,
    StudioSessionMessageStream,
    AgentChatSend,
    AgentChatStream,
    ProfileCreate,
    ProfileUpdate,
    ProfileDelete,
    AgentProfileAssign,
    SkillInstall,
    SkillUninstall,
    SkillQuarantine,
    SkillQuarantineResolve,
    SkillReverify,
    SkillExecute,
    ChannelCreate,
    ChannelReconnect,
    ChannelDelete,
    ChannelInject,
    A2aTaskSend,
    OAuthConnect,
    OAuthConnectionDelete,
    OAuthExecuteApiCall,
    MarketplaceAgentRegister,
    MarketplaceAgentDelist,
    MarketplaceAgentStatusUpdate,
    MarketplaceSkillPublish,
    MarketplaceContractPropose,
    MarketplaceContractAccept,
    MarketplaceContractReject,
    MarketplaceContractStart,
    MarketplaceContractComplete,
    MarketplaceContractDispute,
    MarketplaceContractCancel,
    MarketplaceContractResolve,
    MarketplaceWalletSeed,
    MarketplaceReviewSubmit,
    MarketplaceDiscover,
    // Admin actions.
    SafetyCheckReadList,
    AdminBackupReadList,
    AdminExportRead,
    ProviderKeyReadList,
    CodexAuthReadStatus,
    PcControlStatusRead,
    PcControlStatusWrite,
    PcControlActionRead,
    SafetyPauseAgent,
    SafetyResumeAgent,
    SafetyQuarantineAgent,
    SafetyCheckRegister,
    SafetyCheckDelete,
    WebhookReadList,
    WebhookCreate,
    WebhookUpdate,
    WebhookDelete,
    WebhookTest,
    AdminBackupCreate,
    ProviderKeyWrite,
    ProviderKeyDelete,
    CodexAuthLogin,
    CodexAuthLogout,
    PcControlAllowedAppsWrite,
    PcControlBlockedHotkeysWrite,
    PcControlSafeZonesWrite,
    // Superadmin actions.
    SafetyKillAll,
    AdminRestoreVerify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteId {
    Unknown,
    AuthSession,
    Agents,
    AgentById,
    Audit,
    AuditAggregation,
    AuditExport,
    ConvergenceScores,
    ConvergenceHistoryByAgentId,
    Goals,
    GoalById,
    GoalApproveById,
    GoalRejectById,
    Sessions,
    SessionEventsById,
    SessionBookmarksById,
    SessionBookmarkById,
    SessionBranchById,
    SessionHeartbeatById,
    Memory,
    MemoryGraph,
    MemorySearch,
    MemoryArchived,
    MemoryById,
    MemoryArchiveById,
    MemoryUnarchiveById,
    LiveExecutionById,
    LiveExecutionCancelById,
    StateCrdtByAgentId,
    IntegrityChainByAgentId,
    Workflows,
    WorkflowById,
    WorkflowExecutionsById,
    WorkflowExecuteById,
    WorkflowResumeExecutionById,
    AutonomyStatus,
    AutonomyJobs,
    AutonomyRuns,
    AutonomyPoliciesGlobal,
    AutonomyPoliciesAgentById,
    AutonomySuppressions,
    AutonomyRunApproveById,
    StudioRun,
    StudioSessions,
    StudioSessionById,
    StudioSessionMessagesById,
    StudioSessionMessageStreamById,
    StudioSessionRecoverStreamById,
    AgentChat,
    AgentChatStream,
    TracesBySessionId,
    MeshTrustGraph,
    MeshConsensus,
    MeshDelegations,
    Profiles,
    ProfileByName,
    AgentProfileById,
    Search,
    Skills,
    SkillInstallById,
    SkillUninstallById,
    SkillQuarantineById,
    SkillQuarantineResolveById,
    SkillReverifyById,
    SkillExecuteByName,
    A2aTasks,
    A2aTaskById,
    A2aTaskStreamById,
    A2aDiscover,
    Channels,
    ChannelById,
    ChannelReconnectById,
    ChannelInjectByType,
    Costs,
    ItpEvents,
    WebSocket,
    WebSocketTickets,
    OAuthProviders,
    OAuthCallback,
    OAuthConnections,
    OAuthConnect,
    OAuthConnectionByRefId,
    OAuthExecute,
    MarketplaceAgents,
    MarketplaceAgentById,
    MarketplaceAgentStatusById,
    MarketplaceSkills,
    MarketplaceSkillByName,
    MarketplaceContracts,
    MarketplaceContractById,
    MarketplaceContractAcceptById,
    MarketplaceContractRejectById,
    MarketplaceContractStartById,
    MarketplaceContractCompleteById,
    MarketplaceContractDisputeById,
    MarketplaceContractCancelById,
    MarketplaceContractResolveById,
    MarketplaceWallet,
    MarketplaceWalletSeed,
    MarketplaceWalletTransactions,
    MarketplaceReviews,
    MarketplaceReviewsByAgentId,
    MarketplaceDiscover,
    SafetyStatus,
    SafetyPauseAgent,
    SafetyResumeAgent,
    SafetyQuarantineAgent,
    SafetyKillAll,
    SafetyChecks,
    SafetyCheckById,
    AdminBackupCreate,
    AdminBackupList,
    AdminExport,
    AdminRestore,
    ProviderKeys,
    ProviderKeyByEnvName,
    CodexAuthStatus,
    CodexAuthLogin,
    CodexAuthLogout,
    Webhooks,
    WebhookById,
    WebhookTestById,
    PcControlStatus,
    PcControlActions,
    PcControlAllowedApps,
    PcControlBlockedHotkeys,
    PcControlSafeZones,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Http,
    WebSocket,
    Cli,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceContext<'a> {
    None,
    Agent {
        agent_id: Option<uuid::Uuid>,
    },
    Session {
        session_id: &'a str,
        owner_subject: Option<&'a str>,
    },
    LiveExecution {
        execution_id: &'a str,
        owner_subject: Option<&'a str>,
    },
    ProviderKey {
        env_name: Option<&'a str>,
    },
    BackupArchive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationContext<'a> {
    pub action: Action,
    pub route_id: RouteId,
    pub transport: TransportKind,
    pub resource: ResourceContext<'a>,
}

impl<'a> AuthorizationContext<'a> {
    pub fn new(action: Action, route_id: RouteId) -> Self {
        Self {
            action,
            route_id,
            transport: TransportKind::Http,
            resource: ResourceContext::None,
        }
    }

    pub fn with_transport(mut self, transport: TransportKind) -> Self {
        self.transport = transport;
        self
    }

    pub fn with_resource(mut self, resource: ResourceContext<'a>) -> Self {
        self.resource = resource;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthzClaimsV1 {
    pub sub: String,
    pub role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub authz_v: u16,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
    #[serde(default)]
    pub iss: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub subject: String,
    pub base_role: BaseRole,
    pub capabilities: BTreeSet<Capability>,
    pub auth_mode: AuthMode,
    pub token_id: Option<String>,
    pub authz_version: u16,
    pub issuer: Option<String>,
}

impl Principal {
    pub fn from_claims(claims: &Claims, auth_mode_hint: AuthMode) -> Result<Self, AuthzError> {
        match claims.authz_v {
            Some(AUTHZ_CLAIMS_VERSION_V1) => {
                AuthzClaimsV1::from_claims(claims)?.into_principal(auth_mode_hint)
            }
            Some(version) => Err(AuthzError::UnsupportedAuthzVersion(version)),
            None => {
                if !claims.capabilities.is_empty() {
                    return Err(AuthzError::CapabilitiesRequireVersion);
                }
                Self::from_legacy_claims(claims, auth_mode_hint)
            }
        }
    }

    pub fn from_legacy_claims(
        claims: &Claims,
        auth_mode_hint: AuthMode,
    ) -> Result<Self, AuthzError> {
        let auth_mode = if claims.role == "dev" {
            AuthMode::NoAuthDev
        } else {
            auth_mode_hint
        };
        let (base_role, capabilities) = if claims.role == "security_reviewer" {
            (
                BaseRole::Operator,
                BTreeSet::from([Capability::SafetyReview]),
            )
        } else {
            (BaseRole::parse_wire_role(&claims.role)?, BTreeSet::new())
        };
        Ok(Self {
            subject: claims.sub.clone(),
            base_role,
            capabilities,
            auth_mode,
            token_id: (!claims.jti.is_empty()).then(|| claims.jti.clone()),
            authz_version: 0,
            issuer: claims.iss.clone(),
        })
    }

    pub fn has_capability(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn has_minimum_role(&self, minimum: BaseRole) -> bool {
        self.base_role.satisfies(minimum)
    }

    pub fn canonical_capability_names(&self) -> Vec<&'static str> {
        self.capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect()
    }

    pub fn matches_resource_owner(&self, resource: &ResourceContext<'_>) -> bool {
        match resource {
            ResourceContext::Session { owner_subject, .. }
            | ResourceContext::LiveExecution { owner_subject, .. } => {
                owner_subject.is_some_and(|owner| owner == self.subject)
            }
            _ => false,
        }
    }
}

impl AuthzClaimsV1 {
    pub fn from_claims(claims: &Claims) -> Result<Self, AuthzError> {
        match claims.authz_v {
            Some(AUTHZ_CLAIMS_VERSION_V1) => Ok(Self {
                sub: claims.sub.clone(),
                role: claims.role.clone(),
                capabilities: claims.capabilities.clone(),
                authz_v: AUTHZ_CLAIMS_VERSION_V1,
                exp: claims.exp,
                iat: claims.iat,
                jti: claims.jti.clone(),
                iss: claims.iss.clone(),
            }),
            Some(version) => Err(AuthzError::UnsupportedAuthzVersion(version)),
            None => Err(AuthzError::MissingAuthzVersion),
        }
    }

    pub fn into_principal(self, auth_mode: AuthMode) -> Result<Principal, AuthzError> {
        let mut capabilities = BTreeSet::new();
        for capability in self.capabilities {
            capabilities.insert(capability.parse::<Capability>()?);
        }
        Ok(Principal {
            subject: self.sub,
            base_role: BaseRole::parse_wire_role(&self.role)?,
            capabilities,
            auth_mode,
            token_id: (!self.jti.is_empty()).then_some(self.jti),
            authz_version: self.authz_v,
            issuer: self.iss,
        })
    }
}

pub fn auth_mode_hint_for_claims(claims: &Claims) -> AuthMode {
    if claims.role == "dev" {
        return AuthMode::NoAuthDev;
    }

    if claims.authz_v.is_some()
        || claims.iss.is_some()
        || !claims.jti.is_empty()
        || claims.exp != u64::MAX
        || claims.iat != 0
    {
        return AuthMode::Jwt;
    }

    AuthMode::LegacyToken
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthzError {
    #[error("unknown base role: {0}")]
    UnknownBaseRole(String),
    #[error("unknown capability: {0}")]
    UnknownCapability(String),
    #[error("unsupported authz claim version: {0}")]
    UnsupportedAuthzVersion(u16),
    #[error("authz claim version is missing")]
    MissingAuthzVersion,
    #[error("capabilities require an explicit authz_v")]
    CapabilitiesRequireVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_claims(role: &str) -> Claims {
        Claims {
            sub: "user-1".into(),
            role: role.into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: 42,
            iat: 21,
            jti: "jti-1".into(),
            iss: None,
        }
    }

    #[test]
    fn typed_claims_normalize_to_principal() {
        let claims = Claims {
            sub: "reviewer".into(),
            role: "operator".into(),
            capabilities: vec!["safety_review".into()],
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: 42,
            iat: 21,
            jti: "jwt-1".into(),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        };

        let principal = Principal::from_claims(&claims, AuthMode::Jwt).expect("typed claims");

        assert_eq!(principal.subject, "reviewer");
        assert_eq!(principal.base_role, BaseRole::Operator);
        assert!(principal.has_capability(Capability::SafetyReview));
        assert_eq!(principal.auth_mode, AuthMode::Jwt);
        assert_eq!(principal.authz_version, AUTHZ_CLAIMS_VERSION_V1);
        assert_eq!(principal.issuer.as_deref(), Some(INTERNAL_JWT_ISSUER));
    }

    #[test]
    fn legacy_dev_claims_remain_no_auth_operator() {
        let principal =
            Principal::from_claims(&legacy_claims("dev"), AuthMode::Jwt).expect("legacy dev");

        assert_eq!(principal.base_role, BaseRole::Operator);
        assert_eq!(principal.auth_mode, AuthMode::NoAuthDev);
        assert!(principal.capabilities.is_empty());
        assert_eq!(principal.authz_version, 0);
    }

    #[test]
    fn legacy_capabilities_fail_closed_without_version() {
        let mut claims = legacy_claims("operator");
        claims.capabilities = vec!["safety_review".into()];

        let error = Principal::from_claims(&claims, AuthMode::Jwt).expect_err("must fail");
        assert_eq!(error, AuthzError::CapabilitiesRequireVersion);
    }

    #[test]
    fn unknown_role_fails_closed() {
        let error =
            Principal::from_claims(&legacy_claims("mystery"), AuthMode::Jwt).expect_err("denied");
        assert_eq!(error, AuthzError::UnknownBaseRole("mystery".into()));
    }

    #[test]
    fn security_reviewer_alias_maps_to_safety_review_capability() {
        let claims = Claims {
            sub: "reviewer".into(),
            role: "operator".into(),
            capabilities: vec!["security_reviewer".into()],
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: 42,
            iat: 21,
            jti: "jwt-2".into(),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        };

        let principal = Principal::from_claims(&claims, AuthMode::Jwt).expect("typed claims");
        assert!(principal.has_capability(Capability::SafetyReview));
        assert_eq!(
            principal.canonical_capability_names(),
            vec!["safety_review"]
        );
    }

    #[test]
    fn legacy_security_reviewer_role_maps_to_operator_with_safety_review() {
        let principal = Principal::from_claims(&legacy_claims("security_reviewer"), AuthMode::Jwt)
            .expect("legacy reviewer claims");

        assert_eq!(principal.base_role, BaseRole::Operator);
        assert!(principal.has_capability(Capability::SafetyReview));
        assert_eq!(principal.authz_version, 0);
    }

    #[test]
    fn auth_mode_hint_detects_legacy_token_fallback_shape() {
        let claims = Claims::admin_fallback();
        assert_eq!(auth_mode_hint_for_claims(&claims), AuthMode::LegacyToken);
    }

    #[test]
    fn auth_mode_hint_detects_typed_jwt_shape() {
        let claims = Claims {
            sub: "jwt-user".into(),
            role: "operator".into(),
            capabilities: Vec::new(),
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: 42,
            iat: 21,
            jti: "jwt-3".into(),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        };

        assert_eq!(auth_mode_hint_for_claims(&claims), AuthMode::Jwt);
    }
}
