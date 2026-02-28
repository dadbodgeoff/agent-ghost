//! # cortex-observability
//!
//! Convergence metrics endpoints for the GHOST platform.
//! Exposes Prometheus-compatible gauges, counters, and histograms
//! for convergence scoring, intervention levels, and signal values.

pub mod convergence_metrics;

pub use convergence_metrics::ConvergenceMetrics;
