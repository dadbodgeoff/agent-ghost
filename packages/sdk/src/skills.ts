import type { GhostRequestFn } from './client.js';

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
  async install(name: string): Promise<Skill> {
    return this.request<Skill>('POST', `/api/skills/${encodeURIComponent(name)}/install`);
  }

  /** Uninstall a skill by name. */
  async uninstall(name: string): Promise<{ uninstalled: string }> {
    return this.request<{ uninstalled: string }>(
      'POST',
      `/api/skills/${encodeURIComponent(name)}/uninstall`,
    );
  }
}
