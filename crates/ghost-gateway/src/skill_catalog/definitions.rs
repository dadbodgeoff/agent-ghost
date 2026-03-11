use std::sync::{Arc, Mutex};

use ghost_pc_control::safety::PcControlCircuitBreaker;
use ghost_skills::sandbox::native_sandbox::{NativeContainmentMode, NativeContainmentProfile};
use ghost_skills::skill::Skill;
use utoipa::ToSchema;

use crate::config::GhostConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillExecutionMode {
    Native,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceKind {
    Compiled,
    User,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillMutationKind {
    ReadOnly,
    Transactional,
    ExternalSideEffect,
}

#[derive(Clone)]
pub struct SkillDefinition {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: SkillSourceKind,
    pub removable: bool,
    pub always_on: bool,
    pub installable: bool,
    pub default_enabled: bool,
    pub execution_mode: SkillExecutionMode,
    pub policy_capability: String,
    pub privileges: Vec<String>,
    pub mutation_kind: SkillMutationKind,
    pub native_containment: Option<NativeContainmentProfile>,
    pub skill: Arc<dyn Skill>,
}

pub struct CompiledSkillCatalogSeed {
    pub definitions: Vec<SkillDefinition>,
    pub pc_control_circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

pub fn build_compiled_skill_definitions(config: &GhostConfig) -> CompiledSkillCatalogSeed {
    build_compiled_skill_definitions_with_runtime(
        config,
        ghost_pc_control::safety::PcControlPolicyHandle::new(
            ghost_pc_control::safety::PcControlPolicySnapshot::from_config(&config.pc_control),
        ),
        config.pc_control.circuit_breaker(),
    )
}

pub fn build_compiled_skill_definitions_with_runtime(
    config: &GhostConfig,
    pc_control_policy: ghost_pc_control::safety::PcControlPolicyHandle,
    pc_control_circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
) -> CompiledSkillCatalogSeed {
    let mut definitions = Vec::new();

    extend_skill_definitions(
        &mut definitions,
        ghost_skills::safety_skills::all_safety_skills(),
    );
    extend_skill_definitions(&mut definitions, ghost_skills::git_skills::all_git_skills());
    extend_skill_definitions(
        &mut definitions,
        ghost_skills::code_analysis::all_code_analysis_skills(),
    );
    extend_skill_definitions(
        &mut definitions,
        ghost_skills::bundled_skills::all_bundled_skills(),
    );
    extend_skill_definitions(
        &mut definitions,
        ghost_skills::delegation_skills::all_delegation_skills(),
    );
    extend_skill_definitions(
        &mut definitions,
        ghost_pc_control::all_pc_control_skills_with_runtime(
            &config.pc_control,
            pc_control_policy,
            Arc::clone(&pc_control_circuit_breaker),
        ),
    );

    definitions.sort_by(|left, right| left.name.cmp(&right.name));

    CompiledSkillCatalogSeed {
        definitions,
        pc_control_circuit_breaker,
    }
}

fn extend_skill_definitions(definitions: &mut Vec<SkillDefinition>, skills: Vec<Box<dyn Skill>>) {
    for skill in skills {
        let skill: Arc<dyn Skill> = Arc::from(skill);
        let name = skill.name().to_string();
        let removable = skill.removable();
        let mutation_kind = mutation_kind_for_skill(&name);

        definitions.push(SkillDefinition {
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: skill.description().to_string(),
            source: SkillSourceKind::Compiled,
            always_on: !removable,
            installable: removable,
            default_enabled: removable,
            execution_mode: SkillExecutionMode::Native,
            policy_capability: format!("skill:{name}"),
            privileges: privileges_for_skill(&name),
            mutation_kind,
            native_containment: Some(native_containment_for_skill(&name, mutation_kind)),
            removable,
            name,
            skill,
        });
    }
}

fn mutation_kind_for_skill(name: &str) -> SkillMutationKind {
    match name {
        "reflection_write" | "note_take" | "timer_set" | "email_draft" | "delegate_to_agent"
        | "agent_spawn_safe" | "cancel_task" => SkillMutationKind::Transactional,
        "git_branch" | "git_commit" | "git_stash" | "format_code" | "mouse_move"
        | "mouse_click" | "mouse_drag" | "scroll" | "keyboard_type" | "keyboard_hotkey"
        | "keyboard_press" | "focus_window" | "resize_window" | "launch_app" | "kill_process"
        | "clipboard_write" => SkillMutationKind::ExternalSideEffect,
        _ => SkillMutationKind::ReadOnly,
    }
}

fn native_containment_for_skill(
    name: &str,
    mutation_kind: SkillMutationKind,
) -> NativeContainmentProfile {
    let mut capabilities = vec!["skill_execute".to_string()];
    let mode = native_containment_mode_for_skill(name, mutation_kind);
    match mode {
        NativeContainmentMode::ReadOnly => {
            capabilities.push("db_read".into());
        }
        NativeContainmentMode::Transactional => {
            capabilities.push("db_read".into());
            capabilities.push("db_write".into());
        }
        NativeContainmentMode::HostInteraction => {
            capabilities.push("host_interaction".into());
        }
    }

    match name {
        "git_status" | "git_diff" | "git_log" | "doc_summarize" | "csv_analyze"
        | "sqlite_query" | "parse_ast" | "get_diagnostics" | "find_references"
        | "search_symbols" => capabilities.push("filesystem_read".into()),
        "git_branch" | "git_commit" | "git_stash" | "format_code" => {
            capabilities.push("filesystem_read".into());
            capabilities.push("filesystem_write".into());
            capabilities.push("process_spawn".into());
        }
        "calendar_check" | "arxiv_search" | "github_search" => {
            capabilities.push("network_egress".into());
        }
        "mouse_move" | "mouse_click" | "mouse_drag" | "scroll" | "keyboard_type"
        | "keyboard_hotkey" | "keyboard_press" | "focus_window" | "resize_window"
        | "clipboard_read" | "clipboard_write" | "ocr_extract" | "accessibility_tree"
        | "screenshot" | "list_windows" | "list_processes" | "launch_app" | "kill_process" => {
            capabilities.push("desktop_control".into());
        }
        _ => {}
    }

    NativeContainmentProfile::new(mode, true, capabilities)
}

fn native_containment_mode_for_skill(
    name: &str,
    mutation_kind: SkillMutationKind,
) -> NativeContainmentMode {
    if mutation_kind == SkillMutationKind::ReadOnly && audited_read_only_pc_control_skill(name) {
        // These skills are user-visible read-only operations, but they always append
        // forensic rows to `pc_control_actions`. They therefore need a writable DB
        // handle even though the functional route remains read-only.
        return NativeContainmentMode::Transactional;
    }

    match mutation_kind {
        SkillMutationKind::ReadOnly => NativeContainmentMode::ReadOnly,
        SkillMutationKind::Transactional => NativeContainmentMode::Transactional,
        SkillMutationKind::ExternalSideEffect => NativeContainmentMode::HostInteraction,
    }
}

fn audited_read_only_pc_control_skill(name: &str) -> bool {
    matches!(
        name,
        "clipboard_read" | "accessibility_tree" | "ocr_extract" | "list_windows" | "list_processes"
    )
}

fn privileges_for_skill(name: &str) -> Vec<String> {
    let values: &[&str] = match name {
        "convergence_check" => &[
            "Read agent convergence scores, levels, and safety metrics from the gateway database",
        ],
        "simulation_boundary_check" => {
            &["Inspect proposed text for simulation-boundary and policy-risk patterns"]
        }
        "attachment_monitor" => {
            &["Read attachment indicator history and safety telemetry from the gateway database"]
        }
        "reflection_write" => &["Write structured self-reflections to the gateway database"],
        "reflection_read" => &["Read stored self-reflections from the gateway database"],
        "git_status" | "git_diff" | "git_log" => {
            &["Read local Git repository metadata, commit history, and tracked file state"]
        }
        "git_branch" => &["Create, switch, or delete local Git branches in the current repository"],
        "git_commit" => &["Create new local Git commits from staged repository changes"],
        "git_stash" => {
            &["Modify the local Git working tree by creating, applying, or dropping stashes"]
        }
        "parse_ast" | "get_diagnostics" | "find_references" | "search_symbols" => {
            &["Read local source files for static analysis and code-intelligence queries"]
        }
        "format_code" => {
            &["Read and rewrite local source files when formatting is requested in place"]
        }
        "note_take" => {
            &["Create, read, update, delete, and search notes stored in the gateway database"]
        }
        "timer_set" => {
            &["Create, inspect, fire, and cancel reminders stored in the gateway database"]
        }
        "calendar_check" => &[
            "Call external calendar APIs with a caller-provided OAuth access token",
            "Read upcoming events and calendar metadata from the selected provider",
        ],
        "email_draft" => &[
            "Create, read, list, and delete email drafts stored in the gateway database",
            "Prepare outbound email content without sending it",
        ],
        "arxiv_search" => &["Call the public arXiv API to search research papers"],
        "github_search" => &[
            "Call GitHub search APIs, optionally with a caller-provided access token",
            "Read public repository, issue, or code-search results from GitHub",
        ],
        "doc_summarize" => {
            &["Read local text and markdown files to extract summaries and structural metadata"]
        }
        "csv_analyze" => &["Read local CSV files and compute schema, statistics, and sample rows"],
        "json_transform" => &["Inspect and transform JSON data supplied by the caller"],
        "sqlite_query" => &["Read arbitrary SQLite database files through read-only SQL queries"],
        "delegate_to_agent" => &[
            "Create delegated task records and convergence links in the gateway database",
            "Assign work to another registered agent",
        ],
        "agent_spawn_safe" => &[
            "Create child-agent delegation records and convergence links in the gateway database",
            "Propose a constrained sub-agent configuration for gateway-side spawning",
        ],
        "check_task_status" => &["Read delegated task state from the gateway database"],
        "cancel_task" => &[
            "Transition delegated task state in the gateway database",
            "Cancel or dispute an existing delegation",
        ],
        "mouse_move" | "mouse_click" | "mouse_drag" | "scroll" => &[
            "Control the local mouse pointer and scrolling on the host desktop",
            "Interact with real applications inside the configured safety boundaries",
        ],
        "keyboard_type" | "keyboard_hotkey" | "keyboard_press" => {
            &["Send real keyboard input and hotkeys to local desktop applications"]
        }
        "screenshot" => &["Capture the local screen contents"],
        "accessibility_tree" => {
            &["Read the local desktop accessibility tree and UI element metadata"]
        }
        "ocr_extract" => &["Capture screen content and extract visible text with OCR"],
        "list_windows" | "list_processes" => {
            &["Inspect running local windows and processes on the host desktop"]
        }
        "focus_window" | "resize_window" => &["Control focus and size of local desktop windows"],
        "launch_app" => &["Launch local desktop applications on the host machine"],
        "kill_process" => &["Terminate local processes on the host machine"],
        "clipboard_read" => &["Read the local system clipboard"],
        "clipboard_write" => &["Write text to the local system clipboard"],
        _ => &["Use the compiled skill pipeline through the gateway runtime"],
    };

    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_definitions_mark_non_removable_skills_as_always_on() {
        let seed = build_compiled_skill_definitions(&GhostConfig::default());

        let convergence = seed
            .definitions
            .iter()
            .find(|definition| definition.name == "convergence_check")
            .expect("compiled convergence_check definition");
        let note_take = seed
            .definitions
            .iter()
            .find(|definition| definition.name == "note_take")
            .expect("compiled note_take definition");

        assert!(convergence.always_on);
        assert!(!convergence.installable);
        assert!(note_take.installable);
        assert!(note_take.default_enabled);
        assert!(convergence.native_containment.is_some());
        assert!(note_take.native_containment.is_some());
    }

    #[test]
    fn runtime_seed_keeps_pc_control_skills_registered_when_disabled() {
        let config = GhostConfig::default();
        let seed = build_compiled_skill_definitions_with_runtime(
            &config,
            ghost_pc_control::safety::PcControlPolicyHandle::new(
                ghost_pc_control::safety::PcControlPolicySnapshot::from_config(&config.pc_control),
            ),
            config.pc_control.circuit_breaker(),
        );

        assert!(seed
            .definitions
            .iter()
            .any(|definition| definition.name == "mouse_move"));
        assert!(seed
            .definitions
            .iter()
            .any(|definition| definition.name == "launch_app"));
    }

    #[test]
    fn host_interacting_skills_declare_host_interaction_containment() {
        let mut config = GhostConfig::default();
        config.pc_control.enabled = true;
        let seed = build_compiled_skill_definitions(&config);
        let launch_app = seed
            .definitions
            .iter()
            .find(|definition| definition.name == "launch_app")
            .expect("compiled launch_app definition");
        let containment = launch_app
            .native_containment
            .as_ref()
            .expect("native containment profile");

        assert_eq!(containment.mode, NativeContainmentMode::HostInteraction);
        assert!(containment.audited);
        assert!(containment
            .allowed_capabilities
            .contains("host_interaction"));
    }

    #[test]
    fn audited_read_only_pc_control_skills_use_transactional_db_containment() {
        let mut config = GhostConfig::default();
        config.pc_control.enabled = true;
        let seed = build_compiled_skill_definitions(&config);
        let list_windows = seed
            .definitions
            .iter()
            .find(|definition| definition.name == "list_windows")
            .expect("compiled list_windows definition");
        let containment = list_windows
            .native_containment
            .as_ref()
            .expect("native containment profile");

        assert_eq!(list_windows.mutation_kind, SkillMutationKind::ReadOnly);
        assert_eq!(containment.mode, NativeContainmentMode::Transactional);
        assert!(containment.allowed_capabilities.contains("db_write"));
    }
}
