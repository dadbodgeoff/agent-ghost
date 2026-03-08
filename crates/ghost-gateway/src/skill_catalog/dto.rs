use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillStateDto {
    AlwaysOn,
    Installed,
    Available,
    Disabled,
    Verified,
    Quarantined,
    VerificationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillInstallStateDto {
    AlwaysOn,
    Installed,
    Disabled,
    NotInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillVerificationStatusDto {
    NotApplicable,
    Verified,
    ValidationFailed,
    DigestMismatch,
    MissingSignature,
    InvalidSignature,
    UnknownSigner,
    RevokedSigner,
    UnsupportedCapability,
    UnsupportedExecutionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillQuarantineStateDto {
    Clear,
    Quarantined,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct SkillSummaryDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: super::definitions::SkillSourceKind,
    pub removable: bool,
    pub installable: bool,
    pub execution_mode: super::definitions::SkillExecutionMode,
    pub policy_capability: String,
    pub privileges: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub mutation_kind: super::definitions::SkillMutationKind,
    pub state: SkillStateDto,
    pub install_state: SkillInstallStateDto,
    pub verification_status: SkillVerificationStatusDto,
    pub quarantine_state: SkillQuarantineStateDto,
    pub runtime_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quarantine_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quarantine_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_for_agent: Option<bool>,
    // Compatibility alias for older clients. New callers should use
    // `policy_capability` and `privileges`.
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct SkillListResponseDto {
    pub installed: Vec<SkillSummaryDto>,
    pub available: Vec<SkillSummaryDto>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ExecuteSkillRequestDto {
    #[schema(value_type = String)]
    pub agent_id: Uuid,
    #[schema(value_type = String)]
    pub session_id: Uuid,
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ExecuteSkillResponseDto {
    pub skill: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct SkillQuarantineRequestDto {
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct SkillQuarantineResolutionRequestDto {
    pub expected_quarantine_revision: i64,
}
