use crate::axo_settings::AxoProjectAccess;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoProvidersPayload {
    pub providers: Vec<AxoProvider>,
    pub models_file: AxoModelsFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoProvider {
    pub id: String,
    pub origin: Option<String>,
    pub origin_project: bool,
    pub hides: Option<AxoHiddenProvider>,
    pub env: Vec<String>,
    pub requires_key: bool,
    pub login: bool,
    pub source: AxoCredentialSource,
    pub status: AxoProviderStatus,
    pub models: u32,
}

/// Whether the provider can authenticate requests now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AxoProviderStatus {
    Ready,
    NeedsCredentials,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoModelsFile {
    pub path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AxoCredentialSource {
    Config,
    ApiKey,
    Oauth,
    Env { name: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginStarted {
    pub login_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AxoLoginStatus {
    Pending { view: AxoLoginView },
    Done,
    Failed { message: String },
}

/// What a login in progress currently shows; each field holds the provider's latest request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginView {
    pub url: Option<AxoLoginUrl>,
    pub device_code: Option<AxoLoginDeviceCode>,
    pub progress: Option<String>,
    /// The one question waiting for an answer, if any.
    pub question: Option<AxoLoginQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginUrl {
    pub url: String,
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginDeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginQuestion {
    pub id: u32,
    pub message: String,
    #[serde(flatten)]
    pub kind: AxoLoginQuestionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AxoLoginQuestionKind {
    Prompt {
        placeholder: Option<String>,
        secret: bool,
    },
    Select {
        options: Vec<AxoLoginOption>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoLoginOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoKeywordInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoSearchProviderStatus {
    pub id: String,
    pub label: String,
    pub requires_key: bool,
    pub env: Vec<String>,
    pub configured: bool,
    pub source: AxoCredentialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoDocsSourceInfo {
    pub key: String,
    pub kind: String,
    pub doc_path: String,
    pub sections: u32,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoWebToolsStatusPayload {
    pub search_providers: Vec<AxoSearchProviderStatus>,
    pub docs_sources: Vec<AxoDocsSourceInfo>,
    pub cache_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AxoEnabledExtension {
    pub id: String,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensions {
    pub bun: AxoBun,
    pub host: Vec<AxoExtension>,
    pub project: Option<AxoProjectExtensions>,
    pub collisions: Vec<AxoProviderCollision>,
    pub diagnostics: Vec<AxoExtensionDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoBun {
    pub path: Option<String>,
    pub searched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoProjectExtensions {
    pub folder: String,
    pub access: AxoProjectAccess,
    pub root_exists: bool,
    pub items: Vec<AxoExtension>,
    pub collisions: Vec<AxoProviderCollision>,
    pub diagnostics: Vec<AxoExtensionDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxoExtensionOrigin {
    Installed,
    UserFolder,
    ProjectFolder,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxoExtensionPhase {
    Cold,
    Installing,
    Starting,
    Ready,
    Restarting,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionProblem {
    pub kind: AxoExtensionProblemKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxoExtensionProblemKind {
    Bun,
    NotFound,
    Invalid,
    DependenciesMissing,
    DependencyInstall,
    Load,
    Crash,
    Setup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtension {
    pub id: String,
    pub origin: AxoExtensionOrigin,
    pub enabled: bool,
    pub position: Option<u32>,
    pub group: Option<String>,
    pub phase: Option<AxoExtensionPhase>,
    pub problem: Option<AxoExtensionProblem>,
    pub adds: AxoExtensionAdds,
    pub install: Option<AxoExtensionInstall>,
    pub data_dir: String,
    pub failure: Option<AxoExtensionFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionAdds {
    pub providers: Vec<AxoExtensionProvider>,
    pub tools: Vec<String>,
    pub commands: Vec<String>,
    pub tool_policy: Vec<String>,
    pub spawn_hook: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionProvider {
    pub id: String,
    pub models: u32,
    pub login: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionInstall {
    pub origin: String,
    pub revision: Option<String>,
    pub git_ref: Option<String>,
    pub url: Option<String>,
    pub installed_at: Option<i64>,
    pub previous: Option<AxoExtensionCopy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionCopy {
    pub revision: Option<String>,
    pub installed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionFailure {
    pub title: String,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoProviderCollision {
    pub provider: String,
    pub winner: String,
    pub hidden: AxoHiddenProvider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all_fields = "camelCase",
    rename_all = "camelCase"
)]
pub enum AxoHiddenProvider {
    Extension { id: String },
    Loaded,
    Registered,
    Layer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionDiagnostic {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoRunnerMemory {
    pub members: Vec<String>,
    pub group: Option<String>,
    pub pss_bytes: u64,
    pub project: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all_fields = "camelCase",
    rename_all = "camelCase"
)]
pub enum AxoExtensionSource {
    Folder {
        path: String,
    },
    Git {
        url: String,
        git_ref: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxoExtensionStage {
    pub stage_id: String,
    pub id: String,
    pub adds: AxoExtensionAdds,
    pub origin: String,
    pub revision: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all_fields = "camelCase",
    rename_all = "camelCase"
)]
pub enum AxoInstallOutcome {
    Installed {
        id: String,
    },
    OriginChanged {
        installed: String,
        offered: String,
    },
    Failed {
        title: String,
        output: Option<String>,
    },
}
