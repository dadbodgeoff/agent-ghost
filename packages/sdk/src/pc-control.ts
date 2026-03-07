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

export interface PcControlStatus {
  enabled: boolean;
  action_budget: ActionBudget;
  allowed_apps: string[];
  safe_zones: SafeZone[];
  blocked_hotkeys: string[];
  circuit_breaker_state: string;
}

export interface PcControlActionLogEntry {
  id: string;
  action_type: string;
  target: string;
  timestamp: string;
  result: string;
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
    return this.request<PcControlStatus>('PUT', '/api/pc-control/safe-zones', { zones }, options);
  }
}
