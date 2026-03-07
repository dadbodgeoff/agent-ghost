import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface Skill {
  id: string;
  name: string;
  version: string;
  description: string;
  capabilities: string[];
  source: 'bundled' | 'user' | 'workspace';
  state: string;
}

export interface ListSkillsResult {
  installed: Skill[];
  available: Skill[];
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
  ): Promise<{ uninstalled: string }> {
    return this.request<{ uninstalled: string }>(
      'POST',
      `/api/skills/${encodeURIComponent(name)}/uninstall`,
      undefined,
      options,
    );
  }
}
