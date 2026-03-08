use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use cortex_storage::queries::skill_install_state_queries::{self, SkillInstallState};

use super::definitions::SkillDefinition;
use super::dto::{SkillListResponseDto, SkillStateDto, SkillSummaryDto};
use crate::db_pool::DbPool;
use crate::runtime_safety::ResolvedRuntimeAgent;

#[derive(Debug, thiserror::Error)]
pub enum SkillCatalogError {
    #[error("skill '{0}' not found")]
    SkillNotFound(String),
    #[error("skill '{0}' cannot be installed")]
    NotInstallable(String),
    #[error("skill '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error("skill '{0}' is not installed")]
    NotInstalled(String),
    #[error("skill '{0}' cannot be uninstalled")]
    NotRemovable(String),
    #[error("skill '{0}' is disabled")]
    SkillDisabled(String),
    #[error("skill '{skill_name}' is not enabled for agent '{agent_name}'")]
    NotEnabledForAgent {
        skill_name: String,
        agent_name: String,
    },
    #[error("database pool error: {0}")]
    DbPool(String),
    #[error("storage error: {0}")]
    Storage(String),
}

#[derive(Clone)]
pub struct ResolvedSkill {
    pub definition: Arc<SkillDefinition>,
    pub skill: Arc<dyn ghost_skills::skill::Skill>,
}

#[derive(Clone, Default)]
pub struct ResolvedSkillSet {
    pub skills: Arc<HashMap<String, Arc<dyn ghost_skills::skill::Skill>>>,
    pub granted_policy_capabilities: Vec<String>,
    pub visible_skill_names: Vec<String>,
}

#[derive(Clone)]
pub struct SkillCatalogService {
    definitions: BTreeMap<String, Arc<SkillDefinition>>,
    db: Arc<DbPool>,
}

impl SkillCatalogService {
    pub async fn new(
        definitions: Vec<SkillDefinition>,
        db: Arc<DbPool>,
    ) -> Result<Self, SkillCatalogError> {
        let service = Self {
            definitions: definitions
                .into_iter()
                .map(|definition| (definition.name.clone(), Arc::new(definition)))
                .collect(),
            db,
        };
        service.seed_default_install_state().await?;
        Ok(service)
    }

    pub fn empty_for_tests(db: Arc<DbPool>) -> Self {
        Self {
            definitions: BTreeMap::new(),
            db,
        }
    }

    pub fn list_skills(&self) -> Result<SkillListResponseDto, SkillCatalogError> {
        let states = self.load_install_states()?;
        let mut installed = Vec::new();
        let mut available = Vec::new();

        for definition in self.definitions.values() {
            let summary = self.summary_from_definition(
                definition,
                states.get(definition.name.as_str()),
                None,
            );
            match summary.state {
                SkillStateDto::AlwaysOn | SkillStateDto::Installed => installed.push(summary),
                SkillStateDto::Available => available.push(summary),
                SkillStateDto::Disabled | SkillStateDto::Quarantined => available.push(summary),
            }
        }

        Ok(SkillListResponseDto {
            installed,
            available,
        })
    }

    pub fn get_skill(&self, name: &str) -> Result<SkillSummaryDto, SkillCatalogError> {
        let definition = self.definition(name)?;
        let states = self.load_install_states()?;
        Ok(self.summary_from_definition(&definition, states.get(name), None))
    }

    pub async fn install(
        &self,
        name: &str,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let definition = self.definition(name)?;
        if !definition.installable {
            return Err(SkillCatalogError::NotInstallable(name.to_string()));
        }

        let current = self.load_install_states()?;
        if matches!(current.get(name), Some(SkillInstallState::Installed)) {
            return Err(SkillCatalogError::AlreadyInstalled(name.to_string()));
        }

        let db = self.db.write().await;
        skill_install_state_queries::upsert_skill_install_state(
            &db,
            name,
            SkillInstallState::Installed,
            actor,
        )
        .map_err(|e| SkillCatalogError::Storage(e.to_string()))?;
        drop(db);

        self.get_skill(name)
    }

    pub async fn uninstall(
        &self,
        name: &str,
        actor: Option<&str>,
    ) -> Result<SkillSummaryDto, SkillCatalogError> {
        let definition = self.definition(name)?;
        if !definition.removable || definition.always_on {
            return Err(SkillCatalogError::NotRemovable(name.to_string()));
        }

        let current = self.load_install_states()?;
        if !matches!(current.get(name), Some(SkillInstallState::Installed)) {
            return Err(SkillCatalogError::NotInstalled(name.to_string()));
        }

        let db = self.db.write().await;
        skill_install_state_queries::upsert_skill_install_state(
            &db,
            name,
            SkillInstallState::Disabled,
            actor,
        )
        .map_err(|e| SkillCatalogError::Storage(e.to_string()))?;
        drop(db);

        self.get_skill(name)
    }

    pub fn resolve_for_runtime(
        &self,
        agent: &ResolvedRuntimeAgent,
        allowlist_override: Option<&[String]>,
    ) -> Result<ResolvedSkillSet, SkillCatalogError> {
        let states = self.load_install_states()?;
        let allowlist = allowlist_override.or(agent.skill_allowlist.as_deref());
        let mut skills = HashMap::new();
        let mut granted_policy_capabilities = Vec::new();
        let mut visible_skill_names = Vec::new();

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

            skills.insert(definition.name.clone(), Arc::clone(&definition.skill));
            granted_policy_capabilities.push(definition.policy_capability.clone());
            visible_skill_names.push(definition.name.clone());
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
        name: &str,
        agent: &ResolvedRuntimeAgent,
    ) -> Result<ResolvedSkill, SkillCatalogError> {
        let definition = self.definition(name)?;
        let states = self.load_install_states()?;

        if !self.is_runtime_enabled(&definition, states.get(name)) {
            return Err(SkillCatalogError::SkillDisabled(name.to_string()));
        }

        if !definition.always_on
            && agent
                .skill_allowlist
                .as_deref()
                .is_some_and(|allowed| !allowed.iter().any(|skill_name| skill_name == name))
        {
            return Err(SkillCatalogError::NotEnabledForAgent {
                skill_name: name.to_string(),
                agent_name: agent.name.clone(),
            });
        }

        Ok(ResolvedSkill {
            definition: Arc::clone(&definition),
            skill: Arc::clone(&definition.skill),
        })
    }

    fn definition(&self, name: &str) -> Result<Arc<SkillDefinition>, SkillCatalogError> {
        self.definitions
            .get(name)
            .cloned()
            .ok_or_else(|| SkillCatalogError::SkillNotFound(name.to_string()))
    }

    fn load_install_states(&self) -> Result<HashMap<String, SkillInstallState>, SkillCatalogError> {
        let db = self
            .db
            .read()
            .map_err(|e| SkillCatalogError::DbPool(e.to_string()))?;
        let rows = skill_install_state_queries::list_skill_install_states(&db)
            .map_err(|e| SkillCatalogError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| (row.skill_name, row.state))
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
                .map_err(|e| SkillCatalogError::Storage(e.to_string()))?;
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
        let state = if definition.always_on {
            SkillStateDto::AlwaysOn
        } else {
            match install_state {
                Some(SkillInstallState::Installed) => SkillStateDto::Installed,
                Some(SkillInstallState::Disabled) | None => SkillStateDto::Available,
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
            state,
            quarantine_reason: None,
            enabled_for_agent,
            capabilities: vec![definition.policy_capability.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GhostConfig;

    async fn test_catalog() -> SkillCatalogService {
        let db_path =
            std::env::temp_dir().join(format!("skill-catalog-{}.db", uuid::Uuid::now_v7()));
        let db = crate::db_pool::create_pool(db_path).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let definitions = crate::skill_catalog::definitions::build_compiled_skill_definitions(
            &GhostConfig::default(),
        )
        .definitions;

        SkillCatalogService::new(definitions, db).await.unwrap()
    }

    #[tokio::test]
    async fn seeded_installable_skills_start_installed() {
        let catalog = test_catalog().await;
        let listed = catalog.list_skills().unwrap();

        assert!(listed
            .installed
            .iter()
            .any(|skill| skill.name == "note_take" && skill.state == SkillStateDto::Installed));
        assert!(listed.installed.iter().any(
            |skill| skill.name == "convergence_check" && skill.state == SkillStateDto::AlwaysOn
        ));
    }

    #[tokio::test]
    async fn uninstall_moves_skill_to_available() {
        let catalog = test_catalog().await;

        let summary = catalog
            .uninstall("note_take", Some("tester"))
            .await
            .unwrap();
        assert_eq!(summary.state, SkillStateDto::Available);

        let listed = catalog.list_skills().unwrap();
        assert!(listed
            .available
            .iter()
            .any(|skill| skill.name == "note_take"));
    }

    #[tokio::test]
    async fn resolve_for_runtime_honors_allowlist_but_keeps_always_on() {
        let catalog = test_catalog().await;
        let agent = ResolvedRuntimeAgent {
            id: uuid::Uuid::now_v7(),
            name: "runtime-agent".into(),
            capabilities: Vec::new(),
            spending_cap: 5.0,
            skill_allowlist: Some(vec!["note_take".into()]),
        };

        let resolved = catalog.resolve_for_runtime(&agent, None).unwrap();

        assert!(resolved
            .visible_skill_names
            .contains(&"note_take".to_string()));
        assert!(resolved
            .visible_skill_names
            .contains(&"convergence_check".to_string()));
        assert!(!resolved
            .visible_skill_names
            .contains(&"git_status".to_string()));
        assert!(resolved
            .granted_policy_capabilities
            .contains(&"skill:note_take".to_string()));
    }
}
