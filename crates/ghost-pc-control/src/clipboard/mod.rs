//! Clipboard skills: read and write system clipboard.
//!
//! | Skill            | Risk   | Autonomy Default       | Convergence Max |
//! |------------------|--------|------------------------|-----------------|
//! | `clipboard_read` | Medium | Act with Confirmation  | Level 2         |
//! | `clipboard_write`| Medium | Act with Confirmation  | Level 3         |
//!
//! ## Status
//!
//! All clipboard skills are stubs — `arboard` integration in Week 5.

pub mod clipboard_read;
pub mod clipboard_write;
