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
