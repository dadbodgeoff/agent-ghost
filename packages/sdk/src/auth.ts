import type { GhostRequestFn, GhostRequestOptions } from './client.js';

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

export interface SessionResponse {
  authenticated: boolean;
  subject: string;
  role: string;
  mode: 'jwt' | 'legacy' | 'none';
}

export class AuthAPI {
  constructor(private request: GhostRequestFn) {}

  async login(
    params: LoginParams,
    options?: GhostRequestOptions,
  ): Promise<AuthTokenResponse> {
    return this.request<AuthTokenResponse>('POST', '/api/auth/login', params, options);
  }

  async refresh(options?: GhostRequestOptions): Promise<AuthTokenResponse> {
    return this.request<AuthTokenResponse>('POST', '/api/auth/refresh', undefined, options);
  }

  async session(): Promise<SessionResponse> {
    return this.request<SessionResponse>('GET', '/api/auth/session');
  }

  async logout(options?: GhostRequestOptions): Promise<LogoutResponse | undefined> {
    return this.request<LogoutResponse | undefined>('POST', '/api/auth/logout', undefined, options);
  }
}
