//! WASM sandbox: wasmtime engine with fail-closed imports, memory limits,
//! fuel metering, and timeout enforcement.

use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasmtime::{
    Config, Engine, Error as WasmError, Instance, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, Trap,
};

use super::wasm_abi::{unpack_guest_buffer, ALLOC_EXPORT, MEMORY_EXPORT, RUN_EXPORT};

/// Default execution timeout for WASM skills.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default memory limit in bytes (64 MB).
const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Default fuel budget for a single guest invocation.
const DEFAULT_FUEL_LIMIT: u64 = 10_000_000;

/// WASM sandbox configuration.
#[derive(Debug, Clone)]
pub struct WasmSandboxConfig {
    pub timeout: Duration,
    pub memory_limit_bytes: usize,
    pub fuel_limit: u64,
    pub allowed_capabilities: HashSet<String>,
}

impl Default for WasmSandboxConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            memory_limit_bytes: DEFAULT_MEMORY_LIMIT,
            fuel_limit: DEFAULT_FUEL_LIMIT,
            allowed_capabilities: HashSet::new(),
        }
    }
}

/// Forensic data captured on sandbox escape attempt (AC6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscapeAttempt {
    pub skill_name: String,
    pub skill_hash: String,
    pub escape_type: EscapeType,
    pub details: String,
    pub agent_id: Uuid,
    pub detected_at: DateTime<Utc>,
}

/// Classification of sandbox escape attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscapeType {
    /// Attempted filesystem write without grant.
    FilesystemWrite,
    /// Attempted network access to non-allowlisted domain.
    NetworkAccess,
    /// Attempted environment variable read.
    EnvVarRead,
    /// Attempted process spawn.
    ProcessSpawn,
    /// Memory limit exceeded.
    MemoryExceeded,
    /// Attempted import of a non-brokered host function.
    HiddenImport,
}

/// Result of a WASM skill execution.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Skill completed successfully.
    Success {
        output: serde_json::Value,
        elapsed: Duration,
    },
    /// Skill timed out.
    Timeout { elapsed: Duration },
    /// Skill exhausted its fuel budget.
    FuelExhausted { consumed: u64, limit: u64 },
    /// Skill exceeded memory limit.
    MemoryExceeded {
        used_bytes: usize,
        limit_bytes: usize,
    },
    /// Sandbox escape detected — instance terminated.
    EscapeDetected(EscapeAttempt),
    /// Skill returned an error.
    Error(String),
}

/// WASM sandbox for executing untrusted skill code.
///
/// Uses wasmtime for isolation. Capabilities are scoped at import level:
/// only explicitly granted host functions are available to the WASM module.
pub struct WasmSandbox {
    config: WasmSandboxConfig,
}

struct StoreState {
    limits: StoreLimits,
}

impl WasmSandbox {
    pub fn new(config: WasmSandboxConfig) -> Self {
        Self { config }
    }

    /// Execute a WASM skill module with the given input.
    ///
    /// The sandbox enforces:
    /// - Timeout (default 30s)
    /// - Memory limit (default 64MB)
    /// - Capability-scoped imports only
    /// - Escape detection with forensic capture
    pub fn execute(
        &self,
        wasm_bytes: &[u8],
        input: serde_json::Value,
        agent_id: Uuid,
        skill_name: &str,
    ) -> ExecutionResult {
        let started = Instant::now();
        let input_bytes = match serde_json::to_vec(&input) {
            Ok(bytes) => bytes,
            Err(error) => {
                return ExecutionResult::Error(format!(
                    "failed to serialize sandbox input: {error}"
                ));
            }
        };

        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = match Engine::new(&config) {
            Ok(engine) => engine,
            Err(error) => {
                return ExecutionResult::Error(format!(
                    "failed to initialize wasmtime engine: {error}"
                ));
            }
        };

        let module = match Module::from_binary(&engine, wasm_bytes) {
            Ok(module) => module,
            Err(error) => {
                return ExecutionResult::Error(format!("failed to compile wasm module: {error}"));
            }
        };

        if let Some(import) = module.imports().next() {
            let details = format!(
                "module imports forbidden host function '{}::{}'",
                import.module(),
                import.name()
            );
            let attempt = self.record_escape(
                skill_name,
                "unavailable",
                classify_denied_import(import.module(), import.name()),
                &details,
                agent_id,
            );
            return ExecutionResult::EscapeDetected(attempt);
        }

        let store_limits = StoreLimitsBuilder::new()
            .memory_size(self.config.memory_limit_bytes)
            .build();
        let mut store = Store::new(
            &engine,
            StoreState {
                limits: store_limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        if let Err(error) = store.set_fuel(self.config.fuel_limit) {
            return ExecutionResult::Error(format!("failed to configure wasm fuel: {error}"));
        }
        store.set_epoch_deadline(1);
        let _watchdog = Watchdog::arm(engine.clone(), self.config.timeout);

        let instance = match Instance::new(&mut store, &module, &[]) {
            Ok(instance) => instance,
            Err(error) => return self.map_runtime_error(error, started),
        };

        let memory = match instance.get_memory(&mut store, MEMORY_EXPORT) {
            Some(memory) => memory,
            None => {
                return ExecutionResult::Error(format!(
                    "missing required wasm export '{MEMORY_EXPORT}'"
                ));
            }
        };
        let alloc = match instance.get_typed_func::<i32, i32>(&mut store, ALLOC_EXPORT) {
            Ok(func) => func,
            Err(error) => {
                return ExecutionResult::Error(format!(
                    "missing or invalid allocator export '{ALLOC_EXPORT}': {error}"
                ));
            }
        };
        let run = match instance.get_typed_func::<(i32, i32), i64>(&mut store, RUN_EXPORT) {
            Ok(func) => func,
            Err(error) => {
                return ExecutionResult::Error(format!(
                    "missing or invalid execution export '{RUN_EXPORT}': {error}"
                ));
            }
        };

        let input_len = match i32::try_from(input_bytes.len()) {
            Ok(len) => len,
            Err(_) => {
                return ExecutionResult::Error(format!(
                    "input payload exceeds i32 wasm ABI limit: {} bytes",
                    input_bytes.len()
                ));
            }
        };
        let input_ptr = match alloc.call(&mut store, input_len) {
            Ok(pointer) if pointer >= 0 => pointer,
            Ok(pointer) => {
                return ExecutionResult::Error(format!(
                    "guest allocator returned negative pointer {pointer}"
                ));
            }
            Err(error) => return self.map_runtime_error(error, started),
        };
        if let Err(error) = memory.write(&mut store, input_ptr as usize, &input_bytes) {
            return ExecutionResult::Error(format!(
                "failed to write input into guest memory: {error}"
            ));
        }

        let packed = match run.call(&mut store, (input_ptr, input_len)) {
            Ok(packed) => packed,
            Err(error) => return self.map_runtime_error(error, started),
        };
        let output_bytes = match read_guest_output(&memory, &store, packed) {
            Ok(bytes) => bytes,
            Err(error) => return ExecutionResult::Error(error),
        };
        let output = match serde_json::from_slice::<serde_json::Value>(&output_bytes) {
            Ok(output) => output,
            Err(error) => {
                return ExecutionResult::Error(format!(
                    "guest returned invalid json output: {error}"
                ));
            }
        };

        tracing::info!(
            skill = %skill_name,
            agent_id = %agent_id,
            timeout = ?self.config.timeout,
            memory_limit = self.config.memory_limit_bytes,
            fuel_limit = self.config.fuel_limit,
            capabilities = ?self.config.allowed_capabilities,
            elapsed_ms = started.elapsed().as_millis(),
            "WASM sandbox: executed skill"
        );

        ExecutionResult::Success {
            output,
            elapsed: started.elapsed(),
        }
    }

    /// Check if a capability is granted to this sandbox instance.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.config.allowed_capabilities.contains(capability)
    }

    /// Detect and record a sandbox escape attempt.
    /// Terminates the instance and emits a TriggerEvent::SandboxEscape.
    pub fn record_escape(
        &self,
        skill_name: &str,
        skill_hash: &str,
        escape_type: EscapeType,
        details: &str,
        agent_id: Uuid,
    ) -> EscapeAttempt {
        let attempt = EscapeAttempt {
            skill_name: skill_name.into(),
            skill_hash: skill_hash.into(),
            escape_type,
            details: details.into(),
            agent_id,
            detected_at: Utc::now(),
        };
        tracing::error!(
            skill = %skill_name,
            escape_type = ?escape_type,
            agent_id = %agent_id,
            "SANDBOX ESCAPE DETECTED — instance terminated"
        );
        attempt
    }

    pub fn config(&self) -> &WasmSandboxConfig {
        &self.config
    }

    fn map_runtime_error(&self, error: WasmError, started: Instant) -> ExecutionResult {
        if let Some(trap) = error.downcast_ref::<Trap>() {
            match trap {
                Trap::OutOfFuel => {
                    return ExecutionResult::FuelExhausted {
                        consumed: self.config.fuel_limit,
                        limit: self.config.fuel_limit,
                    };
                }
                Trap::Interrupt => {
                    return ExecutionResult::Timeout {
                        elapsed: started.elapsed(),
                    };
                }
                Trap::AllocationTooLarge | Trap::StackOverflow => {
                    return ExecutionResult::MemoryExceeded {
                        used_bytes: self.config.memory_limit_bytes,
                        limit_bytes: self.config.memory_limit_bytes,
                    };
                }
                Trap::MemoryOutOfBounds => {
                    return ExecutionResult::Error(format!(
                        "wasm guest accessed memory out of bounds: {}",
                        format_error_chain(&error)
                    ));
                }
                _ => {}
            }
        }

        let message = format_error_chain(&error);
        let lower = message.to_ascii_lowercase();
        if lower.contains("all fuel consumed") {
            return ExecutionResult::FuelExhausted {
                consumed: self.config.fuel_limit,
                limit: self.config.fuel_limit,
            };
        }
        if lower.contains("interrupt") {
            return ExecutionResult::Timeout {
                elapsed: started.elapsed(),
            };
        }
        if lower.contains("memory")
            && (lower.contains("grow")
                || lower.contains("limit")
                || lower.contains("allocation")
                || lower.contains("out of bounds"))
        {
            return ExecutionResult::MemoryExceeded {
                used_bytes: self.config.memory_limit_bytes,
                limit_bytes: self.config.memory_limit_bytes,
            };
        }

        ExecutionResult::Error(format!("wasm runtime error: {message}"))
    }
}

impl Default for WasmSandbox {
    fn default() -> Self {
        Self::new(WasmSandboxConfig::default())
    }
}

struct Watchdog {
    completed: Arc<AtomicBool>,
}

impl Watchdog {
    fn arm(engine: Engine, timeout: Duration) -> Self {
        let completed = Arc::new(AtomicBool::new(false));
        let marker = Arc::clone(&completed);
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            if !marker.load(Ordering::Relaxed) {
                engine.increment_epoch();
            }
        });
        Self { completed }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.completed.store(true, Ordering::Relaxed);
    }
}

fn read_guest_output(
    memory: &Memory,
    store: &Store<StoreState>,
    packed: i64,
) -> Result<Vec<u8>, String> {
    let output = unpack_guest_buffer(packed);
    let start = output.pointer as usize;
    let length = output.length as usize;
    let end = start
        .checked_add(length)
        .ok_or_else(|| "guest output pointer overflowed host bounds checks".to_string())?;
    let data = memory.data(store);
    if end > data.len() {
        return Err(format!(
            "guest returned out-of-bounds buffer {}..{} (memory size {})",
            start,
            end,
            data.len()
        ));
    }
    Ok(data[start..end].to_vec())
}

fn format_error_chain(error: &WasmError) -> String {
    let mut chain = error.chain().map(ToString::to_string);
    let mut message = chain.next().unwrap_or_else(|| error.to_string());
    for cause in chain {
        if !cause.is_empty() && !message.contains(&cause) {
            message.push_str(": ");
            message.push_str(&cause);
        }
    }
    message
}

fn classify_denied_import(module: &str, name: &str) -> EscapeType {
    let lowered = format!("{module}::{name}").to_ascii_lowercase();
    if lowered.contains("environ") || lowered.contains("getenv") || lowered.contains("env") {
        return EscapeType::EnvVarRead;
    }
    if lowered.contains("sock")
        || lowered.contains("http")
        || lowered.contains("net")
        || lowered.contains("tcp")
        || lowered.contains("udp")
        || lowered.contains("connect")
    {
        return EscapeType::NetworkAccess;
    }
    if lowered.contains("proc")
        || lowered.contains("spawn")
        || lowered.contains("exec")
        || lowered.contains("command")
    {
        return EscapeType::ProcessSpawn;
    }
    if lowered.contains("fd_")
        || lowered.contains("path_")
        || lowered.contains("file")
        || lowered.contains("filesystem")
    {
        return EscapeType::FilesystemWrite;
    }
    EscapeType::HiddenImport
}
