import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export type ProviderKeyInfo = components['schemas']['ProviderKeyInfo'];
export type ListProviderKeysResult = components['schemas']['ProviderKeysResponse'];
export type SetProviderKeyParams =
  operations['set_provider_key']['requestBody']['content']['application/json'];
export type SetProviderKeyResult = components['schemas']['SetKeyResponse'];
export type DeleteProviderKeyResult = components['schemas']['DeleteKeyResponse'];

export class ProviderKeysAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListProviderKeysResult> {
    return this.request<ListProviderKeysResult>('GET', '/api/admin/provider-keys');
  }

  async set(
    params: SetProviderKeyParams,
    options?: GhostRequestOptions,
  ): Promise<SetProviderKeyResult> {
    return this.request<SetProviderKeyResult>('PUT', '/api/admin/provider-keys', params, options);
  }

  async delete(
    envName: string,
    options?: GhostRequestOptions,
  ): Promise<DeleteProviderKeyResult> {
    return this.request<DeleteProviderKeyResult>(
      'DELETE',
      `/api/admin/provider-keys/${encodeURIComponent(envName)}`,
      undefined,
      options,
    );
  }
}
