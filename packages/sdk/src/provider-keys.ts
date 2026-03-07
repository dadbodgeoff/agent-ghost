import type { GhostRequestFn } from './client.js';

export interface ProviderKeyInfo {
  provider_name: string;
  model: string;
  env_name: string;
  is_set: boolean;
  preview: string | null;
}

export interface ListProviderKeysResult {
  providers: ProviderKeyInfo[];
}

export interface SetProviderKeyParams {
  env_name: string;
  value: string;
}

export interface SetProviderKeyResult {
  env_name: string;
  preview: string;
  message: string;
}

export interface DeleteProviderKeyResult {
  env_name: string;
  message: string;
}

export class ProviderKeysAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListProviderKeysResult> {
    return this.request<ListProviderKeysResult>('GET', '/api/admin/provider-keys');
  }

  async set(params: SetProviderKeyParams): Promise<SetProviderKeyResult> {
    return this.request<SetProviderKeyResult>('PUT', '/api/admin/provider-keys', params);
  }

  async delete(envName: string): Promise<DeleteProviderKeyResult> {
    return this.request<DeleteProviderKeyResult>(
      'DELETE',
      `/api/admin/provider-keys/${encodeURIComponent(envName)}`,
    );
  }
}
