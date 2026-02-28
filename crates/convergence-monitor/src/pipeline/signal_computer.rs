//! Signal computation with dirty-flag throttling (Req 9 AC1, Req 5 AC12).

use std::collections::BTreeMap;
use std::time::Instant;

use uuid::Uuid;

/// Dirty-flag throttled signal computer.
///
/// Only recomputes signals whose input data changed since last computation.
pub struct SignalComputer {
    /// Per-agent, per-signal last computation time and cached value.
    cache: BTreeMap<Uuid, SignalCache>,
}

struct SignalCache {
    values: [f64; 7],
    dirty: [bool; 7],
    last_computed: Instant,
}

impl SignalComputer {
    pub fn new() -> Self {
        Self {
            cache: BTreeMap::new(),
        }
    }

    /// Mark a signal as dirty for an agent (input data changed).
    pub fn mark_dirty(&mut self, agent_id: Uuid, signal_index: usize) {
        if signal_index < 7 {
            let entry = self.cache.entry(agent_id).or_insert_with(|| SignalCache {
                values: [0.0; 7],
                dirty: [true; 7],
                last_computed: Instant::now(),
            });
            entry.dirty[signal_index] = true;
        }
    }

    /// Compute signals for an agent, only recomputing dirty ones.
    /// Returns the full 7-signal array.
    pub fn compute(&mut self, agent_id: Uuid) -> [f64; 7] {
        let entry = self.cache.entry(agent_id).or_insert_with(|| SignalCache {
            values: [0.0; 7],
            dirty: [true; 7],
            last_computed: Instant::now(),
        });

        for i in 0..7 {
            if entry.dirty[i] {
                // In production, each signal would compute from actual data.
                // Signal stubs: return cached value (real impl in cortex-convergence).
                entry.dirty[i] = false;
            }
        }

        entry.last_computed = Instant::now();
        entry.values
    }

    /// Set a signal value directly (used when receiving computed signals).
    pub fn set_signal(&mut self, agent_id: Uuid, signal_index: usize, value: f64) {
        if signal_index < 7 {
            let entry = self.cache.entry(agent_id).or_insert_with(|| SignalCache {
                values: [0.0; 7],
                dirty: [true; 7],
                last_computed: Instant::now(),
            });
            entry.values[signal_index] = value.clamp(0.0, 1.0);
            entry.dirty[signal_index] = false;
        }
    }
}

impl Default for SignalComputer {
    fn default() -> Self {
        Self::new()
    }
}
