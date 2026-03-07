interface Agent {
    id: string;
    name: string;
    status: 'Starting' | 'Running' | 'Stopping' | 'Stopped';
    spending_cap: number;
}
interface AgentDetail extends Agent {
    has_keypair?: boolean;
}
interface CreateAgentParams {
    name: string;
    spending_cap?: number;
    capabilities?: string[];
    generate_keypair?: boolean;
}
interface DeleteAgentResult {
    status: 'deleted';
    id: string;
    name: string;
}
declare class AgentsAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** List all registered agents. */
    list(): Promise<Agent[]>;
    /** Create a new agent with optional keypair generation. */
    create(params: CreateAgentParams): Promise<AgentDetail>;
    /** Delete an agent by ID or name. */
    delete(id: string): Promise<DeleteAgentResult>;
}

interface StudioSession {
    id: string;
    title: string;
    model: string;
    system_prompt: string;
    temperature: number;
    max_tokens: number;
    created_at: string;
    updated_at: string;
}
interface StudioMessage {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    token_count: number;
    safety_status: 'clean' | 'warning' | 'blocked';
    created_at: string;
}
interface StudioSessionWithMessages extends StudioSession {
    messages: StudioMessage[];
}
interface CreateSessionParams {
    title?: string;
    model?: string;
    system_prompt?: string;
    temperature?: number;
    max_tokens?: number;
}
interface ListSessionsParams {
    limit?: number;
    offset?: number;
    before?: string;
}
interface RecoverStreamEvent {
    seq: number;
    event_type: string;
    payload: Record<string, unknown>;
    created_at: string;
}
interface RecoverStreamResult {
    events: RecoverStreamEvent[];
}
declare class SessionsAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** Create a new studio chat session. */
    create(params?: CreateSessionParams): Promise<StudioSession>;
    /** List studio chat sessions. */
    list(params?: ListSessionsParams): Promise<{
        sessions: StudioSession[];
    }>;
    /** Get a session with all its messages. */
    get(id: string): Promise<StudioSessionWithMessages>;
    /** Delete a studio session. */
    delete(id: string): Promise<{
        deleted: boolean;
    }>;
    recoverStream(id: string, params: {
        message_id: string;
        after_seq?: number;
    }): Promise<RecoverStreamResult>;
}

interface SendMessageParams {
    content: string;
    model?: string;
    temperature?: number;
    max_tokens?: number;
}
interface SendMessageResult {
    user_message: StudioMessage;
    assistant_message: StudioMessage;
    safety_status: 'clean' | 'warning' | 'blocked';
}
/** SSE event types emitted during streaming. */
type StreamEvent = {
    type: 'stream_start';
    session_id: string;
    message_id: string;
} | {
    type: 'text_delta';
    content: string;
} | {
    type: 'tool_use';
    tool: string;
    tool_id: string;
    status: string;
} | {
    type: 'tool_result';
    tool: string;
    tool_id: string;
    status: string;
    preview?: string;
} | {
    type: 'heartbeat';
    phase: string;
} | {
    type: 'stream_end';
    message_id: string;
    token_count: number;
    safety_status: 'clean' | 'warning' | 'blocked';
} | {
    type: 'error';
    message: string;
};
type ChatStreamEventHandler = (eventType: string, data: Record<string, unknown>, eventId?: string) => void;
declare class ChatAPI {
    private request;
    private options;
    constructor(request: GhostRequestFn, options: GhostClientOptions);
    /** Send a message and wait for the complete response (blocking). */
    send(sessionId: string, params: SendMessageParams): Promise<SendMessageResult>;
    /** Send a message and receive streaming SSE events. */
    stream(sessionId: string, params: SendMessageParams): AsyncGenerator<StreamEvent>;
    streamWithCallback(sessionId: string, params: SendMessageParams, onEvent: ChatStreamEventHandler, signal?: AbortSignal): Promise<void>;
}

interface ConvergenceScore {
    agent_id: string;
    agent_name: string;
    score: number;
    level: number;
    profile: string;
    signal_scores: Record<string, number>;
    computed_at: string;
}
interface ConvergenceError {
    agent_id: string;
    agent_name: string;
    error: string;
}
interface ConvergenceScoresResult {
    scores: ConvergenceScore[];
    errors?: ConvergenceError[];
}
declare class ConvergenceAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** Get convergence scores for all agents. */
    scores(): Promise<ConvergenceScoresResult>;
}

interface Proposal {
    id: string;
    agent_id: string;
    session_id: string;
    proposer_type: 'agent' | 'human';
    operation: string;
    target_type: string;
    decision: 'approved' | 'rejected' | null;
    dimension_scores: Record<string, number>;
    flags: string[];
    created_at: string;
    resolved_at: string | null;
}
interface ProposalDetail extends Proposal {
    content: Record<string, unknown>;
    cited_memory_ids: string[];
    resolver: string | null;
    denial_reason?: string;
}
interface ListGoalsParams {
    status?: 'pending' | 'approved' | 'rejected';
    agent_id?: string;
    page?: number;
    page_size?: number;
}
interface ListGoalsResult {
    proposals: Proposal[];
    page: number;
    page_size: number;
    total: number;
}
declare class GoalsAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** List goal proposals with optional filtering. */
    list(params?: ListGoalsParams): Promise<ListGoalsResult>;
    /** Get a single proposal with full detail. */
    get(id: string): Promise<ProposalDetail>;
    /** Approve a pending proposal. */
    approve(id: string): Promise<{
        status: 'approved';
        id: string;
    }>;
    /** Reject a pending proposal. */
    reject(id: string): Promise<{
        status: 'rejected';
        id: string;
    }>;
}

interface Skill {
    id: string;
    name: string;
    version: string;
    description: string;
    capabilities: string[];
    source: 'bundled' | 'user' | 'workspace';
    state: string;
}
interface ListSkillsResult {
    installed: Skill[];
    available: Skill[];
}
declare class SkillsAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** List installed and available skills. */
    list(): Promise<ListSkillsResult>;
    /** Install a skill by name. */
    install(name: string): Promise<Skill>;
    /** Uninstall a skill by name. */
    uninstall(name: string): Promise<{
        uninstalled: string;
    }>;
}

interface SafetyStatus {
    platform_level?: string;
    platform_killed: boolean;
    state?: string;
    per_agent?: Record<string, {
        level: string;
        activated_at?: string;
        trigger?: string;
    }>;
    activated_at?: string;
    trigger?: string;
    distributed_gate?: {
        state: string;
        node_id: string;
        closed_at?: string;
        close_reason?: string;
        acked_nodes: string[] | number;
        chain_length: number;
    };
}
interface KillAllResult {
    status: 'kill_all_activated';
    reason: string;
    initiated_by: string;
}
interface PauseResult {
    status: 'paused';
    agent_id: string;
    reason: string;
}
interface ResumeResult {
    status: 'resumed';
    agent_id: string;
    heightened_monitoring: boolean;
    monitoring_duration_hours: number;
}
interface QuarantineResult {
    status: 'quarantined';
    agent_id: string;
    reason: string;
    resume_requires: string;
}
interface ResumeParams {
    level?: 'PAUSE' | 'QUARANTINE';
    forensic_reviewed?: boolean;
    second_confirmation?: boolean;
}
declare class SafetyAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** Get platform and per-agent safety status. */
    status(): Promise<SafetyStatus>;
    /** Activate platform-wide kill switch. */
    killAll(reason: string, initiatedBy: string): Promise<KillAllResult>;
    /** Pause a specific agent. */
    pause(agentId: string, reason: string): Promise<PauseResult>;
    /** Resume a paused or quarantined agent. */
    resume(agentId: string, params?: ResumeParams): Promise<ResumeResult>;
    /** Quarantine an agent (requires forensic review to resume). */
    quarantine(agentId: string, reason: string): Promise<QuarantineResult>;
}

interface HealthStatus {
    status: 'alive' | 'unavailable';
    state: 'Healthy' | 'Degraded' | 'Recovering' | 'Initializing' | 'ShuttingDown' | 'FatalError';
    platform_killed: boolean;
    convergence_monitor?: {
        connected: boolean;
    };
    distributed_gate?: {
        state: string;
        node_id: string;
        closed_at?: string;
        close_reason?: string;
        acked_nodes: number;
        chain_length: number;
    };
}
interface ReadyStatus {
    status: 'ready' | 'not_ready';
    state: string;
}
declare class HealthAPI {
    private request;
    constructor(request: GhostRequestFn);
    /** Check if the gateway is alive. */
    check(): Promise<HealthStatus>;
    /** Check if the gateway is ready to accept requests. */
    ready(): Promise<ReadyStatus>;
}

interface LoginParams {
    token: string;
}
interface AuthTokenResponse {
    access_token: string;
    token_type: string;
    expires_in: number;
}
interface LogoutResponse {
    message?: string;
    status?: string;
}
declare class AuthAPI {
    private request;
    constructor(request: GhostRequestFn);
    login(params: LoginParams): Promise<AuthTokenResponse>;
    refresh(): Promise<AuthTokenResponse>;
    logout(): Promise<LogoutResponse | undefined>;
}

interface AuditEntry {
    id: string;
    timestamp: string;
    event_type: string;
    severity: string;
    details: string;
    agent_id?: string;
    actor_id?: string;
}
interface AuditQueryParams {
    time_start?: string;
    time_end?: string;
    agent_id?: string;
    event_type?: string;
    severity?: string;
    tool_name?: string;
    search?: string;
    page?: number;
    page_size?: number;
}
interface AuditQueryResult {
    entries: AuditEntry[];
    page: number;
    page_size: number;
    total: number;
    filters_applied?: Record<string, unknown>;
}
interface AuditExportParams {
    format?: 'json' | 'csv' | 'jsonl';
    agent_id?: string;
    time_start?: string;
    time_end?: string;
}
declare class AuditAPI {
    private request;
    private options;
    constructor(request: GhostRequestFn, options: GhostClientOptions);
    query(params?: AuditQueryParams): Promise<AuditQueryResult>;
    export(params?: AuditExportParams): Promise<unknown>;
    exportBlob(params?: AuditExportParams): Promise<Blob>;
}

interface AgentCostInfo {
    agent_id: string;
    agent_name: string;
    daily_total: number;
    compaction_cost: number;
    spending_cap: number;
    cap_remaining: number;
    cap_utilization_pct: number;
}
declare class CostsAPI {
    private request;
    constructor(request: GhostRequestFn);
    list(): Promise<AgentCostInfo[]>;
}

interface MemoryEntry {
    id?: number;
    memory_id: string;
    snapshot: string;
    created_at: string;
}
interface ListMemoriesParams {
    agent_id?: string;
    page?: number;
    page_size?: number;
    include_archived?: boolean;
}
interface ListMemoriesResult {
    memories: MemoryEntry[];
    page: number;
    page_size: number;
    total: number;
}
interface SearchMemoriesParams {
    q?: string;
    agent_id?: string;
    memory_type?: string;
    importance?: string;
    confidence_min?: number;
    confidence_max?: number;
    limit?: number;
    include_archived?: boolean;
}
interface MemorySearchResultEntry {
    id: number;
    memory_id: string;
    snapshot: unknown;
    created_at: string;
    score: number;
}
interface SearchMemoriesResult {
    results: MemorySearchResultEntry[];
    count: number;
    query?: string;
    search_mode: 'fts5' | 'like';
    filters: {
        agent_id?: string;
        memory_type?: string;
        importance?: string;
        confidence_min?: number;
        confidence_max?: number;
    };
}
interface MemoryGraphNode {
    id: string;
    label: string;
    type: 'entity' | 'event' | 'concept';
    importance: number;
    decayFactor: number;
}
interface MemoryGraphEdge {
    source: string | MemoryGraphNode;
    target: string | MemoryGraphNode;
    relationship: string;
    strength: number;
}
interface MemoryGraphResult {
    nodes: MemoryGraphNode[];
    edges: MemoryGraphEdge[];
}
declare class MemoryAPI {
    private request;
    constructor(request: GhostRequestFn);
    list(params?: ListMemoriesParams): Promise<ListMemoriesResult>;
    get(id: string): Promise<MemoryEntry>;
    graph(): Promise<MemoryGraphResult>;
    search(params?: SearchMemoriesParams): Promise<SearchMemoriesResult>;
}

interface RuntimeSession {
    session_id: string;
    started_at: string;
    last_event_at: string;
    event_count: number;
    agents: string[] | string;
}
interface SessionEvent {
    id: string;
    event_type: string;
    sender?: string | null;
    timestamp: string;
    sequence_number: number;
    content_hash?: string | null;
    content_length?: number | null;
    privacy_level: string;
    latency_ms?: number | null;
    token_count?: number | null;
    event_hash: string;
    previous_hash: string;
    attributes: Record<string, unknown>;
}
interface SessionEventsParams {
    offset?: number;
    limit?: number;
}
interface SessionEventsResult {
    session_id: string;
    events: SessionEvent[];
    total: number;
    offset: number;
    limit: number;
    chain_valid: boolean;
    cumulative_cost: number;
}
interface SessionBookmark {
    id: string;
    eventIndex: number;
    label: string;
    createdAt: string;
}
interface SessionBookmarksResult {
    bookmarks: SessionBookmark[];
}
interface CreateSessionBookmarkParams {
    id?: string;
    eventIndex: number;
    label: string;
}
interface CreateSessionBookmarkResult {
    id: string;
    status: 'created';
}
interface DeleteSessionBookmarkResult {
    status: 'deleted';
}
interface BranchSessionParams {
    from_event_index: number;
}
interface BranchSessionResult {
    session_id: string;
    branched_from: string;
    events_copied: number;
}
interface ListRuntimeSessionsParams {
    page?: number;
    page_size?: number;
    cursor?: string;
    limit?: number;
}
interface ListRuntimeSessionsPageResult {
    sessions: RuntimeSession[];
    page: number;
    page_size: number;
    total: number;
}
interface ListRuntimeSessionsCursorResult {
    data: RuntimeSession[];
    next_cursor: string | null;
    has_more: boolean;
    total_count: number;
}
declare class RuntimeSessionsAPI {
    private request;
    constructor(request: GhostRequestFn);
    list(params?: ListRuntimeSessionsParams): Promise<ListRuntimeSessionsPageResult | ListRuntimeSessionsCursorResult>;
    events(sessionId: string, params?: SessionEventsParams): Promise<SessionEventsResult>;
    listBookmarks(sessionId: string): Promise<SessionBookmarksResult>;
    createBookmark(sessionId: string, params: CreateSessionBookmarkParams): Promise<CreateSessionBookmarkResult>;
    deleteBookmark(sessionId: string, bookmarkId: string): Promise<DeleteSessionBookmarkResult>;
    branch(sessionId: string, params: BranchSessionParams): Promise<BranchSessionResult>;
    heartbeat(sessionId: string): Promise<void>;
}

interface TraceSpanRecord {
    span_id: string;
    trace_id: string;
    parent_span_id: string | null;
    operation_name: string;
    start_time: string;
    end_time: string | null;
    attributes: Record<string, unknown>;
    status: string;
}
interface TraceGroup {
    trace_id: string;
    spans: TraceSpanRecord[];
}
interface SessionTrace {
    session_id: string;
    traces: TraceGroup[];
    total_spans: number;
}
declare class TracesAPI {
    private request;
    constructor(request: GhostRequestFn);
    get(sessionId: string): Promise<SessionTrace>;
}

/** All possible server-to-client WebSocket events. */
type WsEvent = {
    type: 'ScoreUpdate';
    agent_id: string;
    score: number;
    level: number;
    signals: number[];
} | {
    type: 'InterventionChange';
    agent_id: string;
    old_level: number;
    new_level: number;
} | {
    type: 'KillSwitchActivation';
    level: string;
    agent_id?: string;
    reason: string;
} | {
    type: 'ProposalDecision';
    proposal_id: string;
    decision: 'approved' | 'rejected';
    agent_id: string;
} | {
    type: 'AgentStateChange';
    agent_id: string;
    new_state: string;
} | {
    type: 'SessionEvent';
    session_id: string;
    event_id: string;
    event_type: string;
    sender?: string;
    sequence_number: number;
} | {
    type: 'ChatMessage';
    session_id: string;
    message_id: string;
    role: 'user' | 'assistant';
    content: string;
    safety_status: 'clean' | 'warning' | 'blocked';
} | {
    type: 'Ping';
} | {
    type: 'Resync';
    missed_events: number;
} | {
    type: string;
    [key: string]: unknown;
};
interface GhostWebSocketOptions {
    /** Topics to subscribe to on connect. */
    topics?: string[];
    /** Auto-reconnect on disconnect. Default: true. */
    autoReconnect?: boolean;
    /** Max reconnect delay in ms. Default: 30000. */
    maxReconnectDelay?: number;
    /** Max reconnect attempts before giving up. Default: 10. Set to 0 for unlimited. */
    maxReconnectAttempts?: number;
    /** Called when reconnection is abandoned after maxReconnectAttempts. */
    onReconnectFailed?: () => void;
}
type EventHandler = (event: WsEvent) => void;
declare class GhostWebSocket {
    private clientOptions;
    private wsOptions;
    private ws;
    private handlers;
    private globalHandlers;
    private subscribedTopics;
    private reconnectAttempt;
    private reconnectTimer;
    private closed;
    constructor(clientOptions: GhostClientOptions, wsOptions?: GhostWebSocketOptions);
    /** Open the WebSocket connection. */
    connect(): this;
    /** Close the WebSocket connection. */
    disconnect(): void;
    /** Subscribe to additional topics. */
    subscribe(topics: string[]): void;
    /** Unsubscribe from topics. */
    unsubscribe(topics: string[]): void;
    /** Listen for a specific event type. */
    on(eventType: string, handler: EventHandler): () => void;
    /** Listen for all events. */
    onAny(handler: EventHandler): () => void;
    private doConnect;
    private scheduleReconnect;
}

interface GhostClientOptions {
    /** Base URL of the Ghost gateway. Default: http://127.0.0.1:39780 */
    baseUrl?: string;
    /** Bearer token for authentication. */
    token?: string;
    /** Custom fetch implementation (for testing, SSR, or polyfilling). */
    fetch?: typeof globalThis.fetch;
    /** Default request timeout in milliseconds. */
    timeout?: number;
}
/** Internal request function signature shared with API modules. */
type GhostRequestFn = <T>(method: string, path: string, body?: unknown) => Promise<T>;
declare class GhostClient {
    readonly agents: AgentsAPI;
    readonly sessions: SessionsAPI;
    readonly chat: ChatAPI;
    readonly convergence: ConvergenceAPI;
    readonly goals: GoalsAPI;
    readonly skills: SkillsAPI;
    readonly safety: SafetyAPI;
    readonly health: HealthAPI;
    readonly auth: AuthAPI;
    readonly audit: AuditAPI;
    readonly costs: CostsAPI;
    readonly memory: MemoryAPI;
    readonly runtimeSessions: RuntimeSessionsAPI;
    readonly traces: TracesAPI;
    private readonly options;
    constructor(options?: GhostClientOptions);
    /** Create a WebSocket connection for real-time events. */
    ws(options?: GhostWebSocketOptions): GhostWebSocket;
    /** Internal request method used by all API modules. */
    private request;
}

/** Base error class for all Ghost SDK errors. */
declare class GhostError extends Error {
    readonly status?: number | undefined;
    readonly code?: string | undefined;
    readonly details?: Record<string, unknown> | undefined;
    constructor(message: string, status?: number | undefined, code?: string | undefined, details?: Record<string, unknown> | undefined);
}
/** Thrown when the Ghost API returns a 4xx/5xx response. */
declare class GhostAPIError extends GhostError {
    readonly status: number;
    constructor(message: string, status: number, code?: string, details?: Record<string, unknown>);
}
/** Thrown when a network request fails (no response received). */
declare class GhostNetworkError extends GhostError {
    readonly cause?: Error | undefined;
    constructor(message: string, cause?: Error | undefined);
}
/** Thrown when a request times out. */
declare class GhostTimeoutError extends GhostError {
    readonly timeoutMs: number;
    constructor(timeoutMs: number);
}

/**
 * This file was auto-generated by openapi-typescript.
 * Do not make direct changes to the file.
 */
interface paths {
    "/api/a2a/discover": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["discover_a2a_agents"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/a2a/tasks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_a2a_tasks"];
        put?: never;
        post: operations["send_a2a_task"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/a2a/tasks/{task_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_a2a_task"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/a2a/tasks/{task_id}/stream": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["stream_a2a_task"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/agents": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_agents"];
        put?: never;
        post: operations["create_agent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/agents/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["delete_agent"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/audit": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["query_audit"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/audit/aggregation": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["audit_aggregation"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/audit/export": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["audit_export"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["logout"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/auth/refresh": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["refresh"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/convergence/scores": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_convergence_scores"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/costs": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_costs"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/goals": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_goals"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/goals/{id}/approve": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["approve_goal"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/goals/{id}/reject": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["reject_goal"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/health": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["health"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/memory": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_memories"];
        put?: never;
        post: operations["write_memory"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/memory/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_memory"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/ready": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["ready"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/checks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_safety_checks"];
        put?: never;
        post: operations["register_safety_check"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/checks/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["unregister_safety_check"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/kill-all": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["kill_all"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/pause/{agent_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["pause_agent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/quarantine/{agent_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["quarantine_agent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/resume/{agent_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["resume_agent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/safety/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["safety_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/sessions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_sessions"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/skills": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_skills"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/skills/{id}/install": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["install_skill"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/skills/{id}/uninstall": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["uninstall_skill"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/webhooks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_webhooks"];
        put?: never;
        post: operations["create_webhook"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/webhooks/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put: operations["update_webhook"];
        post?: never;
        delete: operations["delete_webhook"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/webhooks/{id}/test": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["test_webhook"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
interface components {
    schemas: {
        A2ATaskSchema: {
            created_at: string;
            id: string;
            method: string;
            status: string;
            target_agent: string;
        };
        AgentCostSchema: {
            agent_id: string;
            agent_name: string;
            /** Format: double */
            cap_remaining: number;
            /** Format: double */
            cap_utilization_pct: number;
            /** Format: double */
            compaction_cost: number;
            /** Format: double */
            daily_total: number;
            /** Format: double */
            spending_cap: number;
        };
        AgentInfoSchema: {
            id: string;
            name: string;
            /** Format: double */
            spending_cap: number;
            status: string;
        };
        ConvergenceScoreSchema: {
            agent_id: string;
            agent_name: string;
            computed_at?: string | null;
            /** Format: int32 */
            level: number;
            profile: string;
            /** Format: double */
            score: number;
            signal_scores: unknown;
        };
        CreateAgentRequestSchema: {
            capabilities?: string[] | null;
            generate_keypair?: boolean | null;
            name: string;
            /** Format: double */
            spending_cap?: number | null;
        };
        ErrorBodySchema: {
            code: string;
            details?: unknown;
            message: string;
        };
        /** @description Standard error response envelope. */
        ErrorResponseSchema: {
            error: components["schemas"]["ErrorBodySchema"];
        };
        SessionSchema: {
            agents: string;
            /** Format: int64 */
            event_count: number;
            last_event_at: string;
            session_id: string;
            started_at: string;
        };
        SkillSchema: {
            capabilities: string[];
            description: string;
            id: string;
            skill_name: string;
            source: string;
            state: string;
            version: string;
        };
        WebhookSchema: {
            active: boolean;
            events: string[];
            id: string;
            name: string;
            url: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
interface operations {
    discover_a2a_agents: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Discovered A2A agents */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_a2a_tasks: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List of A2A tasks */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["A2ATaskSchema"][];
                };
            };
        };
    };
    send_a2a_task: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description A2A task sent */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["A2ATaskSchema"];
                };
            };
            /** @description Invalid task request */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Target agent unreachable */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_a2a_task: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description A2A task ID */
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Task status and result */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["A2ATaskSchema"];
                };
            };
            /** @description Task not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    stream_a2a_task: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description A2A task ID */
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description SSE stream of task updates */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Task not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_agents: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List of registered agents */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AgentInfoSchema"][];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponseSchema"];
                };
            };
        };
    };
    create_agent: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateAgentRequestSchema"];
            };
        };
        responses: {
            /** @description Agent created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Agent name conflict */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    delete_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent UUID or name */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Agent deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Agent not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Cannot delete quarantined agent */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    query_audit: {
        parameters: {
            query?: {
                /** @description Page number */
                page?: number;
                /** @description Items per page */
                page_size?: number;
                /** @description Filter by agent */
                agent_id?: string;
                /** @description Filter by event type */
                event_type?: string;
                /** @description Filter by severity */
                severity?: string;
                /** @description Start timestamp (RFC3339) */
                from?: string;
                /** @description End timestamp (RFC3339) */
                to?: string;
                /** @description Free-text search */
                q?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Paginated audit entries */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    audit_aggregation: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Audit aggregation data */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    audit_export: {
        parameters: {
            query?: {
                /** @description Export format: json, csv, jsonl */
                format?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Exported audit data */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Login successful, returns access token */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid credentials */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    logout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Logged out, token revoked */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    refresh: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Token refreshed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid or expired refresh token */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_convergence_scores: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Convergence scores per agent */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": unknown;
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_costs: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Per-agent cost summary */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AgentCostSchema"][];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_goals: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List of proposals/goals */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    approve_goal: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Goal/proposal ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Goal approved */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Goal not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Already resolved */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    reject_goal: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Goal/proposal ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Goal rejected */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Goal not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Already resolved */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    health: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Gateway is alive */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Gateway unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_memories: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List of memories */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    write_memory: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Memory created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_memory: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Memory ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Memory detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Memory not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    ready: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Gateway is ready */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Gateway not ready */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_safety_checks: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List registered custom safety checks */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    register_safety_check: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Custom safety check registered */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid check configuration */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    unregister_safety_check: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Safety check ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Safety check removed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Check not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    kill_all: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Platform kill activated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    pause_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent UUID */
                agent_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Agent paused */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Agent not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    quarantine_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent UUID */
                agent_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Agent quarantined */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Agent not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    resume_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent UUID */
                agent_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Agent resumed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Agent not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    safety_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Kill switch and quarantine status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_sessions: {
        parameters: {
            query?: {
                /** @description Page number (1-based) */
                page?: number;
                /** @description Items per page (max 200) */
                page_size?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Paginated session list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_skills: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Installed and available skills */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    install_skill: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Skill ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Skill installed */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Skill not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Skill already installed */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    uninstall_skill: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Skill ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Skill uninstalled */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Skill not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    list_webhooks: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List all webhooks */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WebhookSchema"][];
                };
            };
        };
    };
    create_webhook: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Webhook created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid webhook configuration */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    update_webhook: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Webhook ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Webhook updated */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Webhook not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    delete_webhook: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Webhook ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Webhook deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Webhook not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    test_webhook: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Webhook ID */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Test webhook fired, returns status code */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Webhook not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
}

export { type Agent, type AgentCostInfo, type AgentDetail, AgentsAPI, AuditAPI, type AuditEntry, type AuditExportParams, type AuditQueryParams, type AuditQueryResult, AuthAPI, type AuthTokenResponse, type BranchSessionParams, type BranchSessionResult, ChatAPI, type ChatStreamEventHandler, ConvergenceAPI, type ConvergenceError, type ConvergenceScore, type ConvergenceScoresResult, CostsAPI, type CreateAgentParams, type CreateSessionBookmarkParams, type CreateSessionBookmarkResult, type CreateSessionParams, type DeleteAgentResult, type DeleteSessionBookmarkResult, GhostAPIError, GhostClient, type GhostClientOptions, GhostError, GhostNetworkError, type GhostRequestFn, GhostTimeoutError, GhostWebSocket, type GhostWebSocketOptions, GoalsAPI, HealthAPI, type HealthStatus, type KillAllResult, type ListGoalsParams, type ListGoalsResult, type ListMemoriesParams, type ListMemoriesResult, type ListRuntimeSessionsCursorResult, type ListRuntimeSessionsPageResult, type ListRuntimeSessionsParams, type ListSessionsParams, type ListSkillsResult, type LoginParams, type LogoutResponse, MemoryAPI, type MemoryEntry, type MemoryGraphEdge, type MemoryGraphNode, type MemoryGraphResult, type MemorySearchResultEntry, type PauseResult, type Proposal, type ProposalDetail, type QuarantineResult, type ReadyStatus, type RecoverStreamEvent, type RecoverStreamResult, type ResumeParams, type ResumeResult, type RuntimeSession, RuntimeSessionsAPI, SafetyAPI, type SafetyStatus, type SearchMemoriesParams, type SearchMemoriesResult, type SendMessageParams, type SendMessageResult, type SessionBookmark, type SessionBookmarksResult, type SessionEvent, type SessionEventsParams, type SessionEventsResult, type SessionTrace, SessionsAPI, type Skill, SkillsAPI, type StreamEvent, type StudioMessage, type StudioSession, type StudioSessionWithMessages, type TraceGroup, type TraceSpanRecord, TracesAPI, type WsEvent, type components, type operations, type paths };
