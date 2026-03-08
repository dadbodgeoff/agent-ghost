import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export type SkillSource = 'compiled' | 'user' | 'workspace';
export type SkillExecutionMode = 'native' | 'wasm';
export type SkillState =
  | 'always_on'
  | 'installed'
  | 'available'
  | 'disabled'
  | 'quarantined';

export interface Skill {
  id: string;
  name: string;
  version: string;
  description: string;
  source: SkillSource;
  removable: boolean;
  installable: boolean;
  execution_mode: SkillExecutionMode;
  policy_capability: string;
  privileges: string[];
  state: SkillState;
  quarantine_reason?: string | null;
  enabled_for_agent?: boolean | null;
  // Compatibility alias retained while older clients still read capability badges.
  capabilities: string[];
}

export interface ListSkillsResult {
  installed: Skill[];
  available: Skill[];
}

export interface ExecuteSkillParams {
  agent_id: string;
  session_id: string;
  input?: unknown;
}

export interface ExecuteSkillResult {
  skill: string;
  result: unknown;
}

// ── API ──

export class SkillsAPI {
  constructor(private request: GhostRequestFn) {}

  /** List installed and available skills. */
  async list(): Promise<ListSkillsResult> {
    return this.request<ListSkillsResult>('GET', '/api/skills');
  }

  /** Install a skill by name. */
  async install(name: string, options?: GhostRequestOptions): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(name)}/install`,
      undefined,
      options,
    );
  }

  /** Uninstall a skill by name. */
  async uninstall(
    name: string,
    options?: GhostRequestOptions,
  ): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(name)}/uninstall`,
      undefined,
      options,
    );
  }

  /** Execute a skill for a specific runtime agent/session. */
  async execute(
    name: string,
    params: ExecuteSkillParams,
    options?: GhostRequestOptions,
  ): Promise<ExecuteSkillResult> {
    return this.request<ExecuteSkillResult>(
      'POST',
      `/api/skills/${encodeURIComponent(name)}/execute`,
      params,
      options,
    );
  }
}
