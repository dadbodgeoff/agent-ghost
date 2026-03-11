use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use ghost_channels::adapter::ChannelAdapter;
use tokio::sync::{broadcast, Mutex as TokioMutex};
use uuid::Uuid;

use crate::agents::registry::AgentRegistry;
use crate::api::websocket::{EventReplayBuffer, WsEnvelope, WsEvent};
use crate::config::GhostConfig;
use crate::db_pool::DbPool;

pub const SUPPORTED_CHANNEL_TYPES: &[&str] = &[
    "cli",
    "websocket",
    "telegram",
    "discord",
    "slack",
    "whatsapp",
];

#[derive(Debug, Clone)]
pub struct ChannelRecord {
    pub id: String,
    pub channel_type: String,
    pub status: String,
    pub status_message: Option<String>,
    pub agent_id: String,
    pub routing_key: String,
    pub source: String,
    pub config: serde_json::Value,
    pub last_message_at: Option<String>,
    pub message_count: i64,
    pub updated_at: String,
}

struct ActiveChannel {
    routing_key: String,
    adapter: Box<dyn ChannelAdapter>,
}

pub struct ChannelManager {
    db: Arc<DbPool>,
    agents: Arc<RwLock<AgentRegistry>>,
    event_tx: broadcast::Sender<WsEnvelope>,
    replay_buffer: Arc<EventReplayBuffer>,
    active: Arc<TokioMutex<HashMap<String, ActiveChannel>>>,
}

impl ChannelManager {
    pub fn new(
        db: Arc<DbPool>,
        agents: Arc<RwLock<AgentRegistry>>,
        event_tx: broadcast::Sender<WsEnvelope>,
        replay_buffer: Arc<EventReplayBuffer>,
    ) -> Self {
        Self {
            db,
            agents,
            event_tx,
            replay_buffer,
            active: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub fn supported_channel_types() -> &'static [&'static str] {
        SUPPORTED_CHANNEL_TYPES
    }

    pub fn validate_channel_type(channel_type: &str) -> Result<(), String> {
        if SUPPORTED_CHANNEL_TYPES.contains(&channel_type) {
            Ok(())
        } else {
            Err(format!(
                "unsupported channel_type '{channel_type}' (supported: {})",
                SUPPORTED_CHANNEL_TYPES.join(", ")
            ))
        }
    }

    pub fn normalize_config(config: Option<serde_json::Value>) -> serde_json::Value {
        match config {
            Some(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
            Some(serde_json::Value::Null) | None => serde_json::json!({}),
            Some(other) => serde_json::json!({ "value": other }),
        }
    }

    pub fn derive_routing_key(
        channel_type: &str,
        agent_id: &str,
        config: &serde_json::Value,
    ) -> Result<String, String> {
        Self::validate_channel_type(channel_type)?;
        let cfg = config
            .as_object()
            .ok_or_else(|| "channel config must be a JSON object".to_string())?;

        let string_value = |key: &str| cfg.get(key).and_then(|value| value.as_str());
        let u64_value = |key: &str| cfg.get(key).and_then(|value| value.as_u64());

        let routing_key = match channel_type {
            "cli" => format!("cli:{agent_id}"),
            "websocket" => {
                let bind = string_value("bind").unwrap_or("127.0.0.1");
                let port = u64_value("port").unwrap_or(18789);
                format!("websocket:{bind}:{port}")
            }
            "telegram" => format!(
                "telegram:bot:{}",
                fingerprint(string_value("bot_token").unwrap_or(agent_id))
            ),
            "discord" => match string_value("guild_id") {
                Some(guild_id) if !guild_id.trim().is_empty() => {
                    format!("discord:guild:{guild_id}")
                }
                _ => format!(
                    "discord:bot:{}",
                    fingerprint(string_value("bot_token").unwrap_or(agent_id))
                ),
            },
            "slack" => match string_value("workspace") {
                Some(workspace) if !workspace.trim().is_empty() => {
                    format!("slack:workspace:{workspace}")
                }
                _ => format!(
                    "slack:app:{}",
                    fingerprint(string_value("app_token").unwrap_or(agent_id))
                ),
            },
            "whatsapp" => match string_value("phone_number_id") {
                Some(phone_number_id) if !phone_number_id.trim().is_empty() => {
                    format!("whatsapp:cloud:{phone_number_id}")
                }
                _ => "whatsapp:sidecar:default".to_string(),
            },
            _ => return Err(format!("unsupported channel_type '{channel_type}'")),
        };

        Ok(routing_key)
    }

    pub async fn reconcile_config_channels(&self, config: &GhostConfig) -> Result<(), String> {
        let agent_names = self
            .agents
            .read()
            .map_err(|_| "agent registry lock poisoned".to_string())?
            .all_agents()
            .into_iter()
            .map(|agent| (agent.name.clone(), agent.id.to_string()))
            .collect::<HashMap<_, _>>();

        let mut imported_keys = HashSet::new();
        let db = self.db.write().await;
        for channel in &config.channels {
            Self::validate_channel_type(&channel.channel_type)?;
            let agent_id = agent_names
                .get(&channel.agent)
                .cloned()
                .ok_or_else(|| format!("channel references unknown agent '{}'", channel.agent))?;
            let config_json =
                serde_json::to_value(&channel.options).unwrap_or_else(|_| serde_json::json!({}));
            let routing_key =
                Self::derive_routing_key(&channel.channel_type, &agent_id, &config_json)?;
            imported_keys.insert(routing_key.clone());

            let existing_id: Option<String> = db
                .query_row(
                    "SELECT id FROM channels WHERE routing_key = ?1 LIMIT 1",
                    [routing_key.as_str()],
                    |row| row.get(0),
                )
                .ok();

            if let Some(existing_id) = existing_id {
                db.execute(
                    "UPDATE channels
                     SET channel_type = ?2,
                         agent_id = ?3,
                         routing_key = ?4,
                         source = 'imported_config',
                         status = 'configuring',
                         status_message = NULL,
                         config = ?5,
                         updated_at = datetime('now')
                     WHERE id = ?1",
                    rusqlite::params![
                        existing_id,
                        channel.channel_type,
                        agent_id,
                        routing_key,
                        config_json.to_string(),
                    ],
                )
                .map_err(|error| format!("reconcile imported channel update: {error}"))?;
            } else {
                db.execute(
                    "INSERT INTO channels (
                        id,
                        channel_type,
                        status,
                        agent_id,
                        routing_key,
                        source,
                        config
                    ) VALUES (?1, ?2, 'configuring', ?3, ?4, 'imported_config', ?5)",
                    rusqlite::params![
                        Uuid::now_v7().to_string(),
                        channel.channel_type,
                        agent_id,
                        routing_key,
                        config_json.to_string(),
                    ],
                )
                .map_err(|error| format!("reconcile imported channel insert: {error}"))?;
            }
        }

        let stale_rows = {
            let mut stmt = db
                .prepare("SELECT id, routing_key FROM channels WHERE source = 'imported_config'")
                .map_err(|error| format!("prepare stale imported channels: {error}"))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| format!("query stale imported channels: {error}"))?;
            let stale_rows = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("read stale imported channels: {error}"))?;
            stale_rows
        };

        for (channel_id, routing_key) in stale_rows {
            if imported_keys.contains(&routing_key) {
                continue;
            }
            db.execute(
                "UPDATE channels
                 SET status = 'disconnected',
                     status_message = 'Configuration entry removed from ghost.yml',
                     updated_at = datetime('now')
                 WHERE id = ?1",
                [channel_id],
            )
            .map_err(|error| format!("mark stale imported channel: {error}"))?;
        }

        Ok(())
    }

    pub async fn activate_all(&self) -> Result<(), String> {
        {
            let mut agents = self
                .agents
                .write()
                .map_err(|_| "agent registry lock poisoned".to_string())?;
            agents.clear_channel_bindings();
        }
        self.active.lock().await.clear();

        let channels = self.load_channels()?;
        for channel in channels {
            if !should_activate_on_boot(&channel) {
                continue;
            }
            let _ = self.activate_channel(&channel.id, false).await;
        }
        Ok(())
    }

    pub async fn activate_channel(
        &self,
        channel_id: &str,
        emit_created: bool,
    ) -> Result<ChannelRecord, String> {
        let record = self
            .load_channel(channel_id)?
            .ok_or_else(|| format!("channel {channel_id} not found"))?;
        let agent_uuid = Uuid::parse_str(&record.agent_id)
            .map_err(|error| format!("invalid stored agent id: {error}"))?;

        let agent_exists = {
            let agents = self
                .agents
                .read()
                .map_err(|_| "agent registry lock poisoned".to_string())?;
            let exists = agents.lookup_by_id(agent_uuid).is_some();
            drop(agents);
            exists
        };
        if !agent_exists {
            self.persist_status(
                &record.id,
                "error",
                Some(format!(
                    "agent {} not found in live registry",
                    record.agent_id
                )),
            )
            .await?;
            return Err(format!(
                "agent {} not found in live registry",
                record.agent_id
            ));
        }

        match build_adapter(&record.channel_type, &record.config) {
            Ok(mut adapter) => match adapter.connect().await {
                Ok(()) => {
                    {
                        let mut active = self.active.lock().await;
                        active.insert(
                            record.id.clone(),
                            ActiveChannel {
                                routing_key: record.routing_key.clone(),
                                adapter,
                            },
                        );
                    }
                    {
                        let mut agents = self
                            .agents
                            .write()
                            .map_err(|_| "agent registry lock poisoned".to_string())?;
                        agents.bind_channel(record.routing_key.clone(), agent_uuid)?;
                    }
                    self.persist_status(&record.id, "connected", None).await?;
                    let updated = self.load_channel(channel_id)?.ok_or_else(|| {
                        format!("channel {channel_id} disappeared after activation")
                    })?;
                    if emit_created {
                        self.broadcast_channel_created(&updated);
                    } else {
                        self.broadcast_channel_status_changed(&updated);
                    }
                    Ok(updated)
                }
                Err(error) => {
                    self.persist_status(&record.id, "error", Some(error.clone()))
                        .await?;
                    let updated = self
                        .load_channel(channel_id)?
                        .ok_or_else(|| format!("channel {channel_id} disappeared after error"))?;
                    self.broadcast_channel_status_changed(&updated);
                    Err(error)
                }
            },
            Err(error) => {
                self.persist_status(&record.id, "error", Some(error.clone()))
                    .await?;
                let updated = self
                    .load_channel(channel_id)?
                    .ok_or_else(|| format!("channel {channel_id} disappeared after error"))?;
                self.broadcast_channel_status_changed(&updated);
                Err(error)
            }
        }
    }

    pub async fn reconnect_channel(&self, channel_id: &str) -> Result<ChannelRecord, String> {
        self.deactivate_channel(channel_id, false).await?;
        self.activate_channel(channel_id, false).await
    }

    pub async fn deactivate_channel(
        &self,
        channel_id: &str,
        emit_status: bool,
    ) -> Result<Option<ChannelRecord>, String> {
        let record = self.load_channel(channel_id)?;
        let routing_key = record
            .as_ref()
            .map(|row| row.routing_key.clone())
            .or_else(|| {
                self.active.try_lock().ok().and_then(|active| {
                    active
                        .get(channel_id)
                        .map(|entry| entry.routing_key.clone())
                })
            });

        if let Some(mut active_channel) = self.active.lock().await.remove(channel_id) {
            let _ = active_channel.adapter.disconnect().await;
        }

        if let Some(routing_key) = routing_key {
            if let Ok(mut agents) = self.agents.write() {
                agents.unbind_channel(&routing_key);
            }
        }

        if let Some(record) = record {
            self.persist_status(&record.id, "disconnected", None)
                .await?;
            let updated = self
                .load_channel(channel_id)?
                .ok_or_else(|| format!("channel {channel_id} disappeared after deactivate"))?;
            if emit_status {
                self.broadcast_channel_status_changed(&updated);
            }
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    pub async fn remove_channel_runtime(&self, channel: &ChannelRecord) -> Result<(), String> {
        let _ = self.deactivate_channel(&channel.id, false).await?;
        self.broadcast_channel_deleted(channel);
        Ok(())
    }

    pub fn load_channel(&self, channel_id: &str) -> Result<Option<ChannelRecord>, String> {
        let db = self
            .db
            .read()
            .map_err(|error| format!("channel db read lock: {error}"))?;
        let mut stmt = db
            .prepare(
                "SELECT id, channel_type, status, status_message, agent_id, routing_key, source,
                        config, last_message_at, message_count, updated_at
                 FROM channels WHERE id = ?1 LIMIT 1",
            )
            .map_err(|error| format!("prepare load channel: {error}"))?;
        stmt.query_row([channel_id], map_channel_row)
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(format!("load channel {channel_id}: {other}")),
            })
    }

    pub fn load_channels(&self) -> Result<Vec<ChannelRecord>, String> {
        let db = self
            .db
            .read()
            .map_err(|error| format!("channel db read lock: {error}"))?;
        let channels = {
            let mut stmt = db
                .prepare(
                    "SELECT id, channel_type, status, status_message, agent_id, routing_key, source,
                            config, last_message_at, message_count, updated_at
                     FROM channels ORDER BY channel_type, routing_key",
                )
                .map_err(|error| format!("prepare list channels: {error}"))?;
            let rows = stmt
                .query_map([], map_channel_row)
                .map_err(|error| format!("query list channels: {error}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("collect list channels: {error}"))?
        };
        Ok(channels)
    }

    async fn persist_status(
        &self,
        channel_id: &str,
        status: &str,
        status_message: Option<String>,
    ) -> Result<(), String> {
        let db = self.db.write().await;
        db.execute(
            "UPDATE channels
             SET status = ?2,
                 status_message = ?3,
                 updated_at = datetime('now')
             WHERE id = ?1",
            rusqlite::params![channel_id, status, status_message],
        )
        .map_err(|error| format!("persist channel status: {error}"))?;
        Ok(())
    }

    fn broadcast_channel_created(&self, channel: &ChannelRecord) {
        self.broadcast(WsEvent::ChannelCreated {
            channel_id: channel.id.clone(),
            channel_type: channel.channel_type.clone(),
            agent_id: channel.agent_id.clone(),
            routing_key: channel.routing_key.clone(),
            status: channel.status.clone(),
            status_message: channel.status_message.clone(),
            updated_at: channel.updated_at.clone(),
        });
    }

    fn broadcast_channel_deleted(&self, channel: &ChannelRecord) {
        self.broadcast(WsEvent::ChannelDeleted {
            channel_id: channel.id.clone(),
            channel_type: channel.channel_type.clone(),
            agent_id: channel.agent_id.clone(),
            routing_key: channel.routing_key.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    fn broadcast_channel_status_changed(&self, channel: &ChannelRecord) {
        self.broadcast(WsEvent::ChannelStatusChanged {
            channel_id: channel.id.clone(),
            channel_type: channel.channel_type.clone(),
            agent_id: channel.agent_id.clone(),
            routing_key: channel.routing_key.clone(),
            status: channel.status.clone(),
            status_message: channel.status_message.clone(),
            updated_at: channel.updated_at.clone(),
        });
    }

    fn broadcast(&self, event: WsEvent) {
        let _ = self.replay_buffer.push_and_broadcast(event, &self.event_tx);
    }
}

fn fingerprint(value: &str) -> String {
    let hash = blake3::hash(value.as_bytes()).to_hex();
    hash[..10].to_string()
}

fn config_string(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .as_object()
        .and_then(|map| map.get(key))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn config_u64(config: &serde_json::Value, key: &str) -> Option<u64> {
    config
        .as_object()
        .and_then(|map| map.get(key))
        .and_then(|value| value.as_u64())
}

fn build_adapter(
    channel_type: &str,
    config: &serde_json::Value,
) -> Result<Box<dyn ChannelAdapter>, String> {
    match channel_type {
        "cli" => Ok(Box::new(ghost_channels::adapters::cli::CliAdapter::new())),
        "websocket" => {
            let bind = config_string(config, "bind").unwrap_or_else(|| "127.0.0.1".to_string());
            let port = config_u64(config, "port").unwrap_or(18789);
            Ok(Box::new(
                ghost_channels::adapters::websocket::WebSocketAdapter::new(&format!(
                    "{bind}:{port}"
                )),
            ))
        }
        "telegram" => Ok(Box::new(
            ghost_channels::adapters::telegram::TelegramAdapter::new(
                &config_string(config, "bot_token").unwrap_or_default(),
            ),
        )),
        "discord" => Ok(Box::new(
            ghost_channels::adapters::discord::DiscordAdapter::new(
                &config_string(config, "bot_token").unwrap_or_default(),
            ),
        )),
        "slack" => Ok(Box::new(
            ghost_channels::adapters::slack::SlackAdapter::new(
                &config_string(config, "bot_token").unwrap_or_default(),
                &config_string(config, "app_token").unwrap_or_default(),
            ),
        )),
        "whatsapp" => {
            let access_token = config_string(config, "access_token").unwrap_or_default();
            let phone_number_id = config_string(config, "phone_number_id").unwrap_or_default();
            if !access_token.is_empty() && !phone_number_id.is_empty() {
                Ok(Box::new(
                    ghost_channels::adapters::whatsapp::WhatsAppAdapter::new_cloud_api(
                        &access_token,
                        &phone_number_id,
                    ),
                ))
            } else {
                Ok(Box::new(
                    ghost_channels::adapters::whatsapp::WhatsAppAdapter::new_sidecar(),
                ))
            }
        }
        _ => Err(format!("unsupported channel_type '{channel_type}'")),
    }
}

fn map_channel_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelRecord> {
    Ok(ChannelRecord {
        id: row.get::<_, String>(0)?,
        channel_type: row.get::<_, String>(1)?,
        status: row.get::<_, String>(2)?,
        status_message: row.get::<_, Option<String>>(3)?,
        agent_id: row.get::<_, String>(4)?,
        routing_key: row.get::<_, String>(5)?,
        source: row.get::<_, String>(6)?,
        config: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(7)?)
            .unwrap_or_else(|_| serde_json::json!({})),
        last_message_at: row.get::<_, Option<String>>(8)?,
        message_count: row.get::<_, i64>(9)?,
        updated_at: row.get::<_, String>(10)?,
    })
}

fn should_activate_on_boot(channel: &ChannelRecord) -> bool {
    channel.status != "disconnected"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_routing_key_rejects_unsupported_types() {
        let error =
            ChannelManager::derive_routing_key("webhook", "agent-1", &serde_json::json!({}))
                .unwrap_err();
        assert!(error.contains("unsupported channel_type"), "{error}");
    }

    #[test]
    fn normalize_config_wraps_non_objects() {
        let normalized = ChannelManager::normalize_config(Some(serde_json::json!("value")));
        assert_eq!(normalized, serde_json::json!({ "value": "value" }));
    }

    #[test]
    fn boot_activation_skips_disconnected_channels() {
        let connected = ChannelRecord {
            id: "channel-1".into(),
            channel_type: "cli".into(),
            status: "connected".into(),
            status_message: None,
            agent_id: "agent-1".into(),
            routing_key: "cli:agent-1".into(),
            source: "operator_created".into(),
            config: serde_json::json!({}),
            last_message_at: None,
            message_count: 0,
            updated_at: "2026-03-10T00:00:00Z".into(),
        };
        let disconnected = ChannelRecord {
            status: "disconnected".into(),
            ..connected.clone()
        };

        assert!(should_activate_on_boot(&connected));
        assert!(!should_activate_on_boot(&disconnected));
    }
}
