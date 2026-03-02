//! Confirmation prompts for destructive operations (Task 6.6 — §8).

use std::io::{self, BufRead, Write};

/// Prompt the user for confirmation.
///
/// Returns `true` if `yes_flag` is set or the user types "y"/"yes".
/// Prompt is written to stderr; response is read from stdin.
pub fn confirm(prompt: &str, yes_flag: bool) -> bool {
    if yes_flag {
        return true;
    }

    eprint!("{prompt} ");
    let _ = io::stderr().flush();

    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return false;
    }

    let answer = line.trim().to_lowercase();
    answer == "y" || answer == "yes"
}
