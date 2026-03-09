import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export type Profile = components['schemas']['ProfileSummary'];
export type ListProfilesResult = components['schemas']['ProfileListResponse'];
export type CreateProfileParams =
  operations['create_profile']['requestBody']['content']['application/json'];
export type CreateProfileResult =
  operations['create_profile']['responses'][201]['content']['application/json'];
export type UpdateProfileParams =
  operations['update_profile']['requestBody']['content']['application/json'];
export type DeleteProfileResult = components['schemas']['DeleteProfileResponse'];

export class ProfilesAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListProfilesResult> {
    return this.request<ListProfilesResult>('GET', '/api/profiles');
  }

  async create(
    params: CreateProfileParams,
    options?: GhostRequestOptions,
  ): Promise<CreateProfileResult> {
    return this.request<CreateProfileResult>('POST', '/api/profiles', params, options);
  }

  async update(
    name: string,
    params: UpdateProfileParams,
    options?: GhostRequestOptions,
  ): Promise<Profile> {
    return this.request<Profile>('PUT', `/api/profiles/${encodeURIComponent(name)}`, params, options);
  }

  async delete(name: string, options?: GhostRequestOptions): Promise<DeleteProfileResult> {
    return this.request<DeleteProfileResult>(
      'DELETE',
      `/api/profiles/${encodeURIComponent(name)}`,
      undefined,
      options,
    );
  }
}
