//! Status query command (Task 6.6).

/// Query and display gateway status.
pub async fn show_status(config_path: Option<&str>) {
    let base_url = "http://127.0.0.1:18789";

    println!("GHOST Platform Status");
    println!("─────────────────────");

    // Try to reach the gateway health endpoint
    match reqwest::get(format!("{}/api/health", base_url)).await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                if let Ok(body) = resp.text().await {
                    println!("Gateway:  RUNNING");
                    println!("Health:   {}", body);
                }
            } else {
                println!("Gateway:  ERROR (HTTP {})", status);
            }
        }
        Err(_) => {
            println!("Gateway:  NOT RUNNING");
            println!("          Start with: ghost serve");
        }
    }

    // Try monitor health
    match reqwest::get("http://127.0.0.1:18790/health").await {
        Ok(resp) if resp.status().is_success() => {
            println!("Monitor:  CONNECTED");
        }
        _ => {
            println!("Monitor:  NOT AVAILABLE");
        }
    }

    if let Some(path) = config_path {
        println!("Config:   {}", path);
    }
}
