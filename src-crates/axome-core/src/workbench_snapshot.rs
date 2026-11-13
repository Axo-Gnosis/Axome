use serde::Deserialize;

/// The part of a page's workbench snapshot the registry reads; the rest of
/// each tab and the layout tree are skipped, not parsed into values.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHead {
    version: u64,
    pub active_tab_index: Option<i64>,
    pub tabs: Vec<TabHead>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabHead {
    #[serde(default)]
    pub kind: String,
    pub folder_path: String,
    pub session_id: Option<String>,
}

/// A version-4 snapshot's head, or None for anything else.
pub fn parse(snapshot: &str) -> Option<SnapshotHead> {
    serde_json::from_str::<SnapshotHead>(snapshot)
        .ok()
        .filter(|head| head.version == 4)
}

// ponytail: a tab created after the last snapshot flush is missed; owner tracking on attach if that bites.
pub fn chat_session_ids(snapshot: &str) -> Vec<String> {
    let Some(snapshot) = parse(snapshot) else {
        return Vec::new();
    };
    snapshot
        .tabs
        .into_iter()
        .filter(|tab| tab.kind == "chat")
        .filter_map(|tab| tab.session_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::chat_session_ids;

    #[test]
    fn extracts_only_chat_sessions_from_valid_version_four_snapshots() {
        assert_eq!(
            chat_session_ids(
                r#"{"version":4,"activeTabIndex":0,"tabs":[
                    {"folderPath":"/a","kind":"chat","sessionId":"chat-1"},
                    {"folderPath":"/a","kind":"chat"},
                    {"folderPath":"/a","kind":"chat","sessionId":null},
                    {"folderPath":"/a","kind":"session","sessionId":"not-a-chat"}
                ]}"#,
            ),
            vec!["chat-1"]
        );
        for snapshot in [
            r#"{"version":3,"tabs":[{"folderPath":"/a","kind":"chat","sessionId":"old"}]}"#,
            r#"{"version":4,"tabs":[{"folderPath":"/a","kind":"chat","sessionId":42}]}"#,
            r#"{"version":4}"#,
            "invalid json",
        ] {
            assert!(chat_session_ids(snapshot).is_empty());
        }
    }
}
