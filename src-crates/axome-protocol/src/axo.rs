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
    pub oauth: bool,
    pub source: AxoCredentialSource,
    pub models: u32,
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
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AxoLoginStatus {
    Pending,
    Done,
    Failed { message: String },
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
