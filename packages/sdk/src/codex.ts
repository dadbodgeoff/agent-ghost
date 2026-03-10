import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export type CodexAccount =
  | { type: 'api_key' }
  | { type: 'chatgpt'; email: string; plan_type: string };

export interface CodexStatusResult {
  requires_openai_auth: boolean;
  account: CodexAccount | null;
}

export interface CodexLoginStartResult {
  auth_type: string;
  auth_url?: string;
  login_id?: string;
}

export interface CodexLogoutResult {
  message: string;
}

export class CodexAPI {
  constructor(private request: GhostRequestFn) {}

  async status(): Promise<CodexStatusResult> {
    return this.request<CodexStatusResult>('GET', '/api/admin/codex/status');
  }

  async startLogin(options?: GhostRequestOptions): Promise<CodexLoginStartResult> {
    return this.request<CodexLoginStartResult>(
      'POST',
      '/api/admin/codex/login/start',
      undefined,
      options,
    );
  }

  async logout(options?: GhostRequestOptions): Promise<CodexLogoutResult> {
    return this.request<CodexLogoutResult>(
      'POST',
      '/api/admin/codex/logout',
      undefined,
      options,
    );
  }
}
