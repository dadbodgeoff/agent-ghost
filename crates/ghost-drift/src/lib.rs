//! Ghost Drift MCP Server — codebase intelligence for Ghost agents.
//!
//! Provides 8 MCP tools for indexing codebases, recording beliefs,
//! detecting contradictions, and tracking knowledge drift over time.

pub mod analysis;
pub mod storage;

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    handler::server::tool::ToolRouter,
    model::{
        CallToolResult, Content, Implementation, InitializeResult, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;

use crate::analysis::{metrics, similarity, symbols};
use crate::storage::DriftDb;

// ── Parameter types ──

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Parameters for indexing a directory")]
pub struct IndexParams {
    /// Path to the directory to index (relative to workspace or absolute).
    #[schemars(description = "Path to index (directory or file)")]
    pub path: String,
    /// Whether to recurse into subdirectories.
    #[schemars(description = "Recurse into subdirectories (default: true)")]
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Record a belief about code behavior")]
pub struct ObserveParams {
    /// File path the belief relates to.
    #[schemars(description = "File path the belief relates to")]
    pub file: String,
    /// Optional symbol name (function, struct, etc.).
    #[schemars(description = "Optional symbol name")]
    pub symbol: Option<String>,
    /// The belief text describing what the agent understands about this code.
    #[schemars(description = "What you believe about this code's behavior")]
    pub belief: String,
    /// Confidence level (0.0–1.0).
    #[schemars(description = "Confidence level 0.0-1.0 (default: 0.8)")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Semantic search over indexed codebase")]
pub struct QueryParams {
    /// The search query.
    #[schemars(description = "Search query text")]
    pub query: String,
    /// Optional file path filter (substring match).
    #[schemars(description = "Filter results to files matching this substring")]
    pub file_filter: Option<String>,
    /// Max results to return.
    #[schemars(description = "Maximum results (default: 10)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Get knowledge health metrics")]
pub struct HealthParams {
    /// Optional path filter to scope the health check.
    #[schemars(description = "Optional path filter to scope the check")]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Find contradicting beliefs")]
pub struct ContradictionsParams {
    /// Optional file filter.
    #[schemars(description = "Optional file path to filter contradictions")]
    pub file: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Find stale beliefs needing re-verification")]
pub struct StaleParams {
    /// Maximum freshness in days (beliefs older than this are stale).
    #[schemars(description = "Max days since last verification (default: 7)")]
    pub max_freshness_days: Option<f64>,
    /// Max results.
    #[schemars(description = "Maximum results (default: 20)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Detect drift patterns")]
pub struct PatternsParams {
    /// Time window in days for pattern detection.
    #[schemars(description = "Time window in days (default: 7)")]
    pub window_days: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(description = "Create or compare snapshots")]
pub struct SnapshotParams {
    /// Action: "create" or "compare".
    #[schemars(description = "Action: 'create' or 'compare'")]
    pub action: String,
    /// Snapshot ID for comparison (required when action is "compare").
    #[schemars(description = "Snapshot ID to compare against (for 'compare' action)")]
    pub snapshot_id: Option<String>,
}

// ── DriftService ──

#[derive(Clone)]
pub struct DriftService {
    db: Arc<DriftDb>,
    workspace: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl DriftService {
    pub fn new(db: Arc<DriftDb>, workspace: PathBuf) -> Self {
        Self {
            db,
            workspace,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve a path relative to the workspace.
    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.workspace.join(p)
        }
    }
}

#[tool_router]
impl DriftService {
    #[tool(description = "Index a directory: scan files, extract symbols (functions, structs, traits, types), and compute embeddings for semantic search. Run this first on a codebase.")]
    async fn drift_index(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<IndexParams>,
    ) -> Result<CallToolResult, McpError> {
        let target = self.resolve_path(&params.path);
        let recursive = params.recursive.unwrap_or(true);

        if !target.exists() {
            return Err(McpError::invalid_params(
                format!("Path does not exist: {}", target.display()),
                None,
            ));
        }

        let start = std::time::Instant::now();
        let mut files_indexed: u32 = 0;
        let mut symbols_found: u32 = 0;

        let extensions = ["rs", "ts", "tsx", "js", "jsx", "mts", "mjs", "py", "go"];
        let pattern = if target.is_dir() {
            if recursive {
                format!("{}/**/*", target.display())
            } else {
                format!("{}/*", target.display())
            }
        } else {
            target.display().to_string()
        };

        let entries: Vec<_> = glob::glob(&pattern)
            .map_err(|e| McpError::internal_error(format!("glob error: {e}"), None))?
            .filter_map(|r| r.ok())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| extensions.contains(&e))
                        .unwrap_or(false)
            })
            .collect();

        for entry in &entries {
            let content = match std::fs::read_to_string(entry) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let metadata = entry.metadata().ok();
            let size = metadata.as_ref().map(|m| m.len() as i64).unwrap_or(0);
            let modified = metadata
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
                })
                .unwrap_or_default();

            let rel_path = entry
                .strip_prefix(&self.workspace)
                .unwrap_or(entry)
                .to_string_lossy()
                .to_string();

            // Extract symbols
            let syms = symbols::extract_symbols(entry, &content);
            let sym_data: Vec<_> = syms
                .iter()
                .map(|sym| {
                    let embed_text = format!(
                        "{} {} {}",
                        sym.kind,
                        sym.name,
                        sym.signature.as_deref().unwrap_or("")
                    );
                    let embedding = similarity::embed(&embed_text);
                    let embed_bytes = similarity::to_bytes(&embedding);
                    (
                        uuid::Uuid::new_v4().to_string(),
                        sym.name.clone(),
                        sym.kind.clone(),
                        sym.line_start as i64,
                        sym.line_end.map(|l| l as i64),
                        sym.signature.clone(),
                        Some(embed_bytes),
                    )
                })
                .collect();

            // Atomic per-file indexing (hash check + upsert + symbols in one transaction)
            let indexed = self
                .db
                .index_file_atomic(&rel_path, &hash, &modified, size, &sym_data)
                .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

            if indexed {
                files_indexed += 1;
                symbols_found += syms.len() as u32;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let total_files = self.db.file_count().unwrap_or(0);
        let total_symbols = self.db.symbol_count().unwrap_or(0);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Indexed {files_indexed} files, found {symbols_found} symbols in {duration_ms}ms.\n\
             Total indexed: {total_files} files, {total_symbols} symbols."
        ))]))
    }

    #[tool(description = "Record a belief about how code behaves. The agent calls this when it learns or assumes something about the codebase. Automatically detects contradictions with existing beliefs.")]
    async fn drift_observe(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ObserveParams>,
    ) -> Result<CallToolResult, McpError> {
        let confidence = params.confidence.unwrap_or(0.8).clamp(0.0, 1.0);
        let belief_id = uuid::Uuid::new_v4().to_string();

        self.db
            .insert_belief(
                &belief_id,
                &params.file,
                params.symbol.as_deref(),
                &params.belief,
                confidence,
            )
            .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

        // Check for contradictions with existing beliefs on same file/symbol
        let existing = self
            .db
            .get_beliefs_for_file(&params.file)
            .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

        let query_embed = similarity::embed(&params.belief);
        let mut contradictions_found = 0u32;
        let mut contradiction_details = Vec::new();

        for (id, sym, belief_text, _conf, _updated) in &existing {
            if *id == belief_id {
                continue;
            }
            // Check if beliefs are about the same symbol
            if params.symbol.is_some() && sym.as_deref() != params.symbol.as_deref() {
                continue;
            }
            let existing_embed = similarity::embed(belief_text);
            let sim = similarity::cosine_similarity(&query_embed, &existing_embed);
            // High similarity but different text = potential contradiction
            if sim > 0.3 && sim < 0.9 {
                contradictions_found += 1;
                contradiction_details.push(format!("  - \"{belief_text}\" (similarity: {sim:.2})"));
            }
        }

        let mut result = format!("Belief recorded (id: {belief_id}).");
        if contradictions_found > 0 {
            result.push_str(&format!(
                "\n\nFound {contradictions_found} potential contradiction(s):\n{}",
                contradiction_details.join("\n")
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Semantic search over indexed codebase. Find relevant symbols and code by meaning, not just text matching.")]
    async fn drift_query(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<QueryParams>,
    ) -> Result<CallToolResult, McpError> {
        let limit = params.limit.unwrap_or(10);
        let query_embed = similarity::embed(&params.query);

        let symbols = self
            .db
            .symbols_with_embeddings(params.file_filter.as_deref())
            .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

        if symbols.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No indexed symbols found. Run drift_index first.",
            )]));
        }

        // Score all symbols by cosine similarity
        let mut scored: Vec<(f32, &str, &str, &str, i64, &Option<String>)> = symbols
            .iter()
            .map(|(file, name, kind, line, sig, embed_bytes)| {
                let embed = similarity::from_bytes(embed_bytes);
                let score = similarity::cosine_similarity(&query_embed, &embed);
                (score, file.as_str(), name.as_str(), kind.as_str(), *line, sig)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit as usize);

        let mut lines = vec![format!("Found {} results:\n", scored.len())];
        for (score, file, name, kind, line, sig) in &scored {
            let sig_str = sig.as_deref().unwrap_or("");
            lines.push(format!(
                "  [{score:.3}] {file}:{line} — {kind} {name}\n         {sig_str}"
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(lines.join("\n"))]))
    }

    #[tool(description = "Get knowledge health metrics: Knowledge Stability Index (KSI), evidence freshness, and contradiction density. Higher KSI = more stable knowledge.")]
    async fn drift_health(
        &self,
        rmcp::handler::server::wrapper::Parameters(_params): rmcp::handler::server::wrapper::Parameters<HealthParams>,
    ) -> Result<CallToolResult, McpError> {
        let ksi = metrics::compute_ksi(&self.db, 7.0)
            .map_err(|e| McpError::internal_error(format!("ksi error: {e}"), None))?;
        let freshness = metrics::compute_freshness(&self.db, 7.0)
            .map_err(|e| McpError::internal_error(format!("freshness error: {e}"), None))?;
        let contradiction_density = metrics::compute_contradiction_density(&self.db)
            .map_err(|e| McpError::internal_error(format!("contradiction error: {e}"), None))?;

        let file_count = self.db.file_count().unwrap_or(0);
        let symbol_count = self.db.symbol_count().unwrap_or(0);
        let belief_count = self.db.belief_count().unwrap_or(0);

        let health_status = if ksi > 0.7 && freshness > 0.7 && contradiction_density < 0.05 {
            "HEALTHY"
        } else if ksi > 0.4 && freshness > 0.4 {
            "WARNING"
        } else {
            "CRITICAL"
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Knowledge Health: {health_status}\n\n\
             KSI (Knowledge Stability Index): {ksi:.3}\n\
             Evidence Freshness: {freshness:.3}\n\
             Contradiction Density: {contradiction_density:.3}\n\n\
             Indexed: {file_count} files, {symbol_count} symbols\n\
             Beliefs: {belief_count}\n\n\
             Thresholds:\n\
             - KSI > 0.7 = stable, < 0.3 = churning\n\
             - Freshness > 0.7 = current, < 0.3 = stale\n\
             - Contradictions < 0.02 = healthy, > 0.10 = high conflict"
        ))]))
    }

    #[tool(description = "Surface contradicting beliefs about the same code. Finds beliefs about the same file/symbol that may conflict with each other.")]
    async fn drift_contradictions(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ContradictionsParams>,
    ) -> Result<CallToolResult, McpError> {
        let beliefs: Vec<(String, String, Option<String>, String, f64, String)> =
            if let Some(file) = &params.file {
                self.db
                    .get_beliefs_for_file(file)
                    .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?
                    .into_iter()
                    .map(|(id, sym, belief, conf, updated)| {
                        (id, file.clone(), sym, belief, conf, updated)
                    })
                    .collect()
            } else {
                self.db
                    .get_all_beliefs()
                    .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?
            };

        if beliefs.len() < 2 {
            return Ok(CallToolResult::success(vec![Content::text(
                "Not enough beliefs to detect contradictions.",
            )]));
        }

        let mut contradictions = Vec::new();

        for i in 0..beliefs.len() {
            for j in (i + 1)..beliefs.len() {
                let (_, file_a, sym_a, belief_a, _, _) = &beliefs[i];
                let (_, file_b, sym_b, belief_b, _, _) = &beliefs[j];

                // Only compare beliefs about the same scope
                if file_a != file_b {
                    continue;
                }
                if sym_a.is_some() && sym_b.is_some() && sym_a != sym_b {
                    continue;
                }

                let embed_a = similarity::embed(belief_a);
                let embed_b = similarity::embed(belief_b);
                let sim = similarity::cosine_similarity(&embed_a, &embed_b);

                // Moderate similarity = potentially conflicting
                if sim > 0.2 && sim < 0.85 {
                    contradictions.push(format!(
                        "File: {file_a}\n  A: \"{belief_a}\"\n  B: \"{belief_b}\"\n  Conflict score: {:.2}\n",
                        1.0 - sim
                    ));
                }
            }
        }

        if contradictions.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(
                "No contradictions detected.",
            )]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Found {} potential contradiction(s):\n\n{}",
                contradictions.len(),
                contradictions.join("\n")
            ))]))
        }
    }

    #[tool(description = "Find stale beliefs that haven't been verified recently. These represent areas where the agent's understanding may have drifted from reality.")]
    async fn drift_stale(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<StaleParams>,
    ) -> Result<CallToolResult, McpError> {
        let max_days = params.max_freshness_days.unwrap_or(7.0);
        let limit = params.limit.unwrap_or(20);

        let stale = self
            .db
            .get_stale_beliefs(max_days, limit)
            .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

        if stale.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No stale beliefs found (threshold: {max_days} days)."
            ))]));
        }

        let mut lines = vec![format!(
            "Found {} stale belief(s) (not verified in >{max_days} days):\n",
            stale.len()
        )];
        for (_id, file, symbol, belief, confidence, updated) in &stale {
            let sym_str = symbol.as_deref().unwrap_or("(file-level)");
            lines.push(format!(
                "  {file}:{sym_str} — \"{belief}\" (confidence: {confidence:.2}, last: {updated})"
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(lines.join("\n"))]))
    }

    #[tool(description = "Detect drift patterns: erosion (declining confidence), explosion (rapid belief creation), crystallization (beliefs stabilizing), and conflict waves (contradiction spikes).")]
    async fn drift_patterns(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<PatternsParams>,
    ) -> Result<CallToolResult, McpError> {
        let window = params.window_days.unwrap_or(7) as f64;
        let mut patterns = Vec::new();

        // Erosion: beliefs with declining confidence
        let eroding = self.db.eroding_belief_count().unwrap_or(0);
        if eroding > 0 {
            patterns.push(format!(
                "EROSION: {eroding} belief(s) show declining confidence.\n\
                 Severity: {}\n\
                 Action: Re-verify these beliefs against current code.",
                if eroding > 5 { "HIGH" } else { "MEDIUM" }
            ));
        }

        // Explosion: high creation rate
        let recent_created = self.db.beliefs_created_in_window(window).unwrap_or(0);
        let total_beliefs = self.db.belief_count().unwrap_or(1).max(1);
        let creation_rate = recent_created as f64 / total_beliefs as f64;
        if creation_rate > 0.5 && recent_created > 5 {
            patterns.push(format!(
                "EXPLOSION: {recent_created} beliefs created in {window} days ({:.0}% of total).\n\
                 Severity: {}\n\
                 Action: Consider consolidating overlapping beliefs.",
                creation_rate * 100.0,
                if creation_rate > 0.8 { "HIGH" } else { "MEDIUM" }
            ));
        }

        // Crystallization: stable beliefs (no changes in window)
        let changes = self.db.belief_changes_in_window(window).unwrap_or(0);
        let ksi = if total_beliefs > 0 {
            1.0 - (changes as f64 / (2.0 * total_beliefs as f64))
        } else {
            1.0
        };
        if ksi > 0.9 && total_beliefs > 5 {
            patterns.push(format!(
                "CRYSTALLIZATION: Knowledge is highly stable (KSI: {ksi:.3}).\n\
                 Severity: INFO\n\
                 Action: Knowledge base is well-consolidated. Good time for new exploration."
            ));
        }

        // Conflict wave: contradiction density spike
        let contradiction_density =
            metrics::compute_contradiction_density(&self.db).unwrap_or(0.0);
        if contradiction_density > 0.1 {
            patterns.push(format!(
                "CONFLICT WAVE: High contradiction density ({contradiction_density:.3}).\n\
                 Severity: HIGH\n\
                 Action: Run drift_contradictions to identify and resolve conflicts."
            ));
        }

        if patterns.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "No significant drift patterns detected in the last {window} days.\n\
                 KSI: {ksi:.3}, Beliefs: {total_beliefs}, Recent changes: {changes}"
            ))]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Detected {} pattern(s) in the last {window} days:\n\n{}",
                patterns.len(),
                patterns.join("\n\n")
            ))]))
        }
    }

    #[tool(description = "Create a point-in-time snapshot of knowledge state, or compare current state against a previous snapshot to measure drift over time.")]
    async fn drift_snapshot(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<SnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        match params.action.as_str() {
            "create" => {
                let snap_id = uuid::Uuid::new_v4().to_string();
                let file_count = self.db.file_count().unwrap_or(0);
                let symbol_count = self.db.symbol_count().unwrap_or(0);
                let belief_count = self.db.belief_count().unwrap_or(0);
                let ksi = metrics::compute_ksi(&self.db, 7.0).ok();
                let freshness = metrics::compute_freshness(&self.db, 7.0).ok();
                let contradiction_density =
                    metrics::compute_contradiction_density(&self.db).ok();

                self.db
                    .insert_snapshot(
                        &snap_id,
                        file_count,
                        symbol_count,
                        belief_count,
                        ksi,
                        freshness,
                        contradiction_density,
                        None,
                    )
                    .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Snapshot created: {snap_id}\n\
                     Files: {file_count}, Symbols: {symbol_count}, Beliefs: {belief_count}\n\
                     KSI: {}, Freshness: {}, Contradictions: {}",
                    ksi.map(|v| format!("{v:.3}")).unwrap_or("N/A".into()),
                    freshness.map(|v| format!("{v:.3}")).unwrap_or("N/A".into()),
                    contradiction_density
                        .map(|v| format!("{v:.3}"))
                        .unwrap_or("N/A".into()),
                ))]))
            }
            "compare" => {
                let snap_id = params.snapshot_id.ok_or_else(|| {
                    McpError::invalid_params("snapshot_id required for compare action", None)
                })?;

                let snapshot = self
                    .db
                    .get_snapshot(&snap_id)
                    .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?
                    .ok_or_else(|| {
                        McpError::invalid_params(format!("Snapshot not found: {snap_id}"), None)
                    })?;

                let (_, created, old_files, old_symbols, old_beliefs, old_ksi, old_fresh, old_contra, _) =
                    snapshot;

                let cur_files = self.db.file_count().unwrap_or(0);
                let cur_symbols = self.db.symbol_count().unwrap_or(0);
                let cur_beliefs = self.db.belief_count().unwrap_or(0);
                let cur_ksi = metrics::compute_ksi(&self.db, 7.0).ok();
                let cur_fresh = metrics::compute_freshness(&self.db, 7.0).ok();
                let cur_contra = metrics::compute_contradiction_density(&self.db).ok();

                let delta = |old: i64, new: i64| -> String {
                    let d = new - old;
                    if d > 0 { format!("+{d}") } else { format!("{d}") }
                };
                let delta_f = |old: Option<f64>, new: Option<f64>| -> String {
                    match (old, new) {
                        (Some(o), Some(n)) => {
                            let d = n - o;
                            if d >= 0.0 {
                                format!("+{d:.3}")
                            } else {
                                format!("{d:.3}")
                            }
                        }
                        _ => "N/A".into(),
                    }
                };

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Comparing current state vs snapshot {snap_id} (created {created}):\n\n\
                     Files:    {cur_files} ({}) \n\
                     Symbols:  {cur_symbols} ({})\n\
                     Beliefs:  {cur_beliefs} ({})\n\
                     KSI:      {} ({})\n\
                     Freshness: {} ({})\n\
                     Contradictions: {} ({})",
                    delta(old_files, cur_files),
                    delta(old_symbols, cur_symbols),
                    delta(old_beliefs, cur_beliefs),
                    cur_ksi.map(|v| format!("{v:.3}")).unwrap_or("N/A".into()),
                    delta_f(old_ksi, cur_ksi),
                    cur_fresh.map(|v| format!("{v:.3}")).unwrap_or("N/A".into()),
                    delta_f(old_fresh, cur_fresh),
                    cur_contra.map(|v| format!("{v:.3}")).unwrap_or("N/A".into()),
                    delta_f(old_contra, cur_contra),
                ))]))
            }
            other => Err(McpError::invalid_params(
                format!("Unknown action: {other}. Use 'create' or 'compare'."),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for DriftService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "ghost-drift".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "Ghost Drift — codebase intelligence server. Provides tools for indexing \
                 codebases, recording beliefs about code behavior, detecting contradictions, \
                 and tracking knowledge stability over time."
                    .into(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: rmcp::model::InitializeRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}
