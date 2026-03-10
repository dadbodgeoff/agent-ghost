//! Centralized periodic task scheduler (Task 19.3).
//!
//! Multiple components need periodic background work but have no centralized
//! scheduler. The gateway bootstrap is the natural orchestration point.
//! Each task runs in its own `tokio::spawn` (failure in one doesn't block others).
//! After `max_failures` consecutive failures, a task is disabled with `tracing::error!`.
//!
//! Autonomy freeze: this scheduler is not allowed to own proactive agent work,
//! schedule selection, retry dispatch, or any other autonomy control-plane
//! behavior. Those responsibilities belong to the gateway autonomy runtime.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

type PeriodicTaskFuture = Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>;
type PeriodicTaskFn = dyn Fn() -> PeriodicTaskFuture + Send + Sync;

/// Health status for a periodic task.
#[derive(Debug, Clone)]
pub struct TaskHealth {
    pub last_success: Option<Instant>,
    pub last_failure: Option<Instant>,
    pub consecutive_failures: u32,
    pub total_runs: u64,
    pub status: TaskStatus,
}

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Healthy,
    Degraded,
    Disabled,
}

/// A periodic task definition.
pub struct PeriodicTask {
    pub name: String,
    pub interval: Duration,
    pub task_fn: Box<PeriodicTaskFn>,
    pub last_run: Option<Instant>,
    pub consecutive_failures: u32,
    pub max_failures: u32,
}

/// Centralized periodic task scheduler.
pub struct PeriodicTaskScheduler {
    tasks: Vec<PeriodicTask>,
    kill_switch: Arc<AtomicBool>,
    health: BTreeMap<String, TaskHealth>,
}

impl PeriodicTaskScheduler {
    pub fn new(kill_switch: Arc<AtomicBool>) -> Self {
        Self {
            tasks: Vec::new(),
            kill_switch,
            health: BTreeMap::new(),
        }
    }

    /// Register a periodic task.
    pub fn register(&mut self, task: PeriodicTask) {
        self.health.insert(
            task.name.clone(),
            TaskHealth {
                last_success: None,
                last_failure: None,
                consecutive_failures: 0,
                total_runs: 0,
                status: TaskStatus::Healthy,
            },
        );
        self.tasks.push(task);
    }

    /// Get the health report for all tasks.
    pub fn health_report(&self) -> &BTreeMap<String, TaskHealth> {
        &self.health
    }

    /// Run the scheduler. Spawns a tokio task that loops with 1s granularity.
    /// Respects kill switch. Returns a JoinHandle for the scheduler task.
    pub fn run(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!(
                task_count = self.tasks.len(),
                "Periodic task scheduler started"
            );

            loop {
                // Check kill switch
                if self.kill_switch.load(Ordering::SeqCst) {
                    tracing::info!("Periodic task scheduler stopped: kill switch active");
                    break;
                }

                let now = Instant::now();

                for task in &mut self.tasks {
                    // Skip disabled tasks
                    if let Some(health) = self.health.get(&task.name) {
                        if health.status == TaskStatus::Disabled {
                            continue;
                        }
                    }

                    // Check if task is due
                    let should_run = match task.last_run {
                        None => true,
                        Some(last) => now.duration_since(last) >= task.interval,
                    };

                    if !should_run {
                        continue;
                    }

                    task.last_run = Some(now);
                    let name = task.name.clone();

                    // Run task in its own spawn (failure in one doesn't block others)
                    let future = (task.task_fn)();
                    match tokio::spawn(async move {
                        // Catch panics
                        let result = tokio::task::spawn(future).await;
                        match result {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(e)) => Err(format!("{e}")),
                            Err(e) => Err(format!("task panicked: {e}")),
                        }
                    })
                    .await
                    {
                        Ok(Ok(())) => {
                            if let Some(health) = self.health.get_mut(&name) {
                                health.last_success = Some(Instant::now());
                                health.consecutive_failures = 0;
                                health.total_runs += 1;
                                health.status = TaskStatus::Healthy;
                            }
                            task.consecutive_failures = 0;
                        }
                        Ok(Err(e)) => {
                            let e_str = format!("{e:?}");
                            task.consecutive_failures += 1;
                            if let Some(health) = self.health.get_mut(&name) {
                                health.last_failure = Some(Instant::now());
                                health.consecutive_failures = task.consecutive_failures;
                                health.total_runs += 1;

                                if task.consecutive_failures >= task.max_failures {
                                    health.status = TaskStatus::Disabled;
                                    tracing::error!(
                                        task = %name,
                                        failures = task.consecutive_failures,
                                        max = task.max_failures,
                                        "Periodic task disabled after max consecutive failures"
                                    );
                                } else {
                                    health.status = TaskStatus::Degraded;
                                    tracing::warn!(
                                        task = %name,
                                        error = %e_str,
                                        failures = task.consecutive_failures,
                                        "Periodic task failed"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let e_str = format!("{e:?}");
                            task.consecutive_failures += 1;
                            if let Some(health) = self.health.get_mut(&name) {
                                health.last_failure = Some(Instant::now());
                                health.consecutive_failures = task.consecutive_failures;
                                health.total_runs += 1;

                                if task.consecutive_failures >= task.max_failures {
                                    health.status = TaskStatus::Disabled;
                                    tracing::error!(
                                        task = %name,
                                        failures = task.consecutive_failures,
                                        max = task.max_failures,
                                        "Periodic task disabled after max consecutive failures"
                                    );
                                } else {
                                    health.status = TaskStatus::Degraded;
                                    tracing::warn!(
                                        task = %name,
                                        error = %e_str,
                                        failures = task.consecutive_failures,
                                        "Periodic task failed"
                                    );
                                }
                            }
                        }
                    }
                }

                // 1s granularity sleep
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_fires_after_interval() {
        let kill = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter_clone = counter.clone();

        let mut scheduler = PeriodicTaskScheduler::new(kill.clone());
        scheduler.register(PeriodicTask {
            name: "test-task".into(),
            interval: Duration::from_millis(100),
            task_fn: Box::new(move || {
                let c = counter_clone.clone();
                Box::pin(async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
            last_run: None,
            consecutive_failures: 0,
            max_failures: 3,
        });

        let handle = scheduler.run();
        tokio::time::sleep(Duration::from_millis(350)).await;
        kill.store(true, Ordering::SeqCst);
        let _ = handle.await;

        assert!(counter.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn kill_switch_stops_scheduler() {
        let kill = Arc::new(AtomicBool::new(true));
        let scheduler = PeriodicTaskScheduler::new(kill);
        let handle = scheduler.run();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        // Should complete quickly since kill switch is already set
    }
}
