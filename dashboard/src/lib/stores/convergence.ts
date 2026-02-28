/**
 * Convergence store — reactive state for convergence data.
 */

import { writable } from 'svelte/store';

export interface ConvergenceState {
  compositeScore: number;
  interventionLevel: number;
  signals: number[];
  lastUpdated: string;
}

export const convergence = writable<ConvergenceState>({
  compositeScore: 0,
  interventionLevel: 0,
  signals: [0, 0, 0, 0, 0, 0, 0],
  lastUpdated: '',
});
