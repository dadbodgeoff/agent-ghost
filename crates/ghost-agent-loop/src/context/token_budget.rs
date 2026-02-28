//! Token budget allocation (A2.6).

/// Budget type for a prompt layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    /// No limit — layer is never truncated.
    Uncapped,
    /// Fixed token allocation.
    Fixed(usize),
    /// Gets whatever remains after all fixed layers are allocated.
    Remainder,
}

/// Per-layer budget allocation.
#[derive(Debug, Clone)]
pub struct LayerBudget {
    pub layer_index: u8,
    pub budget: Budget,
    pub allocated: usize,
}

/// Allocates token budgets across 10 prompt layers.
pub struct TokenBudgetAllocator;

impl TokenBudgetAllocator {
    /// Default budgets per layer (Req 11 AC2).
    pub fn default_budgets() -> [Budget; 10] {
        [
            Budget::Uncapped,     // L0: CORP_POLICY.md
            Budget::Fixed(200),   // L1: Simulation boundary prompt
            Budget::Fixed(2000),  // L2: SOUL.md + IDENTITY.md
            Budget::Fixed(3000),  // L3: Tool schemas
            Budget::Fixed(200),   // L4: Environment context
            Budget::Fixed(500),   // L5: Skill index
            Budget::Fixed(1000),  // L6: Convergence state
            Budget::Fixed(4000),  // L7: MEMORY.md + daily logs
            Budget::Remainder,    // L8: Conversation history
            Budget::Uncapped,     // L9: User message
        ]
    }

    /// Allocate budgets given a total context window.
    pub fn allocate(context_window: usize, budgets: &[Budget; 10]) -> [usize; 10] {
        let mut allocated = [0usize; 10];
        let mut fixed_total = 0usize;

        // First pass: allocate fixed budgets
        for (i, budget) in budgets.iter().enumerate() {
            match budget {
                Budget::Uncapped => {
                    // Uncapped layers get their full content (estimated later)
                    allocated[i] = usize::MAX;
                }
                Budget::Fixed(n) => {
                    allocated[i] = *n;
                    fixed_total += n;
                }
                Budget::Remainder => {
                    // Calculated after fixed layers
                }
            }
        }

        // Second pass: allocate remainder
        let remainder = context_window.saturating_sub(fixed_total);
        for (i, budget) in budgets.iter().enumerate() {
            if matches!(budget, Budget::Remainder) {
                allocated[i] = remainder;
            }
        }

        allocated
    }

    /// Truncation priority: L8 > L7 > L5 > L2. NEVER truncate L0, L1, L9.
    pub fn truncation_order() -> [u8; 4] {
        [8, 7, 5, 2]
    }
}
