//! Event schema versioning via upcasters (CR2).

use cortex_storage::queries::event_ops::RawEvent;

/// Current schema version for events.
pub const CURRENT_SCHEMA_VERSION: u16 = 1;

/// Trait for upcasting events from older schema versions.
pub trait EventUpcaster: Send + Sync {
    fn can_upcast(&self, event_type: &str, schema_version: u16) -> bool;
    fn upcast(&self, event: RawEvent) -> RawEvent;
}

/// Registry of upcasters, applied in order on read.
pub struct UpcasterRegistry {
    upcasters: Vec<Box<dyn EventUpcaster>>,
}

impl UpcasterRegistry {
    pub fn new() -> Self {
        Self {
            upcasters: Vec::new(),
        }
    }

    /// Create with the default v1 identity upcaster.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(V1IdentityUpcaster));
        registry
    }

    pub fn register(&mut self, upcaster: Box<dyn EventUpcaster>) {
        self.upcasters.push(upcaster);
    }

    /// Upcast a raw event through all applicable upcasters.
    pub fn upcast_event(&self, mut event: RawEvent) -> RawEvent {
        if event.schema_version >= CURRENT_SCHEMA_VERSION {
            return event; // fast path — no upcasting needed
        }
        for upcaster in &self.upcasters {
            if upcaster.can_upcast(&event.event_type, event.schema_version) {
                event = upcaster.upcast(event);
            }
        }
        event
    }
}

impl Default for UpcasterRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// V1 identity upcaster — no-op for the initial schema version.
struct V1IdentityUpcaster;

impl EventUpcaster for V1IdentityUpcaster {
    fn can_upcast(&self, _event_type: &str, schema_version: u16) -> bool {
        schema_version < 1
    }

    fn upcast(&self, mut event: RawEvent) -> RawEvent {
        event.schema_version = 1;
        event
    }
}
