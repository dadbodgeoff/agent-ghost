//! Simulation boundary prompt — compiled into binary (Req 8 AC4).

/// The simulation boundary prompt, compiled into the binary via include_str!.
pub const SIMULATION_BOUNDARY_PROMPT: &str = include_str!("../prompts/simulation_boundary_v1.txt");

/// Version string for prompt tracking.
pub const SIMULATION_BOUNDARY_VERSION: &str = "v1.1.0";
