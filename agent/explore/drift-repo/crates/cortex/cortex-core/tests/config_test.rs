use cortex_core::config::*;

#[test]
fn config_loads_from_empty_toml_with_all_defaults() {
    let config = CortexConfig::from_toml("").unwrap();

    // Storage defaults
    assert_eq!(config.storage.db_path, "cortex.db");
    assert!(config.storage.wal_mode);
    assert_eq!(config.storage.mmap_size, 268_435_456);
    assert_eq!(config.storage.cache_size, -64_000);
    assert_eq!(config.storage.busy_timeout_ms, 5_000);
    assert_eq!(config.storage.read_pool_size, 4);

    // Embedding defaults
    assert_eq!(config.embedding.provider, "onnx");
    assert_eq!(config.embedding.dimensions, 1024);
    assert_eq!(config.embedding.matryoshka_search_dims, 384);
    assert_eq!(config.embedding.batch_size, 50);

    // Retrieval defaults
    assert_eq!(config.retrieval.default_budget, 2000);
    assert_eq!(config.retrieval.rrf_k, 60);
    assert!(!config.retrieval.query_expansion);

    // Consolidation defaults
    assert_eq!(config.consolidation.min_cluster_size, 2);
    assert_eq!(config.consolidation.novelty_threshold, 0.85);

    // Decay defaults
    assert_eq!(config.decay.archival_threshold, 0.15);

    // Privacy defaults
    assert!(!config.privacy.ner_enabled);
    assert!(config.privacy.context_scoring);

    // Cloud defaults
    assert!(config.cloud.offline_mode);

    // Observability defaults
    assert_eq!(config.observability.log_level, "info");
    assert!(!config.observability.tracing_enabled);
}

#[test]
fn config_loads_partial_toml_with_overrides() {
    let toml = r#"
[storage]
db_path = "/custom/path.db"
read_pool_size = 8

[retrieval]
default_budget = 4000
"#;
    let config = CortexConfig::from_toml(toml).unwrap();
    assert_eq!(config.storage.db_path, "/custom/path.db");
    assert_eq!(config.storage.read_pool_size, 8);
    // Non-overridden fields keep defaults
    assert!(config.storage.wal_mode);
    assert_eq!(config.retrieval.default_budget, 4000);
    assert_eq!(config.retrieval.rrf_k, 60); // default
}

#[test]
fn config_serde_roundtrip() {
    let config = CortexConfig::default();
    let toml_str = toml::to_string(&config).unwrap();
    let roundtripped = CortexConfig::from_toml(&toml_str).unwrap();
    assert_eq!(roundtripped.storage.db_path, config.storage.db_path);
    assert_eq!(
        roundtripped.embedding.dimensions,
        config.embedding.dimensions
    );
}
