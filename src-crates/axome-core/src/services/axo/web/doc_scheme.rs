//! Read-only `doc://` URL scheme backed by the shared docs cache pages directory.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axo::{Scheme, SchemeContext, SchemeUrl, Target};
use axo_agent::ToolError;

pub(crate) struct DocScheme {
    root: PathBuf,
}

impl DocScheme {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Scheme for DocScheme {
    fn name(&self) -> &str {
        "doc"
    }

    fn prompt_doc(&self) -> &str {
        "`doc://<path>`: cached documentation pages indexed by `search_docs`; read-only."
    }

    async fn resolve(&self, url: &SchemeUrl, _cx: &SchemeContext) -> Result<Target, ToolError> {
        let path = join_contained(&self.root, &url.path)?;
        Ok(Target::File {
            path,
            writable: false,
        })
    }
}

fn join_contained(root: &Path, raw: &str) -> Result<PathBuf, ToolError> {
    let rel = raw.trim_end_matches('/');
    if rel.starts_with('/') {
        return Err(ToolError(
            "Absolute paths are not allowed in doc:// URLs".into(),
        ));
    }
    if rel.split('/').any(|part| part == "..") {
        return Err(ToolError(
            "Path traversal (..) is not allowed in doc:// URLs".into(),
        ));
    }
    let joined = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    if let (Ok(canon_root), Ok(canon_target)) =
        (std::fs::canonicalize(root), std::fs::canonicalize(&joined))
    {
        if !canon_target.starts_with(&canon_root) {
            return Err(ToolError(format!("doc://{raw} escapes cache directory")));
        }
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn doc_scheme_resolves_contained_readonly_paths() {
        let dir = std::env::temp_dir().join(format!("axome-doc-scheme-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let scheme = DocScheme::new(dir.clone());
        let cx = SchemeContext {
            session_id: "s1".into(),
            session_dir: dir.clone(),
            cwd: dir.clone(),
        };
        let target = scheme
            .resolve(
                &SchemeUrl {
                    scheme: "doc".into(),
                    path: "docs-rs-123/index.md".into(),
                },
                &cx,
            )
            .await
            .unwrap();
        assert_eq!(
            target,
            Target::File {
                path: dir.join("docs-rs-123/index.md"),
                writable: false,
            }
        );

        assert!(scheme
            .resolve(
                &SchemeUrl {
                    scheme: "doc".into(),
                    path: "../secret".into(),
                },
                &cx,
            )
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
