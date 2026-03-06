//! Input control skills: mouse, keyboard, scroll.
//!
//! All input skills are Medium risk, gated to convergence Level 2,
//! wrapped with `ConvergenceGuard`, and validated by `InputValidator`
//! and `PcControlCircuitBreaker` before execution.
//!
//! | Skill             | Risk   | Autonomy Default       | Convergence Max | Budget   |
//! |-------------------|--------|------------------------|-----------------|----------|
//! | `mouse_move`      | Medium | Act with Confirmation  | Level 2         | total    |
//! | `mouse_click`     | Medium | Act with Confirmation  | Level 2         | 200      |
//! | `mouse_drag`      | Medium | Act with Confirmation  | Level 2         | 20       |
//! | `keyboard_type`   | Medium | Act with Confirmation  | Level 2         | 500      |
//! | `keyboard_hotkey` | Medium | Act with Confirmation  | Level 2         | 50       |
//! | `keyboard_press`  | Medium | Act with Confirmation  | Level 2         | 500      |
//! | `scroll`          | Medium | Act with Confirmation  | Level 2         | total    |

pub mod keyboard_hotkey;
pub mod keyboard_press;
pub mod keyboard_type;
pub mod mouse_click;
pub mod mouse_drag;
pub mod mouse_move;
pub mod scroll;
