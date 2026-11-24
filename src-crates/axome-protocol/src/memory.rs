use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    pub enabled: bool,
    pub embed: MemoryEmbedStatus,
    pub banks: Vec<MemoryBankStatus>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryCaptureTrigger {
    Compaction,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryFactOutcome {
    New,
    Updated,
    Known,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCaptureQuery {
    pub session_id: Option<String>,
    pub bank: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCaptureRun {
    pub id: i64,
    pub session_id: String,
    pub folder_path: String,
    pub title: Option<String>,
    pub bank: String,
    pub trigger: MemoryCaptureTrigger,
    pub anchor_entry_id: Option<String>,
    pub started_at: String,
    /// `None` while the capture runs.
    pub ended_at: Option<String>,
    pub error: Option<String>,
    pub no_model: bool,
    pub facts: Vec<MemoryCapturedFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCapturedFact {
    pub id: i64,
    pub outcome: MemoryFactOutcome,
    pub category: String,
    pub text: String,
    /// The observation an `updated` fact replaced.
    pub previous: Option<String>,
    pub note: String,
    pub path: String,
    pub reverted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCaptureChangedEvent {
    pub capture: MemoryCaptureRun,
    /// The host decided this change deserves a notification.
    pub notify: bool,
}
