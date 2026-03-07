import type { GhostRequestFn } from './client.js';

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

  async updateStatus(enabled: boolean): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/status', { enabled });
  }

  async setAllowedApps(apps: string[]): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/allowed-apps', { apps });
  }

  async setBlockedHotkeys(hotkeys: string[]): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/blocked-hotkeys', { hotkeys });
  }

  async setSafeZones(zones: SafeZone[]): Promise<PcControlStatus> {
    return this.request<PcControlStatus>('PUT', '/api/pc-control/safe-zones', { zones });
  }
}
