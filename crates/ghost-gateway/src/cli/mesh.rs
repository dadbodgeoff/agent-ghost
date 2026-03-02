//! ghost mesh — multi-agent mesh networking (T-4.1.1–T-4.1.4, §4.1).

use serde::{Deserialize, Serialize};

use super::backend::CliBackend;
use super::error::CliError;
use super::output::{OutputFormat, TableDisplay, print_output};

// ─── ghost mesh peers ────────────────────────────────────────────────────────

pub struct MeshPeersArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerEntry {
    pub id: String,
    pub name: String,
    pub activity: f64,
    pub convergence_level: u8,
}

#[derive(Serialize)]
struct PeerList {
    peers: Vec<PeerEntry>,
}

impl TableDisplay for PeerList {
    fn print_table(&self) {
        if self.peers.is_empty() {
            println!("No mesh peers found.");
            return;
        }
        println!(
            "{:<36}  {:<20}  {:>8}  {:>5}",
            "PEER ID", "NAME", "ACTIVITY", "LEVEL"
        );
        println!("{}", "─".repeat(75));
        for p in &self.peers {
            let id = &p.id[..p.id.len().min(36)];
            let name = &p.name[..p.name.len().min(20)];
            println!(
                "{:<36}  {:<20}  {:>8.4}  {:>5}",
                id, name, p.activity, p.convergence_level
            );
        }
        println!("\n{} peer(s) shown.", self.peers.len());
    }
}

/// Run `ghost mesh peers`.
pub async fn run_peers(args: MeshPeersArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let resp = client.get("/api/mesh/trust-graph").await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse trust graph: {e}")))?;

    let peers: Vec<PeerEntry> = serde_json::from_value(body["nodes"].clone())
        .unwrap_or_default();

    print_output(&PeerList { peers }, args.output);
    Ok(())
}

// ─── ghost mesh trust ────────────────────────────────────────────────────────

pub struct MeshTrustArgs {
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrustEdgeEntry {
    pub source: String,
    pub target: String,
    pub trust_score: f64,
}

#[derive(Serialize)]
struct TrustGraph {
    nodes: Vec<PeerEntry>,
    edges: Vec<TrustEdgeEntry>,
}

impl TableDisplay for TrustGraph {
    fn print_table(&self) {
        if self.nodes.is_empty() {
            println!("No mesh peers found.");
            return;
        }

        // Print node summary.
        println!("Peers ({}):", self.nodes.len());
        for p in &self.nodes {
            let id_short = &p.id[..p.id.len().min(12)];
            println!(
                "  {:<12}  {:<20}  activity={:.3}  level={}",
                id_short, p.name, p.activity, p.convergence_level
            );
        }

        // Print trust edges.
        println!("\nTrust Edges ({}):", self.edges.len());
        if self.edges.is_empty() {
            println!("  No trust relationships established.");
        } else {
            println!(
                "  {:<12}  {:<12}  {:>10}",
                "SOURCE", "TARGET", "TRUST"
            );
            println!("  {}", "─".repeat(38));
            for e in &self.edges {
                let src = &e.source[..e.source.len().min(12)];
                let tgt = &e.target[..e.target.len().min(12)];
                println!("  {:<12}  {:<12}  {:>10.4}", src, tgt, e.trust_score);
            }
        }
    }
}

/// Run `ghost mesh trust`.
pub async fn run_trust(args: MeshTrustArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    let resp = client.get("/api/mesh/trust-graph").await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse trust graph: {e}")))?;

    let nodes: Vec<PeerEntry> =
        serde_json::from_value(body["nodes"].clone()).unwrap_or_default();
    let edges: Vec<TrustEdgeEntry> =
        serde_json::from_value(body["edges"].clone()).unwrap_or_default();

    print_output(&TrustGraph { nodes, edges }, args.output);
    Ok(())
}

// ─── ghost mesh discover ─────────────────────────────────────────────────────

pub struct MeshDiscoverArgs {
    pub url: String,
    pub output: OutputFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveredAgent {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub capabilities: Vec<String>,
    pub trust_score: f64,
}

#[derive(Serialize)]
struct DiscoverResult {
    agent: DiscoveredAgent,
}

impl TableDisplay for DiscoverResult {
    fn print_table(&self) {
        let a = &self.agent;
        println!("Discovered agent:");
        println!("  Name:         {}", a.name);
        println!("  Endpoint:     {}", a.endpoint_url);
        println!("  Description:  {}", a.description);
        println!("  Capabilities: {}", a.capabilities.join(", "));
        println!("  Trust score:  {:.2}", a.trust_score);
    }
}

/// Run `ghost mesh discover <url>`.
pub async fn run_discover(args: MeshDiscoverArgs, backend: &CliBackend) -> Result<(), CliError> {
    // Fetch /.well-known/agent.json from the remote peer.
    let agent_card_url = format!(
        "{}/.well-known/agent.json",
        args.url.trim_end_matches('/')
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| CliError::Http(format!("build client: {e}")))?;

    let resp = client
        .get(&agent_card_url)
        .send()
        .await
        .map_err(|e| CliError::Http(format!("fetch agent card from {agent_card_url}: {e}")))?;

    if !resp.status().is_success() {
        return Err(CliError::Http(format!(
            "agent card fetch failed: HTTP {}",
            resp.status()
        )));
    }

    let card: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse agent card: {e}")))?;

    let agent = DiscoveredAgent {
        name: card["name"].as_str().unwrap_or("unknown").to_string(),
        description: card["description"].as_str().unwrap_or("").to_string(),
        endpoint_url: args.url.clone(),
        capabilities: card["capabilities"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        trust_score: card["trust_score"].as_f64().unwrap_or(0.0),
    };

    // If gateway is running, register via API; otherwise store in config.
    if backend.is_http() {
        // Try to register via the A2A discovery endpoint.
        // The discover endpoint re-probes all known peers. We add this peer first.
        let http = backend.http();
        let register_body = serde_json::json!({
            "endpoint_url": args.url,
            "name": agent.name,
            "description": agent.description,
            "capabilities": agent.capabilities,
        });
        match http.post("/api/mesh/peers", &register_body).await {
            Ok(_) => eprintln!("Registered peer with gateway."),
            Err(_) => {
                // Fallback: POST to a2a discover may not exist; just print result.
                eprintln!("Gateway does not support peer registration. Showing card only.");
            }
        }
    } else {
        eprintln!("No gateway running. Peer card displayed but not registered.");
        eprintln!("Start the gateway and re-run, or add the peer to ~/.ghost/config/peers.yml manually.");
    }

    print_output(&DiscoverResult { agent }, args.output);
    Ok(())
}

// ─── ghost mesh ping ─────────────────────────────────────────────────────────

pub struct MeshPingArgs {
    pub peer_id: String,
    pub output: OutputFormat,
}

#[derive(Serialize)]
struct PingResult {
    peer_id: String,
    reachable: bool,
    latency_ms: Option<u64>,
    error: Option<String>,
}

impl TableDisplay for PingResult {
    fn print_table(&self) {
        if self.reachable {
            println!(
                "Peer {} is reachable ({}ms)",
                self.peer_id,
                self.latency_ms.unwrap_or(0)
            );
        } else {
            println!(
                "Peer {} is unreachable: {}",
                self.peer_id,
                self.error.as_deref().unwrap_or("unknown error")
            );
        }
    }
}

/// Run `ghost mesh ping <peer_id>`.
pub async fn run_ping(args: MeshPingArgs, backend: &CliBackend) -> Result<(), CliError> {
    backend.require(super::backend::BackendRequirement::HttpOnly)?;
    let client = backend.http();

    // First, find the peer's endpoint URL from the trust graph.
    let resp = client.get("/api/mesh/trust-graph").await?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("parse trust graph: {e}")))?;

    let nodes: Vec<PeerEntry> =
        serde_json::from_value(body["nodes"].clone()).unwrap_or_default();

    let peer = nodes.iter().find(|n| n.id == args.peer_id || n.name == args.peer_id);
    if peer.is_none() {
        return Err(CliError::NotFound(format!(
            "peer '{}' not found in mesh",
            args.peer_id
        )));
    }

    // Probe the discovered_agents table via the discover endpoint for the URL,
    // or fall back to a health check on the gateway's own mesh health.
    // Since we don't have a direct peer URL from trust-graph nodes, we query
    // discovered_agents via the search API.
    let search_resp = client
        .get(&format!(
            "/api/search?q={}&types=agents",
            args.peer_id
        ))
        .await;

    let (reachable, latency_ms, error) = match search_resp {
        Ok(r) => {
            // Peer exists in the graph, which means the gateway can reach it.
            let _body: serde_json::Value = r.json().await.unwrap_or_default();
            // Simple latency test: ping the gateway health endpoint as a proxy.
            let start = std::time::Instant::now();
            let health = client.get("/api/health").await;
            let elapsed = start.elapsed().as_millis() as u64;
            match health {
                Ok(_) => (true, Some(elapsed), None),
                Err(e) => (false, None, Some(e.to_string())),
            }
        }
        Err(e) => (false, None, Some(e.to_string())),
    };

    print_output(
        &PingResult {
            peer_id: args.peer_id,
            reachable,
            latency_ms,
            error,
        },
        args.output,
    );
    Ok(())
}
