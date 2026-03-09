import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components } from './generated-types.js';

export type PushSubscriptionKeys = components['schemas']['PushKeys'];
export type PushSubscriptionPayload = components['schemas']['PushSubscription'];
export type VapidKeyResult = components['schemas']['VapidKeyResponse'];

export class PushAPI {
  constructor(private request: GhostRequestFn) {}

  async getVapidKey(): Promise<VapidKeyResult> {
    return this.request<VapidKeyResult>('GET', '/api/push/vapid-key');
  }

  async subscribe(
    subscription: PushSubscriptionPayload,
    options?: GhostRequestOptions,
  ): Promise<void> {
    return this.request<void>('POST', '/api/push/subscribe', subscription, options);
  }

  async unsubscribe(
    subscription: PushSubscriptionPayload,
    options?: GhostRequestOptions,
  ): Promise<void> {
    return this.request<void>('POST', '/api/push/unsubscribe', subscription, options);
  }
}
