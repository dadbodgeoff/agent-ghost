import { readLocalStorage, writeLocalStorage } from '$lib/browser-storage';

/**
 * Frecency tracker — frequency x recency scoring for command palette (Phase 2, Task 3.1).
 *
 * Persists usage history in localStorage with 7-day half-life decay.
 */

export interface FrecencyEntry {
  commandId: string;
  lastUsed: number;   // timestamp
  useCount: number;
}

const STORAGE_KEY = 'ghost-command-frecency';
const DECAY_HALF_LIFE = 7 * 24 * 60 * 60 * 1000; // 7 days

class FrecencyTracker {
  private entries: Map<string, FrecencyEntry>;

  constructor() {
    this.entries = new Map();
    this.load();
  }

  /** Record a command usage. */
  record(commandId: string): void {
    const existing = this.entries.get(commandId) ?? {
      commandId,
      lastUsed: 0,
      useCount: 0,
    };
    existing.lastUsed = Date.now();
    existing.useCount += 1;
    this.entries.set(commandId, existing);
    this.persist();
  }

  /** Compute frecency score for a command. */
  score(commandId: string): number {
    const entry = this.entries.get(commandId);
    if (!entry) return 0;
    const age = Date.now() - entry.lastUsed;
    const recency = Math.exp((-age * Math.LN2) / DECAY_HALF_LIFE);
    return entry.useCount * recency;
  }

  /** Get recently used command IDs, sorted by recency. */
  getRecent(limit = 5): string[] {
    return [...this.entries.values()]
      .sort((a, b) => b.lastUsed - a.lastUsed)
      .slice(0, limit)
      .map(e => e.commandId);
  }

  private load(): void {
    try {
      const stored = readLocalStorage(STORAGE_KEY);
      if (stored) {
        const parsed: [string, FrecencyEntry][] = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          this.entries = new Map(
            parsed.filter(
              (entry): entry is [string, FrecencyEntry] =>
                Array.isArray(entry)
                && typeof entry[0] === 'string'
                && !!entry[1]
                && typeof entry[1].commandId === 'string'
                && typeof entry[1].lastUsed === 'number'
                && typeof entry[1].useCount === 'number',
            ),
          );
        }
      }
    } catch {
      // Corrupted data — start fresh.
    }
  }

  private persist(): void {
    writeLocalStorage(
      STORAGE_KEY,
      JSON.stringify([...this.entries.entries()]),
    );
  }
}

export const frecencyTracker = new FrecencyTracker();
