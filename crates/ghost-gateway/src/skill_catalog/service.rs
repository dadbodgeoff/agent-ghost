use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use cortex_storage::queries::external_skill_queries::{
    self, ExternalSkillArtifactRow, ExternalSkillInstallRow, ExternalSkillInstallState,
    ExternalSkillQuarantineRow, ExternalSkillQuarantineState, ExternalSkillVerificationRow,
    ExternalSkillVerificationStatus,
};
use cortex_storage::queries::skill_install_state_queries::{self, SkillInstallState};
use ghost_agent_loop::tools::skill_bridge::{CompiledSkillHandler, SkillHandler};
use rusqlite::Connection;

use super::definitions::{SkillDefinition, SkillExecutionMode, SkillMutationKind, SkillSourceKind};
use super::dto::{
    SkillInstallStateDto, SkillListResponseDto, SkillQuarantineStateDto, SkillStateDto,
    SkillSummaryDto, SkillVerificationStatusDto,
};
use super::external_runtime::ExternalWasmSkillHandler;
use crate::config::ExternalSkillsConfig;
use crate::db_pool::DbPool;
use crate::runtime_safety::ResolvedRuntimeAgent;
use crate::skill_ingest::SkillIngestService;

#[derive(Debug, thiserror::Error)]
pub enum SkillCatalogError {
    #[error("skill '{0}' not found")]
    SkillNotFound(String),
    #[error("skill identifier '{0}' is ambiguous")]
    AmbiguousSkillIdentifier(String),
    #[error("skill '{0}' cannot be installed")]
    NotInstallable(String),
    #[error("skill '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error("skill '{0}' is not installed")]
    NotInstalled(String),
    #[error("skill '{0}' cannot be uninstalled")]
    NotRemovable(String),
    #[error("skill '{0}' is not an external artifact")]
    NotExternalSkill(String),
    #[error("skill '{0}' is disabled")]
    SkillDisabled(String),
    #[error("skill '{0}' is verified but runtime execution is unavailable")]
    ExecutionUnavailable(String),
    #[error("skill '{0}' failed verification")]
    VerificationFailed(String),
    #[error("skill '{0}' is not quarantined")]
    NotQuarantined(String),
    #[error("skill '{skill_id}' quarantine revision is stale (expected {expected_revision}, actual {actual_revision})")]
    StaleQuarantineRevision {
        skill_id: String,
        expected_revision: i64,
        actual_revision: i64,
    },
    #[error("skill '{skill_id}' is quarantined: {reason}")]
    SkillQuarantined { skill_id: String, reason: String },
    #[error("skill '{skill_name}' is not enabled for agent '{agent_name}'")]
    NotEnabledForAgent {
        skill_name: String,
        agent_name: String,
    },
    #[error("database pool error: {0}")]
    DbPool(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("skill ingest error: {0}")]
    Ingest(String),
}

#[derive(Clone)]
pub struct ResolvedSkill {
    pub metadata: ResolvedSkillMetadata,
    pub compiled_skill: Option<Arc<dyn ghost_skills::skill::Skill>>,
    pub handler: Arc<dyn SkillHandler>,
}

#[derive(Clone)]
pub struct ResolvedSkillMetadata {
    pub name: String,
    pub policy_capability: String,
    pub mutation_kind: SkillMutationKind,
    pub native_containment: Option<ghost_skills::sandbox::native_sandbox::NativeContainmentProfile>,
}

#[derive(Clone, Default)]
pub struct ResolvedSkillSet {
    pub skills: Arc<HashMap<String, Arc<dyn SkillHandler>>>,
    pub granted_policy_capabilities: Vec<String>,
    pub visible_skill_names: Vec<String>,
}

#[derive(Clone)]
pub struct SkillCatalogService {
    definitions: BTreeMap<String, Arc<SkillDefinition>>,
    db: Arc<DbPool>,
    ingest: SkillIngestService,
}

#[derive(Clone)]
enum CatalogLookup {
    Compiled {
        definition: Arc<SkillDefinition>,
        install_state: Option<SkillInstallState>,
    },
    External(Box<ExternalCatalogEntry>),
}

#[derive(Debug, Clone)]
struct ExternalCatalogEntry {
    artifact: ExternalSkillArtifactRow,
    verification: Option<ExternalSkillVerificationRow>,
    quarantine: Option<ExternalSkillQuarantineRow>,
    install: Option<ExternalSkillInstallRow>,
}

impl SkillCatalogService {
    pub async fn new(
        definitions: Vec<SkillDefinition>,
        db: Arc<DbPool>,
        external_skills: ExternalSkillsConfig,
    ) -> Result<Self, SkillCatalogError> {
        let service = Self {
            definitions: definitions
                .into_iter()
                .map(|definition| (definition.name.clone(), Arc::new(definition)))
                .collect(),
            db: Arc::clone(&db),
            ingest: SkillIngestService::new(db, external_skills),
        };
        service.seed_default_install_state().await?;
        service
            .ingest
            .seed_trusted_signers()
            .await
            .map_err(|error| SkillCatalogError::Ingest(error.to_string()))?;
        Ok(service)
    }

    pub fn empty_for_tests(db: Arc<DbPool>) -> Self {
        Self {
            definitions: BTreeMap::new(),
            ingest: SkillIngestService::new(Arc::clone(&db), ExternalSkillsConfig::default()),
            db,
        }
    }

    pub async fn rescan_external_skills(&self, actor: &str) -> Result<(), SkillCatalogError> {
        self.ingest
            .seed_trusted_signers()
            .await
            .map_err(|error| SkillCatalogError::Ingest(error.to_string()))?;
        self.ingest
            .scan_approved_roots(actor)
            .await
            .map_err(|error| SkillCatalogError::Ingest(error.to_string()))?;
        Ok(())
    }

    pub fn list_skills(&self) -> Result<SkillListResponseDto, SkillCatalogError> {
        let compiled_states = self.load_install_states()?;
        let external = self.load_external_catalog()?;
        let mut installed = Vec::new();
        let mut available = Vec::new();

        for definition in self.definitions.values() {
            let summary = self.summary_from_definition(
                definition,
                compiled_states.get(definition.name.as_str()),
                None,
            );
            match summary.state {
                SkillStateDto::AlwaysOn | SkillStateDto::Installed => installed.push(summary),
                SkillStateDto::Available
                | SkillStateDto::Disabled
                | SkillStateDto::Verified
                | SkillStateDto::Quarantined
                | SkillStateDto::VerificationFailed => available.push(summary),
            }
        }

        for entry in &external {
            let summary = self.summary_from_external(entry, None)?;
            match summary.state {
                SkillStateDto::AlwaysOn | SkillStateDto::Installed => installed.push(summary),
                SkillStateDto::Available
                | SkillStateDto::Disabled
                | SkillStateDto::Verified
                | SkillStateDto::Quarantined
                | SkillStateDto::VerificationFailed => available.push(summary),
            }
        }

        installed.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.id.cmp(&right.id))
        });
        available.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(SkillListResponseDto {
            installed,
            available,
        })
    }

    pub fn get_skill(&self, identifier: &str) -> Result<SkillSummaryDto, SkillCatalogError> {
        let compiled_states = self.load_install_states()?;
        let external = self.load_external_catalog()?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled {
                definition,
                install_state,
            } => Ok(self.summary_from_definition(&definition, install_state.as_ref(), None)),
            CatalogLookup::External(entry) => self.summary_from_external(&entry, None),
        }
    }

    pub fn install_with_conn(
        &self,
        conn: &Connection,
        identifier: &str,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let compiled_states = self.load_install_states_with_conn(conn)?;
        let external = self.load_external_catalog_with_conn(conn)?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled {
                definition,
                install_state,
            } => self.install_compiled_with_conn(conn, &definition, install_state, actor),
            CatalogLookup::External(entry) => self.install_external_with_conn(conn, *entry, actor),
        }
    }

    pub fn uninstall_with_conn(
        &self,
        conn: &Connection,
        identifier: &str,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let compiled_states = self.load_install_states_with_conn(conn)?;
        let external = self.load_external_catalog_with_conn(conn)?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled {
                definition,
                install_state,
            } => self.uninstall_compiled_with_conn(conn, &definition, install_state, actor),
            CatalogLookup::External(entry) => {
                self.uninstall_external_with_conn(conn, *entry, actor)
            }
        }
    }

    pub fn quarantine_with_conn(
        &self,
        conn: &Connection,
        identifier: &str,
        reason: &str,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let trimmed_reason = reason.trim();
        if trimmed_reason.is_empty() {
            return Err(SkillCatalogError::Storage(
                "quarantine reason cannot be empty".to_string(),
            ));
        }

        let compiled_states = self.load_install_states_with_conn(conn)?;
        let external = self.load_external_catalog_with_conn(conn)?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled { .. } => {
                Err(SkillCatalogError::NotExternalSkill(identifier.to_string()))
            }
            CatalogLookup::External(mut entry) => {
                external_skill_queries::upsert_external_skill_quarantine(
                    conn,
                    &entry.artifact.artifact_digest,
                    ExternalSkillQuarantineState::Quarantined,
                    Some("operator_quarantine"),
                    Some(trimmed_reason),
                    actor,
                )
                .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
                entry.quarantine = external_skill_queries::get_external_skill_quarantine(
                    conn,
                    &entry.artifact.artifact_digest,
                )
                .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
                self.summary_from_external(&entry, None)
            }
        }
    }

    pub fn resolve_quarantine_with_conn(
        &self,
        conn: &Connection,
        identifier: &str,
        expected_revision: i64,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let compiled_states = self.load_install_states_with_conn(conn)?;
        let external = self.load_external_catalog_with_conn(conn)?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled { .. } => {
                Err(SkillCatalogError::NotExternalSkill(identifier.to_string()))
            }
            CatalogLookup::External(mut entry) => {
                let quarantine = entry.quarantine.as_ref().ok_or_else(|| {
                    SkillCatalogError::NotQuarantined(entry.artifact.artifact_digest.clone())
                })?;
                if quarantine.state != ExternalSkillQuarantineState::Quarantined {
                    return Err(SkillCatalogError::NotQuarantined(
                        entry.artifact.artifact_digest.clone(),
                    ));
                }
                if self.external_verification_status(&entry)?
                    != SkillVerificationStatusDto::Verified
                {
                    return Err(SkillCatalogError::VerificationFailed(
                        entry.artifact.artifact_digest.clone(),
                    ));
                }

                let cleared = external_skill_queries::clear_external_skill_quarantine(
                    conn,
                    &entry.artifact.artifact_digest,
                    expected_revision,
                    actor,
                )
                .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
                if !cleared {
                    let current = external_skill_queries::get_external_skill_quarantine(
                        conn,
                        &entry.artifact.artifact_digest,
                    )
                    .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
                    match current {
                        Some(current)
                            if current.state == ExternalSkillQuarantineState::Quarantined =>
                        {
                            return Err(SkillCatalogError::StaleQuarantineRevision {
                                skill_id: entry.artifact.artifact_digest.clone(),
                                expected_revision,
                                actual_revision: current.revision,
                            });
                        }
                        _ => {
                            return Err(SkillCatalogError::NotQuarantined(
                                entry.artifact.artifact_digest.clone(),
                            ));
                        }
                    }
                }

                entry.quarantine = external_skill_queries::get_external_skill_quarantine(
                    conn,
                    &entry.artifact.artifact_digest,
                )
                .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
                self.summary_from_external(&entry, None)
            }
        }
    }

    pub async fn reverify_external_skill(
        &self,
        identifier: &str,
        actor: &str,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let compiled_states = self.load_install_states()?;
        let external = self.load_external_catalog()?;
        let digest = match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled { .. } => {
                return Err(SkillCatalogError::NotExternalSkill(identifier.to_string()));
            }
            CatalogLookup::External(entry) => entry.artifact.artifact_digest,
        };

        self.ingest
            .seed_trusted_signers()
            .await
            .map_err(|error| SkillCatalogError::Ingest(error.to_string()))?;
        self.ingest
            .reverify_managed_artifact(&digest, actor)
            .await
            .map_err(|error| SkillCatalogError::Ingest(error.to_string()))?;
        self.get_skill(&digest)
    }

    pub fn resolve_for_runtime(
        &self,
        agent: &ResolvedRuntimeAgent,
        allowlist_override: Option<&[String]>,
    ) -> Result<ResolvedSkillSet, SkillCatalogError> {
        let states = self.load_install_states()?;
        let external = self.load_external_catalog()?;
        let mut skills = HashMap::new();
        let mut granted_policy_capabilities = Vec::new();
        let mut visible_skill_names = Vec::new();
        let allowlist = allowlist_override.or(agent.skill_allowlist.as_deref());

        for definition in self.definitions.values() {
            if !self.is_runtime_enabled(definition, states.get(definition.name.as_str())) {
                continue;
            }

            if !definition.always_on
                && allowlist
                    .is_some_and(|allowed| !allowed.iter().any(|name| name == &definition.name))
            {
                continue;
            }

            skills.insert(
                definition.name.clone(),
                Arc::new(CompiledSkillHandler::new(Arc::clone(&definition.skill)))
                    as Arc<dyn SkillHandler>,
            );
            granted_policy_capabilities.push(definition.policy_capability.clone());
            visible_skill_names.push(definition.name.clone());
        }

        for entry in &external {
            if !self.external_runtime_visible(entry)? {
                continue;
            }
            if allowlist.is_some_and(|allowed| {
                !allowed
                    .iter()
                    .any(|skill_name| skill_name == &entry.artifact.skill_name)
            }) {
                continue;
            }
            if skills.contains_key(&entry.artifact.skill_name) {
                tracing::warn!(
                    skill_name = %entry.artifact.skill_name,
                    artifact_digest = %entry.artifact.artifact_digest,
                    "external skill runtime exposure blocked due to name collision"
                );
                continue;
            }

            skills.insert(
                entry.artifact.skill_name.clone(),
                self.external_runtime_handler(entry),
            );
            granted_policy_capabilities.push(format!("skill:{}", entry.artifact.skill_name));
            visible_skill_names.push(entry.artifact.skill_name.clone());
        }

        granted_policy_capabilities.sort();
        granted_policy_capabilities.dedup();
        visible_skill_names.sort();

        Ok(ResolvedSkillSet {
            skills: Arc::new(skills),
            granted_policy_capabilities,
            visible_skill_names,
        })
    }

    pub fn resolve_for_execute(
        &self,
        identifier: &str,
        agent: &ResolvedRuntimeAgent,
    ) -> Result<ResolvedSkill, SkillCatalogError> {
        let compiled_states = self.load_install_states()?;
        let external = self.load_external_catalog()?;
        match self.resolve_catalog_entry(identifier, &compiled_states, &external)? {
            CatalogLookup::Compiled {
                definition,
                install_state,
            } => self.resolve_compiled_for_execute(identifier, &definition, install_state, agent),
            CatalogLookup::External(entry) => {
                self.resolve_external_for_execute(identifier, &entry, agent)
            }
        }
    }

    fn install_compiled_with_conn(
        &self,
        conn: &Connection,
        definition: &Arc<SkillDefinition>,
        install_state: Option<SkillInstallState>,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        if !definition.installable {
            return Err(SkillCatalogError::NotInstallable(definition.name.clone()));
        }
        if matches!(install_state, Some(SkillInstallState::Installed)) {
            return Err(SkillCatalogError::AlreadyInstalled(definition.name.clone()));
        }

        skill_install_state_queries::upsert_skill_install_state(
            conn,
            &definition.name,
            SkillInstallState::Installed,
            actor,
        )
        .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;

        Ok(self.summary_from_definition(definition, Some(&SkillInstallState::Installed), None))
    }

    fn uninstall_compiled_with_conn(
        &self,
        conn: &Connection,
        definition: &Arc<SkillDefinition>,
        install_state: Option<SkillInstallState>,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        if !definition.removable || definition.always_on {
            return Err(SkillCatalogError::NotRemovable(definition.name.clone()));
        }
        if !matches!(install_state, Some(SkillInstallState::Installed)) {
            return Err(SkillCatalogError::NotInstalled(definition.name.clone()));
        }

        skill_install_state_queries::upsert_skill_install_state(
            conn,
            &definition.name,
            SkillInstallState::Disabled,
            actor,
        )
        .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;

        Ok(self.summary_from_definition(definition, Some(&SkillInstallState::Disabled), None))
    }

    fn install_external_with_conn(
        &self,
        conn: &Connection,
        mut entry: ExternalCatalogEntry,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let skill_id = entry.artifact.artifact_digest.clone();
        let verification_status = self.external_verification_status(&entry)?;
        if self.external_quarantine_state(&entry, verification_status)
            == SkillQuarantineStateDto::Quarantined
        {
            return Err(SkillCatalogError::SkillQuarantined {
                skill_id,
                reason: self
                    .external_quarantine_reason(&entry, verification_status)
                    .unwrap_or_else(|| "external skill is quarantined".to_string()),
            });
        }
        if verification_status != SkillVerificationStatusDto::Verified {
            return Err(SkillCatalogError::VerificationFailed(
                entry.artifact.artifact_digest.clone(),
            ));
        }
        if matches!(
            entry.install.as_ref().map(|row| row.state),
            Some(ExternalSkillInstallState::Installed)
        ) {
            return Err(SkillCatalogError::AlreadyInstalled(
                entry.artifact.artifact_digest.clone(),
            ));
        }

        external_skill_queries::upsert_external_skill_install_state(
            conn,
            &entry.artifact.artifact_digest,
            &entry.artifact.skill_name,
            &entry.artifact.skill_version,
            ExternalSkillInstallState::Installed,
            actor,
        )
        .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
        external_skill_queries::disable_other_installed_external_versions(
            conn,
            &entry.artifact.skill_name,
            &entry.artifact.artifact_digest,
            actor,
        )
        .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;

        entry.install = Some(ExternalSkillInstallRow {
            artifact_digest: entry.artifact.artifact_digest.clone(),
            skill_name: entry.artifact.skill_name.clone(),
            skill_version: entry.artifact.skill_version.clone(),
            state: ExternalSkillInstallState::Installed,
            updated_at: String::new(),
            updated_by: actor.map(str::to_string),
        });

        self.summary_from_external(&entry, None)
    }

    fn uninstall_external_with_conn(
        &self,
        conn: &Connection,
        mut entry: ExternalCatalogEntry,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        if !matches!(
            entry.install.as_ref().map(|row| row.state),
            Some(ExternalSkillInstallState::Installed)
        ) {
            return Err(SkillCatalogError::NotInstalled(
                entry.artifact.artifact_digest.clone(),
            ));
        }

        external_skill_queries::upsert_external_skill_install_state(
            conn,
            &entry.artifact.artifact_digest,
            &entry.artifact.skill_name,
            &entry.artifact.skill_version,
            ExternalSkillInstallState::Disabled,
            actor,
        )
        .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;

        entry.install = Some(ExternalSkillInstallRow {
            artifact_digest: entry.artifact.artifact_digest.clone(),
            skill_name: entry.artifact.skill_name.clone(),
            skill_version: entry.artifact.skill_version.clone(),
            state: ExternalSkillInstallState::Disabled,
            updated_at: String::new(),
            updated_by: actor.map(str::to_string),
        });

        self.summary_from_external(&entry, None)
    }

    fn resolve_compiled_for_execute(
        &self,
        identifier: &str,
        definition: &Arc<SkillDefinition>,
        install_state: Option<SkillInstallState>,
        agent: &ResolvedRuntimeAgent,
    ) -> Result<ResolvedSkill, SkillCatalogError> {
        if !self.is_runtime_enabled(definition, install_state.as_ref()) {
            return Err(SkillCatalogError::SkillDisabled(identifier.to_string()));
        }
        if definition.execution_mode != SkillExecutionMode::Native {
            return Err(SkillCatalogError::ExecutionUnavailable(
                identifier.to_string(),
            ));
        }
        if !definition.always_on
            && agent.skill_allowlist.as_deref().is_some_and(|allowed| {
                !allowed
                    .iter()
                    .any(|skill_name| skill_name == &definition.name)
            })
        {
            return Err(SkillCatalogError::NotEnabledForAgent {
                skill_name: definition.name.clone(),
                agent_name: agent.name.clone(),
            });
        }

        Ok(ResolvedSkill {
            metadata: ResolvedSkillMetadata {
                name: definition.name.clone(),
                policy_capability: definition.policy_capability.clone(),
                mutation_kind: definition.mutation_kind,
                native_containment: definition.native_containment.clone(),
            },
            compiled_skill: Some(Arc::clone(&definition.skill)),
            handler: Arc::new(CompiledSkillHandler::new(Arc::clone(&definition.skill))),
        })
    }

    fn resolve_external_for_execute(
        &self,
        identifier: &str,
        entry: &ExternalCatalogEntry,
        agent: &ResolvedRuntimeAgent,
    ) -> Result<ResolvedSkill, SkillCatalogError> {
        let skill_id = entry.artifact.artifact_digest.clone();
        let verification_status = self.external_verification_status(entry)?;
        if self.external_quarantine_state(entry, verification_status)
            == SkillQuarantineStateDto::Quarantined
        {
            return Err(SkillCatalogError::SkillQuarantined {
                skill_id,
                reason: self
                    .external_quarantine_reason(entry, verification_status)
                    .unwrap_or_else(|| "external skill is quarantined".to_string()),
            });
        }
        if verification_status != SkillVerificationStatusDto::Verified {
            return Err(SkillCatalogError::VerificationFailed(
                identifier.to_string(),
            ));
        }
        match entry.install.as_ref().map(|row| row.state) {
            Some(ExternalSkillInstallState::Installed) => Ok(()),
            Some(ExternalSkillInstallState::Disabled) => Err(SkillCatalogError::SkillDisabled(
                entry.artifact.artifact_digest.clone(),
            )),
            None => Err(SkillCatalogError::NotInstalled(
                entry.artifact.artifact_digest.clone(),
            )),
        }?;
        if !self.external_runtime_supported(entry)? {
            return Err(SkillCatalogError::ExecutionUnavailable(
                entry.artifact.artifact_digest.clone(),
            ));
        }
        if agent.skill_allowlist.as_deref().is_some_and(|allowed| {
            !allowed
                .iter()
                .any(|skill_name| skill_name == &entry.artifact.skill_name)
        }) {
            return Err(SkillCatalogError::NotEnabledForAgent {
                skill_name: entry.artifact.skill_name.clone(),
                agent_name: agent.name.clone(),
            });
        }

        Ok(ResolvedSkill {
            metadata: ResolvedSkillMetadata {
                name: entry.artifact.skill_name.clone(),
                policy_capability: format!("skill:{}", entry.artifact.skill_name),
                mutation_kind: SkillMutationKind::ReadOnly,
                native_containment: None,
            },
            compiled_skill: None,
            handler: self.external_runtime_handler(entry),
        })
    }

    fn resolve_catalog_entry(
        &self,
        identifier: &str,
        compiled_states: &HashMap<String, SkillInstallState>,
        external: &[ExternalCatalogEntry],
    ) -> Result<CatalogLookup, SkillCatalogError> {
        if let Some(definition) = self.definitions.get(identifier).cloned() {
            return Ok(CatalogLookup::Compiled {
                definition,
                install_state: compiled_states.get(identifier).copied(),
            });
        }

        if let Some(entry) = external
            .iter()
            .find(|entry| entry.artifact.artifact_digest == identifier)
            .cloned()
        {
            return Ok(CatalogLookup::External(Box::new(entry)));
        }

        let mut matches = external
            .iter()
            .filter(|entry| entry.artifact.skill_name == identifier)
            .cloned();
        match (matches.next(), matches.next()) {
            (Some(entry), None) => Ok(CatalogLookup::External(Box::new(entry))),
            (Some(_), Some(_)) => Err(SkillCatalogError::AmbiguousSkillIdentifier(
                identifier.to_string(),
            )),
            (None, _) => Err(SkillCatalogError::SkillNotFound(identifier.to_string())),
        }
    }

    fn load_install_states(&self) -> Result<HashMap<String, SkillInstallState>, SkillCatalogError> {
        let db = self
            .db
            .read()
            .map_err(|error| SkillCatalogError::DbPool(error.to_string()))?;
        self.load_install_states_with_conn(&db)
    }

    fn load_install_states_with_conn(
        &self,
        conn: &Connection,
    ) -> Result<HashMap<String, SkillInstallState>, SkillCatalogError> {
        let rows = skill_install_state_queries::list_skill_install_states(conn)
            .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| (row.skill_name, row.state))
            .collect())
    }

    fn load_external_catalog(&self) -> Result<Vec<ExternalCatalogEntry>, SkillCatalogError> {
        let db = self
            .db
            .read()
            .map_err(|error| SkillCatalogError::DbPool(error.to_string()))?;
        self.load_external_catalog_with_conn(&db)
    }

    fn load_external_catalog_with_conn(
        &self,
        conn: &Connection,
    ) -> Result<Vec<ExternalCatalogEntry>, SkillCatalogError> {
        let artifacts = external_skill_queries::list_external_skill_artifacts(conn)
            .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
        let mut verifications = external_skill_queries::list_external_skill_verifications(conn)
            .map_err(|error| SkillCatalogError::Storage(error.to_string()))?
            .into_iter()
            .map(|row| (row.artifact_digest.clone(), row))
            .collect::<HashMap<_, _>>();
        let mut quarantine = external_skill_queries::list_external_skill_quarantine(conn)
            .map_err(|error| SkillCatalogError::Storage(error.to_string()))?
            .into_iter()
            .map(|row| (row.artifact_digest.clone(), row))
            .collect::<HashMap<_, _>>();
        let mut install = external_skill_queries::list_external_skill_install_states(conn)
            .map_err(|error| SkillCatalogError::Storage(error.to_string()))?
            .into_iter()
            .map(|row| (row.artifact_digest.clone(), row))
            .collect::<HashMap<_, _>>();

        Ok(artifacts
            .into_iter()
            .map(|artifact| {
                let digest = artifact.artifact_digest.clone();
                ExternalCatalogEntry {
                    artifact,
                    verification: verifications.remove(&digest),
                    quarantine: quarantine.remove(&digest),
                    install: install.remove(&digest),
                }
            })
            .collect())
    }

    async fn seed_default_install_state(&self) -> Result<(), SkillCatalogError> {
        let db = self.db.write().await;
        for definition in self.definitions.values() {
            if definition.installable && definition.default_enabled {
                skill_install_state_queries::seed_skill_install_state(
                    &db,
                    &definition.name,
                    SkillInstallState::Installed,
                )
                .map_err(|error| SkillCatalogError::Storage(error.to_string()))?;
            }
        }
        Ok(())
    }

    fn is_runtime_enabled(
        &self,
        definition: &SkillDefinition,
        install_state: Option<&SkillInstallState>,
    ) -> bool {
        if definition.always_on {
            return true;
        }

        definition.installable && matches!(install_state, Some(SkillInstallState::Installed))
    }

    fn summary_from_definition(
        &self,
        definition: &SkillDefinition,
        install_state: Option<&SkillInstallState>,
        enabled_for_agent: Option<bool>,
    ) -> SkillSummaryDto {
        let runtime_visible = self.is_runtime_enabled(definition, install_state);
        let state = if definition.always_on {
            SkillStateDto::AlwaysOn
        } else {
            match install_state {
                Some(SkillInstallState::Installed) => SkillStateDto::Installed,
                Some(SkillInstallState::Disabled) | None => SkillStateDto::Available,
            }
        };

        let install_state = if definition.always_on {
            SkillInstallStateDto::AlwaysOn
        } else {
            match install_state {
                Some(SkillInstallState::Installed) => SkillInstallStateDto::Installed,
                Some(SkillInstallState::Disabled) => SkillInstallStateDto::Disabled,
                None => SkillInstallStateDto::NotInstalled,
            }
        };

        SkillSummaryDto {
            id: definition.name.clone(),
            name: definition.name.clone(),
            version: definition.version.clone(),
            description: definition.description.clone(),
            source: definition.source,
            removable: definition.removable,
            installable: definition.installable,
            execution_mode: definition.execution_mode,
            policy_capability: definition.policy_capability.clone(),
            privileges: definition.privileges.clone(),
            requested_capabilities: Vec::new(),
            mutation_kind: definition.mutation_kind,
            state,
            install_state,
            verification_status: SkillVerificationStatusDto::NotApplicable,
            quarantine_state: SkillQuarantineStateDto::Clear,
            runtime_visible,
            artifact_digest: None,
            publisher: None,
            source_uri: None,
            signer_key_id: None,
            signer_publisher: None,
            quarantine_reason: None,
            quarantine_revision: None,
            enabled_for_agent,
            capabilities: vec![definition.policy_capability.clone()],
        }
    }

    fn summary_from_external(
        &self,
        entry: &ExternalCatalogEntry,
        enabled_for_agent: Option<bool>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let requested_capabilities = self.external_requested_capabilities(entry)?;
        let privileges = self.parse_json_vec(
            &entry.artifact.declared_privileges,
            "declared_privileges",
            &entry.artifact.artifact_digest,
        )?;
        let execution_mode = self.execution_mode_from_db(&entry.artifact.execution_mode)?;
        let source = self.source_kind_from_db(&entry.artifact.source_kind)?;
        let verification_status =
            self.external_verification_status_with_requested(entry, &requested_capabilities);
        let quarantine_state = self.external_quarantine_state(entry, verification_status);
        let install_state = self.external_install_state(entry);
        let runtime_visible = install_state == SkillInstallStateDto::Installed
            && verification_status == SkillVerificationStatusDto::Verified
            && quarantine_state == SkillQuarantineStateDto::Clear
            && self.external_runtime_supported_with_requested(entry, &requested_capabilities);
        let state = if quarantine_state == SkillQuarantineStateDto::Quarantined {
            SkillStateDto::Quarantined
        } else if verification_status != SkillVerificationStatusDto::Verified {
            SkillStateDto::VerificationFailed
        } else {
            match install_state {
                SkillInstallStateDto::Installed => SkillStateDto::Installed,
                SkillInstallStateDto::Disabled => SkillStateDto::Disabled,
                SkillInstallStateDto::NotInstalled => SkillStateDto::Verified,
                SkillInstallStateDto::AlwaysOn => SkillStateDto::AlwaysOn,
            }
        };
        let policy_capability = format!("skill:{}", entry.artifact.skill_name);

        Ok(SkillSummaryDto {
            id: entry.artifact.artifact_digest.clone(),
            name: entry.artifact.skill_name.clone(),
            version: entry.artifact.skill_version.clone(),
            description: entry.artifact.description.clone(),
            source,
            removable: true,
            installable: true,
            execution_mode,
            policy_capability: policy_capability.clone(),
            privileges,
            requested_capabilities: requested_capabilities.clone(),
            mutation_kind: if requested_capabilities.is_empty() {
                SkillMutationKind::ReadOnly
            } else {
                SkillMutationKind::ExternalSideEffect
            },
            state,
            install_state,
            verification_status,
            quarantine_state,
            runtime_visible,
            artifact_digest: Some(entry.artifact.artifact_digest.clone()),
            publisher: Some(entry.artifact.publisher.clone()),
            source_uri: Some(entry.artifact.source_uri.clone()),
            signer_key_id: entry
                .verification
                .as_ref()
                .and_then(|row| row.signer_key_id.clone())
                .or_else(|| entry.artifact.signer_key_id.clone()),
            signer_publisher: entry
                .verification
                .as_ref()
                .and_then(|row| row.signer_publisher.clone()),
            quarantine_reason: self.external_quarantine_reason(entry, verification_status),
            quarantine_revision: entry.quarantine.as_ref().map(|row| row.revision),
            enabled_for_agent,
            capabilities: vec![policy_capability],
        })
    }

    fn external_requested_capabilities(
        &self,
        entry: &ExternalCatalogEntry,
    ) -> Result<Vec<String>, SkillCatalogError> {
        self.parse_json_vec(
            &entry.artifact.requested_capabilities,
            "requested_capabilities",
            &entry.artifact.artifact_digest,
        )
    }

    fn parse_json_vec(
        &self,
        raw: &str,
        field: &str,
        artifact_digest: &str,
    ) -> Result<Vec<String>, SkillCatalogError> {
        serde_json::from_str::<Vec<String>>(raw).map_err(|error| {
            SkillCatalogError::Storage(format!(
                "failed to parse {field} for external skill artifact {artifact_digest}: {error}"
            ))
        })
    }

    fn source_kind_from_db(&self, value: &str) -> Result<SkillSourceKind, SkillCatalogError> {
        match value {
            "compiled" => Ok(SkillSourceKind::Compiled),
            "user" => Ok(SkillSourceKind::User),
            "workspace" => Ok(SkillSourceKind::Workspace),
            other => Err(SkillCatalogError::Storage(format!(
                "unknown skill source kind '{other}'"
            ))),
        }
    }

    fn execution_mode_from_db(&self, value: &str) -> Result<SkillExecutionMode, SkillCatalogError> {
        match value {
            "native" => Ok(SkillExecutionMode::Native),
            "wasm" => Ok(SkillExecutionMode::Wasm),
            other => Err(SkillCatalogError::Storage(format!(
                "unknown skill execution mode '{other}'"
            ))),
        }
    }

    fn stored_external_verification_status(
        &self,
        entry: &ExternalCatalogEntry,
    ) -> SkillVerificationStatusDto {
        match entry.verification.as_ref().map(|row| row.status) {
            Some(ExternalSkillVerificationStatus::Verified) => SkillVerificationStatusDto::Verified,
            Some(ExternalSkillVerificationStatus::ValidationFailed) => {
                SkillVerificationStatusDto::ValidationFailed
            }
            Some(ExternalSkillVerificationStatus::DigestMismatch) => {
                SkillVerificationStatusDto::DigestMismatch
            }
            Some(ExternalSkillVerificationStatus::MissingSignature) => {
                SkillVerificationStatusDto::MissingSignature
            }
            Some(ExternalSkillVerificationStatus::InvalidSignature) => {
                SkillVerificationStatusDto::InvalidSignature
            }
            Some(ExternalSkillVerificationStatus::UnknownSigner) => {
                SkillVerificationStatusDto::UnknownSigner
            }
            Some(ExternalSkillVerificationStatus::RevokedSigner) => {
                SkillVerificationStatusDto::RevokedSigner
            }
            Some(ExternalSkillVerificationStatus::UnsupportedCapability) => {
                SkillVerificationStatusDto::UnsupportedCapability
            }
            Some(ExternalSkillVerificationStatus::UnsupportedExecutionMode) => {
                SkillVerificationStatusDto::UnsupportedExecutionMode
            }
            None => SkillVerificationStatusDto::ValidationFailed,
        }
    }

    fn external_verification_status(
        &self,
        entry: &ExternalCatalogEntry,
    ) -> Result<SkillVerificationStatusDto, SkillCatalogError> {
        let requested_capabilities = self.external_requested_capabilities(entry)?;
        Ok(self.external_verification_status_with_requested(entry, &requested_capabilities))
    }

    fn external_verification_status_with_requested(
        &self,
        entry: &ExternalCatalogEntry,
        requested_capabilities: &[String],
    ) -> SkillVerificationStatusDto {
        let stored = self.stored_external_verification_status(entry);
        if stored != SkillVerificationStatusDto::Verified {
            return stored;
        }
        if entry.artifact.execution_mode != "wasm" {
            return SkillVerificationStatusDto::UnsupportedExecutionMode;
        }
        if !requested_capabilities.is_empty() {
            return SkillVerificationStatusDto::UnsupportedCapability;
        }
        stored
    }

    fn external_quarantine_state(
        &self,
        entry: &ExternalCatalogEntry,
        verification_status: SkillVerificationStatusDto,
    ) -> SkillQuarantineStateDto {
        match entry.quarantine.as_ref().map(|row| row.state) {
            Some(ExternalSkillQuarantineState::Clear) => SkillQuarantineStateDto::Clear,
            Some(ExternalSkillQuarantineState::Quarantined) => SkillQuarantineStateDto::Quarantined,
            None if verification_status == SkillVerificationStatusDto::Verified => {
                SkillQuarantineStateDto::Quarantined
            }
            None => SkillQuarantineStateDto::Clear,
        }
    }

    fn external_quarantine_reason(
        &self,
        entry: &ExternalCatalogEntry,
        verification_status: SkillVerificationStatusDto,
    ) -> Option<String> {
        match &entry.quarantine {
            Some(row) if row.state == ExternalSkillQuarantineState::Quarantined => row
                .reason_detail
                .clone()
                .or_else(|| row.reason_code.clone()),
            None if verification_status == SkillVerificationStatusDto::Verified => {
                Some("catalog quarantine state is missing".to_string())
            }
            _ => None,
        }
    }

    fn external_install_state(&self, entry: &ExternalCatalogEntry) -> SkillInstallStateDto {
        match entry.install.as_ref().map(|row| row.state) {
            Some(ExternalSkillInstallState::Installed) => SkillInstallStateDto::Installed,
            Some(ExternalSkillInstallState::Disabled) => SkillInstallStateDto::Disabled,
            None => SkillInstallStateDto::NotInstalled,
        }
    }

    fn external_runtime_visible(
        &self,
        entry: &ExternalCatalogEntry,
    ) -> Result<bool, SkillCatalogError> {
        let requested_capabilities = self.external_requested_capabilities(entry)?;
        let verification_status =
            self.external_verification_status_with_requested(entry, &requested_capabilities);
        let quarantine_state = self.external_quarantine_state(entry, verification_status);
        Ok(
            self.external_install_state(entry) == SkillInstallStateDto::Installed
                && verification_status == SkillVerificationStatusDto::Verified
                && quarantine_state == SkillQuarantineStateDto::Clear
                && self.external_runtime_supported_with_requested(entry, &requested_capabilities),
        )
    }

    fn external_runtime_supported(
        &self,
        entry: &ExternalCatalogEntry,
    ) -> Result<bool, SkillCatalogError> {
        let requested_capabilities = self.external_requested_capabilities(entry)?;
        Ok(self.external_runtime_supported_with_requested(entry, &requested_capabilities))
    }

    fn external_runtime_supported_with_requested(
        &self,
        entry: &ExternalCatalogEntry,
        requested_capabilities: &[String],
    ) -> bool {
        self.ingest.execution_enabled()
            && entry.artifact.execution_mode == "wasm"
            && requested_capabilities.is_empty()
            && !self.definitions.contains_key(&entry.artifact.skill_name)
    }

    fn external_runtime_handler(&self, entry: &ExternalCatalogEntry) -> Arc<dyn SkillHandler> {
        Arc::new(ExternalWasmSkillHandler::new(
            entry.artifact.artifact_digest.clone(),
            entry.artifact.skill_name.clone(),
            entry.artifact.description.clone(),
            entry.artifact.managed_artifact_path.clone(),
            Arc::clone(&self.db),
            ghost_skills::sandbox::wasm_sandbox::WasmSandboxConfig::default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use ghost_skills::registry::SkillSource;
    use ghost_skills::skill::{Skill, SkillContext, SkillResult};

    use super::*;

    #[derive(Clone)]
    struct TestSkill {
        name: String,
        removable: bool,
    }

    impl Skill for TestSkill {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "test skill"
        }

        fn removable(&self) -> bool {
            self.removable
        }

        fn source(&self) -> SkillSource {
            SkillSource::Bundled
        }

        fn execute(&self, _ctx: &SkillContext<'_>, _input: &serde_json::Value) -> SkillResult {
            Ok(serde_json::json!({"ok": true}))
        }
    }

    struct TestHarness {
        _temp_dir: tempfile::TempDir,
        db: Arc<DbPool>,
        service: SkillCatalogService,
    }

    #[derive(Clone, Copy)]
    struct ExternalSeed<'a> {
        digest: &'a str,
        name: &'a str,
        version: &'a str,
        verification: ExternalSkillVerificationStatus,
        quarantine: ExternalSkillQuarantineState,
        install: Option<ExternalSkillInstallState>,
    }

    async fn test_harness(definitions: Vec<SkillDefinition>) -> TestHarness {
        test_harness_with_external_config(definitions, ExternalSkillsConfig::default()).await
    }

    async fn test_harness_with_external_config(
        definitions: Vec<SkillDefinition>,
        external_config: ExternalSkillsConfig,
    ) -> TestHarness {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("catalog.db");
        let db = crate::db_pool::create_pool(db_path).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }
        let service = SkillCatalogService::new(definitions, Arc::clone(&db), external_config)
            .await
            .unwrap();
        TestHarness {
            _temp_dir: temp_dir,
            db,
            service,
        }
    }

    fn compiled_definition(name: &str, removable: bool) -> SkillDefinition {
        SkillDefinition {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("compiled {name}"),
            source: SkillSourceKind::Compiled,
            removable,
            always_on: !removable,
            installable: removable,
            default_enabled: removable,
            execution_mode: SkillExecutionMode::Native,
            policy_capability: format!("skill:{name}"),
            privileges: vec!["compiled privilege".to_string()],
            mutation_kind: SkillMutationKind::ReadOnly,
            native_containment: Some(
                ghost_skills::sandbox::native_sandbox::NativeContainmentProfile::new(
                    ghost_skills::sandbox::native_sandbox::NativeContainmentMode::ReadOnly,
                    true,
                    ["skill_execute".to_string(), "db_read".to_string()],
                ),
            ),
            skill: Arc::new(TestSkill {
                name: name.to_string(),
                removable,
            }),
        }
    }

    fn seed_external_skill(conn: &Connection, seed: ExternalSeed<'_>) {
        external_skill_queries::upsert_external_skill_artifact(
            conn,
            seed.digest,
            1,
            seed.name,
            seed.version,
            "ghost-test",
            "external skill",
            "workspace",
            "wasm",
            "module.wasm",
            &format!("/source/{name}.ghostskill", name = seed.name),
            &format!(
                "/managed/{digest}/artifact.ghostskill",
                digest = seed.digest
            ),
            &format!("/managed/{digest}/module.wasm", digest = seed.digest),
            "{}",
            "[]",
            "[\"Pure WASM computation\"]",
            Some("key-1"),
            256,
        )
        .unwrap();
        external_skill_queries::upsert_external_skill_verification(
            conn,
            seed.digest,
            seed.verification,
            Some("key-1"),
            Some("ghost-test"),
            "{}",
        )
        .unwrap();
        external_skill_queries::upsert_external_skill_quarantine(
            conn,
            seed.digest,
            seed.quarantine,
            (seed.quarantine == ExternalSkillQuarantineState::Quarantined)
                .then_some("operator_quarantine"),
            (seed.quarantine == ExternalSkillQuarantineState::Quarantined)
                .then_some("manual quarantine"),
            Some("operator"),
        )
        .unwrap();
        if let Some(install) = seed.install {
            external_skill_queries::upsert_external_skill_install_state(
                conn,
                seed.digest,
                seed.name,
                seed.version,
                install,
                Some("operator"),
            )
            .unwrap();
        }
    }

    fn synthetic_agent() -> ResolvedRuntimeAgent {
        ResolvedRuntimeAgent {
            id: uuid::Uuid::now_v7(),
            name: "catalog-test-agent".to_string(),
            full_access: false,
            capabilities: Vec::new(),
            skill_allowlist: None,
            spending_cap: 5.0,
            isolation_mode: crate::config::IsolationMode::InProcess,
            sandbox_config: crate::config::AgentSandboxConfig::default(),
        }
    }

    #[tokio::test]
    async fn list_skills_merges_compiled_and_external_truth() {
        let harness = test_harness(vec![
            compiled_definition("always_on", false),
            compiled_definition("compiled_tool", true),
        ])
        .await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-verified",
                name: "echo",
                version: "1.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-quarantined",
                name: "evil",
                version: "9.9.9",
                verification: ExternalSkillVerificationStatus::InvalidSignature,
                quarantine: ExternalSkillQuarantineState::Quarantined,
                install: None,
            },
        );
        drop(writer);

        let response = harness.service.list_skills().unwrap();

        assert!(response
            .installed
            .iter()
            .any(|skill| { skill.id == "always_on" && skill.source == SkillSourceKind::Compiled }));
        assert!(response.installed.iter().any(|skill| {
            skill.id == "compiled_tool"
                && skill.source == SkillSourceKind::Compiled
                && skill.state == SkillStateDto::Installed
        }));

        let echo = response
            .available
            .iter()
            .find(|skill| skill.id == "digest-verified")
            .unwrap();
        assert_eq!(echo.name, "echo");
        assert_eq!(echo.state, SkillStateDto::Verified);
        assert_eq!(echo.install_state, SkillInstallStateDto::NotInstalled);
        assert_eq!(
            echo.verification_status,
            SkillVerificationStatusDto::Verified
        );
        assert_eq!(echo.quarantine_state, SkillQuarantineStateDto::Clear);
        assert!(!echo.runtime_visible);
        assert_eq!(echo.source, SkillSourceKind::Workspace);

        let evil = response
            .available
            .iter()
            .find(|skill| skill.id == "digest-quarantined")
            .unwrap();
        assert_eq!(evil.state, SkillStateDto::Quarantined);
        assert_eq!(
            evil.verification_status,
            SkillVerificationStatusDto::InvalidSignature
        );
        assert_eq!(evil.quarantine_state, SkillQuarantineStateDto::Quarantined);
        assert_eq!(evil.quarantine_reason.as_deref(), Some("manual quarantine"));
    }

    #[tokio::test]
    async fn compiled_names_take_precedence_over_external_name_collisions() {
        let harness = test_harness(vec![compiled_definition("echo", true)]).await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-echo",
                name: "echo",
                version: "2.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );
        drop(writer);

        let compiled = harness.service.get_skill("echo").unwrap();
        let external = harness.service.get_skill("digest-echo").unwrap();

        assert_eq!(compiled.id, "echo");
        assert_eq!(compiled.source, SkillSourceKind::Compiled);
        assert_eq!(external.id, "digest-echo");
        assert_eq!(external.source, SkillSourceKind::Workspace);
    }

    #[tokio::test]
    async fn external_name_resolution_requires_digest_when_multiple_versions_exist() {
        let harness = test_harness(Vec::new()).await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-a",
                name: "echo",
                version: "1.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-b",
                name: "echo",
                version: "2.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );
        drop(writer);

        let error = harness.service.get_skill("echo").unwrap_err();
        assert!(matches!(
            error,
            SkillCatalogError::AmbiguousSkillIdentifier(value) if value == "echo"
        ));
    }

    #[tokio::test]
    async fn external_install_and_runtime_resolution_fail_closed_until_runtime_exists() {
        let harness = test_harness(vec![compiled_definition("compiled_tool", true)]).await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-echo",
                name: "echo",
                version: "1.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );
        drop(writer);

        let before_install = match harness
            .service
            .resolve_for_execute("digest-echo", &synthetic_agent())
        {
            Ok(_) => panic!("uninstalled external skill must not resolve for execute"),
            Err(error) => error,
        };
        assert!(matches!(
            before_install,
            SkillCatalogError::NotInstalled(value) if value == "digest-echo"
        ));

        let writer = harness.db.write().await;
        let installed = harness
            .service
            .install_with_conn(&writer, "digest-echo", Some("operator"))
            .unwrap();
        assert_eq!(installed.install_state, SkillInstallStateDto::Installed);
        assert_eq!(installed.state, SkillStateDto::Installed);
        assert!(!installed.runtime_visible);
        drop(writer);

        let runtime = harness
            .service
            .resolve_for_runtime(&synthetic_agent(), None)
            .unwrap();
        assert!(!runtime
            .visible_skill_names
            .iter()
            .any(|name| name == "echo"));
        assert!(!runtime
            .granted_policy_capabilities
            .iter()
            .any(|capability| capability == "skill:echo"));
        assert!(runtime
            .visible_skill_names
            .iter()
            .any(|name| name == "compiled_tool"));

        let execute = match harness
            .service
            .resolve_for_execute("digest-echo", &synthetic_agent())
        {
            Ok(_) => panic!("runtime-dark external skill must not resolve for execute"),
            Err(error) => error,
        };
        assert!(matches!(
            execute,
            SkillCatalogError::ExecutionUnavailable(value) if value == "digest-echo"
        ));
    }

    #[tokio::test]
    async fn quarantined_external_skills_cannot_install_or_execute() {
        let harness = test_harness(Vec::new()).await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-bad",
                name: "bad",
                version: "1.0.0",
                verification: ExternalSkillVerificationStatus::InvalidSignature,
                quarantine: ExternalSkillQuarantineState::Quarantined,
                install: None,
            },
        );

        let install = harness
            .service
            .install_with_conn(&writer, "digest-bad", Some("operator"))
            .unwrap_err();
        assert!(matches!(
            install,
            SkillCatalogError::SkillQuarantined { skill_id, .. } if skill_id == "digest-bad"
        ));
        drop(writer);

        let execute = match harness
            .service
            .resolve_for_execute("digest-bad", &synthetic_agent())
        {
            Ok(_) => panic!("quarantined external skill must not resolve for execute"),
            Err(error) => error,
        };
        assert!(matches!(
            execute,
            SkillCatalogError::SkillQuarantined { skill_id, .. } if skill_id == "digest-bad"
        ));
    }

    #[tokio::test]
    async fn operator_quarantine_resolution_requires_a_fresh_revision() {
        let harness = test_harness(Vec::new()).await;
        let writer = harness.db.write().await;
        seed_external_skill(
            &writer,
            ExternalSeed {
                digest: "digest-echo",
                name: "echo",
                version: "1.0.0",
                verification: ExternalSkillVerificationStatus::Verified,
                quarantine: ExternalSkillQuarantineState::Clear,
                install: None,
            },
        );

        let quarantined = harness
            .service
            .quarantine_with_conn(&writer, "digest-echo", "manual review", Some("operator"))
            .unwrap();
        assert_eq!(quarantined.state, SkillStateDto::Quarantined);
        assert_eq!(
            quarantined.quarantine_state,
            SkillQuarantineStateDto::Quarantined
        );
        assert_eq!(
            quarantined.quarantine_reason.as_deref(),
            Some("manual review")
        );
        let revision = quarantined
            .quarantine_revision
            .expect("quarantine revision recorded");
        assert_eq!(revision, 2);

        let stale = harness
            .service
            .resolve_quarantine_with_conn(&writer, "digest-echo", revision - 1, Some("operator"))
            .unwrap_err();
        assert!(matches!(
            stale,
            SkillCatalogError::StaleQuarantineRevision {
                skill_id,
                expected_revision,
                actual_revision,
            } if skill_id == "digest-echo" && expected_revision == revision - 1 && actual_revision == revision
        ));

        let install = harness
            .service
            .install_with_conn(&writer, "digest-echo", Some("operator"))
            .unwrap_err();
        assert!(matches!(
            install,
            SkillCatalogError::SkillQuarantined { skill_id, .. } if skill_id == "digest-echo"
        ));

        let resolved = harness
            .service
            .resolve_quarantine_with_conn(&writer, "digest-echo", revision, Some("operator"))
            .unwrap();
        assert_eq!(resolved.state, SkillStateDto::Verified);
        assert_eq!(resolved.quarantine_state, SkillQuarantineStateDto::Clear);
        assert_eq!(
            resolved.verification_status,
            SkillVerificationStatusDto::Verified
        );
        assert_eq!(resolved.quarantine_reason, None);
        assert_eq!(resolved.quarantine_revision, Some(revision + 1));
    }
}
