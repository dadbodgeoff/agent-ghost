//! E2E: Observability metrics lifecycle.
//!
//! Validates cortex-observability convergence metrics collection and export.

use cortex_observability::ConvergenceMetrics;

/// Full metrics lifecycle: set → snapshot → prometheus export.
#[test]
fn metrics_lifecycle() {
    let metrics = ConvergenceMetrics::new();

    // Set metrics for multiple agents
    metrics.set_score("agent-001", 0.42);
    metrics.set_score("agent-002", 0.78);
    metrics.set_level("agent-001", 1);
    metrics.set_level("agent-002", 3);
    metrics.inc_interventions();
    metrics.inc_interventions();
    metrics.inc_violations();
    metrics.set_signals("agent-001", [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.0]);

    // Snapshot
    let snap = metrics.snapshot();
    assert_eq!(snap.scores["agent-001"], 0.42);
    assert_eq!(snap.scores["agent-002"], 0.78);
    assert_eq!(snap.levels["agent-001"], 1);
    assert_eq!(snap.levels["agent-002"], 3);
    assert_eq!(snap.intervention_count, 2);
    assert_eq!(snap.violation_count, 1);
    assert_eq!(snap.signals["agent-001"][3], 0.4);

    // Prometheus export
    let prom = metrics.to_prometheus();
    assert!(prom.contains("ghost_convergence_score{agent_id=\"agent-001\"} 0.42"));
    assert!(prom.contains("ghost_convergence_score{agent_id=\"agent-002\"} 0.78"));
    assert!(prom.contains("ghost_intervention_level{agent_id=\"agent-001\"} 1"));
    assert!(prom.contains("ghost_intervention_total 2"));
    assert!(prom.contains("ghost_violation_total 1"));
}

/// Metrics update overwrites previous values.
#[test]
fn metrics_overwrite() {
    let metrics = ConvergenceMetrics::new();

    metrics.set_score("agent-001", 0.3);
    assert_eq!(metrics.snapshot().scores["agent-001"], 0.3);

    metrics.set_score("agent-001", 0.7);
    assert_eq!(metrics.snapshot().scores["agent-001"], 0.7);
}

/// Empty metrics produce valid prometheus output.
#[test]
fn empty_metrics_valid_prometheus() {
    let metrics = ConvergenceMetrics::new();
    let prom = metrics.to_prometheus();

    assert!(prom.contains("# HELP ghost_convergence_score"));
    assert!(prom.contains("# TYPE ghost_convergence_score gauge"));
    assert!(prom.contains("ghost_intervention_total 0"));
    assert!(prom.contains("ghost_violation_total 0"));
}

/// Counters only increment, never decrement.
#[test]
fn counters_monotonic() {
    let metrics = ConvergenceMetrics::new();

    for i in 1..=10 {
        metrics.inc_interventions();
        assert_eq!(metrics.snapshot().intervention_count, i);
    }
}
