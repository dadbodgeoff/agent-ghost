//! OpenTelemetry OTLP transport (Req 4 AC4).
//!
//! Feature-gated: only compiled when `otel` feature is enabled.
//! Maps ITP events to OpenTelemetry spans with `itp.*` attributes.

#[cfg(feature = "otel")]
use crate::adapter::ITPAdapter;
#[cfg(feature = "otel")]
use crate::events::*;

/// OTel transport stub — maps ITP events to OTel spans.
///
/// When the `otel` feature is enabled, this transport sends ITP events
/// as OpenTelemetry spans with `itp.*` prefixed attributes to an OTLP
/// collector endpoint.
#[cfg(feature = "otel")]
pub struct OtelTransport {
    endpoint: String,
}

#[cfg(feature = "otel")]
impl OtelTransport {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[cfg(feature = "otel")]
impl ITPAdapter for OtelTransport {
    fn on_session_start(&self, event: &SessionStartEvent) {
        // In production: create OTel span with itp.session.id, itp.agent.id, etc.
        tracing::debug!(
            session_id = %event.session_id,
            agent_id = %event.agent_id,
            "OTel: session_start span"
        );
    }

    fn on_message(&self, event: &InteractionMessageEvent) {
        tracing::debug!(
            session_id = %event.session_id,
            message_id = %event.message_id,
            "OTel: interaction_message span"
        );
    }

    fn on_session_end(&self, event: &SessionEndEvent) {
        tracing::debug!(
            session_id = %event.session_id,
            reason = %event.reason,
            "OTel: session_end span"
        );
    }

    fn on_agent_state(&self, event: &AgentStateSnapshotEvent) {
        tracing::debug!(
            session_id = %event.session_id,
            agent_id = %event.agent_id,
            "OTel: agent_state_snapshot span"
        );
    }
}
