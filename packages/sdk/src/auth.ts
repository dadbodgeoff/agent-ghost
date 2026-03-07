import type { GhostRequestFn } from './client.js';

export interface LoginParams {
  token: string;
}

export interface AuthTokenResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
}

export interface LogoutResponse {
  message?: string;
  status?: string;
}

export class AuthAPI {
  constructor(private request: GhostRequestFn) {}

  async login(params: LoginParams): Promise<AuthTokenResponse> {
    return this.request<AuthTokenResponse>('POST', '/api/auth/login', params);
  }

  async refresh(): Promise<AuthTokenResponse> {
    return this.request<AuthTokenResponse>('POST', '/api/auth/refresh');
  }

  async logout(): Promise<LogoutResponse | undefined> {
    return this.request<LogoutResponse | undefined>('POST', '/api/auth/logout');
  }
}
