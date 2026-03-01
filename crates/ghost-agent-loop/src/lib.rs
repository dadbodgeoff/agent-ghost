//! # ghost-agent-loop
//!
//! Core agent runner with recursive loop, gate checks, 10-layer prompt
//! compilation, proposal extraction/routing, tool registry/executor,
//! and output inspection.
//!
//! Gate check order (HARD INVARIANT):
//! GATE 0: circuit breaker
//! GATE 1: recursion depth
//! GATE 1.5: damage counter
//! GATE 2: spending cap
//! GATE 3: kill switch
//! GATE 3.5: distributed kill gate (when enabled)

pub mod runner;
pub use runner::FlushExecutor;
pub mod circuit_breaker;
pub mod damage_counter;
pub mod itp_emitter;
pub mod response;
pub mod context;
pub mod proposal;
pub mod tools;
pub mod output_inspector;

/// Initialize the OpenTelemetry tracing pipeline.
///
/// When `GHOST_OTLP_ENDPOINT` is set, spans are exported to the OTLP collector.
/// Otherwise, spans flow only to the standard tracing subscriber (logs).
///
/// Call this once at startup before spawning any agent loops.
pub fn init_otel_tracing() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer().compact();

    if let Ok(endpoint) = std::env::var("GHOST_OTLP_ENDPOINT") {
        let sample_rate: f64 = std::env::var("GHOST_TRACE_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);

        match build_otel_provider(&endpoint, sample_rate) {
            Ok(provider) => {
                use opentelemetry::trace::TracerProvider as _;
                let tracer = provider.tracer("ghost-agent-loop");
                let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .with(otel_layer)
                    .init();
                // Keep the provider alive (drop shuts down export pipeline).
                std::mem::forget(provider);
                tracing::info!(endpoint = %endpoint, sample_rate, "OTel tracing initialized");
                return;
            }
            Err(e) => {
                eprintln!("Failed to initialize OTel: {e} — falling back to log-only");
            }
        }
    }

    // No OTLP endpoint — log-only tracing.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

fn build_otel_provider(
    endpoint: &str,
    sample_rate: f64,
) -> Result<opentelemetry_sdk::trace::TracerProvider, Box<dyn std::error::Error>> {
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, TracerProvider};

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let sampler = if (sample_rate - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else {
        Sampler::TraceIdRatioBased(sample_rate)
    };

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", "ghost-agent-loop"),
        ]))
        .build();

    Ok(provider)
}
