/**
 * Agents store — reactive state for agent data.
 */

import { writable } from 'svelte/store';

export interface Agent {
  id: string;
  name: string;
  status: string;
  convergenceScore: number;
  interventionLevel: number;
}

export const agents = writable<Agent[]>([]);
