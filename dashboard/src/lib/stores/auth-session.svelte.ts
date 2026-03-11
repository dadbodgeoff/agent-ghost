import type { SessionResponse } from '@ghost/sdk';
import { getGhostClient } from '$lib/ghost-client';

function hasCapability(session: SessionResponse | null, capability: string): boolean {
  return (session?.capabilities ?? []).includes(capability);
}

class AuthSessionStore {
  session = $state<SessionResponse | null>(null);
  loading = $state(false);
  error = $state('');

  get role(): string {
    return this.session?.role ?? 'anonymous';
  }

  get capabilities(): string[] {
    return this.session?.capabilities ?? [];
  }

  get canTriggerKillAll(): boolean {
    return this.role === 'superadmin';
  }

  get canReviewSandbox(): boolean {
    return this.role === 'superadmin'
      || this.role === 'admin'
      || (this.role === 'operator' && hasCapability(this.session, 'safety_review'));
  }

  hydrate(session: SessionResponse | null) {
    this.session = session;
    this.error = '';
  }

  clear() {
    this.session = null;
    this.error = '';
  }

  async refresh() {
    this.loading = true;
    this.error = '';
    try {
      const client = await getGhostClient();
      this.session = await client.auth.session();
    } catch (error: unknown) {
      this.error = error instanceof Error ? error.message : 'Failed to load auth session';
      throw error;
    } finally {
      this.loading = false;
    }
  }
}

export const authSessionStore = new AuthSessionStore();
