import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components } from './generated-types.js';

export type SafeZone = components['schemas']['SafeZone'];
export type ActionBudget = components['schemas']['ActionBudget'];
export type PcControlStatus = components['schemas']['PcControlStatus'];
export type PcControlActionLogEntry = components['schemas']['ActionLogEntry'];
export type PcControlActionLogResult = components['schemas']['PcControlActionsResponse'];

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
