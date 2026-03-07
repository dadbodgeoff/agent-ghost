import type { GhostRequestFn } from './client.js';

export interface PushSubscriptionKeys {
  p256dh?: string;
  auth?: string;
}

export interface PushSubscriptionPayload {
  endpoint: string;
  keys?: PushSubscriptionKeys;
}

export interface VapidKeyResult {
  key?: string;
}

export class PushAPI {
  constructor(private request: GhostRequestFn) {}

  async getVapidKey(): Promise<VapidKeyResult> {
    return this.request<VapidKeyResult>('GET', '/api/push/vapid-key');
  }

  async subscribe(subscription: PushSubscriptionPayload): Promise<void> {
    return this.request<void>('POST', '/api/push/subscribe', subscription);
  }

  async unsubscribe(subscription: PushSubscriptionPayload): Promise<void> {
    return this.request<void>('POST', '/api/push/unsubscribe', subscription);
  }
}
