import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface SafeZone {
  x: number;
  y: number;
  width: number;
  height: number;
  label: string;
}

export interface ActionBudget {
  max_per_minute: number;
  max_per_hour: number;
  used_this_minute: number;
  used_this_hour: number;
}

export interface DisplayGeometry {
  width: number;
  height: number;
}

export interface PolicyBudgetConfig {
  mouse_click: number;
  keyboard_type: number;
  keyboard_hotkey: number;
  mouse_drag: number;
  total: number;
}

export interface CircuitBreakerConfigState {
  max_actions_per_second: number;
  failure_threshold: number;
  cooldown_seconds: number;
}

export interface PcControlUsageTelemetry {
  executed_this_minute: number;
  executed_this_hour: number;
  blocked_this_minute: number;
  blocked_this_hour: number;
}

export interface PcControlTelemetry {
  throughput: ActionBudget;
  policy_budgets: PolicyBudgetConfig;
  usage: PcControlUsageTelemetry;
}

export interface PcControlPersistedState {
  enabled: boolean;
  allowed_apps: string[];
  safe_zone: SafeZone | null;
  blocked_hotkeys: string[];
  budgets: PolicyBudgetConfig;
  circuit_breaker: CircuitBreakerConfigState;
}

export interface PcControlRuntimeState {
  revision: number;
  enabled: boolean;
  activation_state: string;
  effective_allowed_apps: string[];
  effective_safe_zone: SafeZone | null;
  effective_blocked_hotkeys: string[];
  circuit_breaker_state: string;
  last_applied_at: string;
  last_apply_source: string;
}

export interface PcControlStatus {
  enabled: boolean;
  action_budget: ActionBudget;
  allowed_apps: string[];
  safe_zone: SafeZone | null;
  safe_zones: SafeZone[];
  blocked_hotkeys: string[];
  circuit_breaker_state: string;
  display: DisplayGeometry;
  persisted: PcControlPersistedState;
  runtime: PcControlRuntimeState;
  telemetry: PcControlTelemetry;
}

export interface PcControlActionLogEntry {
  id: string;
  action_type: string;
  target: string;
  timestamp: string;
  result: string;
  input_json: string;
  result_json: string;
  target_app: string | null;
  coordinates: string | null;
  blocked: boolean;
  block_reason: string | null;
  agent_id: string;
  session_id: string;
}

export interface PcControlActionLogResult {
  actions: PcControlActionLogEntry[];
}

export class PcControlAPI {
  constructor(private request: GhostRequestFn) {}

  async getStatus(): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('GET', '/api/pc-control/status');
  }

  async listActions(limit = 100): Promise<PcControlActionLogResult> {
    return this.request<PcControlActionLogResult>(
      'GET',
      `/api/pc-control/actions?limit=${encodeURIComponent(String(limit))}`,
    );
  }

  async updateStatus(enabled: boolean, options?: GhostRequestOptions): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/status', { enabled }, options);
  }

  async setAllowedApps(apps: string[], options?: GhostRequestOptions): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/allowed-apps', { apps }, options);
  }

  async setBlockedHotkeys(
    hotkeys: string[],
    options?: GhostRequestOptions,
  ): Promise<PcControlStatus> {
    return this.request<PcControlStatus>(
      'PUT',
      '/api/pc-control/blocked-hotkeys',
      { hotkeys },
      options,
    );
  }

  async setSafeZones(
    zones: SafeZone[],
    options?: GhostRequestOptions,
  ): Promise<PcControlStatus> {
    return this.setSafeZone(zones[0] ?? null, options);
  }

  async setSafeZone(
    safeZone: Omit<SafeZone, 'label'> | SafeZone | null,
    options?: GhostRequestOptions,
  ): Promise<PcControlStatus> {
    return this.request<PcControlStatus>(
      'PUT',
      '/api/pc-control/safe-zones',
      { safe_zone: safeZone },
      options,
    );
  }
}
