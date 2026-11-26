use serde::{Deserialize, Serialize};

/// Text to show after the caret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PredictSuggestion {
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PredictModelState {
    Off,
    Missing,
    Downloading { received: u64, total: u64 },
    Ready,
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PredictStatus {
    pub enabled: bool,
    pub model: PredictModelState,
}
