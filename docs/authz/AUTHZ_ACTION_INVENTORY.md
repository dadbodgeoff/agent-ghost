# Authz Action Inventory

Status: complete and live on March 9, 2026.

This file is the action authority for the authz remediation. It records the
current protected gateway surface, the canonical `Action` identifier, and the
intended minimum policy shape.

Notes:

- `Public` routes are intentionally excluded.
- `GET` routes are still listed when they require authenticated access.
- `LiveExecutionRead` and `SafetyResumeAgent` are special policies and do not
  collapse to a single minimum base role.
- The inventory now matches the live typed authorizer and route binding.

## Special Policies

| Action | Route | Intended policy |
| --- | --- | --- |
| `LiveExecutionRead` | `GET /api/live-executions/:execution_id` | `admin` or resource owner |
| `SafetyResumeAgent` | `POST /api/safety/resume/:agent_id` | `admin+` or `operator + safety_review` |

## Viewer Minimum

| Action | Route |
| --- | --- |
| `AuthSessionRead` | `GET /api/auth/session` |
| `AgentReadList` | `GET /api/agents` |
| `AuditReadQuery` | `GET /api/audit` |
| `AuditAggregationRead` | `GET /api/audit/aggregation` |
| `AuditExportRead` | `GET /api/audit/export` |
| `ConvergenceScoreRead` | `GET /api/convergence/scores` |
| `GoalReadList` | `GET /api/goals` |
| `GoalReadItem` | `GET /api/goals/:id` |
| `SessionReadList` | `GET /api/sessions` |
| `SessionEventRead` | `GET /api/sessions/:id/events` |
| `SessionBookmarkRead` | `GET /api/sessions/:id/bookmarks` |
| `MemoryReadList` | `GET /api/memory` |
| `MemoryGraphRead` | `GET /api/memory/graph` |
| `MemorySearch` | `GET /api/memory/search` |
| `MemoryArchivedRead` | `GET /api/memory/archived` |
| `MemoryItemRead` | `GET /api/memory/:id` |
| `StateCrdtRead` | `GET /api/state/crdt/:agent_id` |
| `IntegrityChainRead` | `GET /api/integrity/chain/:agent_id` |
| `WorkflowReadList` | `GET /api/workflows` |
| `WorkflowReadItem` | `GET /api/workflows/:id` |
| `WorkflowExecutionRead` | `GET /api/workflows/:id/executions` |
| `StudioSessionReadList` | `GET /api/studio/sessions` |
| `StudioSessionReadItem` | `GET /api/studio/sessions/:id` |
| `StudioSessionRecoverStream` | `GET /api/studio/sessions/:id/stream/recover` |
| `TraceRead` | `GET /api/traces/:session_id` |
| `MeshTrustGraphRead` | `GET /api/mesh/trust-graph` |
| `MeshConsensusRead` | `GET /api/mesh/consensus` |
| `MeshDelegationRead` | `GET /api/mesh/delegations` |
| `ProfileReadList` | `GET /api/profiles` |
| `SearchRead` | `GET /api/search` |
| `SkillReadList` | `GET /api/skills` |
| `A2aTaskReadList` | `GET /api/a2a/tasks` |
| `A2aTaskReadItem` | `GET /api/a2a/tasks/:task_id` |
| `A2aTaskStreamRead` | `GET /api/a2a/tasks/:task_id/stream` |
| `A2aDiscoverRead` | `GET /api/a2a/discover` |
| `ChannelReadList` | `GET /api/channels` |
| `CostRead` | `GET /api/costs` |
| `ItpEventRead` | `GET /api/itp/events` |
| `WebSocketConnect` | `GET /api/ws` |
| `WebSocketTicketIssue` | `POST /api/ws/tickets` |
| `OAuthProviderRead` | `GET /api/oauth/providers` |
| `OAuthCallbackReceive` | `GET /api/oauth/callback` |
| `OAuthConnectionRead` | `GET /api/oauth/connections` |
| `MarketplaceAgentReadList` | `GET /api/marketplace/agents` |
| `MarketplaceAgentReadItem` | `GET /api/marketplace/agents/:id` |
| `MarketplaceSkillReadList` | `GET /api/marketplace/skills` |
| `MarketplaceSkillReadItem` | `GET /api/marketplace/skills/:name` |
| `MarketplaceContractReadList` | `GET /api/marketplace/contracts` |
| `MarketplaceContractReadItem` | `GET /api/marketplace/contracts/:id` |
| `MarketplaceWalletRead` | `GET /api/marketplace/wallet` |
| `MarketplaceWalletTransactionRead` | `GET /api/marketplace/wallet/transactions` |
| `MarketplaceReviewReadList` | `GET /api/marketplace/reviews/:agent_id` |

## Operator Minimum

| Action | Route |
| --- | --- |
| `SafetyStatusRead` | `GET /api/safety/status` |
| `AgentCreate` | `POST /api/agents` |
| `AgentDelete` | `DELETE /api/agents/:id` |
| `GoalApprove` | `POST /api/goals/:id/approve` |
| `GoalReject` | `POST /api/goals/:id/reject` |
| `MemoryWrite` | `POST /api/memory` |
| `MemoryArchive` | `POST /api/memory/:id/archive` |
| `MemoryUnarchive` | `POST /api/memory/:id/unarchive` |
| `WorkflowCreate` | `POST /api/workflows` |
| `WorkflowUpdate` | `PUT /api/workflows/:id` |
| `WorkflowExecute` | `POST /api/workflows/:id/execute` |
| `WorkflowExecutionResume` | `POST /api/workflows/:id/resume/:execution_id` |
| `SessionBookmarkCreate` | `POST /api/sessions/:id/bookmarks` |
| `SessionBookmarkDelete` | `DELETE /api/sessions/:id/bookmarks/:bookmark_id` |
| `SessionBranch` | `POST /api/sessions/:id/branch` |
| `SessionHeartbeat` | `POST /api/sessions/:id/heartbeat` |
| `StudioRunPrompt` | `POST /api/studio/run` |
| `StudioSessionCreate` | `POST /api/studio/sessions` |
| `StudioSessionDelete` | `DELETE /api/studio/sessions/:id` |
| `StudioSessionMessageSend` | `POST /api/studio/sessions/:id/messages` |
| `StudioSessionMessageStream` | `POST /api/studio/sessions/:id/messages/stream` |
| `AgentChatSend` | `POST /api/agent/chat` |
| `AgentChatStream` | `POST /api/agent/chat/stream` |
| `ProfileCreate` | `POST /api/profiles` |
| `ProfileUpdate` | `PUT /api/profiles/:name` |
| `ProfileDelete` | `DELETE /api/profiles/:name` |
| `AgentProfileAssign` | `POST /api/agents/:id/profile` |
| `SkillInstall` | `POST /api/skills/:id/install` |
| `SkillUninstall` | `POST /api/skills/:id/uninstall` |
| `SkillQuarantine` | `POST /api/skills/:id/quarantine` |
| `SkillQuarantineResolve` | `POST /api/skills/:id/quarantine/resolve` |
| `SkillReverify` | `POST /api/skills/:id/reverify` |
| `SkillExecute` | `POST /api/skills/:name/execute` |
| `ChannelCreate` | `POST /api/channels` |
| `ChannelReconnect` | `POST /api/channels/:id/reconnect` |
| `ChannelDelete` | `DELETE /api/channels/:id` |
| `ChannelInject` | `POST /api/channels/:type/inject` |
| `A2aTaskSend` | `POST /api/a2a/tasks` |
| `OAuthConnect` | `POST /api/oauth/connect` |
| `OAuthConnectionDelete` | `DELETE /api/oauth/connections/:ref_id` |
| `OAuthExecuteApiCall` | `POST /api/oauth/execute` |
| `MarketplaceAgentRegister` | `POST /api/marketplace/agents` |
| `MarketplaceAgentDelist` | `DELETE /api/marketplace/agents/:id` |
| `MarketplaceAgentStatusUpdate` | `PUT /api/marketplace/agents/:id/status` |
| `MarketplaceSkillPublish` | `POST /api/marketplace/skills` |
| `MarketplaceContractPropose` | `POST /api/marketplace/contracts` |
| `MarketplaceContractAccept` | `POST /api/marketplace/contracts/:id/accept` |
| `MarketplaceContractReject` | `POST /api/marketplace/contracts/:id/reject` |
| `MarketplaceContractStart` | `POST /api/marketplace/contracts/:id/start` |
| `MarketplaceContractComplete` | `POST /api/marketplace/contracts/:id/complete` |
| `MarketplaceContractDispute` | `POST /api/marketplace/contracts/:id/dispute` |
| `MarketplaceContractCancel` | `POST /api/marketplace/contracts/:id/cancel` |
| `MarketplaceContractResolve` | `POST /api/marketplace/contracts/:id/resolve` |
| `MarketplaceWalletSeed` | `POST /api/marketplace/wallet/seed` |
| `MarketplaceReviewSubmit` | `POST /api/marketplace/reviews` |
| `MarketplaceDiscover` | `POST /api/marketplace/discover` |

## Admin Minimum

| Action | Route |
| --- | --- |
| `SafetyCheckReadList` | `GET /api/safety/checks` |
| `AdminBackupReadList` | `GET /api/admin/backups` |
| `AdminExportRead` | `GET /api/admin/export` |
| `ProviderKeyReadList` | `GET /api/admin/provider-keys` |
| `PcControlStatusRead` | `GET /api/pc-control/status` |
| `PcControlStatusWrite` | `PUT /api/pc-control/status` |
| `PcControlActionRead` | `GET /api/pc-control/actions` |
| `SafetyPauseAgent` | `POST /api/safety/pause/:agent_id` |
| `SafetyQuarantineAgent` | `POST /api/safety/quarantine/:agent_id` |
| `SafetyCheckRegister` | `POST /api/safety/checks` |
| `SafetyCheckDelete` | `DELETE /api/safety/checks/:id` |
| `WebhookReadList` | `GET /api/webhooks` |
| `WebhookCreate` | `POST /api/webhooks` |
| `WebhookUpdate` | `PUT /api/webhooks/:id` |
| `WebhookDelete` | `DELETE /api/webhooks/:id` |
| `WebhookTest` | `POST /api/webhooks/:id/test` |
| `AdminBackupCreate` | `POST /api/admin/backup` |
| `ProviderKeyWrite` | `PUT /api/admin/provider-keys` |
| `ProviderKeyDelete` | `DELETE /api/admin/provider-keys/:env_name` |
| `PcControlAllowedAppsWrite` | `PUT /api/pc-control/allowed-apps` |
| `PcControlBlockedHotkeysWrite` | `PUT /api/pc-control/blocked-hotkeys` |
| `PcControlSafeZonesWrite` | `PUT /api/pc-control/safe-zones` |

## Superadmin Minimum

| Action | Route |
| --- | --- |
| `SafetyKillAll` | `POST /api/safety/kill-all` |
| `AdminRestoreVerify` | `POST /api/admin/restore` |
