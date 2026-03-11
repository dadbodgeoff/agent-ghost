import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GhostClient } from '../client.js';
import { assessGhostClientCompatibility } from '../compatibility.js';
import { GhostAPIError, GhostNetworkError, GhostTimeoutError } from '../errors.js';
import type { components, operations } from '../generated-types.js';
import type { SendMessageParams, SendMessageResult } from '../chat.js';
import type {
  ConvergenceHistoryResult,
  ConvergenceScoresResult,
} from '../convergence.js';
import type {
  Backup,
  CreateBackupResult,
  ListBackupsResult,
  VerifyRestoreParams,
  VerifyRestoreResult,
} from '../backups.js';
import type {
  GoalProposalTransition,
  GoalDecisionResult,
  ListGoalsParams,
  ListGoalsResult,
  Proposal,
  ProposalDetail,
} from '../goals.js';
import type {
  CrdtDelta,
  CrdtStateResult,
  GetCrdtStateParams,
} from '../state.js';
import type {
  Delegation,
  DelegationsResult,
  ConsensusResult,
  ConsensusRound,
  SybilMetrics,
  TrustEdge,
  TrustGraphResult,
  TrustNode,
} from '../mesh.js';
import type {
  IntegrityBreak,
  IntegrityChains,
  ItpEventsIntegrity,
  MemoryEventsIntegrity,
  VerifyChainParams,
  VerifyChainResult,
} from '../integrity.js';
import type {
  ItpEvent,
  ListItpEventsParams,
  ListItpEventsResult,
} from '../itp.js';
import type {
  ListMemoriesParams,
  ListMemoriesResult,
  MemoryEntry,
  MemoryGraphEdge,
  MemoryGraphNode,
  MemoryGraphResult,
  MemorySearchResultEntry,
  SearchMemoriesParams,
  SearchMemoriesResult,
} from '../memory.js';
import type {
  BranchSessionParams,
  BranchSessionResult,
  CreateSessionBookmarkParams,
  CreateSessionBookmarkResult,
  DeleteSessionBookmarkResult,
  ListRuntimeSessionsResult,
  ListRuntimeSessionsParams,
  RuntimeSession,
  RuntimeSessionDetailResult,
  SessionBookmark,
  SessionBookmarksResult,
  SessionEvent,
  SessionEventsParams,
  SessionEventsResult,
} from '../runtime-sessions.js';
import type {
  CreateProfileParams,
  CreateProfileResult,
  DeleteProfileResult,
  ListProfilesResult,
  Profile,
  UpdateProfileParams,
} from '../profiles.js';
import type { PcControlActionLogResult, PcControlStatus, SafeZone } from '../pc-control.js';
import type {
  DeleteProviderKeyResult,
  ListProviderKeysResult,
  SetProviderKeyParams,
  SetProviderKeyResult,
} from '../provider-keys.js';
import type { ListSessionsParams, ListSessionsResult, RecoverStreamResult } from '../sessions.js';
import type {
  PushSubscriptionKeys,
  PushSubscriptionPayload,
  VapidKeyResult,
} from '../push.js';
import type { StudioRunParams, StudioRunResult } from '../studio.js';
import type { SessionTrace } from '../traces.js';
import type { AdeObservabilitySnapshot } from '../observability.js';

type Assert<T extends true> = T;
type Extends<A, B> = [A] extends [B] ? true : false;
type IsUnknown<T> = unknown extends T
  ? ([keyof T] extends [never] ? true : false)
  : false;

type GeneratedListSessionsParams = NonNullable<
  operations['list_studio_sessions']['parameters']['query']
>;
type GeneratedListSessionsResult = Omit<
  components['schemas']['StudioSessionListResponseSchema'],
  'sessions'
> & {
  sessions: components['schemas']['StudioSessionSchema'][];
};
type GeneratedRecoverStreamResult = Omit<
  components['schemas']['StudioRecoverStreamResponseSchema'],
  'events'
> & {
  events: Array<
    Omit<components['schemas']['StudioRecoverStreamEventSchema'], 'payload'> & {
      payload: Record<string, unknown>;
    }
  >;
};
type GeneratedSendMessageParams =
  operations['send_studio_message']['requestBody']['content']['application/json'];
type GeneratedSendMessageCompletedResult = Omit<
  components['schemas']['StudioSendMessageResponseSchema'],
  'assistant_message' | 'safety_status' | 'user_message'
> & {
  user_message: components['schemas']['StudioMessageSchema'] & {
    role: 'user' | 'assistant' | 'system';
    safety_status: 'clean' | 'warning' | 'blocked';
  };
  assistant_message: components['schemas']['StudioMessageSchema'] & {
    role: 'user' | 'assistant' | 'system';
    safety_status: 'clean' | 'warning' | 'blocked';
  };
  safety_status: 'clean' | 'warning' | 'blocked';
};
type GeneratedSendMessageAcceptedResult =
  components['schemas']['StudioMessageAcceptedResponseSchema'];
type GeneratedSendMessageResult =
  GeneratedSendMessageCompletedResult | GeneratedSendMessageAcceptedResult;
type GeneratedAgentChatRequest =
  operations['agent_chat']['requestBody']['content']['application/json'];
type GeneratedAgentChatResponse =
  operations['agent_chat']['responses'][200]['content']['application/json'];
type GeneratedAgentChatAcceptedResponse =
  operations['agent_chat']['responses'][202]['content']['application/json'];
type GeneratedAgentChatStreamRequest =
  operations['agent_chat_stream']['requestBody']['content']['application/json'];
type GeneratedConvergenceScoresResult =
  operations['get_convergence_scores']['responses'][200]['content']['application/json'];
type GeneratedListChannelsResponse =
  operations['list_channels']['responses'][200]['content']['application/json'];
type GeneratedCreateBackupResult =
  operations['create_backup']['responses'][200]['content']['application/json'];
type GeneratedListBackupsResult =
  operations['list_backups']['responses'][200]['content']['application/json'];
type GeneratedRestoreBackupRequest =
  operations['restore_backup']['requestBody']['content']['application/json'];
type GeneratedRestoreBackupResponse =
  operations['restore_backup']['responses'][200]['content']['application/json'];
type GeneratedExportBackupDataJsonResponse =
  operations['export_backup_data']['responses'][200]['content']['application/json'];
type GeneratedExportBackupDataNdjsonResponse =
  operations['export_backup_data']['responses'][200]['content']['application/x-ndjson'];
type GeneratedCreateChannelRequest =
  operations['create_channel']['requestBody']['content']['application/json'];
type GeneratedCreateChannelResponse =
  operations['create_channel']['responses'][201]['content']['application/json'];
type GeneratedReconnectChannelResponse =
  operations['reconnect_channel']['responses'][200]['content']['application/json'];
type GeneratedDeleteChannelResponse =
  operations['delete_channel']['responses'][200]['content']['application/json'];
type GeneratedTracesResult =
  operations['get_traces']['responses'][200]['content']['application/json'];
type GeneratedListProfilesResult =
  operations['list_profiles']['responses'][200]['content']['application/json'];
type GeneratedCreateProfileParams =
  operations['create_profile']['requestBody']['content']['application/json'];
type GeneratedCreateProfileResult =
  operations['create_profile']['responses'][201]['content']['application/json'];
type GeneratedUpdateProfileParams =
  operations['update_profile']['requestBody']['content']['application/json'];
type GeneratedUpdateProfileResult =
  operations['update_profile']['responses'][200]['content']['application/json'];
type GeneratedDeleteProfileResult =
  operations['delete_profile']['responses'][200]['content']['application/json'];
type GeneratedAssignProfileRequest =
  operations['assign_agent_profile']['requestBody']['content']['application/json'];
type GeneratedAssignProfileResponse =
  operations['assign_agent_profile']['responses'][200]['content']['application/json'];
type GeneratedListProviderKeysResult =
  operations['list_provider_keys']['responses'][200]['content']['application/json'];
type GeneratedSetProviderKeyParams =
  operations['set_provider_key']['requestBody']['content']['application/json'];
type GeneratedSetProviderKeyResult =
  operations['set_provider_key']['responses'][200]['content']['application/json'];
type GeneratedDeleteProviderKeyResult =
  operations['delete_provider_key']['responses'][200]['content']['application/json'];
type GeneratedInjectChannelMessageRequest =
  operations['inject_channel_message']['requestBody']['content']['application/json'];
type GeneratedInjectChannelMessageResponse =
  operations['inject_channel_message']['responses'][202]['content']['application/json'];
type GeneratedConnectOAuthProviderRequest =
  operations['connect_oauth_provider']['requestBody']['content']['application/json'];
type GeneratedConnectOAuthProviderResponse =
  operations['connect_oauth_provider']['responses'][200]['content']['application/json'];
type GeneratedOAuthCallbackResponse =
  operations['oauth_callback']['responses'][200]['content']['application/json'];
type GeneratedDisconnectOAuthConnectionResponse =
  operations['disconnect_oauth_connection']['responses'][200]['content']['application/json'];
type GeneratedExecuteOAuthApiCallRequest =
  operations['execute_oauth_api_call']['requestBody']['content']['application/json'];
type GeneratedExecuteOAuthApiCallResponse =
  operations['execute_oauth_api_call']['responses'][200]['content']['application/json'];
type GeneratedExecuteOAuthApiCallAcceptedResponse =
  operations['execute_oauth_api_call']['responses'][202]['content']['application/json'];
type GeneratedStudioRunParams = Omit<
  operations['studio_run']['requestBody']['content']['application/json'],
  'messages'
> & {
  messages: Array<
    Omit<components['schemas']['StudioMessage'], 'role'> & {
      role: 'user' | 'assistant' | 'system';
    }
  >;
};
type GeneratedStudioRunResult = components['schemas']['StudioRunResponse'];
type GeneratedProposal = Omit<
  components['schemas']['GoalProposalSummary'],
  'decision' | 'resolved_at'
> & {
  decision: string | null;
  resolved_at: string | null;
};
type GeneratedGoalProposalTransition = Omit<
  components['schemas']['GoalProposalTransition'],
  | 'actor_id'
  | 'reason_code'
  | 'rationale'
  | 'expected_state'
  | 'expected_revision'
  | 'operation_id'
  | 'request_id'
  | 'idempotency_key'
> & {
  actor_id: string | null;
  reason_code: string | null;
  rationale: string | null;
  expected_state: string | null;
  expected_revision: string | null;
  operation_id: string | null;
  request_id: string | null;
  idempotency_key: string | null;
};
type GeneratedListGoalsParams = NonNullable<
  operations['list_goals']['parameters']['query']
>;
type GeneratedListGoalsResult = Omit<
  operations['list_goals']['responses'][200]['content']['application/json'],
  'proposals'
> & {
  proposals: GeneratedProposal[];
};
type GeneratedGoalDetailResult = Omit<
  operations['get_goal']['responses'][200]['content']['application/json'],
  'decision' | 'resolved_at' | 'resolver' | 'transition_history' | 'content'
> & {
  decision: string | null;
  resolved_at: string | null;
  resolver: string | null;
  content: Record<string, unknown>;
  transition_history?: GeneratedGoalProposalTransition[];
};
type GeneratedGoalDecisionResult =
  operations['approve_goal']['responses'][200]['content']['application/json'];
type GeneratedGetCrdtStateParams = NonNullable<
  operations['get_crdt_state']['parameters']['query']
>;
type GeneratedGetCrdtStateResult =
  operations['get_crdt_state']['responses'][200]['content']['application/json'];
type GeneratedTrustGraphResult =
  operations['get_mesh_trust_graph']['responses'][200]['content']['application/json'];
type GeneratedConsensusResult =
  operations['get_mesh_consensus']['responses'][200]['content']['application/json'];
type GeneratedDelegationsResult =
  operations['list_mesh_delegations']['responses'][200]['content']['application/json'];
type GeneratedVerifyChainParams = NonNullable<
  operations['verify_integrity_chain']['parameters']['query']
>;
type GeneratedVerifyChainResult =
  operations['verify_integrity_chain']['responses'][200]['content']['application/json'];
type GeneratedListItpEventsParams = NonNullable<
  operations['list_itp_events']['parameters']['query']
>;
type GeneratedListItpEventsResult =
  operations['list_itp_events']['responses'][200]['content']['application/json'];
type GeneratedListMemoriesParams = NonNullable<
  operations['list_memories']['parameters']['query']
>;
type GeneratedListMemoriesResult =
  operations['list_memories']['responses'][200]['content']['application/json'];
type GeneratedGetMemoryResult =
  operations['get_memory']['responses'][200]['content']['application/json'];
type GeneratedMemoryGraphEdge = Omit<components['schemas']['MemoryGraphEdge'], 'source' | 'target'> & {
  source: string | MemoryGraphNode;
  target: string | MemoryGraphNode;
};
type GeneratedMemoryGraphResult = Omit<
  operations['get_memory_graph']['responses'][200]['content']['application/json'],
  'edges'
> & {
  edges: GeneratedMemoryGraphEdge[];
};
type GeneratedSearchMemoriesParams = NonNullable<
  operations['search_memories']['parameters']['query']
>;
type GeneratedSearchMemoriesResult =
  operations['search_memories']['responses'][200]['content']['application/json'];
type GeneratedListRuntimeSessionsParams = ListRuntimeSessionsParams;
type GeneratedListRuntimeSessionsResult = ListRuntimeSessionsResult;
type GeneratedRuntimeSessionDetailResult = RuntimeSessionDetailResult;
type GeneratedSessionEventsParams = SessionEventsParams;
type GeneratedSessionEventsResult = SessionEventsResult;
type GeneratedSessionBookmarksResult = SessionBookmarksResult;
type GeneratedCreateSessionBookmarkParams = CreateSessionBookmarkParams;
type GeneratedCreateSessionBookmarkResult = CreateSessionBookmarkResult;
type GeneratedDeleteSessionBookmarkResult = DeleteSessionBookmarkResult;
type GeneratedBranchSessionParams = BranchSessionParams;
type GeneratedBranchSessionResult = BranchSessionResult;
type GeneratedPushSubscriptionKeys = components['schemas']['PushKeys'];
type GeneratedPushSubscriptionPayload = components['schemas']['PushSubscription'];
type GeneratedVapidKeyResult =
  operations['get_push_vapid_key']['responses'][200]['content']['application/json'];

const _listSessionsParamsWrapperMatchesGenerated:
  Assert<Extends<ListSessionsParams, GeneratedListSessionsParams>> &
  Assert<Extends<GeneratedListSessionsParams, ListSessionsParams>> = true;
const _listSessionsResultWrapperMatchesGenerated:
  Assert<Extends<ListSessionsResult, GeneratedListSessionsResult>> &
  Assert<Extends<GeneratedListSessionsResult, ListSessionsResult>> = true;
const _recoverStreamResultWrapperMatchesGenerated:
  Assert<Extends<RecoverStreamResult, GeneratedRecoverStreamResult>> &
  Assert<Extends<GeneratedRecoverStreamResult, RecoverStreamResult>> = true;
const _sendMessageParamsWrapperMatchesGenerated:
  Assert<Extends<SendMessageParams, GeneratedSendMessageParams>> &
  Assert<Extends<GeneratedSendMessageParams, SendMessageParams>> = true;
const _sendMessageResultWrapperMatchesGenerated:
  Assert<Extends<SendMessageResult, GeneratedSendMessageResult>> &
  Assert<Extends<GeneratedSendMessageResult, SendMessageResult>> = true;
const _agentChatRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedAgentChatRequest> extends false ? true : false> = true;
const _agentChatResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedAgentChatResponse> extends false ? true : false> = true;
const _agentChatAcceptedResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedAgentChatAcceptedResponse> extends false ? true : false> = true;
const _agentChatStreamRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedAgentChatStreamRequest> extends false ? true : false> = true;
const _convergenceScoresResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedConvergenceScoresResult> extends false ? true : false> = true;
const _backupWrapperMatchesGenerated:
  Assert<Extends<Backup, GeneratedCreateBackupResult>> &
  Assert<Extends<GeneratedCreateBackupResult, Backup>> = true;
const _createBackupResultWrapperMatchesGenerated:
  Assert<Extends<CreateBackupResult, GeneratedCreateBackupResult>> &
  Assert<Extends<GeneratedCreateBackupResult, CreateBackupResult>> = true;
const _listBackupsResultWrapperMatchesGenerated:
  Assert<Extends<ListBackupsResult, GeneratedListBackupsResult>> &
  Assert<Extends<GeneratedListBackupsResult, ListBackupsResult>> = true;
const _verifyRestoreParamsWrapperMatchesGenerated:
  Assert<Extends<VerifyRestoreParams, GeneratedRestoreBackupRequest>> &
  Assert<Extends<GeneratedRestoreBackupRequest, VerifyRestoreParams>> = true;
const _verifyRestoreResultWrapperMatchesGenerated:
  Assert<Extends<VerifyRestoreResult, GeneratedRestoreBackupResponse>> &
  Assert<Extends<GeneratedRestoreBackupResponse, VerifyRestoreResult>> = true;
const _exportBackupDataJsonResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedExportBackupDataJsonResponse> extends false ? true : false> = true;
const _exportBackupDataNdjsonResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedExportBackupDataNdjsonResponse> extends false ? true : false> = true;
const _tracesResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedTracesResult> extends false ? true : false> = true;
const _listProfilesResultWrapperMatchesGenerated:
  Assert<Extends<ListProfilesResult, GeneratedListProfilesResult>> &
  Assert<Extends<GeneratedListProfilesResult, ListProfilesResult>> = true;
const _createProfileParamsWrapperMatchesGenerated:
  Assert<Extends<CreateProfileParams, GeneratedCreateProfileParams>> &
  Assert<Extends<GeneratedCreateProfileParams, CreateProfileParams>> = true;
const _createProfileResultWrapperMatchesGenerated:
  Assert<Extends<CreateProfileResult, GeneratedCreateProfileResult>> &
  Assert<Extends<GeneratedCreateProfileResult, CreateProfileResult>> = true;
const _updateProfileParamsWrapperMatchesGenerated:
  Assert<Extends<UpdateProfileParams, GeneratedUpdateProfileParams>> &
  Assert<Extends<GeneratedUpdateProfileParams, UpdateProfileParams>> = true;
const _profileWrapperMatchesGenerated:
  Assert<Extends<Profile, GeneratedUpdateProfileResult>> &
  Assert<Extends<GeneratedUpdateProfileResult, Profile>> = true;
const _deleteProfileResultWrapperMatchesGenerated:
  Assert<Extends<DeleteProfileResult, GeneratedDeleteProfileResult>> &
  Assert<Extends<GeneratedDeleteProfileResult, DeleteProfileResult>> = true;
const _assignProfileRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedAssignProfileRequest> extends false ? true : false> = true;
const _assignProfileResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedAssignProfileResponse> extends false ? true : false> = true;
// PC-control currently uses explicit SDK contract types because the generated
// schema mirror for this surface is not yet authoritative in this workspace.
const _listChannelsResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedListChannelsResponse> extends false ? true : false> = true;
const _createChannelRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedCreateChannelRequest> extends false ? true : false> = true;
const _createChannelResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedCreateChannelResponse> extends false ? true : false> = true;
const _reconnectChannelResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedReconnectChannelResponse> extends false ? true : false> = true;
const _deleteChannelResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedDeleteChannelResponse> extends false ? true : false> = true;
const _listProviderKeysResultWrapperMatchesGenerated:
  Assert<Extends<ListProviderKeysResult, GeneratedListProviderKeysResult>> &
  Assert<Extends<GeneratedListProviderKeysResult, ListProviderKeysResult>> = true;
const _setProviderKeyParamsWrapperMatchesGenerated:
  Assert<Extends<SetProviderKeyParams, GeneratedSetProviderKeyParams>> &
  Assert<Extends<GeneratedSetProviderKeyParams, SetProviderKeyParams>> = true;
const _setProviderKeyResultWrapperMatchesGenerated:
  Assert<Extends<SetProviderKeyResult, GeneratedSetProviderKeyResult>> &
  Assert<Extends<GeneratedSetProviderKeyResult, SetProviderKeyResult>> = true;
const _deleteProviderKeyResultWrapperMatchesGenerated:
  Assert<Extends<DeleteProviderKeyResult, GeneratedDeleteProviderKeyResult>> &
  Assert<Extends<GeneratedDeleteProviderKeyResult, DeleteProviderKeyResult>> = true;
const _injectChannelMessageRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedInjectChannelMessageRequest> extends false ? true : false> = true;
const _injectChannelMessageResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedInjectChannelMessageResponse> extends false ? true : false> = true;
const _connectOAuthProviderRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedConnectOAuthProviderRequest> extends false ? true : false> = true;
const _connectOAuthProviderResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedConnectOAuthProviderResponse> extends false ? true : false> = true;
const _oauthCallbackResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedOAuthCallbackResponse> extends false ? true : false> = true;
const _disconnectOAuthConnectionResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedDisconnectOAuthConnectionResponse> extends false ? true : false> = true;
const _executeOAuthApiCallRequestSchemaIsTyped:
  Assert<IsUnknown<GeneratedExecuteOAuthApiCallRequest> extends false ? true : false> = true;
const _executeOAuthApiCallResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedExecuteOAuthApiCallResponse> extends false ? true : false> = true;
const _executeOAuthApiCallAcceptedResponseSchemaIsTyped:
  Assert<IsUnknown<GeneratedExecuteOAuthApiCallAcceptedResponse> extends false ? true : false> = true;
const _studioRunParamsWrapperMatchesGenerated:
  Assert<Extends<StudioRunParams, GeneratedStudioRunParams>> &
  Assert<Extends<GeneratedStudioRunParams, StudioRunParams>> = true;
const _studioRunResultWrapperMatchesGenerated:
  Assert<Extends<StudioRunResult, GeneratedStudioRunResult>> &
  Assert<Extends<GeneratedStudioRunResult, StudioRunResult>> = true;
const _listGoalsParamsWrapperMatchesGenerated:
  Assert<Extends<ListGoalsParams, GeneratedListGoalsParams>> &
  Assert<Extends<GeneratedListGoalsParams, ListGoalsParams>> = true;
const _listGoalsResultWrapperMatchesGenerated:
  Assert<Extends<ListGoalsResult, GeneratedListGoalsResult>> &
  Assert<Extends<GeneratedListGoalsResult, ListGoalsResult>> = true;
const _proposalWrapperMatchesGenerated:
  Assert<Extends<Proposal, GeneratedProposal>> &
  Assert<Extends<GeneratedProposal, Proposal>> = true;
const _goalProposalTransitionWrapperMatchesGenerated:
  Assert<Extends<GoalProposalTransition, GeneratedGoalProposalTransition>> &
  Assert<Extends<GeneratedGoalProposalTransition, GoalProposalTransition>> = true;
const _proposalDetailWrapperMatchesGenerated:
  Assert<Extends<ProposalDetail, GeneratedGoalDetailResult>> &
  Assert<Extends<GeneratedGoalDetailResult, ProposalDetail>> = true;
const _goalDecisionResultWrapperMatchesGenerated:
  Assert<Extends<GoalDecisionResult, GeneratedGoalDecisionResult>> &
  Assert<Extends<GeneratedGoalDecisionResult, GoalDecisionResult>> = true;
const _getCrdtStateParamsWrapperMatchesGenerated:
  Assert<Extends<GetCrdtStateParams, GeneratedGetCrdtStateParams>> &
  Assert<Extends<GeneratedGetCrdtStateParams, GetCrdtStateParams>> = true;
const _crdtDeltaWrapperMatchesGenerated:
  Assert<Extends<CrdtDelta, components['schemas']['CrdtDelta']>> &
  Assert<Extends<components['schemas']['CrdtDelta'], CrdtDelta>> = true;
const _crdtStateResultWrapperMatchesGenerated:
  Assert<Extends<CrdtStateResult, GeneratedGetCrdtStateResult>> &
  Assert<Extends<GeneratedGetCrdtStateResult, CrdtStateResult>> = true;
const _trustNodeWrapperMatchesGenerated:
  Assert<Extends<TrustNode, components['schemas']['TrustNode']>> &
  Assert<Extends<components['schemas']['TrustNode'], TrustNode>> = true;
const _trustEdgeWrapperMatchesGenerated:
  Assert<Extends<TrustEdge, components['schemas']['TrustEdge']>> &
  Assert<Extends<components['schemas']['TrustEdge'], TrustEdge>> = true;
const _trustGraphResultWrapperMatchesGenerated:
  Assert<Extends<TrustGraphResult, GeneratedTrustGraphResult>> &
  Assert<Extends<GeneratedTrustGraphResult, TrustGraphResult>> = true;
const _consensusRoundWrapperMatchesGenerated:
  Assert<Extends<ConsensusRound, components['schemas']['ConsensusRound']>> &
  Assert<Extends<components['schemas']['ConsensusRound'], ConsensusRound>> = true;
const _consensusResultWrapperMatchesGenerated:
  Assert<Extends<ConsensusResult, GeneratedConsensusResult>> &
  Assert<Extends<GeneratedConsensusResult, ConsensusResult>> = true;
const _delegationWrapperMatchesGenerated:
  Assert<Extends<Delegation, components['schemas']['Delegation']>> &
  Assert<Extends<components['schemas']['Delegation'], Delegation>> = true;
const _sybilMetricsWrapperMatchesGenerated:
  Assert<Extends<SybilMetrics, components['schemas']['SybilMetrics']>> &
  Assert<Extends<components['schemas']['SybilMetrics'], SybilMetrics>> = true;
const _delegationsResultWrapperMatchesGenerated:
  Assert<Extends<DelegationsResult, GeneratedDelegationsResult>> &
  Assert<Extends<GeneratedDelegationsResult, DelegationsResult>> = true;
const _verifyChainParamsWrapperMatchesGenerated:
  Assert<Extends<VerifyChainParams, GeneratedVerifyChainParams>> &
  Assert<Extends<GeneratedVerifyChainParams, VerifyChainParams>> = true;
const _verifyChainResultWrapperMatchesGenerated:
  Assert<Extends<VerifyChainResult, GeneratedVerifyChainResult>> &
  Assert<Extends<GeneratedVerifyChainResult, VerifyChainResult>> = true;
const _integrityBreakWrapperIsTyped:
  Assert<IsUnknown<components['schemas']['IntegrityBreak']> extends false ? true : false> = true;
const _integrityChainsWrapperMatchesGenerated:
  Assert<Extends<IntegrityChains, components['schemas']['IntegrityChains']>> &
  Assert<Extends<components['schemas']['IntegrityChains'], IntegrityChains>> = true;
const _itpEventsIntegrityWrapperMatchesGenerated:
  Assert<Extends<ItpEventsIntegrity, components['schemas']['ItpEventsIntegrity']>> &
  Assert<Extends<components['schemas']['ItpEventsIntegrity'], ItpEventsIntegrity>> = true;
const _memoryEventsIntegrityWrapperMatchesGenerated:
  Assert<Extends<MemoryEventsIntegrity, components['schemas']['MemoryEventsIntegrity']>> &
  Assert<Extends<components['schemas']['MemoryEventsIntegrity'], MemoryEventsIntegrity>> = true;
const _listItpEventsParamsWrapperMatchesGenerated:
  Assert<Extends<ListItpEventsParams, GeneratedListItpEventsParams>> &
  Assert<Extends<GeneratedListItpEventsParams, ListItpEventsParams>> = true;
const _itpEventWrapperMatchesGenerated:
  Assert<Extends<ItpEvent, components['schemas']['ItpEvent']>> &
  Assert<Extends<components['schemas']['ItpEvent'], ItpEvent>> = true;
const _listItpEventsResultWrapperMatchesGenerated:
  Assert<Extends<ListItpEventsResult, GeneratedListItpEventsResult>> &
  Assert<Extends<GeneratedListItpEventsResult, ListItpEventsResult>> = true;
const _listMemoriesParamsWrapperMatchesGenerated:
  Assert<Extends<ListMemoriesParams, GeneratedListMemoriesParams>> &
  Assert<Extends<GeneratedListMemoriesParams, ListMemoriesParams>> = true;
const _memoryEntryWrapperMatchesGenerated:
  Assert<Extends<MemoryEntry, GeneratedGetMemoryResult>> &
  Assert<Extends<GeneratedGetMemoryResult, MemoryEntry>> = true;
const _listMemoriesResultWrapperMatchesGenerated:
  Assert<Extends<ListMemoriesResult, GeneratedListMemoriesResult>> &
  Assert<Extends<GeneratedListMemoriesResult, ListMemoriesResult>> = true;
const _memoryGraphNodeWrapperMatchesGenerated:
  Assert<Extends<MemoryGraphNode, components['schemas']['MemoryGraphNode']>> &
  Assert<Extends<components['schemas']['MemoryGraphNode'], MemoryGraphNode>> = true;
const _memoryGraphEdgeWrapperMatchesGenerated:
  Assert<Extends<MemoryGraphEdge, GeneratedMemoryGraphEdge>> &
  Assert<Extends<GeneratedMemoryGraphEdge, MemoryGraphEdge>> = true;
const _memoryGraphResultWrapperMatchesGenerated:
  Assert<Extends<MemoryGraphResult, GeneratedMemoryGraphResult>> &
  Assert<Extends<GeneratedMemoryGraphResult, MemoryGraphResult>> = true;
const _searchMemoriesParamsWrapperMatchesGenerated:
  Assert<Extends<SearchMemoriesParams, GeneratedSearchMemoriesParams>> &
  Assert<Extends<GeneratedSearchMemoriesParams, SearchMemoriesParams>> = true;
const _memorySearchResultEntryWrapperMatchesGenerated:
  Assert<Extends<MemorySearchResultEntry, components['schemas']['MemorySearchResultEntry']>> &
  Assert<Extends<components['schemas']['MemorySearchResultEntry'], MemorySearchResultEntry>> = true;
const _searchMemoriesResultWrapperMatchesGenerated:
  Assert<Extends<SearchMemoriesResult, GeneratedSearchMemoriesResult>> &
  Assert<Extends<GeneratedSearchMemoriesResult, SearchMemoriesResult>> = true;
const _listRuntimeSessionsParamsWrapperMatchesGenerated:
  Assert<Extends<ListRuntimeSessionsParams, GeneratedListRuntimeSessionsParams>> &
  Assert<Extends<GeneratedListRuntimeSessionsParams, ListRuntimeSessionsParams>> = true;
const _runtimeSessionWrapperMatchesGenerated:
  Assert<Extends<RuntimeSession, RuntimeSession>> &
  Assert<Extends<RuntimeSession, RuntimeSession>> = true;
const _listRuntimeSessionsResultWrapperMatchesGenerated:
  Assert<Extends<ListRuntimeSessionsResult, GeneratedListRuntimeSessionsResult>> &
  Assert<Extends<GeneratedListRuntimeSessionsResult, ListRuntimeSessionsResult>> = true;
const _runtimeSessionDetailResultWrapperMatchesGenerated:
  Assert<Extends<RuntimeSessionDetailResult, GeneratedRuntimeSessionDetailResult>> &
  Assert<Extends<GeneratedRuntimeSessionDetailResult, RuntimeSessionDetailResult>> = true;
const _sessionEventsParamsWrapperMatchesGenerated:
  Assert<Extends<SessionEventsParams, GeneratedSessionEventsParams>> &
  Assert<Extends<GeneratedSessionEventsParams, SessionEventsParams>> = true;
const _sessionEventsResultWrapperMatchesGenerated:
  Assert<Extends<SessionEventsResult, GeneratedSessionEventsResult>> &
  Assert<Extends<GeneratedSessionEventsResult, SessionEventsResult>> = true;
const _sessionBookmarkWrapperMatchesGenerated:
  Assert<Extends<SessionBookmark, SessionBookmark>> &
  Assert<Extends<SessionBookmark, SessionBookmark>> = true;
const _sessionBookmarksResultWrapperMatchesGenerated:
  Assert<Extends<SessionBookmarksResult, GeneratedSessionBookmarksResult>> &
  Assert<Extends<GeneratedSessionBookmarksResult, SessionBookmarksResult>> = true;
const _createSessionBookmarkParamsWrapperMatchesGenerated:
  Assert<Extends<CreateSessionBookmarkParams, GeneratedCreateSessionBookmarkParams>> &
  Assert<Extends<GeneratedCreateSessionBookmarkParams, CreateSessionBookmarkParams>> = true;
const _createSessionBookmarkResultWrapperMatchesGenerated:
  Assert<Extends<CreateSessionBookmarkResult, GeneratedCreateSessionBookmarkResult>> &
  Assert<Extends<GeneratedCreateSessionBookmarkResult, CreateSessionBookmarkResult>> = true;
const _deleteSessionBookmarkResultWrapperMatchesGenerated:
  Assert<Extends<DeleteSessionBookmarkResult, GeneratedDeleteSessionBookmarkResult>> &
  Assert<Extends<GeneratedDeleteSessionBookmarkResult, DeleteSessionBookmarkResult>> = true;
const _branchSessionParamsWrapperMatchesGenerated:
  Assert<Extends<BranchSessionParams, GeneratedBranchSessionParams>> &
  Assert<Extends<GeneratedBranchSessionParams, BranchSessionParams>> = true;
const _branchSessionResultWrapperMatchesGenerated:
  Assert<Extends<BranchSessionResult, GeneratedBranchSessionResult>> &
  Assert<Extends<GeneratedBranchSessionResult, BranchSessionResult>> = true;
const _pushSubscriptionKeysWrapperMatchesGenerated:
  Assert<Extends<PushSubscriptionKeys, GeneratedPushSubscriptionKeys>> &
  Assert<Extends<GeneratedPushSubscriptionKeys, PushSubscriptionKeys>> = true;
const _pushSubscriptionPayloadWrapperMatchesGenerated:
  Assert<Extends<PushSubscriptionPayload, GeneratedPushSubscriptionPayload>> &
  Assert<Extends<GeneratedPushSubscriptionPayload, PushSubscriptionPayload>> = true;
const _vapidKeyResultWrapperMatchesGenerated:
  Assert<Extends<VapidKeyResult, GeneratedVapidKeyResult>> &
  Assert<Extends<GeneratedVapidKeyResult, VapidKeyResult>> = true;

// ── Helpers ──

type MockFetchResponse = Omit<Partial<Response>, 'body' | 'ok' | 'status' | 'headers' | 'text' | 'json'> & {
  ok: boolean;
  status: number;
  bodyText?: string;
  body?: ReadableStream<Uint8Array> | null;
  headers?: Headers;
  json?: () => Promise<unknown>;
};

function mockFetch(response: MockFetchResponse | MockFetchResponse[]) {
  const responses = Array.isArray(response) ? [...response] : [response];
  let index = 0;
  return vi.fn().mockImplementation(async () => {
    const next = responses[Math.min(index, responses.length - 1)];
    index += 1;
    if (!next) {
      throw new Error('mockFetch called with no configured responses');
    }
    const body = next.json ? next.json : () => Promise.resolve(undefined);
    const bodyText = next.bodyText ?? '';
    return {
      ok: next.ok,
      status: next.status,
      json: typeof body === 'function' ? body : () => Promise.resolve(body),
      text: () => Promise.resolve(bodyText),
      body: next.body ?? null,
      headers: next.headers ?? new Headers(),
    } as Response;
  });
}

function jsonResponse(data: unknown, status = 200): MockFetchResponse {
  const bodyText = JSON.stringify(data);
  return {
    ok: true,
    status,
    bodyText,
    json: () => Promise.resolve(data),
    headers: new Headers({
      'content-type': 'application/json',
      'content-length': String(bodyText.length),
    }),
  };
}

function errorResponse(status: number, body?: unknown): MockFetchResponse {
  const bodyText = body === undefined ? '' : JSON.stringify(body);
  return {
    ok: false,
    status,
    bodyText,
    json: () => (body !== undefined ? Promise.resolve(body) : Promise.reject(new Error('no body'))),
    headers: new Headers({
      'content-type': 'application/json',
      'content-length': String(bodyText.length),
    }),
  };
}

function sseResponse(chunks: string[], status = 200): MockFetchResponse {
  const encoder = new TextEncoder();
  return {
    ok: status >= 200 && status < 300,
    status,
    bodyText: chunks.join(''),
    body: new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) {
          controller.enqueue(encoder.encode(chunk));
        }
        controller.close();
      },
    }) as ReadableStream<Uint8Array> | null,
    headers: new Headers({
      'content-type': 'text/event-stream',
    }),
  };
}

// ── Tests ──

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('GhostClient', () => {
  it('uses default baseUrl', () => {
    const client = new GhostClient();
    expect(client).toBeDefined();
  });

  it('accepts custom baseUrl', () => {
    const client = new GhostClient({ baseUrl: 'http://example.com:8080' });
    expect(client).toBeDefined();
  });

  it('has all API namespaces', () => {
    const client = new GhostClient();
    expect(client.agents).toBeDefined();
    expect(client.sessions).toBeDefined();
    expect(client.chat).toBeDefined();
    expect(client.convergence).toBeDefined();
    expect(client.goals).toBeDefined();
    expect(client.skills).toBeDefined();
    expect(client.safety).toBeDefined();
    expect(client.health).toBeDefined();
    expect('approvals' in client).toBe(false);
  });

  it('creates WebSocket connections', () => {
    const client = new GhostClient();
    // ws() returns a GhostWebSocket instance (doesn't actually connect until connect() is called)
    const ws = client.ws();
    expect(ws).toBeDefined();
  });
});

describe('AgentsAPI', () => {
  let fetch: ReturnType<typeof vi.fn>;
  let client: GhostClient;

  beforeEach(() => {
    fetch = mockFetch(jsonResponse([]));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });
  });

  it('lists agents', async () => {
    const agents = [{ id: 'a1', name: 'Test Agent' }];
    fetch = mockFetch(jsonResponse(agents));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.list();
    expect(result).toEqual(agents);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('creates an agent', async () => {
    const newAgent = { id: 'a2', name: 'New Agent' };
    fetch = mockFetch(jsonResponse(newAgent));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.create({ name: 'New Agent' });
    expect(result).toEqual(newAgent);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'New Agent' }),
      }),
    );
    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Request-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    );
    expect(headers['X-Ghost-Client-Name']).toBe('sdk');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Ghost-Operation-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(headers['Idempotency-Key']).toBe(headers['X-Ghost-Operation-ID']);
  });

  it('gets one agent detail', async () => {
    const agent = {
      id: 'a1',
      name: 'Agent Alpha',
      status: 'ready',
      lifecycle_state: 'ready',
      safety_state: 'normal',
      effective_state: 'ready',
      spending_cap: 10,
      action_policy: {
        can_pause: true,
        can_quarantine: true,
        can_resume: false,
        can_delete: true,
        resume_kind: null,
        requires_forensic_review: false,
        requires_second_confirmation: false,
        monitoring_duration_hours: null,
      },
    };
    fetch = mockFetch(jsonResponse(agent));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.get('a1');
    expect(result).toEqual(agent);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents/a1',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('gets the agent overview read model', async () => {
    const overview = {
      agent: {
        id: 'a1',
        name: 'Agent Alpha',
        status: 'paused',
        lifecycle_state: 'ready',
        safety_state: 'paused',
        effective_state: 'paused',
        spending_cap: 10,
      },
      convergence: null,
      cost: null,
      recent_sessions: [],
      recent_audit_entries: [],
      crdt_summary: null,
      integrity_summary: null,
      panel_health: {
        convergence: { state: 'empty' },
        cost: { state: 'ready' },
        recent_sessions: { state: 'empty' },
        recent_audit_entries: { state: 'empty' },
        crdt_summary: { state: 'empty' },
        integrity_summary: { state: 'empty' },
      },
    };
    fetch = mockFetch(jsonResponse(overview));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.getOverview('a1', { sessions_limit: 5, audit_limit: 10 });
    expect(result).toEqual(overview);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents/a1/overview?sessions_limit=5&audit_limit=10',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('lists tracked agent costs', async () => {
    const costs = [
      {
        agent_id: 'a1',
        agent_name: 'Agent Alpha',
        daily_total: 1.25,
        compaction_cost: 0.15,
        spending_cap: 10,
        cap_remaining: 8.75,
        cap_utilization_pct: 12.5,
      },
    ];
    fetch = mockFetch(jsonResponse(costs));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.costs.list();
    expect(result).toEqual(costs);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/costs',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('deletes an agent', async () => {
    fetch = mockFetch(jsonResponse({ deleted: true }, 200));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.delete('a1');
    expect(result).toEqual({ deleted: true });
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents/a1',
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});

describe('Operation envelope', () => {
  it('does not attach operation headers to GET requests by default', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await client.agents.list();
    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('sdk');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toBeUndefined();
    expect(headers['X-Ghost-Operation-ID']).toBeUndefined();
    expect(headers['Idempotency-Key']).toBeUndefined();
  });

  it('preserves caller-supplied operation identity on goal approval', async () => {
    const approved = { status: 'approved' as const, id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(approved));
    const client = new GhostClient({
      fetch,
      baseUrl: 'http://test:1234',
      clientName: 'dashboard',
      clientVersion: '0.1.0',
    });

    await client.goals.approve(
      'goal-1',
      {
        expectedState: 'pending_review',
        expectedLineageId: 'ln-123',
        expectedSubjectKey: 'goal:agent-1:primary',
        expectedReviewedRevision: 'rev-42',
      },
      {
        requestId: 'request-123',
        operationId: '018f0f23-8c65-7abc-9def-1234567890ab',
        idempotencyKey: 'idem-123',
      },
    );

    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('dashboard');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toBe('request-123');
    expect(headers['X-Ghost-Operation-ID']).toBe('018f0f23-8c65-7abc-9def-1234567890ab');
    expect(headers['Idempotency-Key']).toBe('idem-123');
  });

  it('assesses compatibility against the gateway contract', () => {
    const supported = assessGhostClientCompatibility(
      {
        gatewayVersion: '0.1.0',
        compatibilityContractVersion: 1,
        policyAWritesRequireExplicitClientIdentity: true,
        requiredMutationHeaders: ['x-ghost-client-name', 'x-ghost-client-version'],
        supportedClients: [
          {
            clientName: 'dashboard',
            minimumVersion: '0.1.0',
            maximumVersionExclusive: '0.2.0',
            enforcement: 'policy_a_writes',
          },
        ],
      },
      { name: 'dashboard', version: '0.1.0' },
    );
    expect(supported.supported).toBe(true);

    const unsupported = assessGhostClientCompatibility(
      {
        gatewayVersion: '0.1.0',
        compatibilityContractVersion: 1,
        policyAWritesRequireExplicitClientIdentity: true,
        requiredMutationHeaders: ['x-ghost-client-name', 'x-ghost-client-version'],
        supportedClients: [
          {
            clientName: 'dashboard',
            minimumVersion: '0.1.0',
            maximumVersionExclusive: '0.2.0',
            enforcement: 'policy_a_writes',
          },
        ],
      },
      { name: 'dashboard', version: '0.0.99' },
    );
    expect(unsupported.supported).toBe(false);
    expect(unsupported.reason).toBe('unsupported_version');
  });
});

describe('SessionsAPI', () => {
  it('creates a session', async () => {
    const session = {
      id: 's1',
      agent_id: 'agent-1',
      title: 'Session',
      model: 'gpt-4o-mini',
      system_prompt: '',
      temperature: 0.2,
      max_tokens: 512,
      created_at: '2026-03-07T00:00:00Z',
      updated_at: '2026-03-07T00:00:00Z',
    };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.create({ title: 'Session' });
    expect(result).toEqual(session);
  });

  it('lists sessions', async () => {
    const sessions = {
      sessions: [
        {
          id: 's1',
          agent_id: 'agent-1',
          title: 'Session 1',
          model: 'gpt-4o-mini',
          system_prompt: '',
          temperature: 0.2,
          max_tokens: 512,
          created_at: '2026-03-07T00:00:00Z',
          updated_at: '2026-03-07T00:00:00Z',
        },
        {
          id: 's2',
          agent_id: 'agent-2',
          title: 'Session 2',
          model: 'gpt-4o-mini',
          system_prompt: '',
          temperature: 0.4,
          max_tokens: 1024,
          created_at: '2026-03-08T00:00:00Z',
          updated_at: '2026-03-08T00:00:00Z',
        },
      ],
      next_cursor: 'cursor-2',
      has_more: true,
    };
    const fetch = mockFetch(jsonResponse(sessions));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.list();
    expect(result).toEqual(sessions);
  });

  it('lists sessions with a cursor', async () => {
    const sessions = {
      sessions: [
        {
          id: 's3',
          agent_id: 'agent-3',
          title: 'Session 3',
          model: 'gpt-4o-mini',
          system_prompt: '',
          temperature: 0.5,
          max_tokens: 256,
          created_at: '2026-03-09T00:00:00Z',
          updated_at: '2026-03-09T00:00:00Z',
        },
      ],
      next_cursor: null,
      has_more: false,
    };
    const fetch = mockFetch(jsonResponse(sessions));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await client.sessions.list({ limit: 25, cursor: 'cursor-2' });

    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions?limit=25&cursor=cursor-2',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('gets a session with messages', async () => {
    const session = {
      id: 's1',
      agent_id: 'agent-1',
      title: 'Session',
      model: 'gpt-4o-mini',
      system_prompt: '',
      temperature: 0.2,
      max_tokens: 512,
      created_at: '2026-03-07T00:00:00Z',
      updated_at: '2026-03-07T00:00:00Z',
      messages: [],
    };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.get('s1');
    expect(result).toEqual(session);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('recovers a stream without after_seq by default', async () => {
    const recovered = { events: [] };
    const fetch = mockFetch(jsonResponse(recovered));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.recoverStream('s1', { message_id: 'm1' });
    expect(result).toEqual(recovered);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1/stream/recover?message_id=m1',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('recovers a stream with after_seq when provided', async () => {
    const recovered = {
      events: [
        {
          seq: 4,
          event_type: 'text_delta',
          payload: { content: 'world' },
          created_at: '2026-03-08T00:00:00Z',
          reconstructed: true,
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(recovered));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.recoverStream('s1', {
      message_id: 'm1',
      after_seq: 3,
    });
    expect(result).toEqual(recovered);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1/stream/recover?message_id=m1&after_seq=3',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('StudioAPI', () => {
  it('runs the prompt playground route with the typed contract', async () => {
    const result = {
      content: 'Hello from the playground',
      model: 'gpt-4o-mini',
      token_count: 42,
      finish_reason: 'stop',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });
    const params: StudioRunParams = {
      system_prompt: 'You are helpful.',
      messages: [{ role: 'user', content: 'Hello' }],
      model: 'gpt-4o-mini',
      temperature: 0.2,
      max_tokens: 128,
    };

    const response = await client.studio.run(params);
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/run',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(params),
      }),
    );
  });
});

describe('ChatAPI', () => {
  it('sends a studio message and returns the completed typed response', async () => {
    const result: SendMessageResult = {
      user_message: {
        id: 'msg-user-1',
        role: 'user',
        content: 'Hello',
        token_count: 3,
        safety_status: 'clean',
        created_at: '2026-03-08T00:00:00Z',
      },
      assistant_message: {
        id: 'msg-assistant-1',
        role: 'assistant',
        content: 'Hi there',
        token_count: 5,
        safety_status: 'clean',
        created_at: '2026-03-08T00:00:01Z',
      },
      safety_status: 'clean',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });
    const params: SendMessageParams = {
      content: 'Hello',
      model: 'gpt-4o-mini',
      temperature: 0.2,
      max_tokens: 128,
    };

    const response = await client.chat.send('s1', params);
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1/messages',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(params),
      }),
    );
  });

  it('returns accepted recovery metadata when the blocking route defers completion', async () => {
    const result: SendMessageResult = {
      status: 'accepted',
      session_id: 's1',
      execution_id: 'exec-1',
      user_message_id: 'msg-user-1',
      assistant_message_id: 'msg-assistant-1',
      recovery_required: true,
    };
    const fetch = mockFetch(jsonResponse(result, 202));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.chat.send('s1', { content: 'Hello again' });
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1/messages',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ content: 'Hello again' }),
      }),
    );
  });

  it('sends compatibility headers on streaming requests', async () => {
    const fetch = mockFetch([
      sseResponse([
        'event: stream_end\n',
        'data: {"type":"stream_end","message_id":"msg-1","token_count":1,"safety_status":"clean"}\n\n',
      ]),
    ][0]);
    const client = new GhostClient({
      fetch,
      baseUrl: 'http://test:1234',
      clientName: 'dashboard',
      clientVersion: '0.1.0',
    });

    const events: Array<{ type: string }> = [];
    for await (const event of client.chat.stream('s1', { content: 'hello' })) {
      events.push({ type: event.type });
    }

    expect(events).toEqual([{ type: 'stream_end' }]);

    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('dashboard');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    );
    expect(headers['X-Ghost-Operation-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(headers['Idempotency-Key']).toBe(headers['X-Ghost-Operation-ID']);
  });

  it('sends compatibility headers on callback streaming requests', async () => {
    const fetch = mockFetch([
      sseResponse([
        'event: text_delta\n',
        'id: 1\n',
        'data: {"content":"hello"}\n\n',
      ]),
    ][0]);
    const client = new GhostClient({
      fetch,
      baseUrl: 'http://test:1234',
      clientName: 'dashboard',
      clientVersion: '0.1.0',
    });
    const onEvent = vi.fn();

    await client.chat.streamWithCallback('s1', { content: 'hello' }, onEvent);

    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('dashboard');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    );
    expect(headers['X-Ghost-Operation-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(headers['Idempotency-Key']).toBe(headers['X-Ghost-Operation-ID']);
    expect(onEvent).toHaveBeenCalledWith('text_delta', { content: 'hello' }, '1');
  });
});

describe('BackupsAPI', () => {
  it('lists backups with the typed contract', async () => {
    const result: ListBackupsResult = {
      backups: [
        {
          backup_id: 'backup-1',
          created_at: '2026-03-08T00:00:00Z',
          size_bytes: 2048,
          entry_count: 12,
          blake3_checksum: 'deadbeefcafebabe',
          status: 'complete',
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.backups.list();
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/backups',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('creates a backup with the typed contract', async () => {
    const result: CreateBackupResult = {
      backup_id: 'backup-2',
      created_at: '2026-03-08T00:05:00Z',
      size_bytes: 4096,
      entry_count: 24,
      blake3_checksum: '0123456789abcdef',
      status: 'complete',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.backups.create();
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/backup',
      expect.objectContaining({
        method: 'POST',
      }),
    );
  });

  it('verifies a backup archive for restore with the typed contract', async () => {
    const params: VerifyRestoreParams = {
      backup_path: '/tmp/ghost-backup-1.ghost-backup',
    };
    const result: VerifyRestoreResult = {
      valid: true,
      entry_count: 24,
      version: '1.0.0',
      message: 'Archive verified.',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.backups.verifyRestore(params);
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/restore',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(params),
      }),
    );
  });
});

describe('ProviderKeysAPI', () => {
  it('lists provider keys with the typed contract', async () => {
    const result: ListProviderKeysResult = {
      providers: [
        {
          provider_name: 'openai',
          model: 'gpt-4o-mini',
          env_name: 'OPENAI_API_KEY',
          is_set: true,
          preview: 'sk-a...1234',
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.providerKeys.list();
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/provider-keys',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('sets a provider key with the typed contract', async () => {
    const params: SetProviderKeyParams = {
      env_name: 'OPENAI_API_KEY',
      value: 'sk-live-secret',
    };
    const result: SetProviderKeyResult = {
      env_name: 'OPENAI_API_KEY',
      preview: 'sk-l...cret',
      message: 'API key saved successfully',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.providerKeys.set(params);
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/provider-keys',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify(params),
      }),
    );
  });

  it('deletes a provider key with the typed contract', async () => {
    const result: DeleteProviderKeyResult = {
      env_name: 'OPENAI_API_KEY',
      message: 'API key removed',
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.providerKeys.delete('OPENAI_API_KEY');
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/admin/provider-keys/OPENAI_API_KEY',
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});

describe('HealthAPI', () => {
  it('checks health', async () => {
    const health = {
      status: 'alive',
      state: 'Healthy',
      platform_killed: false,
      convergence_monitor: {
        connected: true,
      },
      convergence_protection: {
        execution_mode: 'block',
        stale_after_secs: 90,
        agents: {
          healthy: 2,
          missing: 0,
          stale: 0,
          corrupted: 0,
        },
      },
      distributed_kill: {
        enabled: false,
        status: 'gated',
      },
      speculative_context: {
        enabled: true,
        mode: 'shadow',
        shadow_mode: true,
        outstanding_entries: 0,
        pending_tokens: 0,
      },
    };
    const fetch = mockFetch(jsonResponse(health));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.health.check();
    expect(result).toEqual(health);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('checks readiness', async () => {
    const ready = { status: 'ready', state: 'Healthy' };
    const fetch = mockFetch(jsonResponse(ready));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.health.ready();
    expect(result).toEqual(ready);
  });
});

describe('ObservabilityAPI', () => {
  it('loads the ADE observability snapshot with the typed contract', async () => {
    const result: AdeObservabilitySnapshot = {
      sampled_at: '2026-03-11T00:00:00Z',
      stale: false,
      status: 'healthy',
      gateway: {
        liveness: 'alive',
        readiness: 'ready',
        state: 'Healthy',
        uptime_seconds: 7200,
        platform_killed: false,
      },
      monitor: {
        enabled: true,
        connected: true,
        status: 'running',
        uptime_seconds: 3600,
        agent_count: 4,
        event_count: 420,
        last_computation: '2026-03-11T00:00:00Z',
        last_error: null,
      },
      agents: {
        active_count: 3,
        registered_count: 4,
      },
      websocket: {
        active_connections: 7,
        per_ip_limit: 8,
        status: 'healthy',
      },
      database: {
        path: '/tmp/ghost.db',
        size_bytes: 5242880,
        wal_mode: true,
        status: 'healthy',
        last_error: null,
      },
      backup_scheduler: {
        enabled: true,
        status: 'healthy',
        retention_days: 30,
        schedule: 'daily at 03:00 UTC',
        last_success_at: '2026-03-10T03:00:00Z',
        last_failure_at: null,
        last_error: null,
      },
      config_watcher: {
        enabled: true,
        status: 'healthy',
        watched_path: '/tmp/ghost.yml',
        mode: 'native',
        last_reload_at: '2026-03-10T20:00:00Z',
        last_error: null,
      },
      autonomy: {
        deployment_mode: 'single_node',
        runtime_state: 'running',
        scheduler_running: true,
        worker_count: 2,
        due_jobs: 0,
        leased_jobs: 0,
        running_jobs: 1,
        waiting_jobs: 0,
        paused_jobs: 0,
        quarantined_jobs: 0,
        manual_review_jobs: 0,
        oldest_overdue_at: undefined,
        last_successful_dispatch_at: '2026-03-11T00:00:00Z',
        owner_identity: 'gateway:test',
        saturation: {
          saturated: false,
          reserved_slots: 1,
          global_concurrency: 2,
          per_agent_concurrency: 1,
          blocked_due_jobs: 0,
          reason: undefined,
        },
      },
      convergence_protection: {
        execution_mode: 'block',
        stale_after_secs: 90,
        agents: {
          healthy: 3,
          missing: 1,
          stale: 0,
          corrupted: 0,
        },
      },
      distributed_kill: {
        enabled: false,
        status: 'gated',
        authoritative: false,
        reason: 'distributed kill is feature-gated for this remediation milestone',
      },
      speculative_context: {
        enabled: true,
        mode: 'shadow',
        shadow_mode: true,
        outstanding_entries: 0,
        pending_tokens: 0,
      },
    };

    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.observability.ade();
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/observability/ade',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('SafetyAPI', () => {
  it('gets safety status', async () => {
    const status = { platform_killed: false };
    const fetch = mockFetch(jsonResponse(status));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.safety.status();
    expect(result).toEqual(status);
  });

  it('activates kill-all', async () => {
    const result = { status: 'kill_all_activated', reason: 'test', initiated_by: 'operator' };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const res = await client.safety.killAll('test', 'operator');
    expect(res).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/safety/kill-all',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reason: 'test', initiated_by: 'operator' }),
      }),
    );
  });

  it('pauses an agent', async () => {
    const result = { status: 'paused', agent_id: 'a1', reason: 'testing' };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const res = await client.safety.pause('a1', 'testing');
    expect(res).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/safety/pause/a1',
      expect.objectContaining({ method: 'POST' }),
    );
  });
});

describe('ConvergenceAPI', () => {
  it('gets convergence scores', async () => {
    const scores: ConvergenceScoresResult = {
      scores: [
        {
          agent_id: 'a1',
          agent_name: 'Agent One',
          score: 0.85,
          level: 3,
          profile: 'standard',
          signal_scores: {
            coherence: 0.9,
            consistency: 0.8,
          },
          computed_at: '2026-03-08T00:00:00Z',
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(scores));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.convergence.scores();
    expect(result).toEqual(scores);
  });

  it('gets convergence history for one agent', async () => {
    const history: ConvergenceHistoryResult = {
      agent_id: 'a1',
      entries: [
        {
          session_id: 's1',
          score: 0.62,
          level: 2,
          profile: 'standard',
          signal_scores: {
            coherence: 0.7,
            consistency: 0.54,
          },
          computed_at: '2026-03-08T00:00:00Z',
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(history));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.convergence.history('a1', {
      since: '2026-03-01T00:00:00Z',
      limit: 24,
    });
    expect(result).toEqual(history);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/convergence/history/a1?since=2026-03-01T00%3A00%3A00Z&limit=24',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('ProfilesAPI', () => {
  const weights = [0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125];
  const thresholds = [0.3, 0.5, 0.7, 0.85];

  it('lists profiles with the typed contract', async () => {
    const result: ListProfilesResult = {
      profiles: [
        {
          name: 'standard',
          description: 'Balanced scoring',
          is_preset: true,
          weights,
          thresholds,
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.profiles.list();
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/profiles',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('creates a profile with the typed 201 contract', async () => {
    const params: CreateProfileParams = {
      name: 'ops',
      description: 'Operational profile',
      weights,
      thresholds,
    };
    const result: CreateProfileResult = {
      name: 'ops',
      description: 'Operational profile',
      is_preset: false,
      weights,
      thresholds,
    };
    const fetch = mockFetch(jsonResponse(result, 201));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.profiles.create(params);
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/profiles',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(params),
      }),
    );
  });

  it('updates and deletes a profile with the typed contracts', async () => {
    const updateParams: UpdateProfileParams = {
      description: 'Updated profile',
      thresholds: [0.35, 0.55, 0.75, 0.9],
    };
    const updated: Profile = {
      name: 'ops',
      description: 'Updated profile',
      is_preset: false,
      weights,
      thresholds: [0.35, 0.55, 0.75, 0.9],
    };
    const deleted: DeleteProfileResult = {
      deleted: 'ops',
    };

    const fetch = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(updated))
      .mockResolvedValueOnce(jsonResponse(deleted));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const updatedResponse = await client.profiles.update('ops', updateParams);
    expect(updatedResponse).toEqual(updated);
    expect(fetch).toHaveBeenNthCalledWith(
      1,
      'http://test:1234/api/profiles/ops',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify(updateParams),
      }),
    );

    const deletedResponse = await client.profiles.delete('ops');
    expect(deletedResponse).toEqual(deleted);
    expect(fetch).toHaveBeenNthCalledWith(
      2,
      'http://test:1234/api/profiles/ops',
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});

describe('PcControlAPI', () => {
  const safeZone: SafeZone = {
    x: 10,
    y: 20,
    width: 800,
    height: 600,
    label: 'Primary Safe Zone',
  };

  const status: PcControlStatus = {
    enabled: true,
    action_budget: {
      max_per_minute: 60,
      max_per_hour: 3600,
      used_this_minute: 2,
      used_this_hour: 12,
    },
    allowed_apps: ['Finder'],
    safe_zone: safeZone,
    safe_zones: [safeZone],
    blocked_hotkeys: ['cmd+q'],
    circuit_breaker_state: 'closed',
    display: {
      width: 1920,
      height: 1080,
    },
    persisted: {
      enabled: true,
      allowed_apps: ['Finder'],
      safe_zone: safeZone,
      blocked_hotkeys: ['cmd+q'],
      budgets: {
        mouse_click: 200,
        keyboard_type: 500,
        keyboard_hotkey: 50,
        mouse_drag: 20,
        total: 1000,
      },
      circuit_breaker: {
        max_actions_per_second: 1,
        failure_threshold: 3,
        cooldown_seconds: 30,
      },
    },
    runtime: {
      revision: 4,
      enabled: true,
      activation_state: 'active',
      effective_allowed_apps: ['Finder'],
      effective_safe_zone: safeZone,
      effective_blocked_hotkeys: ['cmd+q'],
      circuit_breaker_state: 'closed',
      last_applied_at: '2026-03-08T00:00:00Z',
      last_apply_source: 'api',
    },
    telemetry: {
      throughput: {
        max_per_minute: 60,
        max_per_hour: 3600,
        used_this_minute: 2,
        used_this_hour: 12,
      },
      policy_budgets: {
        mouse_click: 200,
        keyboard_type: 500,
        keyboard_hotkey: 50,
        mouse_drag: 20,
        total: 1000,
      },
      usage: {
        executed_this_minute: 2,
        executed_this_hour: 12,
        blocked_this_minute: 0,
        blocked_this_hour: 1,
      },
    },
  };

  it('gets PC control status with the typed contract', async () => {
    const fetch = mockFetch(jsonResponse(status));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.pcControl.getStatus();
    expect(response).toEqual(status);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/pc-control/status',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('lists actions and updates safe zones with the typed contracts', async () => {
    const actions: PcControlActionLogResult = {
      actions: [
        {
          id: 'act-1',
          action_type: 'click',
          target: 'Finder',
          timestamp: '2026-03-08T00:00:00Z',
          result: 'ok',
          input_json: '{}',
          result_json: '{}',
          target_app: 'Finder',
          coordinates: '10,20',
          blocked: false,
          block_reason: null,
          agent_id: 'agent-1',
          session_id: 'session-1',
        },
      ],
    };
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(actions))
      .mockResolvedValueOnce(jsonResponse(status));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const actionsResponse = await client.pcControl.listActions(25);
    expect(actionsResponse).toEqual(actions);
    expect(fetch).toHaveBeenNthCalledWith(
      1,
      'http://test:1234/api/pc-control/actions?limit=25',
      expect.objectContaining({ method: 'GET' }),
    );

    const statusResponse = await client.pcControl.setSafeZones([safeZone]);
    expect(statusResponse).toEqual(status);
    expect(fetch).toHaveBeenNthCalledWith(
      2,
      'http://test:1234/api/pc-control/safe-zones',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ safe_zone: safeZone }),
      }),
    );
  });
});

describe('TracesAPI', () => {
  it('gets session traces with the typed contract', async () => {
    const result: SessionTrace = {
      session_id: 'session-1',
      total_spans: 1,
      traces: [
        {
          trace_id: 'trace-1',
          spans: [
            {
              span_id: 'span-1',
              trace_id: 'trace-1',
              parent_span_id: null,
              operation_name: 'chat.turn',
              start_time: '2026-03-08T00:00:00Z',
              end_time: '2026-03-08T00:00:01Z',
              attributes: {
                model: 'gpt-4o-mini',
              },
              status: 'ok',
            },
          ],
        },
      ],
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.traces.get('session-1');
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/traces/session-1',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('RuntimeSessionsAPI', () => {
  it('lists runtime sessions with the typed cursor contract', async () => {
    const result: ListRuntimeSessionsResult = {
      data: [
        {
          session_id: 'runtime-1',
          agent_ids: ['agent-1', 'agent-2'],
          started_at: '2026-03-08T00:00:00Z',
          last_event_at: '2026-03-08T00:01:00Z',
          event_count: 2,
          chain_valid: true,
          cumulative_cost: 0.000126,
          branched_from: null,
        },
      ],
      has_more: false,
      next_cursor: null,
      total_count: 1,
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.runtimeSessions.list({ limit: 50 });
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/sessions?limit=50',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('loads session detail, events, bookmarks, and branches with typed contracts', async () => {
    const detail: RuntimeSessionDetailResult = {
      session: {
        session_id: 'runtime-1',
        agent_ids: ['agent-1'],
        started_at: '2026-03-08T00:00:00Z',
        last_event_at: '2026-03-08T00:01:00Z',
        event_count: 1,
        chain_valid: true,
        cumulative_cost: 0.000126,
        branched_from: null,
      },
      bookmark_count: 1,
    };
    const events: SessionEventsResult = {
      session_id: 'runtime-1',
      events: [
        {
          id: 'evt-1',
          event_type: 'assistant_message',
          sender: 'agent-1',
          timestamp: '2026-03-08T00:00:00Z',
          sequence_number: 1,
          content_hash: null,
          content_length: 12,
          privacy_level: 'internal',
          latency_ms: 120,
          token_count: 42,
          event_hash: 'hash-1',
          previous_hash: '',
          attributes: { text: 'hello' },
        },
      ],
      total: 1,
      limit: 100,
      has_more: false,
      next_after_sequence_number: null,
      chain_valid: true,
      cumulative_cost: 0.000126,
    };
    const bookmarks: SessionBookmarksResult = {
      bookmarks: [
        {
          id: 'bm-1',
          session_id: 'runtime-1',
          sequence_number: 1,
          label: 'Start',
          created_at: '2026-03-08T00:00:00Z',
        },
      ],
    };
    const created: CreateSessionBookmarkResult = {
      bookmark: {
        id: 'bm-2',
        session_id: 'runtime-1',
        sequence_number: 2,
        label: 'Checkpoint',
        created_at: '2026-03-08T00:00:05Z',
      },
    };
    const deleted: DeleteSessionBookmarkResult = { status: 'deleted' };
    const branched: BranchSessionResult = {
      session: {
        session_id: 'runtime-2',
        agent_ids: ['agent-1'],
        started_at: '2026-03-08T00:00:00Z',
        last_event_at: '2026-03-08T00:00:00Z',
        event_count: 1,
        chain_valid: true,
        cumulative_cost: 0.000126,
        branched_from: 'runtime-1',
      },
    };
    const fetch = mockFetch([
      jsonResponse(detail),
      jsonResponse(events),
      jsonResponse(bookmarks),
      jsonResponse(created, 201),
      jsonResponse(deleted),
      jsonResponse(branched, 201),
    ] as MockFetchResponse[]);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    expect(await client.runtimeSessions.get('runtime-1')).toEqual(detail);
    expect(await client.runtimeSessions.events('runtime-1')).toEqual(events);
    expect(await client.runtimeSessions.listBookmarks('runtime-1')).toEqual(bookmarks);
    expect(
      await client.runtimeSessions.createBookmark('runtime-1', {
        id: 'bm-2',
        sequence_number: 2,
        label: 'Checkpoint',
      }),
    ).toEqual(created);
    expect(await client.runtimeSessions.deleteBookmark('runtime-1', 'bm-2')).toEqual(deleted);
    expect(
      await client.runtimeSessions.branch('runtime-1', {
        from_sequence_number: 1,
      }),
    ).toEqual(branched);
  });
});

describe('MemoryAPI', () => {
  it('lists, gets, graphs, and searches memories with typed contracts', async () => {
    const listResult: ListMemoriesResult = {
      memories: [
        {
          id: 1,
          memory_id: 'mem-1',
          snapshot: '{"summary":"Test memory"}',
          created_at: '2026-03-08T00:00:00Z',
        },
      ],
      page: 1,
      page_size: 50,
      total: 1,
    };
    const memory: MemoryEntry = listResult.memories[0];
    const graph: MemoryGraphResult = {
      nodes: [
        {
          id: 'mem-1',
          label: 'Test memory',
          type: 'event',
          importance: 0.8,
          decayFactor: 0.2,
        },
      ],
      edges: [],
    };
    const search: SearchMemoriesResult = {
      results: [
        {
          id: 1,
          memory_id: 'mem-1',
          snapshot: { summary: 'Test memory' },
          created_at: '2026-03-08T00:00:00Z',
          score: 0.91,
        },
      ],
      count: 1,
      query: 'test',
      search_mode: 'fts5',
      filters: {
        agent_id: 'agent-1',
        memory_type: 'episodic',
        importance: 'high',
        confidence_min: 0.5,
        confidence_max: 1,
      },
    };
    const fetch = mockFetch([
      jsonResponse(listResult),
      jsonResponse(memory),
      jsonResponse(graph),
      jsonResponse(search),
    ] as MockFetchResponse[]);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    expect(await client.memory.list({ agent_id: 'agent-1' })).toEqual(listResult);
    expect(await client.memory.get('mem-1')).toEqual(memory);
    expect(await client.memory.graph()).toEqual(graph);
    expect(await client.memory.search({ q: 'test', agent_id: 'agent-1' })).toEqual(search);
  });
});

describe('StateAPI', () => {
  it('loads CRDT state with the typed contract', async () => {
    const result: CrdtStateResult = {
      agent_id: 'agent-1',
      deltas: [
        {
          event_id: 1,
          memory_id: 'mem-1',
          event_type: 'append',
          delta: '{"op":"add"}',
          actor_id: 'agent-1',
          recorded_at: '2026-03-08T00:00:00Z',
          event_hash: 'hash-1',
          previous_hash: '',
        },
      ],
      total: 1,
      limit: 100,
      offset: 0,
      chain_valid: true,
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.state.getCrdtState('agent-1', { memory_id: 'mem-1' });
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/state/crdt/agent-1?memory_id=mem-1',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('IntegrityAPI', () => {
  it('verifies chains with the typed contract', async () => {
    const result: VerifyChainResult = {
      agent_id: 'agent-1',
      chain_type: 'both',
      chains: {
        itp_events: {
          sessions_checked: 1,
          total_events: 2,
          verified_events: 2,
          is_valid: true,
          breaks: [],
        },
        memory_events: {
          memory_chains_checked: 1,
          total_events: 2,
          verified_events: 2,
          is_valid: true,
          breaks: [],
        },
      },
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.integrity.verifyChain('agent-1', { chain: 'both' });
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/integrity/chain/agent-1?chain=both',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('ItpAPI', () => {
  it('lists ITP events with the typed contract', async () => {
    const result: ListItpEventsResult = {
      events: [
        {
          id: 'evt-1',
          event_type: 'input',
          session_id: 'runtime-1',
          timestamp: '2026-03-08T00:00:00Z',
          sequence_number: 42,
          sender: 'agent-1',
          source: 'browser_extension',
          platform: 'chatgpt',
          route: 'agent_chat',
          privacy_level: 'standard',
          content_length: 128,
          session_path: '/sessions/runtime-1',
          replay_path: '/sessions/runtime-1/replay',
        },
      ],
      limit: 25,
      offset: 10,
      total_persisted: 11,
      total_filtered: 3,
      returned: 1,
      monitor_connected: true,
      live_updates_supported: true,
    };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const response = await client.itp.list({
      limit: 25,
      offset: 10,
      session_id: 'runtime-1',
      event_type: 'input',
    });
    expect(response).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/itp/events?limit=25&offset=10&session_id=runtime-1&event_type=input',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('MeshAPI', () => {
  it('loads trust, consensus, and delegation views with typed contracts', async () => {
    const trust: TrustGraphResult = {
      nodes: [
        {
          id: 'agent-1',
          name: 'Agent 1',
          activity: 0.9,
          convergence_level: 4,
        },
      ],
      edges: [
        {
          source: 'agent-1',
          target: 'agent-2',
          trust_score: 0.8,
        },
      ],
    };
    const consensus: ConsensusResult = {
      rounds: [
        {
          proposal_id: 'goal-1',
          status: 'pending',
          approvals: 1,
          rejections: 0,
          threshold: 2,
        },
      ],
    };
    const delegations: DelegationsResult = {
      delegations: [
        {
          delegator_id: 'agent-1',
          delegate_id: 'agent-2',
          scope: 'triage',
          state: 'active',
          created_at: '2026-03-08T00:00:00Z',
        },
      ],
      sybil_metrics: {
        total_delegations: 1,
        max_chain_depth: 1,
        unique_delegators: 1,
      },
    };
    const fetch = mockFetch([
      jsonResponse(trust),
      jsonResponse(consensus),
      jsonResponse(delegations),
    ] as MockFetchResponse[]);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    expect(await client.mesh.trustGraph()).toEqual(trust);
    expect(await client.mesh.consensus()).toEqual(consensus);
    expect(await client.mesh.delegations()).toEqual(delegations);
  });
});

describe('PushAPI', () => {
  it('loads the VAPID key and posts subscription payloads with typed contracts', async () => {
    const vapid: VapidKeyResult = { key: 'public-vapid-key' };
    const fetch = mockFetch([
      jsonResponse(vapid),
      { ok: true, status: 204 },
      { ok: true, status: 204 },
    ] as MockFetchResponse[]);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });
    const subscription: PushSubscriptionPayload = {
      endpoint: 'https://example.test/push',
      keys: { p256dh: 'p256', auth: 'auth' },
    };

    expect(await client.push.getVapidKey()).toEqual(vapid);
    await client.push.subscribe(subscription);
    await client.push.unsubscribe(subscription);

    expect(fetch).toHaveBeenNthCalledWith(
      2,
      'http://test:1234/api/push/subscribe',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(subscription),
      }),
    );
    expect(fetch).toHaveBeenNthCalledWith(
      3,
      'http://test:1234/api/push/unsubscribe',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(subscription),
      }),
    );
  });
});

describe('GoalsAPI', () => {
  it('lists goals/proposals', async () => {
    const goals = { proposals: [], page: 1, page_size: 50, total: 0 };
    const fetch = mockFetch(jsonResponse(goals));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.list();
    expect(result).toEqual(goals);
  });

  it('gets a proposal detail', async () => {
    const proposal = {
      id: 'goal-1',
      agent_id: 'agent-1',
      session_id: 'session-1',
      proposer_type: 'agent',
      operation: 'delete_memory',
      target_type: 'memory',
      decision: null,
      dimension_scores: {},
      flags: [],
      created_at: '2026-03-07T00:00:00Z',
      resolved_at: null,
      content: { memory_id: 'm1' },
      cited_memory_ids: [],
      resolver: null,
    };
    const fetch = mockFetch(jsonResponse(proposal));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.get('goal-1');
    expect(result).toEqual(proposal);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('approves a proposal', async () => {
    const approved = { status: 'approved', id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(approved));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.approve('goal-1', {
      expectedState: 'pending_review',
      expectedLineageId: 'ln-123',
      expectedSubjectKey: 'goal:agent-1:primary',
      expectedReviewedRevision: 'rev-42',
    });
    expect(result).toEqual(approved);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/approve',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          expected_state: 'pending_review',
          expected_lineage_id: 'ln-123',
          expected_subject_key: 'goal:agent-1:primary',
          expected_reviewed_revision: 'rev-42',
          rationale: undefined,
        }),
      }),
    );
  });

  it('rejects a proposal', async () => {
    const rejected = { status: 'rejected', id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(rejected));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.reject('goal-1', {
      expectedState: 'pending_review',
      expectedLineageId: 'ln-123',
      expectedSubjectKey: 'goal:agent-1:primary',
      expectedReviewedRevision: 'rev-42',
      rationale: 'unsafe',
    });
    expect(result).toEqual(rejected);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/reject',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          expected_state: 'pending_review',
          expected_lineage_id: 'ln-123',
          expected_subject_key: 'goal:agent-1:primary',
          expected_reviewed_revision: 'rev-42',
          rationale: 'unsafe',
        }),
      }),
    );
  });
});

describe('SkillsAPI', () => {
  const catalogSkill = (overrides: Record<string, unknown> = {}) => ({
    id: 'test-skill',
    name: 'test-skill',
    version: '0.1.0',
    description: 'Compiled test skill',
    source: 'compiled',
    removable: true,
    installable: true,
    execution_mode: 'native',
    policy_capability: 'skill:test-skill',
    privileges: ['Read test data'],
    requested_capabilities: [],
    mutation_kind: 'read_only',
    state: 'installed',
    install_state: 'installed',
    verification_status: 'not_applicable',
    quarantine_state: 'clear',
    runtime_visible: true,
    capabilities: ['skill:test-skill'],
    ...overrides,
  });

  it('lists skills', async () => {
    const skills = { installed: [], available: [] };
    const fetch = mockFetch(jsonResponse(skills));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.list();
    expect(result).toEqual(skills);
  });

  it('installs a skill', async () => {
    const skill = catalogSkill();
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.install('test-skill');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/test-skill/install',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('uninstalls a skill with the same catalog shape', async () => {
    const skill = catalogSkill({
      state: 'available',
      install_state: 'disabled',
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.uninstall('test-skill');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/test-skill/uninstall',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('quarantines an external skill by catalog identifier', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'quarantined',
      install_state: 'not_installed',
      verification_status: 'verified',
      quarantine_state: 'quarantined',
      quarantine_reason: 'manual review',
      quarantine_revision: 2,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.quarantine('digest-1', {
      reason: 'manual review',
    });
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/quarantine',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reason: 'manual review' }),
      }),
    );
  });

  it('resolves quarantine with an expected revision guard', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'verified',
      install_state: 'not_installed',
      verification_status: 'verified',
      quarantine_state: 'clear',
      quarantine_revision: 3,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.resolveQuarantine('digest-1', {
      expected_quarantine_revision: 2,
    });
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/quarantine/resolve',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ expected_quarantine_revision: 2 }),
      }),
    );
  });

  it('reverifies an external skill by catalog identifier', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'verification_failed',
      install_state: 'disabled',
      verification_status: 'revoked_signer',
      quarantine_state: 'quarantined',
      quarantine_reason: 'revoked during incident response',
      quarantine_revision: 4,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.reverify('digest-1');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/reverify',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('executes a skill with the canonical request envelope', async () => {
    const response = {
      skill: 'note_take',
      result: { status: 'created', note_id: 'note-1' },
    };
    const fetch = mockFetch(jsonResponse(response));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.execute('note_take', {
      agent_id: 'agent-1',
      session_id: 'session-1',
      input: { action: 'create', title: 'Test', content: 'Body' },
    });

    expect(result).toEqual(response);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/note_take/execute',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          agent_id: 'agent-1',
          session_id: 'session-1',
          input: { action: 'create', title: 'Test', content: 'Body' },
        }),
      }),
    );
  });
});

describe('Error handling', () => {
  it('throws GhostAPIError on non-ok response', async () => {
    const fetch = mockFetch(errorResponse(404, { error: 'Agent not found' }));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const request = client.agents.list();

    await expect(request).rejects.toThrow(GhostAPIError);
    await expect(request).rejects.toMatchObject({
      status: 404,
      message: 'Agent not found',
    });
  });

  it('throws GhostAPIError with structured error body', async () => {
    const fetch = mockFetch(
      errorResponse(422, {
        error: { message: 'Validation failed', code: 'VALIDATION_ERROR', details: { field: 'name' } },
      }),
    );
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toMatchObject({
      status: 422,
      message: 'Validation failed',
      code: 'VALIDATION_ERROR',
      details: { field: 'name' },
    });
  });

  it('throws GhostNetworkError on fetch failure', async () => {
    const fetch = vi.fn().mockRejectedValue(new TypeError('Network error'));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toThrow(GhostNetworkError);
  });

  it('throws GhostTimeoutError on timeout', async () => {
    const err = new DOMException('Signal timed out', 'TimeoutError');
    const fetch = vi.fn().mockRejectedValue(err);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', timeout: 5000 });

    await expect(client.agents.list()).rejects.toThrow(GhostTimeoutError);
  });

  it('handles 204 No Content', async () => {
    const fetch = mockFetch({ ok: true, status: 204, json: () => Promise.resolve(undefined) });
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.delete('a1');
    expect(result).toBeUndefined();
  });

  it('retries safe requests on transient network failure with bounded backoff', async () => {
    vi.useFakeTimers();
    const fetch = vi.fn()
      .mockRejectedValueOnce(new TypeError('temporary network failure'))
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: () => Promise.resolve([{ id: 'a1' }]),
        text: () => Promise.resolve('[{"id":"a1"}]'),
        headers: new Headers({
          'content-type': 'application/json',
          'content-length': '13',
        }),
      } as Response);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const request = client.agents.list();
    await vi.runAllTimersAsync();

    await expect(request).resolves.toEqual([{ id: 'a1' }]);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('does not retry semantic 4xx responses', async () => {
    const fetch = mockFetch(errorResponse(401, { error: 'unauthorized' }));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toMatchObject({
      status: 401,
      message: 'unauthorized',
    });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it('does not retry mutating requests by default', async () => {
    const fetch = vi.fn().mockRejectedValue(new TypeError('temporary network failure'));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.create({ name: 'New Agent' })).rejects.toThrow(GhostNetworkError);
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});

describe('Security hardening', () => {
  it('requires secure crypto when generating operation identifiers', async () => {
    vi.stubGlobal('crypto', undefined);

    const client = new GhostClient({
      fetch: mockFetch(jsonResponse({ id: 'a1' })),
      baseUrl: 'http://test:1234',
    });

    await expect(client.agents.create({ name: 'New Agent' })).rejects.toThrow(/Web Crypto/);
  });

  it('uses timeout signals for blob exports', async () => {
    const fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      blob: () => Promise.resolve(new Blob(['ok'])),
    } as Response);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', timeout: 5000 });

    await client.audit.exportBlob({ format: 'json' });

    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/audit/export?format=json',
      expect.objectContaining({
        method: 'GET',
        signal: expect.any(AbortSignal),
      }),
    );
  });

  it('includes active filters in blob exports', async () => {
    const fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      blob: () => Promise.resolve(new Blob(['ok'])),
    } as Response);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', timeout: 5000 });

    await client.audit.exportBlob({
      format: 'csv',
      event_type: 'kill_all,pause_agent',
      severity: 'critical,warn',
      search: 'sandbox',
      operation_id: 'op-123',
    });

    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/audit/export?format=csv&event_type=kill_all%2Cpause_agent&severity=critical%2Cwarn&search=sandbox&operation_id=op-123',
      expect.objectContaining({
        method: 'GET',
        signal: expect.any(AbortSignal),
      }),
    );
  });
});

describe('Authentication', () => {
  it('gets the current session', async () => {
    const session = {
      authenticated: true,
      subject: 'admin',
      role: 'admin',
      capabilities: ['safety_review'],
      authz_v: 1,
      mode: 'jwt' as const,
    };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', token: 'my-token' });

    const result = await client.auth.session();
    expect(result).toEqual(session);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/auth/session',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('sends Authorization header when token is set', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', token: 'my-token' });

    await client.agents.list();
    expect(fetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer my-token',
        }),
      }),
    );
  });

  it('does not send Authorization header when no token', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await client.agents.list();
    const callArgs = fetch.mock.calls[0][1];
    expect(callArgs.headers).not.toHaveProperty('Authorization');
  });

  it('handles 204 responses without reading JSON', async () => {
    const fetch = mockFetch({
      ok: true,
      status: 204,
      bodyText: '',
      headers: new Headers({
        'content-length': '0',
      }),
    });
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.auth.logout();
    expect(result).toBeUndefined();
  });
});
