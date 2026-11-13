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
