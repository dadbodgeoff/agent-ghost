//! ghost channel — messaging channel management (T-1.9.1, T-1.9.2, T-4.6.2, §4.1).

use serde::Serialize;
use std::collections::BTreeMap;

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{print_output, OutputFormat, TableDisplay};

// ─── ghost channel list ──────────────────────────────────────────────────────

pub struct ChannelListArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize)]
pub struct ChannelListEntry {
    pub channel: String,
    pub agent: String,
    pub mode: String,
    pub streaming: bool,
    pub editing: bool,
    pub credentials: String,
}

#[derive(Serialize)]
struct ChannelList {
    channels: Vec<ChannelListEntry>,
}

impl TableDisplay for ChannelList {
    fn print_table(&self) {
        if self.channels.is_empty() {
            eprintln!(
                "No channels configured. Add a `channels:` block to ghost.yml or run `ghost init`."
            );
            return;
        }
        println!(
            "{:<12}  {:<16}  {:<12}  {:<9}  {:<7}  {}",
            "CHANNEL", "AGENT", "MODE", "STREAMING", "EDITING", "CREDENTIALS"
        );
        println!("{}", "─".repeat(80));
        for c in &self.channels {
            let streaming = if c.streaming { "yes" } else { "no" };
            let editing = if c.editing { "yes" } else { "no" };
            println!(
                "{:<12}  {:<16}  {:<12}  {:<9}  {:<7}  {}",
                c.channel, c.agent, c.mode, streaming, editing, c.credentials
            );
        }
        println!("\n{} channel(s) configured.", self.channels.len());
    }
}

/// Derive channel display properties from channel_type and options.
fn derive_channel_props(
    channel_type: &str,
    options: &BTreeMap<String, serde_json::Value>,
) -> (String, bool, bool) {
    match channel_type {
        "telegram" => ("bot".to_string(), true, true),
        "whatsapp" => {
            let has_phone_id = options.contains_key("phone_number_id");
            if has_phone_id {
                ("cloud-api".to_string(), false, false)
            } else {
                ("sidecar".to_string(), false, false)
            }
        }
        "slack" => ("socket-mode".to_string(), false, true),
        "discord" => ("gateway".to_string(), false, true),
        "cli" => ("stdio".to_string(), true, false),
        _ => ("unknown".to_string(), false, false),
    }
}

/// Summarize credential keys present in options.
fn summarize_credentials(options: &BTreeMap<String, serde_json::Value>) -> String {
    let cred_keys: Vec<&str> = options
        .keys()
        .filter(|k| k.ends_with("_key") || k.ends_with("_token"))
        .map(|k| k.as_str())
        .collect();
    if cred_keys.is_empty() {
        "-".to_string()
    } else {
        format!("{}, \u{2713}", cred_keys.join(", "))
    }
}

/// Run `ghost channel list`.
pub async fn run_list(
    args: ChannelListArgs,
    config: &crate::config::GhostConfig,
) -> Result<(), CliError> {
    let channels: Vec<ChannelListEntry> = config
        .channels
        .iter()
        .map(|ch| {
            let (mode, streaming, editing) = derive_channel_props(&ch.channel_type, &ch.options);
            let credentials = summarize_credentials(&ch.options);
            ChannelListEntry {
                channel: ch.channel_type.clone(),
                agent: ch.agent.clone(),
                mode,
                streaming,
                editing,
                credentials,
            }
        })
        .collect();

    print_output(&ChannelList { channels }, args.output);
    Ok(())
}

// ─── ghost channel test ──────────────────────────────────────────────────────

pub struct ChannelTestArgs {
    pub channel_type: Option<String>,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize)]
pub struct ProbeResult {
    pub channel: String,
    pub agent: String,
    pub status: String,
    pub detail: String,
}

#[derive(Serialize)]
struct TestResults {
    results: Vec<ProbeResult>,
}

impl TableDisplay for TestResults {
    fn print_table(&self) {
        if self.results.is_empty() {
            eprintln!("No channels to test.");
            return;
        }
        for r in &self.results {
            let icon = match r.status.as_str() {
                "ok" => "\u{2713}",
                "warning" => "~",
                _ => "\u{2717}",
            };
            println!("{} {} \u{2192} {}: {}", icon, r.channel, r.agent, r.detail);
        }
    }
}

/// Probe a single channel's external API.
pub async fn probe_channel(
    entry: &crate::config::ChannelConfig,
    _provider: Option<&dyn ghost_secrets::SecretProvider>,
) -> ProbeResult {
    let channel = entry.channel_type.clone();
    let agent = entry.agent.clone();

    // Resolve credentials from options.
    let resolve_key = |key_name: &str| -> Option<String> {
        // Check for _key reference first (e.g., bot_token_key → env var name).
        if let Some(env_key) = entry.options.get(&format!("{key_name}_key")) {
            if let Some(env_name) = env_key.as_str() {
                if let Ok(val) = std::env::var(env_name) {
                    return Some(val);
                }
            }
        }
        // Fallback: literal value in options.
        entry
            .options
            .get(key_name)
            .and_then(|v| v.as_str().map(String::from))
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    match channel.as_str() {
        "telegram" => {
            let token = match resolve_key("bot_token") {
                Some(t) => t,
                None => {
                    return ProbeResult {
                        channel,
                        agent,
                        status: "error".into(),
                        detail: "credential not found: bot_token".into(),
                    };
                }
            };
            let url = format!("https://api.telegram.org/bot{token}/getMe");
            match client.get(&url).send().await {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    if body["ok"].as_bool() == Some(true) {
                        let username = body["result"]["username"].as_str().unwrap_or("?");
                        let id = body["result"]["id"].as_i64().unwrap_or(0);
                        ProbeResult {
                            channel,
                            agent,
                            status: "ok".into(),
                            detail: format!("@{username} (id={id})"),
                        }
                    } else {
                        let desc = body["description"].as_str().unwrap_or("unknown error");
                        ProbeResult {
                            channel,
                            agent,
                            status: "error".into(),
                            detail: desc.to_string(),
                        }
                    }
                }
                Err(e) => ProbeResult {
                    channel,
                    agent,
                    status: "error".into(),
                    detail: format!("connection failed ({e})"),
                },
            }
        }
        "whatsapp" => {
            let has_phone_id = entry.options.contains_key("phone_number_id");
            if !has_phone_id {
                // Sidecar mode — cannot probe.
                return ProbeResult {
                    channel,
                    agent,
                    status: "warning".into(),
                    detail: "sidecar mode \u{2014} cannot probe remotely. Ensure Node.js/Baileys process is running.".into(),
                };
            }
            let token = match resolve_key("access_token") {
                Some(t) => t,
                None => {
                    return ProbeResult {
                        channel,
                        agent,
                        status: "error".into(),
                        detail: "credential not found: access_token".into(),
                    };
                }
            };
            let phone_id = entry.options["phone_number_id"].as_str().unwrap_or("");
            let url = format!(
                "https://graph.facebook.com/v18.0/{phone_id}?fields=display_phone_number,verified_name&access_token={token}"
            );
            match client.get(&url).send().await {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    if let Some(phone) = body["display_phone_number"].as_str() {
                        let name = body["verified_name"].as_str().unwrap_or("?");
                        ProbeResult {
                            channel,
                            agent,
                            status: "ok".into(),
                            detail: format!("{phone} ({name})"),
                        }
                    } else {
                        let msg = body["error"]["message"].as_str().unwrap_or("unknown error");
                        ProbeResult {
                            channel,
                            agent,
                            status: "error".into(),
                            detail: msg.to_string(),
                        }
                    }
                }
                Err(e) => ProbeResult {
                    channel,
                    agent,
                    status: "error".into(),
                    detail: format!("connection failed ({e})"),
                },
            }
        }
        "slack" => {
            let token = match resolve_key("bot_token") {
                Some(t) => t,
                None => {
                    return ProbeResult {
                        channel,
                        agent,
                        status: "error".into(),
                        detail: "credential not found: bot_token".into(),
                    };
                }
            };
            match client
                .post("https://slack.com/api/auth.test")
                .bearer_auth(&token)
                .send()
                .await
            {
                Ok(resp) => {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    if body["ok"].as_bool() == Some(true) {
                        let user = body["user"].as_str().unwrap_or("?");
                        let team = body["team"].as_str().unwrap_or("?");
                        ProbeResult {
                            channel,
                            agent,
                            status: "ok".into(),
                            detail: format!("{user} @ {team}"),
                        }
                    } else {
                        let err = body["error"].as_str().unwrap_or("unknown error");
                        ProbeResult {
                            channel,
                            agent,
                            status: "error".into(),
                            detail: err.to_string(),
                        }
                    }
                }
                Err(e) => ProbeResult {
                    channel,
                    agent,
                    status: "error".into(),
                    detail: format!("connection failed ({e})"),
                },
            }
        }
        "discord" => {
            let token = match resolve_key("bot_token") {
                Some(t) => t,
                None => {
                    return ProbeResult {
                        channel,
                        agent,
                        status: "error".into(),
                        detail: "credential not found: bot_token".into(),
                    };
                }
            };
            match client
                .get("https://discord.com/api/v10/users/@me")
                .header("Authorization", format!("Bot {token}"))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let username = body["username"].as_str().unwrap_or("?");
                        let disc = body["discriminator"].as_str().unwrap_or("0");
                        ProbeResult {
                            channel,
                            agent,
                            status: "ok".into(),
                            detail: format!("@{username}#{disc}"),
                        }
                    } else {
                        ProbeResult {
                            channel,
                            agent,
                            status: "error".into(),
                            detail: "invalid token".into(),
                        }
                    }
                }
                Err(e) => ProbeResult {
                    channel,
                    agent,
                    status: "error".into(),
                    detail: format!("connection failed ({e})"),
                },
            }
        }
        "cli" => ProbeResult {
            channel,
            agent,
            status: "ok".into(),
            detail: "ready (no external dependency)".into(),
        },
        _ => ProbeResult {
            channel,
            agent,
            status: "warning".into(),
            detail: format!("unknown channel type '{}'", entry.channel_type),
        },
    }
}

/// Run `ghost channel test [<type>]`.
pub async fn run_test(
    args: ChannelTestArgs,
    config: &crate::config::GhostConfig,
) -> Result<(), CliError> {
    let targets: Vec<&crate::config::ChannelConfig> = if let Some(ref ct) = args.channel_type {
        config
            .channels
            .iter()
            .filter(|c| c.channel_type == *ct)
            .collect()
    } else {
        config.channels.iter().collect()
    };

    if targets.is_empty() {
        if args.channel_type.is_some() {
            return Err(CliError::NotFound(format!(
                "no channel of type '{}' configured",
                args.channel_type.as_deref().unwrap_or("")
            )));
        }
        eprintln!(
            "No channels configured. Add a `channels:` block to ghost.yml or run `ghost init`."
        );
        return Ok(());
    }

    let mut results = Vec::new();
    for entry in targets {
        let result = probe_channel(entry, None).await;
        results.push(result);
    }

    let has_error = results.iter().any(|r| r.status == "error");
    print_output(&TestResults { results }, args.output);

    if has_error {
        return Err(CliError::Http("one or more channel probes failed".into()));
    }
    Ok(())
}

// ─── ghost channel send ──────────────────────────────────────────────────────

pub struct ChannelSendArgs {
    pub channel_type: String,
    pub message: String,
    pub agent: Option<String>,
    pub sender: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct InjectRequest {
    content: String,
    sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
}

#[derive(Serialize)]
struct SendResult {
    message_id: String,
    agent_id: String,
    routed: bool,
}

impl TableDisplay for SendResult {
    fn print_table(&self) {
        println!(
            "Injected \u{2192} agent {} (message_id={})",
            self.agent_id, self.message_id
        );
    }
}

/// Run `ghost channel send <type> <message>`.
pub async fn run_send(args: ChannelSendArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let path = format!("/api/channels/{}/inject", args.channel_type);
    let body = InjectRequest {
        content: args.message,
        sender: args.sender,
        agent_id: args.agent.clone(),
    };

    let resp = client.post(&path, &body).await?;
    let result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse inject response: {e}")))?;

    let send_result = SendResult {
        message_id: result["message_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        agent_id: result["agent_id"].as_str().unwrap_or("unknown").to_string(),
        routed: result["routed"].as_bool().unwrap_or(false),
    };

    print_output(&send_result, args.output);

    // Advise operator how to observe the agent's response.
    eprintln!(
        "Use `ghost logs --agent {}` to monitor agent activity.",
        send_result.agent_id
    );

    Ok(())
}
