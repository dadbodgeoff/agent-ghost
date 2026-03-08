import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components } from './generated-types.js';

// ── Types ──

export type Skill = components['schemas']['SkillSummaryDto'];
export type SkillSource = components['schemas']['SkillSourceKind'];
export type SkillExecutionMode = components['schemas']['SkillExecutionMode'];
export type SkillState = components['schemas']['SkillStateDto'];
export type SkillInstallState = components['schemas']['SkillInstallStateDto'];
export type SkillVerificationStatus = components['schemas']['SkillVerificationStatusDto'];
export type SkillQuarantineState = components['schemas']['SkillQuarantineStateDto'];
export type SkillMutationKind = components['schemas']['SkillMutationKind'];
export type ListSkillsResult = components['schemas']['SkillListResponseDto'];
export type ExecuteSkillParams = components['schemas']['ExecuteSkillRequestDto'];
export type ExecuteSkillResult = components['schemas']['ExecuteSkillResponseDto'];
export type QuarantineSkillParams = components['schemas']['SkillQuarantineRequestDto'];
export type ResolveSkillQuarantineParams =
  components['schemas']['SkillQuarantineResolutionRequestDto'];

// ── API ──

export class SkillsAPI {
  constructor(private request: GhostRequestFn) {}

  /** List installed and available skills from the mixed-source catalog. */
  async list(): Promise<ListSkillsResult> {
    return this.request<ListSkillsResult>('GET', '/api/skills');
  }

  /** Install a skill by canonical catalog identifier. */
  async install(id: string, options?: GhostRequestOptions): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(id)}/install`,
      undefined,
      options,
    );
  }

  /** Uninstall a skill by canonical catalog identifier. */
  async uninstall(
    id: string,
    options?: GhostRequestOptions,
  ): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(id)}/uninstall`,
      undefined,
      options,
    );
  }

  /** Manually quarantine an external skill artifact by catalog identifier. */
  async quarantine(
    id: string,
    params: QuarantineSkillParams,
    options?: GhostRequestOptions,
  ): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(id)}/quarantine`,
      params,
      options,
    );
  }

  /** Resolve a quarantined external skill artifact with an expected revision guard. */
  async resolveQuarantine(
    id: string,
    params: ResolveSkillQuarantineParams,
    options?: GhostRequestOptions,
  ): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(id)}/quarantine/resolve`,
      params,
      options,
    );
  }

  /** Re-run verification against the gateway-managed artifact. */
  async reverify(id: string, options?: GhostRequestOptions): Promise<Skill> {
    return this.request<Skill>(
      'POST',
      `/api/skills/${encodeURIComponent(id)}/reverify`,
      undefined,
      options,
    );
  }

  /** Execute a catalog skill for a specific runtime agent/session. */
  async execute(
    idOrName: string,
    params: ExecuteSkillParams,
    options?: GhostRequestOptions,
  ): Promise<ExecuteSkillResult> {
    return this.request<ExecuteSkillResult>(
      'POST',
      `/api/skills/${encodeURIComponent(idOrName)}/execute`,
      params,
      options,
    );
  }
}
