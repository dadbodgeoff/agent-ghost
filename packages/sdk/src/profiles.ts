import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface Profile {
  name: string;
  description: string;
  is_preset: boolean;
  weights: number[];
  thresholds: number[];
}

export interface ListProfilesResult {
  profiles: Profile[];
}

export interface CreateProfileParams {
  name: string;
  description?: string;
  weights: number[];
  thresholds: number[];
}

export interface UpdateProfileParams {
  description?: string;
  weights?: number[];
  thresholds?: number[];
}

export interface DeleteProfileResult {
  deleted: string;
}

export class ProfilesAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListProfilesResult> {
    return this.request<ListProfilesResult>('GET', '/api/profiles');
  }

  async create(params: CreateProfileParams, options?: GhostRequestOptions): Promise<Profile> {
    return this.request<Profile>('POST', '/api/profiles', params, options);
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
