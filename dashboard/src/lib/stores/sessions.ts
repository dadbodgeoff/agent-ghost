/**
 * Sessions store — reactive state for session data.
 */

import { writable } from 'svelte/store';

export interface Session {
  id: string;
  agentId: string;
  channel: string;
  startedAt: string;
  messageCount: number;
  status: string;
}

export const sessions = writable<Session[]>([]);
