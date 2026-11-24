//! `memory://<bank>/<path>`: memory banks as ordinary files, so axo's `read`, `write`,
//! `edit`, `grep`, `find` and `bash` work on notes, record their changes, and obey the
//! session's write scope.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axo::{Scheme, SchemeContext, SchemeUrl, Target};
use axo_agent::ToolError;

use super::{checked_path, FolderMemory, MemoryService};

pub(crate) struct MemoryScheme {
    memory: Arc<MemoryService>,
    fm: FolderMemory,
    doc: String,
}

impl MemoryScheme {
    pub(crate) fn new(memory: Arc<MemoryService>, fm: FolderMemory) -> Self {
        let banks = fm
            .banks
            .iter()
            .map(|bank| format!("`{}`", bank.name))
            .collect::<Vec<_>>()
            .join(", ");
        let doc = format!(
            "`memory://<bank>/<path>`: long-term memory, Markdown notes kept across sessions; \
             read, grep, find, write and edit them like files. Banks: {banks}; this project's is \
             `{}`. Write to memory only when the user asks you to remember something or states a \
             durable preference or decision. Notes may start with YAML frontmatter (`title`, \
             `type`, `tags`); facts are list items `- [category] text`, links \
             `- relation_type [[Other Note]]`.",
            fm.bank.name
        );
        Self { memory, fm, doc }
    }

    fn path(&self, raw: &str) -> anyhow::Result<PathBuf> {
        let raw = raw.trim_end_matches('/');
        let (name, rest) = raw.split_once('/').unwrap_or((raw, ""));
        anyhow::ensure!(
            !name.is_empty(),
            "memory:// needs a bank, such as memory://{}/",
            self.fm.bank.name
        );
        let bank = self
            .fm
            .banks
            .iter()
            .find(|bank| bank.name == name)
            .ok_or_else(|| anyhow::anyhow!("Unknown memory bank `{name}`"))?;
        anyhow::ensure!(
            !rest.starts_with('/') && !rest.split('/').any(|part| part == ".."),
            "memory:// paths stay inside their bank"
        );
        checked_path(bank, &bank.dir.join(rest))
    }
}

#[async_trait]
impl Scheme for MemoryScheme {
    fn name(&self) -> &str {
        "memory"
    }

    fn prompt_doc(&self) -> &str {
        &self.doc
    }

    async fn resolve(&self, url: &SchemeUrl, _cx: &SchemeContext) -> Result<Target, ToolError> {
        self.memory
            .ensure_active(&self.fm)
            .and_then(|()| self.path(&url.path))
            .map(|path| Target::File {
                path,
                writable: true,
            })
            .map_err(|error| ToolError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn resolve(
        scheme: &MemoryScheme,
        cx: &SchemeContext,
        path: &str,
    ) -> Result<Target, ToolError> {
        let url = SchemeUrl {
            scheme: "memory".into(),
            path: path.into(),
        };
        scheme.resolve(&url, cx).await
    }

    #[tokio::test]
    async fn paths_stay_inside_their_bank() {
        let root =
            std::env::temp_dir().join(format!("axome-memory-scheme-{}", uuid::Uuid::new_v4()));
        let dirs = crate::HostDirs::under(&root);
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(
            dirs.settings_path(),
            r#"{"memory":{"enabled":true,"semantic":false}}"#,
        )
        .unwrap();
        let folder = root.join("project");
        let memory = MemoryService::new(
            dirs.clone(),
            reqwest::Client::new(),
            Arc::new(crate::services::db::DatabaseService::new(root.join("test.db")).unwrap()),
            Arc::new(|_| Ok(())),
        );
        let scheme = MemoryScheme::new(Arc::clone(&memory), memory.active(&folder).unwrap());
        let cx = SchemeContext {
            session_id: "s".into(),
            session_dir: root.clone(),
            cwd: folder,
        };
        let main = dirs.data.join("memory/banks/main");
        assert_eq!(
            resolve(&scheme, &cx, "main/proj/Note.md").await.unwrap(),
            Target::File {
                path: main.join("proj/Note.md"),
                writable: true
            }
        );
        assert!(!main.exists());
        std::fs::create_dir_all(&main).unwrap();
        for bad in ["", "other/x.md", "main/../x.md", "main//etc/passwd"] {
            assert!(resolve(&scheme, &cx, bad).await.is_err(), "{bad}");
        }
        std::os::unix::fs::symlink(&root, main.join("escape")).unwrap();
        assert!(resolve(&scheme, &cx, "main/escape/x.md").await.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
