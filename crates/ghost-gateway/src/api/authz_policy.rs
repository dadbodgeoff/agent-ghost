//! Authorization policy registry and pure policy evaluation.

use axum::http::Method;

use crate::api::auth::Claims;
use crate::api::authz::{
    auth_mode_hint_for_claims, Action, AuthMode, AuthorizationContext, BaseRole, Capability,
    Principal, RouteId,
};
use crate::api::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyPredicate {
    True,
    MinRole(BaseRole),
    HasCapability(Capability),
    AuthModeIs(AuthMode),
    AuthModeIn(Vec<AuthMode>),
    SubjectMatchesResourceOwner,
    Any(Vec<PolicyPredicate>),
    All(Vec<PolicyPredicate>),
    Not(Box<PolicyPredicate>),
}

impl PolicyPredicate {
    pub fn evaluate(&self, principal: &Principal, context: &AuthorizationContext<'_>) -> bool {
        match self {
            Self::True => true,
            Self::MinRole(minimum) => principal.has_minimum_role(*minimum),
            Self::HasCapability(capability) => principal.has_capability(*capability),
            Self::AuthModeIs(mode) => principal.auth_mode == *mode,
            Self::AuthModeIn(modes) => modes.contains(&principal.auth_mode),
            Self::SubjectMatchesResourceOwner => {
                principal.matches_resource_owner(&context.resource)
            }
            Self::Any(predicates) => predicates
                .iter()
                .any(|predicate| predicate.evaluate(principal, context)),
            Self::All(predicates) => predicates
                .iter()
                .all(|predicate| predicate.evaluate(principal, context)),
            Self::Not(predicate) => !predicate.evaluate(principal, context),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    pub allow_if: PolicyPredicate,
    pub deny_if: Vec<PolicyPredicate>,
    pub audit_on_deny: bool,
}

impl PolicyRule {
    pub fn allows(&self, principal: &Principal, context: &AuthorizationContext<'_>) -> bool {
        !self
            .deny_if
            .iter()
            .any(|predicate| predicate.evaluate(principal, context))
            && self.allow_if.evaluate(principal, context)
    }

    pub fn evaluate(
        &self,
        action: Action,
        policy_id: &'static str,
        principal: &Principal,
        context: &AuthorizationContext<'_>,
    ) -> AuthorizationDecision {
        if self
            .deny_if
            .iter()
            .any(|predicate| predicate.evaluate(principal, context))
        {
            return AuthorizationDecision {
                action,
                policy_id,
                allowed: false,
                denial_reason: Some(DenialReason::DenyPredicateMatched),
            };
        }

        if self.allow_if.evaluate(principal, context) {
            AuthorizationDecision {
                action,
                policy_id,
                allowed: true,
                denial_reason: None,
            }
        } else {
            AuthorizationDecision {
                action,
                policy_id,
                allowed: false,
                denial_reason: Some(DenialReason::AllowPredicateNotSatisfied),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuthorizationKind {
    MinimumRole(BaseRole),
    SafetyReview,
    OwnerOrAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteAuthorizationSpec {
    pub route_id: RouteId,
    pub action: Action,
    pub compatibility_required: bool,
    pub authorization_kind: RouteAuthorizationKind,
}

impl RouteAuthorizationSpec {
    pub const fn new(
        route_id: RouteId,
        action: Action,
        compatibility_required: bool,
        authorization_kind: RouteAuthorizationKind,
    ) -> Self {
        Self {
            route_id,
            action,
            compatibility_required,
            authorization_kind,
        }
    }
}

const fn viewer_spec(route_id: RouteId, action: Action) -> RouteAuthorizationSpec {
    RouteAuthorizationSpec::new(
        route_id,
        action,
        false,
        RouteAuthorizationKind::MinimumRole(BaseRole::Viewer),
    )
}

const fn operator_spec(route_id: RouteId, action: Action) -> RouteAuthorizationSpec {
    RouteAuthorizationSpec::new(
        route_id,
        action,
        true,
        RouteAuthorizationKind::MinimumRole(BaseRole::Operator),
    )
}

const fn admin_spec(
    route_id: RouteId,
    action: Action,
    compatibility_required: bool,
) -> RouteAuthorizationSpec {
    RouteAuthorizationSpec::new(
        route_id,
        action,
        compatibility_required,
        RouteAuthorizationKind::MinimumRole(BaseRole::Admin),
    )
}

const fn superadmin_spec(route_id: RouteId, action: Action) -> RouteAuthorizationSpec {
    RouteAuthorizationSpec::new(
        route_id,
        action,
        true,
        RouteAuthorizationKind::MinimumRole(BaseRole::SuperAdmin),
    )
}

const fn owner_or_admin_spec(route_id: RouteId, action: Action) -> RouteAuthorizationSpec {
    RouteAuthorizationSpec::new(
        route_id,
        action,
        false,
        RouteAuthorizationKind::OwnerOrAdmin,
    )
}

pub fn route_spec_for(route_id: RouteId, method: &Method) -> Option<RouteAuthorizationSpec> {
    match (route_id, method.as_str()) {
        (RouteId::AuthSession, "GET") => {
            Some(viewer_spec(RouteId::AuthSession, Action::AuthSessionRead))
        }
        (RouteId::Agents, "GET") => Some(viewer_spec(RouteId::Agents, Action::AgentReadList)),
        (RouteId::Agents, "POST") => Some(operator_spec(RouteId::Agents, Action::AgentCreate)),
        (RouteId::AgentById, "GET") => Some(viewer_spec(RouteId::AgentById, Action::AgentReadList)),
        (RouteId::AgentOverviewById, "GET") => Some(viewer_spec(
            RouteId::AgentOverviewById,
            Action::AgentReadList,
        )),
        (RouteId::AgentById, "PATCH") => {
            Some(operator_spec(RouteId::AgentById, Action::AgentUpdate))
        }
        (RouteId::AgentById, "DELETE") => {
            Some(operator_spec(RouteId::AgentById, Action::AgentDelete))
        }
        (RouteId::Audit, "GET") => Some(viewer_spec(RouteId::Audit, Action::AuditReadQuery)),
        (RouteId::AuditAggregation, "GET") => Some(viewer_spec(
            RouteId::AuditAggregation,
            Action::AuditAggregationRead,
        )),
        (RouteId::AuditExport, "GET") => {
            Some(viewer_spec(RouteId::AuditExport, Action::AuditExportRead))
        }
        (RouteId::ConvergenceScores, "GET") => Some(viewer_spec(
            RouteId::ConvergenceScores,
            Action::ConvergenceScoreRead,
        )),
        (RouteId::ConvergenceHistoryByAgentId, "GET") => Some(viewer_spec(
            RouteId::ConvergenceHistoryByAgentId,
            Action::ConvergenceScoreRead,
        )),
        (RouteId::Goals, "GET") => Some(viewer_spec(RouteId::Goals, Action::GoalReadList)),
        (RouteId::GoalsActive, "GET") => {
            Some(viewer_spec(RouteId::GoalsActive, Action::GoalReadList))
        }
        (RouteId::GoalById, "GET") => Some(viewer_spec(RouteId::GoalById, Action::GoalReadItem)),
        (RouteId::GoalApproveById, "POST") => {
            Some(operator_spec(RouteId::GoalApproveById, Action::GoalApprove))
        }
        (RouteId::GoalRejectById, "POST") => {
            Some(operator_spec(RouteId::GoalRejectById, Action::GoalReject))
        }
        (RouteId::Sessions, "GET") => Some(viewer_spec(RouteId::Sessions, Action::SessionReadList)),
        (RouteId::SessionById, "GET") => {
            Some(viewer_spec(RouteId::SessionById, Action::SessionReadItem))
        }
        (RouteId::SessionEventsById, "GET") => Some(viewer_spec(
            RouteId::SessionEventsById,
            Action::SessionEventRead,
        )),
        (RouteId::SessionBookmarksById, "GET") => Some(viewer_spec(
            RouteId::SessionBookmarksById,
            Action::SessionBookmarkRead,
        )),
        (RouteId::SessionBookmarksById, "POST") => Some(operator_spec(
            RouteId::SessionBookmarksById,
            Action::SessionBookmarkCreate,
        )),
        (RouteId::SessionBookmarkById, "DELETE") => Some(operator_spec(
            RouteId::SessionBookmarkById,
            Action::SessionBookmarkDelete,
        )),
        (RouteId::SessionBranchById, "POST") => Some(operator_spec(
            RouteId::SessionBranchById,
            Action::SessionBranch,
        )),
        (RouteId::SessionHeartbeatById, "POST") => Some(operator_spec(
            RouteId::SessionHeartbeatById,
            Action::SessionHeartbeat,
        )),
        (RouteId::Memory, "GET") => Some(viewer_spec(RouteId::Memory, Action::MemoryReadList)),
        (RouteId::Memory, "POST") => Some(operator_spec(RouteId::Memory, Action::MemoryWrite)),
        (RouteId::MemoryGraph, "GET") => {
            Some(viewer_spec(RouteId::MemoryGraph, Action::MemoryGraphRead))
        }
        (RouteId::MemorySearch, "GET") => {
            Some(viewer_spec(RouteId::MemorySearch, Action::MemorySearch))
        }
        (RouteId::MemoryArchived, "GET") => Some(viewer_spec(
            RouteId::MemoryArchived,
            Action::MemoryArchivedRead,
        )),
        (RouteId::MemoryById, "GET") => {
            Some(viewer_spec(RouteId::MemoryById, Action::MemoryItemRead))
        }
        (RouteId::MemoryArchiveById, "POST") => Some(operator_spec(
            RouteId::MemoryArchiveById,
            Action::MemoryArchive,
        )),
        (RouteId::MemoryUnarchiveById, "POST") => Some(operator_spec(
            RouteId::MemoryUnarchiveById,
            Action::MemoryUnarchive,
        )),
        (RouteId::LiveExecutionById, "GET") => Some(owner_or_admin_spec(
            RouteId::LiveExecutionById,
            Action::LiveExecutionRead,
        )),
        (RouteId::LiveExecutionCancelById, "POST") => Some(owner_or_admin_spec(
            RouteId::LiveExecutionCancelById,
            Action::LiveExecutionCancel,
        )),
        (RouteId::StateCrdtByAgentId, "GET") => Some(viewer_spec(
            RouteId::StateCrdtByAgentId,
            Action::StateCrdtRead,
        )),
        (RouteId::IntegrityChainByAgentId, "GET") => Some(viewer_spec(
            RouteId::IntegrityChainByAgentId,
            Action::IntegrityChainRead,
        )),
        (RouteId::Workflows, "GET") => {
            Some(viewer_spec(RouteId::Workflows, Action::WorkflowReadList))
        }
        (RouteId::Workflows, "POST") => {
            Some(operator_spec(RouteId::Workflows, Action::WorkflowCreate))
        }
        (RouteId::WorkflowById, "GET") => {
            Some(viewer_spec(RouteId::WorkflowById, Action::WorkflowReadItem))
        }
        (RouteId::WorkflowById, "PUT") => {
            Some(operator_spec(RouteId::WorkflowById, Action::WorkflowUpdate))
        }
        (RouteId::WorkflowExecutionsById, "GET") => Some(viewer_spec(
            RouteId::WorkflowExecutionsById,
            Action::WorkflowExecutionRead,
        )),
        (RouteId::WorkflowExecutionById, "GET") => Some(viewer_spec(
            RouteId::WorkflowExecutionById,
            Action::WorkflowExecutionRead,
        )),
        (RouteId::AutonomyStatus, "GET") => Some(viewer_spec(
            RouteId::AutonomyStatus,
            Action::AutonomyReadStatus,
        )),
        (RouteId::AutonomyJobs, "GET") => {
            Some(viewer_spec(RouteId::AutonomyJobs, Action::AutonomyReadJobs))
        }
        (RouteId::AutonomyRuns, "GET") => {
            Some(viewer_spec(RouteId::AutonomyRuns, Action::AutonomyReadRuns))
        }
        (RouteId::AutonomyPoliciesGlobal, "GET") => Some(viewer_spec(
            RouteId::AutonomyPoliciesGlobal,
            Action::AutonomyReadStatus,
        )),
        (RouteId::AutonomyPoliciesAgentById, "GET") => Some(viewer_spec(
            RouteId::AutonomyPoliciesAgentById,
            Action::AutonomyReadStatus,
        )),
        (RouteId::WorkflowExecuteById, "POST") => Some(operator_spec(
            RouteId::WorkflowExecuteById,
            Action::WorkflowExecute,
        )),
        (RouteId::WorkflowResumeExecutionById, "POST") => Some(operator_spec(
            RouteId::WorkflowResumeExecutionById,
            Action::WorkflowExecutionResume,
        )),
        (RouteId::AutonomyPoliciesGlobal, "PUT") => Some(operator_spec(
            RouteId::AutonomyPoliciesGlobal,
            Action::AutonomyPolicyWrite,
        )),
        (RouteId::AutonomyPoliciesAgentById, "PUT") => Some(operator_spec(
            RouteId::AutonomyPoliciesAgentById,
            Action::AutonomyPolicyWrite,
        )),
        (RouteId::AutonomySuppressions, "POST") => Some(operator_spec(
            RouteId::AutonomySuppressions,
            Action::AutonomySuppressionWrite,
        )),
        (RouteId::AutonomyRunApproveById, "POST") => Some(operator_spec(
            RouteId::AutonomyRunApproveById,
            Action::AutonomyRunApprove,
        )),
        (RouteId::StudioRun, "POST") => {
            Some(operator_spec(RouteId::StudioRun, Action::StudioRunPrompt))
        }
        (RouteId::StudioSessions, "GET") => Some(viewer_spec(
            RouteId::StudioSessions,
            Action::StudioSessionReadList,
        )),
        (RouteId::StudioSessions, "POST") => Some(operator_spec(
            RouteId::StudioSessions,
            Action::StudioSessionCreate,
        )),
        (RouteId::StudioSessionById, "GET") => Some(viewer_spec(
            RouteId::StudioSessionById,
            Action::StudioSessionReadItem,
        )),
        (RouteId::StudioSessionById, "DELETE") => Some(operator_spec(
            RouteId::StudioSessionById,
            Action::StudioSessionDelete,
        )),
        (RouteId::StudioSessionMessagesById, "POST") => Some(operator_spec(
            RouteId::StudioSessionMessagesById,
            Action::StudioSessionMessageSend,
        )),
        (RouteId::StudioSessionMessageStreamById, "POST") => Some(operator_spec(
            RouteId::StudioSessionMessageStreamById,
            Action::StudioSessionMessageStream,
        )),
        (RouteId::StudioSessionRecoverStreamById, "GET") => Some(viewer_spec(
            RouteId::StudioSessionRecoverStreamById,
            Action::StudioSessionRecoverStream,
        )),
        (RouteId::AgentChat, "POST") => {
            Some(operator_spec(RouteId::AgentChat, Action::AgentChatSend))
        }
        (RouteId::AgentChatStream, "POST") => Some(operator_spec(
            RouteId::AgentChatStream,
            Action::AgentChatStream,
        )),
        (RouteId::TracesBySessionId, "GET") => {
            Some(viewer_spec(RouteId::TracesBySessionId, Action::TraceRead))
        }
        (RouteId::MeshTrustGraph, "GET") => Some(viewer_spec(
            RouteId::MeshTrustGraph,
            Action::MeshTrustGraphRead,
        )),
        (RouteId::MeshConsensus, "GET") => Some(viewer_spec(
            RouteId::MeshConsensus,
            Action::MeshConsensusRead,
        )),
        (RouteId::MeshDelegations, "GET") => Some(viewer_spec(
            RouteId::MeshDelegations,
            Action::MeshDelegationRead,
        )),
        (RouteId::Profiles, "GET") => Some(viewer_spec(RouteId::Profiles, Action::ProfileReadList)),
        (RouteId::Profiles, "POST") => {
            Some(operator_spec(RouteId::Profiles, Action::ProfileCreate))
        }
        (RouteId::ProfileByName, "PUT") => {
            Some(operator_spec(RouteId::ProfileByName, Action::ProfileUpdate))
        }
        (RouteId::ProfileByName, "DELETE") => {
            Some(operator_spec(RouteId::ProfileByName, Action::ProfileDelete))
        }
        (RouteId::AgentProfileById, "POST") => Some(operator_spec(
            RouteId::AgentProfileById,
            Action::AgentProfileAssign,
        )),
        (RouteId::Search, "GET") => Some(viewer_spec(RouteId::Search, Action::SearchRead)),
        (RouteId::ObservabilityAde, "GET") => Some(viewer_spec(
            RouteId::ObservabilityAde,
            Action::ObservabilityAdeRead,
        )),
        (RouteId::Skills, "GET") => Some(viewer_spec(RouteId::Skills, Action::SkillReadList)),
        (RouteId::SkillInstallById, "POST") => Some(operator_spec(
            RouteId::SkillInstallById,
            Action::SkillInstall,
        )),
        (RouteId::SkillUninstallById, "POST") => Some(operator_spec(
            RouteId::SkillUninstallById,
            Action::SkillUninstall,
        )),
        (RouteId::SkillQuarantineById, "POST") => Some(operator_spec(
            RouteId::SkillQuarantineById,
            Action::SkillQuarantine,
        )),
        (RouteId::SkillQuarantineResolveById, "POST") => Some(operator_spec(
            RouteId::SkillQuarantineResolveById,
            Action::SkillQuarantineResolve,
        )),
        (RouteId::SkillReverifyById, "POST") => Some(operator_spec(
            RouteId::SkillReverifyById,
            Action::SkillReverify,
        )),
        (RouteId::SkillExecuteByName, "POST") => Some(operator_spec(
            RouteId::SkillExecuteByName,
            Action::SkillExecute,
        )),
        (RouteId::A2aTasks, "GET") => Some(viewer_spec(RouteId::A2aTasks, Action::A2aTaskReadList)),
        (RouteId::A2aTasks, "POST") => Some(operator_spec(RouteId::A2aTasks, Action::A2aTaskSend)),
        (RouteId::A2aTaskById, "GET") => {
            Some(viewer_spec(RouteId::A2aTaskById, Action::A2aTaskReadItem))
        }
        (RouteId::A2aTaskStreamById, "GET") => Some(viewer_spec(
            RouteId::A2aTaskStreamById,
            Action::A2aTaskStreamRead,
        )),
        (RouteId::A2aDiscover, "GET") => {
            Some(viewer_spec(RouteId::A2aDiscover, Action::A2aDiscoverRead))
        }
        (RouteId::Channels, "GET") => Some(viewer_spec(RouteId::Channels, Action::ChannelReadList)),
        (RouteId::Channels, "POST") => {
            Some(operator_spec(RouteId::Channels, Action::ChannelCreate))
        }
        (RouteId::ChannelById, "DELETE") => {
            Some(operator_spec(RouteId::ChannelById, Action::ChannelDelete))
        }
        (RouteId::ChannelReconnectById, "POST") => Some(operator_spec(
            RouteId::ChannelReconnectById,
            Action::ChannelReconnect,
        )),
        (RouteId::ChannelInjectByType, "POST") => Some(operator_spec(
            RouteId::ChannelInjectByType,
            Action::ChannelInject,
        )),
        (RouteId::Costs, "GET") => Some(viewer_spec(RouteId::Costs, Action::CostRead)),
        (RouteId::ItpEvents, "GET") => Some(viewer_spec(RouteId::ItpEvents, Action::ItpEventRead)),
        (RouteId::WebSocket, "GET") => {
            Some(viewer_spec(RouteId::WebSocket, Action::WebSocketConnect))
        }
        (RouteId::WebSocketTickets, "POST") => Some(viewer_spec(
            RouteId::WebSocketTickets,
            Action::WebSocketTicketIssue,
        )),
        (RouteId::OAuthProviders, "GET") => Some(viewer_spec(
            RouteId::OAuthProviders,
            Action::OAuthProviderRead,
        )),
        (RouteId::OAuthCallback, "GET") => Some(viewer_spec(
            RouteId::OAuthCallback,
            Action::OAuthCallbackReceive,
        )),
        (RouteId::OAuthConnections, "GET") => Some(viewer_spec(
            RouteId::OAuthConnections,
            Action::OAuthConnectionRead,
        )),
        (RouteId::OAuthConnect, "POST") => {
            Some(operator_spec(RouteId::OAuthConnect, Action::OAuthConnect))
        }
        (RouteId::OAuthConnectionByRefId, "DELETE") => Some(operator_spec(
            RouteId::OAuthConnectionByRefId,
            Action::OAuthConnectionDelete,
        )),
        (RouteId::OAuthExecute, "POST") => Some(operator_spec(
            RouteId::OAuthExecute,
            Action::OAuthExecuteApiCall,
        )),
        (RouteId::MarketplaceAgents, "GET") => Some(viewer_spec(
            RouteId::MarketplaceAgents,
            Action::MarketplaceAgentReadList,
        )),
        (RouteId::MarketplaceAgents, "POST") => Some(operator_spec(
            RouteId::MarketplaceAgents,
            Action::MarketplaceAgentRegister,
        )),
        (RouteId::MarketplaceAgentById, "GET") => Some(viewer_spec(
            RouteId::MarketplaceAgentById,
            Action::MarketplaceAgentReadItem,
        )),
        (RouteId::MarketplaceAgentById, "DELETE") => Some(operator_spec(
            RouteId::MarketplaceAgentById,
            Action::MarketplaceAgentDelist,
        )),
        (RouteId::MarketplaceAgentStatusById, "PUT") => Some(operator_spec(
            RouteId::MarketplaceAgentStatusById,
            Action::MarketplaceAgentStatusUpdate,
        )),
        (RouteId::MarketplaceSkills, "GET") => Some(viewer_spec(
            RouteId::MarketplaceSkills,
            Action::MarketplaceSkillReadList,
        )),
        (RouteId::MarketplaceSkills, "POST") => Some(operator_spec(
            RouteId::MarketplaceSkills,
            Action::MarketplaceSkillPublish,
        )),
        (RouteId::MarketplaceSkillByName, "GET") => Some(viewer_spec(
            RouteId::MarketplaceSkillByName,
            Action::MarketplaceSkillReadItem,
        )),
        (RouteId::MarketplaceContracts, "GET") => Some(viewer_spec(
            RouteId::MarketplaceContracts,
            Action::MarketplaceContractReadList,
        )),
        (RouteId::MarketplaceContracts, "POST") => Some(operator_spec(
            RouteId::MarketplaceContracts,
            Action::MarketplaceContractPropose,
        )),
        (RouteId::MarketplaceContractById, "GET") => Some(viewer_spec(
            RouteId::MarketplaceContractById,
            Action::MarketplaceContractReadItem,
        )),
        (RouteId::MarketplaceContractAcceptById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractAcceptById,
            Action::MarketplaceContractAccept,
        )),
        (RouteId::MarketplaceContractRejectById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractRejectById,
            Action::MarketplaceContractReject,
        )),
        (RouteId::MarketplaceContractStartById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractStartById,
            Action::MarketplaceContractStart,
        )),
        (RouteId::MarketplaceContractCompleteById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractCompleteById,
            Action::MarketplaceContractComplete,
        )),
        (RouteId::MarketplaceContractDisputeById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractDisputeById,
            Action::MarketplaceContractDispute,
        )),
        (RouteId::MarketplaceContractCancelById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractCancelById,
            Action::MarketplaceContractCancel,
        )),
        (RouteId::MarketplaceContractResolveById, "POST") => Some(operator_spec(
            RouteId::MarketplaceContractResolveById,
            Action::MarketplaceContractResolve,
        )),
        (RouteId::MarketplaceWallet, "GET") => Some(viewer_spec(
            RouteId::MarketplaceWallet,
            Action::MarketplaceWalletRead,
        )),
        (RouteId::MarketplaceWalletSeed, "POST") => Some(operator_spec(
            RouteId::MarketplaceWalletSeed,
            Action::MarketplaceWalletSeed,
        )),
        (RouteId::MarketplaceWalletTransactions, "GET") => Some(viewer_spec(
            RouteId::MarketplaceWalletTransactions,
            Action::MarketplaceWalletTransactionRead,
        )),
        (RouteId::MarketplaceReviews, "POST") => Some(operator_spec(
            RouteId::MarketplaceReviews,
            Action::MarketplaceReviewSubmit,
        )),
        (RouteId::MarketplaceReviewsByAgentId, "GET") => Some(viewer_spec(
            RouteId::MarketplaceReviewsByAgentId,
            Action::MarketplaceReviewReadList,
        )),
        (RouteId::MarketplaceDiscover, "POST") => Some(operator_spec(
            RouteId::MarketplaceDiscover,
            Action::MarketplaceDiscover,
        )),
        (RouteId::SafetyStatus, "GET") => Some(RouteAuthorizationSpec::new(
            RouteId::SafetyStatus,
            Action::SafetyStatusRead,
            false,
            RouteAuthorizationKind::MinimumRole(BaseRole::Operator),
        )),
        (RouteId::SandboxReviews, "GET") => Some(RouteAuthorizationSpec::new(
            RouteId::SandboxReviews,
            Action::SandboxReviewReadList,
            false,
            RouteAuthorizationKind::MinimumRole(BaseRole::Operator),
        )),
        (RouteId::SandboxReviewApproveById, "POST") => Some(RouteAuthorizationSpec::new(
            RouteId::SandboxReviewApproveById,
            Action::SandboxReviewApprove,
            true,
            RouteAuthorizationKind::SafetyReview,
        )),
        (RouteId::SandboxReviewRejectById, "POST") => Some(RouteAuthorizationSpec::new(
            RouteId::SandboxReviewRejectById,
            Action::SandboxReviewReject,
            true,
            RouteAuthorizationKind::SafetyReview,
        )),
        (RouteId::SafetyPauseAgent, "POST") => Some(admin_spec(
            RouteId::SafetyPauseAgent,
            Action::SafetyPauseAgent,
            true,
        )),
        (RouteId::SafetyResumeAgent, "POST") => Some(RouteAuthorizationSpec::new(
            RouteId::SafetyResumeAgent,
            Action::SafetyResumeAgent,
            true,
            RouteAuthorizationKind::SafetyReview,
        )),
        (RouteId::SafetyQuarantineAgent, "POST") => Some(admin_spec(
            RouteId::SafetyQuarantineAgent,
            Action::SafetyQuarantineAgent,
            true,
        )),
        (RouteId::SafetyKillAll, "POST") => Some(superadmin_spec(
            RouteId::SafetyKillAll,
            Action::SafetyKillAll,
        )),
        (RouteId::SafetyChecks, "GET") => Some(admin_spec(
            RouteId::SafetyChecks,
            Action::SafetyCheckReadList,
            true,
        )),
        (RouteId::SafetyChecks, "POST") => Some(admin_spec(
            RouteId::SafetyChecks,
            Action::SafetyCheckRegister,
            true,
        )),
        (RouteId::SafetyCheckById, "DELETE") => Some(admin_spec(
            RouteId::SafetyCheckById,
            Action::SafetyCheckDelete,
            true,
        )),
        (RouteId::AdminBackupList, "GET") => Some(admin_spec(
            RouteId::AdminBackupList,
            Action::AdminBackupReadList,
            false,
        )),
        (RouteId::AdminBackupCreate, "POST") => Some(admin_spec(
            RouteId::AdminBackupCreate,
            Action::AdminBackupCreate,
            true,
        )),
        (RouteId::AdminExport, "GET") => Some(admin_spec(
            RouteId::AdminExport,
            Action::AdminExportRead,
            false,
        )),
        (RouteId::AdminRestore, "POST") => Some(superadmin_spec(
            RouteId::AdminRestore,
            Action::AdminRestoreVerify,
        )),
        (RouteId::ProviderKeys, "GET") => Some(admin_spec(
            RouteId::ProviderKeys,
            Action::ProviderKeyReadList,
            false,
        )),
        (RouteId::ProviderKeys, "PUT") => Some(admin_spec(
            RouteId::ProviderKeys,
            Action::ProviderKeyWrite,
            true,
        )),
        (RouteId::ProviderKeyByEnvName, "DELETE") => Some(admin_spec(
            RouteId::ProviderKeyByEnvName,
            Action::ProviderKeyDelete,
            true,
        )),
        (RouteId::CodexAuthStatus, "GET") => Some(admin_spec(
            RouteId::CodexAuthStatus,
            Action::CodexAuthReadStatus,
            false,
        )),
        (RouteId::CodexAuthLogin, "POST") => Some(admin_spec(
            RouteId::CodexAuthLogin,
            Action::CodexAuthLogin,
            true,
        )),
        (RouteId::CodexAuthLogout, "POST") => Some(admin_spec(
            RouteId::CodexAuthLogout,
            Action::CodexAuthLogout,
            true,
        )),
        (RouteId::Webhooks, "GET") => {
            Some(admin_spec(RouteId::Webhooks, Action::WebhookReadList, true))
        }
        (RouteId::Webhooks, "POST") => {
            Some(admin_spec(RouteId::Webhooks, Action::WebhookCreate, true))
        }
        (RouteId::WebhookById, "PUT") => Some(admin_spec(
            RouteId::WebhookById,
            Action::WebhookUpdate,
            true,
        )),
        (RouteId::WebhookById, "DELETE") => Some(admin_spec(
            RouteId::WebhookById,
            Action::WebhookDelete,
            true,
        )),
        (RouteId::WebhookTestById, "POST") => Some(admin_spec(
            RouteId::WebhookTestById,
            Action::WebhookTest,
            true,
        )),
        (RouteId::PcControlStatus, "GET") => Some(admin_spec(
            RouteId::PcControlStatus,
            Action::PcControlStatusRead,
            false,
        )),
        (RouteId::PcControlStatus, "PUT") => Some(admin_spec(
            RouteId::PcControlStatus,
            Action::PcControlStatusWrite,
            true,
        )),
        (RouteId::PcControlActions, "GET") => Some(admin_spec(
            RouteId::PcControlActions,
            Action::PcControlActionRead,
            false,
        )),
        (RouteId::PcControlAllowedApps, "PUT") => Some(admin_spec(
            RouteId::PcControlAllowedApps,
            Action::PcControlAllowedAppsWrite,
            true,
        )),
        (RouteId::PcControlBlockedHotkeys, "PUT") => Some(admin_spec(
            RouteId::PcControlBlockedHotkeys,
            Action::PcControlBlockedHotkeysWrite,
            true,
        )),
        (RouteId::PcControlSafeZones, "PUT") => Some(admin_spec(
            RouteId::PcControlSafeZones,
            Action::PcControlSafeZonesWrite,
            true,
        )),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    MissingClaims,
    MalformedClaims,
    DenyPredicateMatched,
    AllowPredicateNotSatisfied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationDecision {
    pub action: Action,
    pub policy_id: &'static str,
    pub allowed: bool,
    pub denial_reason: Option<DenialReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationEvaluation {
    pub principal: Option<Principal>,
    pub decision: AuthorizationDecision,
}

pub fn authorize(
    principal: &Principal,
    context: &AuthorizationContext<'_>,
) -> AuthorizationDecision {
    let registered = policy_for(context.action);
    registered
        .rule
        .evaluate(context.action, registered.policy_id, principal, context)
}

pub fn evaluate_claims(
    claims: Option<&Claims>,
    context: &AuthorizationContext<'_>,
) -> AuthorizationEvaluation {
    let Some(claims) = claims else {
        return AuthorizationEvaluation {
            principal: None,
            decision: AuthorizationDecision {
                action: context.action,
                policy_id: "typed/missing_claims",
                allowed: false,
                denial_reason: Some(DenialReason::MissingClaims),
            },
        };
    };

    let principal = match Principal::from_claims(claims, auth_mode_hint_for_claims(claims)) {
        Ok(principal) => principal,
        Err(_) => {
            return AuthorizationEvaluation {
                principal: None,
                decision: AuthorizationDecision {
                    action: context.action,
                    policy_id: "typed/malformed_claims",
                    allowed: false,
                    denial_reason: Some(DenialReason::MalformedClaims),
                },
            };
        }
    };

    AuthorizationEvaluation {
        decision: authorize(&principal, context),
        principal: Some(principal),
    }
}

pub fn authorize_claims(
    claims: Option<&Claims>,
    context: &AuthorizationContext<'_>,
) -> Result<(Principal, AuthorizationDecision), ApiError> {
    let evaluation = evaluate_claims(claims, context);
    if evaluation.decision.allowed {
        if let Some(principal) = evaluation.principal {
            return Ok((principal, evaluation.decision));
        }
        return Err(ApiError::Unauthorized(
            "Invalid authorization principal".into(),
        ));
    }

    match evaluation.decision.denial_reason {
        Some(DenialReason::MissingClaims) | Some(DenialReason::MalformedClaims) => Err(
            ApiError::Unauthorized("Invalid authorization claims".into()),
        ),
        _ => Err(ApiError::Forbidden(
            "Insufficient permissions for this operation".into(),
        )),
    }
}

pub struct RegisteredPolicy {
    pub policy_id: &'static str,
    pub rule: PolicyRule,
}

pub fn policy_for(action: Action) -> RegisteredPolicy {
    match action {
        Action::LiveExecutionRead | Action::LiveExecutionCancel => RegisteredPolicy {
            policy_id: "typed/live_execution_owner_or_admin",
            rule: PolicyRule {
                allow_if: PolicyPredicate::Any(vec![
                    PolicyPredicate::MinRole(BaseRole::Admin),
                    PolicyPredicate::SubjectMatchesResourceOwner,
                ]),
                deny_if: Vec::new(),
                audit_on_deny: false,
            },
        },
        Action::SafetyResumeAgent => RegisteredPolicy {
            policy_id: "typed/safety_resume_admin_or_operator_safety_review",
            rule: PolicyRule {
                allow_if: PolicyPredicate::Any(vec![
                    PolicyPredicate::MinRole(BaseRole::Admin),
                    PolicyPredicate::All(vec![
                        PolicyPredicate::MinRole(BaseRole::Operator),
                        PolicyPredicate::HasCapability(Capability::SafetyReview),
                    ]),
                ]),
                deny_if: Vec::new(),
                audit_on_deny: true,
            },
        },
        Action::SandboxReviewApprove | Action::SandboxReviewReject => RegisteredPolicy {
            policy_id: "typed/sandbox_review_admin_or_operator_safety_review",
            rule: PolicyRule {
                allow_if: PolicyPredicate::Any(vec![
                    PolicyPredicate::MinRole(BaseRole::Admin),
                    PolicyPredicate::All(vec![
                        PolicyPredicate::MinRole(BaseRole::Operator),
                        PolicyPredicate::HasCapability(Capability::SafetyReview),
                    ]),
                ]),
                deny_if: Vec::new(),
                audit_on_deny: true,
            },
        },
        action if viewer_actions().contains(&action) => {
            minimum_role_rule("typed/min_role_viewer", BaseRole::Viewer)
        }
        action if operator_actions().contains(&action) => {
            minimum_role_rule("typed/min_role_operator", BaseRole::Operator)
        }
        action if admin_actions().contains(&action) => {
            minimum_role_rule("typed/min_role_admin", BaseRole::Admin)
        }
        action if superadmin_actions().contains(&action) => {
            minimum_role_rule("typed/min_role_superadmin", BaseRole::SuperAdmin)
        }
        _ => minimum_role_rule("typed/min_role_superadmin_default", BaseRole::SuperAdmin),
    }
}

fn minimum_role_rule(policy_id: &'static str, minimum: BaseRole) -> RegisteredPolicy {
    RegisteredPolicy {
        policy_id,
        rule: PolicyRule {
            allow_if: PolicyPredicate::Any(vec![PolicyPredicate::MinRole(minimum)]),
            deny_if: Vec::new(),
            audit_on_deny: minimum >= BaseRole::Admin,
        },
    }
}

pub fn viewer_actions() -> &'static [Action] {
    &[
        Action::AuthSessionRead,
        Action::AgentReadList,
        Action::AuditReadQuery,
        Action::AuditAggregationRead,
        Action::AuditExportRead,
        Action::ConvergenceScoreRead,
        Action::GoalReadList,
        Action::GoalReadItem,
        Action::SessionReadList,
        Action::SessionEventRead,
        Action::SessionBookmarkRead,
        Action::MemoryReadList,
        Action::MemoryGraphRead,
        Action::MemorySearch,
        Action::MemoryArchivedRead,
        Action::MemoryItemRead,
        Action::StateCrdtRead,
        Action::IntegrityChainRead,
        Action::WorkflowReadList,
        Action::WorkflowReadItem,
        Action::WorkflowExecutionRead,
        Action::StudioSessionReadList,
        Action::StudioSessionReadItem,
        Action::StudioSessionRecoverStream,
        Action::TraceRead,
        Action::MeshTrustGraphRead,
        Action::MeshConsensusRead,
        Action::MeshDelegationRead,
        Action::ProfileReadList,
        Action::SearchRead,
        Action::SkillReadList,
        Action::A2aTaskReadList,
        Action::A2aTaskReadItem,
        Action::A2aTaskStreamRead,
        Action::A2aDiscoverRead,
        Action::ChannelReadList,
        Action::CostRead,
        Action::ItpEventRead,
        Action::WebSocketConnect,
        Action::WebSocketTicketIssue,
        Action::OAuthProviderRead,
        Action::OAuthCallbackReceive,
        Action::OAuthConnectionRead,
        Action::MarketplaceAgentReadList,
        Action::MarketplaceAgentReadItem,
        Action::MarketplaceSkillReadList,
        Action::MarketplaceSkillReadItem,
        Action::MarketplaceContractReadList,
        Action::MarketplaceContractReadItem,
        Action::MarketplaceWalletRead,
        Action::MarketplaceWalletTransactionRead,
        Action::MarketplaceReviewReadList,
        Action::SandboxReviewReadList,
    ]
}

pub fn operator_actions() -> &'static [Action] {
    &[
        Action::SafetyStatusRead,
        Action::AgentCreate,
        Action::AgentUpdate,
        Action::AgentDelete,
        Action::GoalApprove,
        Action::GoalReject,
        Action::MemoryWrite,
        Action::MemoryArchive,
        Action::MemoryUnarchive,
        Action::WorkflowCreate,
        Action::WorkflowUpdate,
        Action::WorkflowExecute,
        Action::WorkflowExecutionResume,
        Action::SessionBookmarkCreate,
        Action::SessionBookmarkDelete,
        Action::SessionBranch,
        Action::SessionHeartbeat,
        Action::StudioRunPrompt,
        Action::StudioSessionCreate,
        Action::StudioSessionDelete,
        Action::StudioSessionMessageSend,
        Action::StudioSessionMessageStream,
        Action::AgentChatSend,
        Action::AgentChatStream,
        Action::ProfileCreate,
        Action::ProfileUpdate,
        Action::ProfileDelete,
        Action::AgentProfileAssign,
        Action::SkillInstall,
        Action::SkillUninstall,
        Action::SkillQuarantine,
        Action::SkillQuarantineResolve,
        Action::SkillReverify,
        Action::SkillExecute,
        Action::ChannelCreate,
        Action::ChannelReconnect,
        Action::ChannelDelete,
        Action::ChannelInject,
        Action::A2aTaskSend,
        Action::OAuthConnect,
        Action::OAuthConnectionDelete,
        Action::OAuthExecuteApiCall,
        Action::MarketplaceAgentRegister,
        Action::MarketplaceAgentDelist,
        Action::MarketplaceAgentStatusUpdate,
        Action::MarketplaceSkillPublish,
        Action::MarketplaceContractPropose,
        Action::MarketplaceContractAccept,
        Action::MarketplaceContractReject,
        Action::MarketplaceContractStart,
        Action::MarketplaceContractComplete,
        Action::MarketplaceContractDispute,
        Action::MarketplaceContractCancel,
        Action::MarketplaceContractResolve,
        Action::MarketplaceWalletSeed,
        Action::MarketplaceReviewSubmit,
        Action::MarketplaceDiscover,
        Action::SandboxReviewReadList,
    ]
}

pub fn admin_actions() -> &'static [Action] {
    &[
        Action::SafetyCheckReadList,
        Action::AdminBackupReadList,
        Action::AdminExportRead,
        Action::ProviderKeyReadList,
        Action::CodexAuthReadStatus,
        Action::PcControlStatusRead,
        Action::PcControlStatusWrite,
        Action::PcControlActionRead,
        Action::SafetyPauseAgent,
        Action::SafetyQuarantineAgent,
        Action::SafetyCheckRegister,
        Action::SafetyCheckDelete,
        Action::WebhookReadList,
        Action::WebhookCreate,
        Action::WebhookUpdate,
        Action::WebhookDelete,
        Action::WebhookTest,
        Action::AdminBackupCreate,
        Action::ProviderKeyWrite,
        Action::ProviderKeyDelete,
        Action::CodexAuthLogin,
        Action::CodexAuthLogout,
        Action::PcControlAllowedAppsWrite,
        Action::PcControlBlockedHotkeysWrite,
        Action::PcControlSafeZonesWrite,
    ]
}

pub fn superadmin_actions() -> &'static [Action] {
    &[Action::SafetyKillAll, Action::AdminRestoreVerify]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::authz::{
        AuthMode, AuthorizationContext, AUTHZ_CLAIMS_VERSION_V1, INTERNAL_JWT_ISSUER,
    };
    use axum::http::Method;

    fn principal(role: BaseRole) -> Principal {
        Principal {
            subject: "actor-1".into(),
            base_role: role,
            capabilities: Default::default(),
            auth_mode: AuthMode::Jwt,
            token_id: Some("jti-1".into()),
            authz_version: AUTHZ_CLAIMS_VERSION_V1,
            issuer: Some(INTERNAL_JWT_ISSUER.into()),
        }
    }

    #[test]
    fn live_execution_allows_owner_without_admin_role() {
        let actor = principal(BaseRole::Operator);
        let context =
            AuthorizationContext::new(Action::LiveExecutionRead, RouteId::LiveExecutionById)
                .with_resource(crate::api::authz::ResourceContext::LiveExecution {
                    execution_id: "exec-1",
                    owner_subject: Some("actor-1"),
                });

        let decision = authorize(&actor, &context);
        assert!(decision.allowed);
        assert_eq!(decision.denial_reason, None);
    }

    #[test]
    fn live_execution_denies_non_owner_non_admin() {
        let actor = principal(BaseRole::Operator);
        let context =
            AuthorizationContext::new(Action::LiveExecutionRead, RouteId::LiveExecutionById)
                .with_resource(crate::api::authz::ResourceContext::LiveExecution {
                    execution_id: "exec-1",
                    owner_subject: Some("different-actor"),
                });

        let decision = authorize(&actor, &context);
        assert!(!decision.allowed);
        assert_eq!(
            decision.denial_reason,
            Some(DenialReason::AllowPredicateNotSatisfied)
        );
    }

    #[test]
    fn live_execution_cancel_allows_owner_without_admin_role() {
        let actor = principal(BaseRole::Viewer);
        let context = AuthorizationContext::new(
            Action::LiveExecutionCancel,
            RouteId::LiveExecutionCancelById,
        )
        .with_resource(crate::api::authz::ResourceContext::LiveExecution {
            execution_id: "exec-1",
            owner_subject: Some("actor-1"),
        });

        let decision = authorize(&actor, &context);
        assert!(decision.allowed);
        assert_eq!(decision.denial_reason, None);
    }

    #[test]
    fn safety_resume_allows_operator_with_safety_review() {
        let mut actor = principal(BaseRole::Operator);
        actor.capabilities.insert(Capability::SafetyReview);
        let context =
            AuthorizationContext::new(Action::SafetyResumeAgent, RouteId::SafetyResumeAgent);

        let decision = authorize(&actor, &context);
        assert!(decision.allowed);
    }

    #[test]
    fn safety_resume_denies_plain_operator() {
        let actor = principal(BaseRole::Operator);
        let context =
            AuthorizationContext::new(Action::SafetyResumeAgent, RouteId::SafetyResumeAgent);

        let decision = authorize(&actor, &context);
        assert!(!decision.allowed);
        assert_eq!(
            decision.denial_reason,
            Some(DenialReason::AllowPredicateNotSatisfied)
        );
    }

    #[test]
    fn admin_backup_still_denies_dev_operator() {
        let actor = Principal {
            auth_mode: AuthMode::NoAuthDev,
            authz_version: 0,
            issuer: None,
            ..principal(BaseRole::Operator)
        };
        let context =
            AuthorizationContext::new(Action::AdminBackupCreate, RouteId::AdminBackupCreate);

        let decision = authorize(&actor, &context);
        assert!(!decision.allowed);
    }

    #[test]
    fn route_spec_for_provider_keys_distinguishes_get_and_put() {
        let read_spec = route_spec_for(RouteId::ProviderKeys, &Method::GET).expect("read spec");
        let write_spec = route_spec_for(RouteId::ProviderKeys, &Method::PUT).expect("write spec");

        assert_eq!(read_spec.action, Action::ProviderKeyReadList);
        assert!(!read_spec.compatibility_required);
        assert_eq!(
            read_spec.authorization_kind,
            RouteAuthorizationKind::MinimumRole(BaseRole::Admin)
        );
        assert_eq!(write_spec.action, Action::ProviderKeyWrite);
        assert!(write_spec.compatibility_required);
        assert_eq!(
            write_spec.authorization_kind,
            RouteAuthorizationKind::MinimumRole(BaseRole::Admin)
        );
    }

    #[test]
    fn route_spec_for_codex_login_requires_admin_and_compatibility() {
        let status_spec =
            route_spec_for(RouteId::CodexAuthStatus, &Method::GET).expect("status spec");
        let login_spec =
            route_spec_for(RouteId::CodexAuthLogin, &Method::POST).expect("login spec");
        let logout_spec =
            route_spec_for(RouteId::CodexAuthLogout, &Method::POST).expect("logout spec");

        assert_eq!(status_spec.action, Action::CodexAuthReadStatus);
        assert!(!status_spec.compatibility_required);
        assert_eq!(login_spec.action, Action::CodexAuthLogin);
        assert!(login_spec.compatibility_required);
        assert_eq!(logout_spec.action, Action::CodexAuthLogout);
        assert!(logout_spec.compatibility_required);
    }

    #[test]
    fn route_spec_for_safety_status_skips_compatibility() {
        let spec = route_spec_for(RouteId::SafetyStatus, &Method::GET).expect("spec");
        assert_eq!(spec.action, Action::SafetyStatusRead);
        assert!(!spec.compatibility_required);
        assert_eq!(
            spec.authorization_kind,
            RouteAuthorizationKind::MinimumRole(BaseRole::Operator)
        );
    }

    #[test]
    fn route_spec_for_safety_resume_uses_safety_review_authorization_kind() {
        let spec =
            route_spec_for(RouteId::SafetyResumeAgent, &Method::POST).expect("safety resume spec");

        assert_eq!(spec.action, Action::SafetyResumeAgent);
        assert!(spec.compatibility_required);
        assert_eq!(
            spec.authorization_kind,
            RouteAuthorizationKind::SafetyReview
        );
    }
}
