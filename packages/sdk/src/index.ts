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

// ── Traces ──
export { TracesAPI } from './traces.js';
export type { TraceSpanRecord, TraceGroup, SessionTrace } from './traces.js';

// ── WebSocket ──
export { GhostWebSocket } from './websocket.js';
export type { WsEvent, GhostWebSocketOptions } from './websocket.js';

// ── Generated OpenAPI Types ──
export type { paths, components, operations } from './generated-types.js';
