use cortex_core::memory::{BaseMemory, Confidence};

/// Archival eligibility check.
///
/// A memory is eligible for archival when its confidence drops below
/// the archival threshold (0.15 by default). Already-archived memories
/// are skipped.
///
/// Default archival threshold from the spec.
pub const DEFAULT_ARCHIVAL_THRESHOLD: f64 = Confidence::ARCHIVAL;

/// Check if a memory should be archived based on its decayed confidence.
pub fn should_archive(decayed_confidence: f64, threshold: f64) -> bool {
    decayed_confidence < threshold
}

/// Archival decision with metadata for audit logging.
#[derive(Debug, Clone)]
pub struct ArchivalDecision {
    pub memory_id: String,
    pub should_archive: bool,
    pub decayed_confidence: f64,
    pub threshold: f64,
    pub reason: String,
}

/// Evaluate archival eligibility for a memory.
pub fn evaluate(memory: &BaseMemory, decayed_confidence: f64, threshold: f64) -> ArchivalDecision {
    if memory.archived {
        return ArchivalDecision {
            memory_id: memory.id.clone(),
            should_archive: false,
            decayed_confidence,
            threshold,
            reason: "already archived".to_string(),
        };
    }

    let archive = should_archive(decayed_confidence, threshold);
    let reason = if archive {
        format!(
            "confidence {:.3} below threshold {:.3} for type {:?}",
            decayed_confidence, threshold, memory.memory_type
        )
    } else {
        "confidence above threshold".to_string()
    };

    ArchivalDecision {
        memory_id: memory.id.clone(),
        should_archive: archive,
        decayed_confidence,
        threshold,
        reason,
    }
}
