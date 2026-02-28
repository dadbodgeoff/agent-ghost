//! Active learning loop: candidate selection, prompt generation, feedback processing.

pub mod candidate_selector;
pub mod feedback_processor;
pub mod prompt_generator;

pub use candidate_selector::{select_candidates, SelectionCriteria};
pub use feedback_processor::{process_feedback, Feedback, FeedbackResult};
pub use prompt_generator::{generate_prompt, ValidationPrompt};
