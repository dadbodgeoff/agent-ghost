//! Window management skills: list, focus, resize, launch, kill, processes.
//!
//! | Skill            | Risk   | Autonomy Default       | Convergence Max |
//! |------------------|--------|------------------------|-----------------|
//! | `list_windows`   | Low    | Act Autonomously       | Level 4         |
//! | `focus_window`   | Medium | Act with Confirmation  | Level 3         |
//! | `resize_window`  | Medium | Act with Confirmation  | Level 2         |
//! | `launch_app`     | Medium | Act with Confirmation  | Level 2         |
//! | `kill_process`   | High   | Plan and Propose       | Level 1         |
//! | `list_processes` | Low    | Act Autonomously       | Level 4         |
//!
//! ## Status
//!
//! All window skills are stubs — implementations in Week 5.

pub mod focus_window;
pub mod kill_process;
pub mod launch_app;
pub mod list_processes;
pub mod list_windows;
pub mod resize_window;
