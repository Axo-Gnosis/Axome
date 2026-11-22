use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    pub enabled: bool,
    pub embed: MemoryEmbedStatus,
    pub banks: Vec<MemoryBankStatus>,
    pub last_capture: Option<MemoryCaptureStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEmbedStatus {
    pub state: MemoryEmbedState,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryEmbedState {
    Disabled,
    Missing,
    Downloading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBankStatus {
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub notes: u64,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCaptureStatus {
    pub at: String,
    pub session_id: String,
    pub added: u32,
    pub replaced: u32,
    pub error: Option<String>,
}
