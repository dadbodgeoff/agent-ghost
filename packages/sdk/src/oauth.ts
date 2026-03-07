import type { GhostRequestFn } from './client.js';

export interface OAuthProvider {
  name: string;
}

export interface OAuthConnection {
  ref_id: string;
  provider: string;
  scopes: string[];
  connected_at: string;
  status: 'connected' | 'expired' | 'revoked' | 'error';
}

export interface ConnectOAuthProviderParams {
  provider: string;
  scopes?: string[];
  redirect_uri?: string;
}

export interface ConnectOAuthProviderResult {
  authorization_url: string;
  ref_id: string;
}

export interface DisconnectOAuthConnectionResult {
  status: string;
  ref_id: string;
}

export class OAuthAPI {
  constructor(private request: GhostRequestFn) {}

  async providers(): Promise<OAuthProvider[]> {
    return this.request<OAuthProvider[]>('GET', '/api/oauth/providers');
  }

  async connections(): Promise<OAuthConnection[]> {
    return this.request<OAuthConnection[]>('GET', '/api/oauth/connections');
  }

  async connect(params: ConnectOAuthProviderParams): Promise<ConnectOAuthProviderResult> {
    return this.request<ConnectOAuthProviderResult>('POST', '/api/oauth/connect', {
      scopes: [],
      ...params,
    });
  }

  async disconnect(refId: string): Promise<DisconnectOAuthConnectionResult> {
    return this.request<DisconnectOAuthConnectionResult>(
      'DELETE',
      `/api/oauth/connections/${encodeURIComponent(refId)}`,
    );
  }
}
