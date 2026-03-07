// ── Client ──
export { GhostClient } from './client.js';
export type { GhostClientOptions, GhostRequestFn } from './client.js';

// ── Errors ──
export {
  GhostError,
  GhostAPIError,
  GhostNetworkError,
  GhostTimeoutError,
} from './errors.js';

// ── Agents ──
export { AgentsAPI } from './agents.js';
export type { Agent, AgentDetail, CreateAgentParams, DeleteAgentResult } from './agents.js';

// ── Sessions ──
export { SessionsAPI } from './sessions.js';
export type {
  StudioSession,
  StudioMessage,
  StudioSessionWithMessages,
  CreateSessionParams,
  ListSessionsParams,
  RecoverStreamEvent,
  RecoverStreamResult,
} from './sessions.js';

// ── Chat ──
export { ChatAPI } from './chat.js';
export type {
  SendMessageParams,
  SendMessageResult,
  StreamEvent,
  ChatStreamEventHandler,
} from './chat.js';

// ── Convergence ──
export { ConvergenceAPI } from './convergence.js';
export type {
  ConvergenceScore,
  ConvergenceError,
  ConvergenceScoresResult,
} from './convergence.js';

// ── Goals / Proposals ──
export { GoalsAPI } from './goals.js';
export type {
  Proposal,
  ProposalDetail,
  ListGoalsParams,
  ListGoalsResult,
} from './goals.js';

// ── Skills ──
export { SkillsAPI } from './skills.js';
export type { Skill, ListSkillsResult } from './skills.js';

// ── Safety ──
export { SafetyAPI } from './safety.js';
export type {
  SafetyStatus,
  KillAllResult,
  PauseResult,
  ResumeResult,
  QuarantineResult,
  ResumeParams,
} from './safety.js';

// ── Health ──
export { HealthAPI } from './health.js';
export type { HealthStatus, ReadyStatus } from './health.js';

// ── Auth ──
export { AuthAPI } from './auth.js';
export type { LoginParams, AuthTokenResponse, LogoutResponse } from './auth.js';

// ── Audit ──
export { AuditAPI } from './audit.js';
export type { AuditEntry, AuditQueryParams, AuditQueryResult, AuditExportParams } from './audit.js';

// ── Costs ──
export { CostsAPI } from './costs.js';
export type { AgentCostInfo } from './costs.js';

// ── Memory ──
export { MemoryAPI } from './memory.js';
export type {
  MemoryEntry,
  ListMemoriesParams,
  ListMemoriesResult,
  MemoryGraphNode,
  MemoryGraphEdge,
  MemoryGraphResult,
  MemorySearchResultEntry,
  SearchMemoriesParams,
  SearchMemoriesResult,
} from './memory.js';

// ── Runtime Sessions ──
export { RuntimeSessionsAPI } from './runtime-sessions.js';
export type {
  RuntimeSession,
  SessionEvent,
  SessionEventsParams,
  SessionEventsResult,
  SessionBookmark,
  SessionBookmarksResult,
  CreateSessionBookmarkParams,
  CreateSessionBookmarkResult,
  DeleteSessionBookmarkResult,
  BranchSessionParams,
  BranchSessionResult,
  ListRuntimeSessionsParams,
  ListRuntimeSessionsPageResult,
  ListRuntimeSessionsCursorResult,
} from './runtime-sessions.js';

// ── Search ──
export { SearchAPI } from './search.js';
export type { SearchParams, SearchResult, SearchResponse } from './search.js';

// ── Traces ──
export { TracesAPI } from './traces.js';
export type { TraceSpanRecord, TraceGroup, SessionTrace } from './traces.js';

// ── Workflows ──
export { WorkflowsAPI } from './workflows.js';
export type {
  Workflow,
  ListWorkflowsParams,
  ListWorkflowsResult,
  CreateWorkflowParams,
  CreateWorkflowResult,
  UpdateWorkflowParams,
  UpdateWorkflowResult,
  ExecuteWorkflowParams,
  WorkflowExecutionStep,
  ExecuteWorkflowResult,
} from './workflows.js';

// ── Profiles ──
export { ProfilesAPI } from './profiles.js';
export type {
  Profile,
  ListProfilesResult,
  CreateProfileParams,
  UpdateProfileParams,
  DeleteProfileResult,
} from './profiles.js';

// ── Webhooks ──
export { WebhooksAPI } from './webhooks.js';
export type {
  WebhookEventType,
  WebhookSummary,
  ListWebhooksResult,
  CreateWebhookParams,
  UpdateWebhookParams,
  DeleteWebhookResult,
  TestWebhookResult,
} from './webhooks.js';

// ── Backups ──
export { BackupsAPI } from './backups.js';
export type { Backup, ListBackupsResult } from './backups.js';

// ── Provider Keys ──
export { ProviderKeysAPI } from './provider-keys.js';
export type {
  ProviderKeyInfo,
  ListProviderKeysResult,
  SetProviderKeyParams,
  SetProviderKeyResult,
  DeleteProviderKeyResult,
} from './provider-keys.js';

// ── Push ──
export { PushAPI } from './push.js';
export type {
  PushSubscriptionKeys,
  PushSubscriptionPayload,
  VapidKeyResult,
} from './push.js';

// ── Channels ──
export { ChannelsAPI } from './channels.js';
export type {
  ChannelInfo,
  ListChannelsResult,
  CreateChannelParams,
  CreateChannelResult,
  ReconnectChannelResult,
  DeleteChannelResult,
} from './channels.js';

// ── State ──
export { StateAPI } from './state.js';
export type {
  CrdtDelta,
  GetCrdtStateParams,
  CrdtStateResult,
} from './state.js';

// ── Integrity ──
export { IntegrityAPI } from './integrity.js';
export type {
  IntegrityBreak,
  ItpEventsIntegrity,
  MemoryEventsIntegrity,
  IntegrityChains,
  VerifyChainParams,
  VerifyChainResult,
} from './integrity.js';

// ── Mesh ──
export { MeshAPI } from './mesh.js';
export type {
  TrustNode,
  TrustEdge,
  TrustGraphResult,
  ConsensusRound,
  ConsensusResult,
  Delegation,
  SybilMetrics,
  DelegationsResult,
} from './mesh.js';

// ── A2A ──
export { A2AAPI } from './a2a.js';
export type {
  A2ATask,
  SendA2ATaskParams,
  ListA2ATasksResult,
  DiscoveredA2AAgent,
  DiscoverA2AAgentsResult,
} from './a2a.js';

// ── Studio ──
export { StudioAPI } from './studio.js';
export type {
  StudioRunMessageInput,
  StudioRunParams,
  StudioRunResult,
} from './studio.js';

// ── Approvals ──
export { ApprovalsAPI } from './approvals.js';
export type {
  Approval,
  ApprovalDetails,
  ApprovalRiskLevel,
  ApprovalStatus,
  ApprovalType,
  ListApprovalsParams,
  ListApprovalsResult,
  ApproveApprovalParams,
} from './approvals.js';

// ── PC Control ──
export { PcControlAPI } from './pc-control.js';
export type {
  SafeZone,
  ActionBudget,
  PcControlStatus,
  PcControlActionLogEntry,
  PcControlActionLogResult,
} from './pc-control.js';

// ── WebSocket ──
export { GhostWebSocket } from './websocket.js';
export type { WsEvent, GhostWebSocketOptions } from './websocket.js';

// ── Generated OpenAPI Types ──
export type { paths, components, operations } from './generated-types.js';
