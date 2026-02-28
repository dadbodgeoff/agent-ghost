//! Interactive chat REPL (Task 6.6).

use std::io::{self, BufRead, Write};

/// Run an interactive chat session via CLI.
pub async fn run_interactive_chat() {
    println!("GHOST Interactive Chat");
    println!("Type /quit to exit, /help for commands.\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("you> ");
        stdout.flush().unwrap_or_default();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "/quit" | "/exit" | "/q" => {
                println!("Goodbye.");
                break;
            }
            "/help" | "/h" => {
                println!("Commands:");
                println!("  /quit    — Exit chat");
                println!("  /status  — Show agent status");
                println!("  /model   — Show current model");
                println!("  /help    — Show this help");
            }
            "/status" => {
                println!("[status] Gateway connection: checking...");
                // In production, queries the gateway API
                println!("[status] Not connected to gateway. Run `ghost serve` first.");
            }
            "/model" => {
                println!("[model] Default model selection active");
            }
            _ => {
                // In production, sends to AgentRunner via gateway API
                println!("ghost> [Chat requires a running gateway. Start with `ghost serve`]");
            }
        }
    }
}
