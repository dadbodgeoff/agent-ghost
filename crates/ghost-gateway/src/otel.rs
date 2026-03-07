//! WP9-A: OpenTelemetry tracing exporter (behind `otel` feature flag).
//!
//! Initializes an OTLP gRPC exporter and installs a `tracing-opentelemetry`
//! layer alongside the existing `tracing_subscriber::fmt` layer.
//! Call `init_otel_tracing()` early in startup (before bootstrap).

use opentelemetry::trace::TracerProvider as TracerProviderTrait;
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::OtelConfig;

/// Opaque guard — drop to flush and shut down the OTEL exporter.
pub struct OtelGuard {
    provider: TracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("OTEL shutdown error: {e}");
        }
    }
}

/// Initialize tracing with both fmt and OTEL layers.
///
/// Returns an `OtelGuard` that must be held for the process lifetime.
/// On drop, remaining spans are flushed to the collector.
pub fn init_otel_tracing(config: &OtelConfig) -> Result<OtelGuard, Box<dyn std::error::Error>> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()?;

    // opentelemetry_sdk 0.27: Resource::new() with KeyValue vec,
    // with_batch_exporter takes (exporter, runtime).
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
    ]);

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("ghost-gateway");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    tracing::info!(
        endpoint = %config.endpoint,
        service = %config.service_name,
        "OpenTelemetry tracing initialized"
    );

    Ok(OtelGuard { provider })
}
