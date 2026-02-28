//! Config importer — maps OpenClaw config to ghost.yml format.

use std::path::Path;

use crate::{MigrateError, MigrateResult};

/// Import config, mapping to ghost.yml format.
pub fn import_config(source: &Path, target: &Path) -> MigrateResult<String> {
    let config_candidates = ["config.yml", "config.yaml", "openclaw.yml", "openclaw.yaml"];

    let config_path = config_candidates
        .iter()
        .map(|name| source.join(name))
        .find(|p| p.exists());

    let config_path = match config_path {
        Some(p) => p,
        None => {
            return Ok("No config file found, using defaults".to_string());
        }
    };

    let content = std::fs::read_to_string(&config_path)?;
    let source_config: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| MigrateError::ParseError(e.to_string()))?;

    // Map to ghost.yml structure
    let ghost_config = map_to_ghost_config(&source_config);

    let ghost_yml = serde_yaml::to_string(&ghost_config)
        .map_err(|e| MigrateError::ConversionError(e.to_string()))?;

    std::fs::write(target.join("ghost.yml"), &ghost_yml)?;

    Ok("Config imported to ghost.yml".to_string())
}

fn map_to_ghost_config(source: &serde_yaml::Value) -> serde_yaml::Value {
    let mut config = serde_yaml::Mapping::new();

    // Map gateway settings
    let mut gateway = serde_yaml::Mapping::new();
    gateway.insert(
        serde_yaml::Value::String("bind".into()),
        serde_yaml::Value::String("127.0.0.1".into()),
    );
    gateway.insert(
        serde_yaml::Value::String("port".into()),
        serde_yaml::Value::Number(18789.into()),
    );
    config.insert(
        serde_yaml::Value::String("gateway".into()),
        serde_yaml::Value::Mapping(gateway),
    );

    // Map agent settings if present
    if let Some(agent) = source.get("agent") {
        let mut agents = Vec::new();
        let mut agent_config = serde_yaml::Mapping::new();

        if let Some(name) = agent.get("name").and_then(|n| n.as_str()) {
            agent_config.insert(
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String(name.into()),
            );
        } else {
            agent_config.insert(
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String("migrated-agent".into()),
            );
        }

        agents.push(serde_yaml::Value::Mapping(agent_config));
        config.insert(
            serde_yaml::Value::String("agents".into()),
            serde_yaml::Value::Sequence(agents),
        );
    }

    serde_yaml::Value::Mapping(config)
}
