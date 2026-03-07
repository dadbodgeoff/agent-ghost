"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  AgentsAPI: () => AgentsAPI,
  AuditAPI: () => AuditAPI,
  AuthAPI: () => AuthAPI,
  ChatAPI: () => ChatAPI,
  ConvergenceAPI: () => ConvergenceAPI,
  CostsAPI: () => CostsAPI,
  GhostAPIError: () => GhostAPIError,
  GhostClient: () => GhostClient,
  GhostError: () => GhostError,
  GhostNetworkError: () => GhostNetworkError,
  GhostTimeoutError: () => GhostTimeoutError,
  GhostWebSocket: () => GhostWebSocket,
  GoalsAPI: () => GoalsAPI,
  HealthAPI: () => HealthAPI,
  MemoryAPI: () => MemoryAPI,
  RuntimeSessionsAPI: () => RuntimeSessionsAPI,
  SafetyAPI: () => SafetyAPI,
  SessionsAPI: () => SessionsAPI,
  SkillsAPI: () => SkillsAPI,
  TracesAPI: () => TracesAPI
});
module.exports = __toCommonJS(index_exports);

// src/errors.ts
var GhostError = class extends Error {
  constructor(message, status, code, details) {
    super(message);
    this.status = status;
    this.code = code;
    this.details = details;
    this.name = "GhostError";
  }
};
var GhostAPIError = class extends GhostError {
  constructor(message, status, code, details) {
    super(message, status, code, details);
    this.status = status;
    this.name = "GhostAPIError";
  }
};
var GhostNetworkError = class extends GhostError {
  constructor(message, cause) {
    super(message);
    this.cause = cause;
    this.name = "GhostNetworkError";
  }
};
var GhostTimeoutError = class extends GhostError {
  constructor(timeoutMs) {
    super(`Request timed out after ${timeoutMs}ms`);
    this.timeoutMs = timeoutMs;
    this.name = "GhostTimeoutError";
  }
};

// src/agents.ts
var AgentsAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** List all registered agents. */
  async list() {
    return this.request("GET", "/api/agents");
  }
  /** Create a new agent with optional keypair generation. */
  async create(params) {
    return this.request("POST", "/api/agents", params);
  }
  /** Delete an agent by ID or name. */
  async delete(id) {
    return this.request("DELETE", `/api/agents/${encodeURIComponent(id)}`);
  }
};

// src/sessions.ts
var SessionsAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** Create a new studio chat session. */
  async create(params) {
    return this.request("POST", "/api/studio/sessions", params ?? {});
  }
  /** List studio chat sessions. */
  async list(params) {
    const query = new URLSearchParams();
    if (params?.limit !== void 0) query.set("limit", String(params.limit));
    if (params?.offset !== void 0) query.set("offset", String(params.offset));
    if (params?.before) query.set("before", params.before);
    const qs = query.toString();
    return this.request(
      "GET",
      `/api/studio/sessions${qs ? `?${qs}` : ""}`
    );
  }
  /** Get a session with all its messages. */
  async get(id) {
    return this.request(
      "GET",
      `/api/studio/sessions/${encodeURIComponent(id)}`
    );
  }
  /** Delete a studio session. */
  async delete(id) {
    return this.request(
      "DELETE",
      `/api/studio/sessions/${encodeURIComponent(id)}`
    );
  }
  async recoverStream(id, params) {
    const query = new URLSearchParams();
    query.set("message_id", params.message_id);
    if (params.after_seq !== void 0) query.set("after_seq", String(params.after_seq));
    const qs = query.toString();
    return this.request(
      "GET",
      `/api/studio/sessions/${encodeURIComponent(id)}/stream/recover?${qs}`
    );
  }
};

// src/chat.ts
var ChatAPI = class {
  constructor(request, options) {
    this.request = request;
    this.options = options;
  }
  /** Send a message and wait for the complete response (blocking). */
  async send(sessionId, params) {
    return this.request(
      "POST",
      `/api/studio/sessions/${encodeURIComponent(sessionId)}/messages`,
      params
    );
  }
  /** Send a message and receive streaming SSE events. */
  async *stream(sessionId, params) {
    const baseUrl = this.options.baseUrl ?? "http://127.0.0.1:39780";
    const url = `${baseUrl}/api/studio/sessions/${encodeURIComponent(sessionId)}/messages/stream`;
    const headers = {
      "Content-Type": "application/json",
      Accept: "text/event-stream"
    };
    if (this.options.token) {
      headers["Authorization"] = `Bearer ${this.options.token}`;
    }
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    let response;
    try {
      response = await fetchFn(url, {
        method: "POST",
        headers,
        body: JSON.stringify(params),
        signal: this.options.timeout ? AbortSignal.timeout(this.options.timeout) : void 0
      });
    } catch (err) {
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : void 0
      );
    }
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new GhostAPIError(
        text || `HTTP ${response.status}`,
        response.status
      );
    }
    if (!response.body) {
      throw new GhostNetworkError("Response body is null \u2014 streaming not supported in this environment");
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        let eventType = "";
        let dataLines = [];
        for (const line of lines) {
          if (line.startsWith("event: ")) {
            eventType = line.slice(7).trim();
          } else if (line.startsWith("data: ")) {
            dataLines.push(line.slice(6));
          } else if (line.startsWith("data:")) {
            dataLines.push(line.slice(5));
          } else if (line === "" && dataLines.length > 0) {
            const eventData = dataLines.join("\n");
            try {
              const parsed = JSON.parse(eventData);
              yield { type: eventType || parsed.type, ...parsed };
            } catch {
            }
            eventType = "";
            dataLines = [];
          } else if (line === "") {
            eventType = "";
            dataLines = [];
          }
        }
      }
    } finally {
      reader.releaseLock();
    }
  }
  async streamWithCallback(sessionId, params, onEvent, signal) {
    const baseUrl = this.options.baseUrl ?? "http://127.0.0.1:39780";
    const url = `${baseUrl}/api/studio/sessions/${encodeURIComponent(sessionId)}/messages/stream`;
    const headers = {
      "Content-Type": "application/json",
      Accept: "text/event-stream"
    };
    if (this.options.token) {
      headers["Authorization"] = `Bearer ${this.options.token}`;
    }
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    let response;
    try {
      response = await fetchFn(url, {
        method: "POST",
        headers,
        body: JSON.stringify(params),
        signal
      });
    } catch (err) {
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : void 0
      );
    }
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new GhostAPIError(text || `HTTP ${response.status}`, response.status);
    }
    if (!response.body) {
      throw new GhostNetworkError("Response body is null \u2014 streaming not supported in this environment");
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let aborted = false;
    const onAbort = () => {
      aborted = true;
      reader.cancel().catch(() => {
      });
    };
    signal?.addEventListener("abort", onAbort);
    try {
      while (!aborted) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const events = buffer.split("\n\n");
        buffer = events.pop() || "";
        for (const eventBlock of events) {
          if (!eventBlock.trim()) continue;
          const lines = eventBlock.split("\n");
          let eventType = "message";
          let eventId;
          const dataLines = [];
          for (const line of lines) {
            if (line.startsWith("event: ")) {
              eventType = line.slice(7).trim();
            } else if (line.startsWith("data: ")) {
              dataLines.push(line.slice(6));
            } else if (line.startsWith("id: ")) {
              eventId = line.slice(4).trim();
            }
          }
          if (dataLines.length === 0) continue;
          const dataStr = dataLines.join("\n");
          try {
            const data = JSON.parse(dataStr);
            onEvent(eventType, data, eventId);
          } catch {
            onEvent(eventType, { message: dataStr }, eventId);
          }
        }
      }
    } finally {
      signal?.removeEventListener("abort", onAbort);
      reader.releaseLock();
    }
  }
};

// src/convergence.ts
var ConvergenceAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** Get convergence scores for all agents. */
  async scores() {
    return this.request("GET", "/api/convergence/scores");
  }
};

// src/goals.ts
var GoalsAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** List goal proposals with optional filtering. */
  async list(params) {
    const query = new URLSearchParams();
    if (params?.status) query.set("status", params.status);
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.page !== void 0) query.set("page", String(params.page));
    if (params?.page_size !== void 0) query.set("page_size", String(params.page_size));
    const qs = query.toString();
    return this.request("GET", `/api/goals${qs ? `?${qs}` : ""}`);
  }
  /** Get a single proposal with full detail. */
  async get(id) {
    return this.request("GET", `/api/goals/${encodeURIComponent(id)}`);
  }
  /** Approve a pending proposal. */
  async approve(id) {
    return this.request(
      "POST",
      `/api/goals/${encodeURIComponent(id)}/approve`
    );
  }
  /** Reject a pending proposal. */
  async reject(id) {
    return this.request(
      "POST",
      `/api/goals/${encodeURIComponent(id)}/reject`
    );
  }
};

// src/skills.ts
var SkillsAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** List installed and available skills. */
  async list() {
    return this.request("GET", "/api/skills");
  }
  /** Install a skill by name. */
  async install(name) {
    return this.request("POST", `/api/skills/${encodeURIComponent(name)}/install`);
  }
  /** Uninstall a skill by name. */
  async uninstall(name) {
    return this.request(
      "POST",
      `/api/skills/${encodeURIComponent(name)}/uninstall`
    );
  }
};

// src/safety.ts
var SafetyAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** Get platform and per-agent safety status. */
  async status() {
    return this.request("GET", "/api/safety/status");
  }
  /** Activate platform-wide kill switch. */
  async killAll(reason, initiatedBy) {
    return this.request("POST", "/api/safety/kill-all", {
      reason,
      initiated_by: initiatedBy
    });
  }
  /** Pause a specific agent. */
  async pause(agentId, reason) {
    return this.request(
      "POST",
      `/api/safety/pause/${encodeURIComponent(agentId)}`,
      { reason }
    );
  }
  /** Resume a paused or quarantined agent. */
  async resume(agentId, params) {
    return this.request(
      "POST",
      `/api/safety/resume/${encodeURIComponent(agentId)}`,
      params ?? {}
    );
  }
  /** Quarantine an agent (requires forensic review to resume). */
  async quarantine(agentId, reason) {
    return this.request(
      "POST",
      `/api/safety/quarantine/${encodeURIComponent(agentId)}`,
      { reason }
    );
  }
};

// src/health.ts
var HealthAPI = class {
  constructor(request) {
    this.request = request;
  }
  /** Check if the gateway is alive. */
  async check() {
    return this.request("GET", "/api/health");
  }
  /** Check if the gateway is ready to accept requests. */
  async ready() {
    return this.request("GET", "/api/ready");
  }
};

// src/auth.ts
var AuthAPI = class {
  constructor(request) {
    this.request = request;
  }
  async login(params) {
    return this.request("POST", "/api/auth/login", params);
  }
  async refresh() {
    return this.request("POST", "/api/auth/refresh");
  }
  async logout() {
    return this.request("POST", "/api/auth/logout");
  }
};

// src/audit.ts
var AuditAPI = class {
  constructor(request, options) {
    this.request = request;
    this.options = options;
  }
  async query(params) {
    const query = new URLSearchParams();
    if (params?.time_start) query.set("time_start", params.time_start);
    if (params?.time_end) query.set("time_end", params.time_end);
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.event_type) query.set("event_type", params.event_type);
    if (params?.severity) query.set("severity", params.severity);
    if (params?.tool_name) query.set("tool_name", params.tool_name);
    if (params?.search) query.set("search", params.search);
    if (params?.page !== void 0) query.set("page", String(params.page));
    if (params?.page_size !== void 0) query.set("page_size", String(params.page_size));
    const qs = query.toString();
    return this.request("GET", `/api/audit${qs ? `?${qs}` : ""}`);
  }
  async export(params) {
    const query = new URLSearchParams();
    if (params?.format) query.set("format", params.format);
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.time_start) query.set("time_start", params.time_start);
    if (params?.time_end) query.set("time_end", params.time_end);
    const qs = query.toString();
    return this.request("GET", `/api/audit/export${qs ? `?${qs}` : ""}`);
  }
  async exportBlob(params) {
    const query = new URLSearchParams();
    if (params?.format) query.set("format", params.format);
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.time_start) query.set("time_start", params.time_start);
    if (params?.time_end) query.set("time_end", params.time_end);
    const qs = query.toString();
    const baseUrl = this.options.baseUrl ?? "http://127.0.0.1:39780";
    const url = `${baseUrl}/api/audit/export${qs ? `?${qs}` : ""}`;
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    const headers = {};
    if (this.options.token) {
      headers["Authorization"] = `Bearer ${this.options.token}`;
    }
    let response;
    try {
      response = await fetchFn(url, { method: "GET", headers });
    } catch (err) {
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : void 0
      );
    }
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new GhostAPIError(text || `HTTP ${response.status}`, response.status);
    }
    return response.blob();
  }
};

// src/costs.ts
var CostsAPI = class {
  constructor(request) {
    this.request = request;
  }
  async list() {
    return this.request("GET", "/api/costs");
  }
};

// src/memory.ts
var MemoryAPI = class {
  constructor(request) {
    this.request = request;
  }
  async list(params) {
    const query = new URLSearchParams();
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.page !== void 0) query.set("page", String(params.page));
    if (params?.page_size !== void 0) query.set("page_size", String(params.page_size));
    if (params?.include_archived !== void 0) {
      query.set("include_archived", String(params.include_archived));
    }
    const qs = query.toString();
    return this.request("GET", `/api/memory${qs ? `?${qs}` : ""}`);
  }
  async get(id) {
    return this.request("GET", `/api/memory/${encodeURIComponent(id)}`);
  }
  async graph() {
    return this.request("GET", "/api/memory/graph");
  }
  async search(params) {
    const query = new URLSearchParams();
    if (params?.q) query.set("q", params.q);
    if (params?.agent_id) query.set("agent_id", params.agent_id);
    if (params?.memory_type) query.set("memory_type", params.memory_type);
    if (params?.importance) query.set("importance", params.importance);
    if (params?.confidence_min !== void 0) query.set("confidence_min", String(params.confidence_min));
    if (params?.confidence_max !== void 0) query.set("confidence_max", String(params.confidence_max));
    if (params?.limit !== void 0) query.set("limit", String(params.limit));
    if (params?.include_archived !== void 0) {
      query.set("include_archived", String(params.include_archived));
    }
    const qs = query.toString();
    return this.request("GET", `/api/memory/search${qs ? `?${qs}` : ""}`);
  }
};

// src/runtime-sessions.ts
var RuntimeSessionsAPI = class {
  constructor(request) {
    this.request = request;
  }
  async list(params) {
    const query = new URLSearchParams();
    if (params?.page !== void 0) query.set("page", String(params.page));
    if (params?.page_size !== void 0) query.set("page_size", String(params.page_size));
    if (params?.cursor) query.set("cursor", params.cursor);
    if (params?.limit !== void 0) query.set("limit", String(params.limit));
    const qs = query.toString();
    return this.request(
      "GET",
      `/api/sessions${qs ? `?${qs}` : ""}`
    );
  }
  async events(sessionId, params) {
    const query = new URLSearchParams();
    if (params?.offset !== void 0) query.set("offset", String(params.offset));
    if (params?.limit !== void 0) query.set("limit", String(params.limit));
    const qs = query.toString();
    return this.request(
      "GET",
      `/api/sessions/${encodeURIComponent(sessionId)}/events${qs ? `?${qs}` : ""}`
    );
  }
  async listBookmarks(sessionId) {
    return this.request(
      "GET",
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks`
    );
  }
  async createBookmark(sessionId, params) {
    return this.request(
      "POST",
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks`,
      params
    );
  }
  async deleteBookmark(sessionId, bookmarkId) {
    return this.request(
      "DELETE",
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks/${encodeURIComponent(bookmarkId)}`
    );
  }
  async branch(sessionId, params) {
    return this.request(
      "POST",
      `/api/sessions/${encodeURIComponent(sessionId)}/branch`,
      params
    );
  }
  async heartbeat(sessionId) {
    await this.request(
      "POST",
      `/api/sessions/${encodeURIComponent(sessionId)}/heartbeat`,
      {}
    );
  }
};

// src/traces.ts
var TracesAPI = class {
  constructor(request) {
    this.request = request;
  }
  async get(sessionId) {
    return this.request("GET", `/api/traces/${encodeURIComponent(sessionId)}`);
  }
};

// src/websocket.ts
var GhostWebSocket = class {
  constructor(clientOptions, wsOptions = {}) {
    this.clientOptions = clientOptions;
    this.wsOptions = wsOptions;
    this.subscribedTopics = wsOptions.topics ?? [];
  }
  ws = null;
  handlers = /* @__PURE__ */ new Map();
  globalHandlers = /* @__PURE__ */ new Set();
  subscribedTopics = [];
  reconnectAttempt = 0;
  reconnectTimer = null;
  closed = false;
  /** Open the WebSocket connection. */
  connect() {
    this.closed = false;
    this.reconnectAttempt = 0;
    this.doConnect();
    return this;
  }
  /** Close the WebSocket connection. */
  disconnect() {
    this.closed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }
  /** Subscribe to additional topics. */
  subscribe(topics) {
    this.subscribedTopics.push(...topics);
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "Subscribe", topics }));
    }
  }
  /** Unsubscribe from topics. */
  unsubscribe(topics) {
    this.subscribedTopics = this.subscribedTopics.filter((t) => !topics.includes(t));
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "Unsubscribe", topics }));
    }
  }
  /** Listen for a specific event type. */
  on(eventType, handler) {
    let set = this.handlers.get(eventType);
    if (!set) {
      set = /* @__PURE__ */ new Set();
      this.handlers.set(eventType, set);
    }
    set.add(handler);
    return () => set.delete(handler);
  }
  /** Listen for all events. */
  onAny(handler) {
    this.globalHandlers.add(handler);
    return () => this.globalHandlers.delete(handler);
  }
  doConnect() {
    const baseUrl = this.clientOptions.baseUrl ?? "http://127.0.0.1:39780";
    const wsUrl = baseUrl.replace(/^http/, "ws") + "/api/ws";
    const protocols = this.clientOptions.token ? [`ghost-token.${this.clientOptions.token}`] : void 0;
    this.ws = new WebSocket(wsUrl, protocols);
    this.ws.onopen = () => {
      this.reconnectAttempt = 0;
      if (this.subscribedTopics.length > 0) {
        this.ws.send(
          JSON.stringify({ type: "Subscribe", topics: this.subscribedTopics })
        );
      }
    };
    this.ws.onmessage = (msg) => {
      try {
        const event = JSON.parse(String(msg.data));
        const typeHandlers = this.handlers.get(event.type);
        if (typeHandlers) {
          for (const h of typeHandlers) h(event);
        }
        for (const h of this.globalHandlers) h(event);
      } catch {
      }
    };
    this.ws.onclose = () => {
      this.ws = null;
      if (!this.closed && (this.wsOptions.autoReconnect ?? true)) {
        this.scheduleReconnect();
      }
    };
    this.ws.onerror = () => {
    };
  }
  scheduleReconnect() {
    const maxAttempts = this.wsOptions.maxReconnectAttempts ?? 10;
    if (maxAttempts > 0 && this.reconnectAttempt >= maxAttempts) {
      this.closed = true;
      this.wsOptions.onReconnectFailed?.();
      return;
    }
    const maxDelay = this.wsOptions.maxReconnectDelay ?? 3e4;
    const delay = Math.min(1e3 * 2 ** this.reconnectAttempt, maxDelay);
    this.reconnectAttempt++;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.doConnect();
    }, delay);
  }
};

// src/client.ts
var GhostClient = class {
  agents;
  sessions;
  chat;
  convergence;
  goals;
  skills;
  safety;
  health;
  auth;
  audit;
  costs;
  memory;
  runtimeSessions;
  traces;
  options;
  constructor(options) {
    this.options = {
      baseUrl: "http://127.0.0.1:39780",
      ...options
    };
    const request = this.request.bind(this);
    this.agents = new AgentsAPI(request);
    this.sessions = new SessionsAPI(request);
    this.chat = new ChatAPI(request, this.options);
    this.convergence = new ConvergenceAPI(request);
    this.goals = new GoalsAPI(request);
    this.skills = new SkillsAPI(request);
    this.safety = new SafetyAPI(request);
    this.health = new HealthAPI(request);
    this.auth = new AuthAPI(request);
    this.audit = new AuditAPI(request, this.options);
    this.costs = new CostsAPI(request);
    this.memory = new MemoryAPI(request);
    this.runtimeSessions = new RuntimeSessionsAPI(request);
    this.traces = new TracesAPI(request);
  }
  /** Create a WebSocket connection for real-time events. */
  ws(options) {
    return new GhostWebSocket(this.options, options);
  }
  /** Internal request method used by all API modules. */
  async request(method, path, body) {
    const url = `${this.options.baseUrl}${path}`;
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    const headers = {
      Accept: "application/json"
    };
    if (body !== void 0) {
      headers["Content-Type"] = "application/json";
    }
    if (this.options.token) {
      headers["Authorization"] = `Bearer ${this.options.token}`;
    }
    let response;
    try {
      response = await fetchFn(url, {
        method,
        headers,
        body: body !== void 0 ? JSON.stringify(body) : void 0,
        signal: this.options.timeout ? AbortSignal.timeout(this.options.timeout) : void 0
      });
    } catch (err) {
      if (err instanceof DOMException && err.name === "TimeoutError") {
        throw new GhostTimeoutError(this.options.timeout);
      }
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${this.options.baseUrl}`,
        err instanceof Error ? err : void 0
      );
    }
    if (!response.ok) {
      let errorMessage = `HTTP ${response.status}`;
      let errorCode;
      let errorDetails;
      try {
        const text = await response.text();
        if (text && text.trim().length > 0) {
          try {
            const errorBody = JSON.parse(text);
            if (typeof errorBody === "object" && errorBody !== null) {
              if ("error" in errorBody) {
                if (typeof errorBody.error === "string") {
                  errorMessage = errorBody.error;
                } else if (typeof errorBody.error === "object" && errorBody.error !== null) {
                  const e = errorBody.error;
                  errorMessage = e.message ?? errorMessage;
                  errorCode = e.code;
                  errorDetails = e.details;
                }
              }
            }
          } catch {
            errorMessage = text.substring(0, 500);
          }
        }
      } catch {
      }
      throw new GhostAPIError(errorMessage, response.status, errorCode, errorDetails);
    }
    const contentLength = response.headers.get("content-length");
    const contentType = response.headers.get("content-type") ?? "";
    if (response.status === 204 || contentLength === "0" || !contentType.includes("application/json")) {
      if (contentLength === "0" || response.status === 204) {
        return void 0;
      }
      const text = await response.text();
      if (!text || text.trim().length === 0) {
        return void 0;
      }
      try {
        return JSON.parse(text);
      } catch {
        return void 0;
      }
    }
    return response.json();
  }
};
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  AgentsAPI,
  AuditAPI,
  AuthAPI,
  ChatAPI,
  ConvergenceAPI,
  CostsAPI,
  GhostAPIError,
  GhostClient,
  GhostError,
  GhostNetworkError,
  GhostTimeoutError,
  GhostWebSocket,
  GoalsAPI,
  HealthAPI,
  MemoryAPI,
  RuntimeSessionsAPI,
  SafetyAPI,
  SessionsAPI,
  SkillsAPI,
  TracesAPI
});
//# sourceMappingURL=index.cjs.map