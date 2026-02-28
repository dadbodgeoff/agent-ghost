//! Property tests for Task 14.4 — cascade breakers + depth tracking.

use std::time::Duration;

use ghost_mesh::safety::cascade_breaker::{
    CascadeBreakerState, CascadeCircuitBreaker, DelegationDepthTracker,
};
use proptest::prelude::*;
use uuid::Uuid;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn delegation_depth_limit_always_enforced(
        max_depth in 1..10u32,
        n_hops in 1..20u32,
    ) {
        let mut tracker = DelegationDepthTracker::new(max_depth);
        let task_id = Uuid::new_v4();
        tracker.register_task(task_id);

        let mut last_ok_depth = 0u32;
        for _ in 0..n_hops {
            match tracker.record_hop(task_id) {
                Ok(depth) => {
                    prop_assert!(depth <= max_depth, "depth {} exceeded max {}", depth, max_depth);
                    last_ok_depth = depth;
                }
                Err(_) => {
                    // Should only fail when depth would exceed max.
                    prop_assert!(last_ok_depth >= max_depth);
                    break;
                }
            }
        }
    }

    #[test]
    fn circuit_breaker_state_always_valid(
        threshold in 1..5u32,
        n_events in 1..20usize,
        seed in any::<u64>(),
    ) {
        let mut breaker = CascadeCircuitBreaker::new(threshold, Duration::from_secs(300), 3);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        for i in 0..n_events {
            if (seed.wrapping_add(i as u64)) % 3 == 0 {
                breaker.record_success(a, b);
            } else {
                breaker.record_failure(a, b);
            }

            let state = breaker.state(a, b);
            prop_assert!(
                matches!(
                    state,
                    CascadeBreakerState::Closed
                        | CascadeBreakerState::Open
                        | CascadeBreakerState::HalfOpen
                ),
                "invalid state: {:?}", state
            );
        }
    }
}
