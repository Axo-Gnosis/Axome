//! Saving a reviewed plan-mode result under `plansDir` and accepting it.

use std::{
    fs::{File, OpenOptions},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use axome_protocol::chat::{ChatPart, ChatPlanThen};
use serde_json::{json, Value};

use super::{axo_error, config, convert, AxoService, Held};
use crate::errors::ApiError;

/// Compactions retried after `Busy`, when something else took the idle slot first.
const COMPACT_RETRIES: usize = 3;

impl AxoService {
    /// Copies the review's `local://` draft to `plansDir/<save_as>` (never overwriting), accepts
    /// the review with the saved path, then deletes the draft. Returns the saved path: relative
    /// to `folder` for a relative `plansDir`, absolute for an absolute one.
    pub(crate) async fn accept_plan(
        &self,
        folder: &Path,
        id: &str,
        call_id: String,
        save_as: String,
        then: ChatPlanThen,
    ) -> Result<String, ApiError> {
        let held = self.live(folder, id).await?;
        let review = held
            .review(&call_id)
            .ok_or_else(|| ApiError::NotFound("No pending review".into()))?;
        if !review.plan {
            return Err(ApiError::InvalidArgument("Not a plan result".into()));
        }
        let (Some(title), Some(file)) = (
            review.result.get("title").and_then(Value::as_str),
            review.result.get("file").and_then(Value::as_str),
        ) else {
            return Err(ApiError::InvalidArgument("Not a plan result".into()));
        };
        let draft = local_draft(held.session.schemes(), file).await?;
        let configured = config::plans_dir(&self.dirs, folder).map_err(axo_error)?;
        let configured = Path::new(&configured);
        let plans_dir = folder.join(configured);
        let target = {
            let (file, save_as) = (file.to_owned(), save_as.clone());
            blocking(move || {
                if !std::fs::metadata(&draft).is_ok_and(|meta| meta.is_file()) {
                    return Err(ApiError::InvalidArgument(format!(
                        "Plan draft not found: {file}"
                    )));
                }
                save_plan(&plans_dir, &save_as, &draft)
            })
            .await?
        };
        let path = if configured.is_absolute() {
            target.to_string_lossy().into_owned()
        } else {
            convert::display_path(&target, folder)
        };
        let message = matches!(then, ChatPlanThen::Work).then(|| {
            format!("The plan is saved at {path}. Plan mode has ended. Implement the plan now.")
        });
        // axo settles a review once and validates the replacement against the mode's own
        // schema; on any refusal the review stays pending and only our new file is removed.
        let accepted = held.session.accept_result(
            &call_id,
            axo::AcceptResultOptions {
                replacement: Some(json!({ "title": title, "file": path, "saveAs": save_as })),
                message,
            },
        );
        if let Err(error) = accepted {
            blocking(move || {
                remove_created(&target);
                Ok(())
            })
            .await?;
            return Err(axo_error(error));
        }
        // Resolved again so the deleted path is still contained in this session's
        // `local://` root at deletion time; `remove_file` never follows a final symlink.
        match local_draft(held.session.schemes(), file).await {
            Ok(draft) => {
                blocking(move || {
                    if let Err(error) = std::fs::remove_file(&draft) {
                        tracing::warn!(%error, draft = %draft.display(), "could not delete the accepted plan draft");
                    }
                    Ok(())
                })
                .await?
            }
            Err(error) => tracing::warn!(%error, "accepted plan draft no longer resolves; kept"),
        }
        if matches!(then, ChatPlanThen::CompactWork) {
            self.compact_then_work(&held, call_id, path.clone());
        }
        Ok(path)
    }

    /// After the accepting run ends: compact, then start a turn that implements the plan.
    /// The task holds its own pin, so the session stays loaded until it finishes.
    fn compact_then_work(&self, held: &Held<'_>, call_id: String, path: String) {
        let Some(service) = self.this.upgrade() else {
            return;
        };
        let live = held.live.clone();
        live.pin();
        held.live.track(async move {
            let held = Held {
                service: &service,
                live,
            };
            let failure = async {
                let mut retries = 0;
                loop {
                    if !held.wait_idle().await {
                        return None;
                    }
                    match held.session.compact(None).await {
                        Ok(()) => break,
                        Err(axo::Error::Busy) if retries < COMPACT_RETRIES => retries += 1,
                        // The plan is saved and the review consumed: a compaction axo cannot produce
                        // (a short chat has nothing to compact) must not cancel the work turn.
                        Err(axo::Error::Compaction(reason)) => {
                            tracing::warn!(%reason, "compaction skipped before plan work");
                            break;
                        }
                        Err(error) => return Some(error.to_string()),
                    }
                }
                if !held.wait_idle().await {
                    return None;
                }
                let parts = vec![
                    ChatPart::Text {
                        text: "Implement the plan in ".into(),
                    },
                    ChatPart::File { path },
                ];
                held.live
                    .prompt(format!("plan-{call_id}"), parts)
                    .await
                    .err()
                    .map(|error| error.to_string())
            }
            .await;
            if let Some(message) = failure {
                held.broadcast_error(message);
            }
        });
    }
}

/// Resolves a `local://` draft through the session's own scheme table. axo rejects absolute
/// paths, `..` and symlink escapes from the session's `local://` root, so the result is
/// always one of this session's files.
async fn local_draft(schemes: &axo::Schemes, file: &str) -> Result<PathBuf, ApiError> {
    let url = file
        .starts_with("local://")
        .then(|| schemes.exact(file))
        .flatten()
        .filter(|url| url.scheme == "local")
        .ok_or_else(|| {
            ApiError::InvalidArgument("The plan draft must be a local:// file".into())
        })?;
    match schemes
        .resolve(&url)
        .await
        .map_err(|error| ApiError::InvalidArgument(error.to_string()))?
    {
        axo::schemes::Target::File { path, .. } => Ok(path),
        axo::schemes::Target::Virtual => Err(ApiError::InvalidArgument(format!(
            "Plan draft not found: {file}"
        ))),
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| ApiError::Internal(format!("plan file task failed: {error}")))?
}

/// Copies `draft` to `plans_dir/save_as`, creating parent folders. `save_as` must be a
/// relative `.md` path of plain components; an existing target is refused untouched, and
/// a failed copy removes the partial file it created.
pub(super) fn save_plan(
    plans_dir: &Path,
    save_as: &str,
    draft: &Path,
) -> Result<PathBuf, ApiError> {
    let relative = Path::new(save_as);
    if relative.extension() != Some(std::ffi::OsStr::new("md"))
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(ApiError::InvalidArgument(format!(
            "Invalid plan path: {save_as}"
        )));
    }
    let io =
        |path: &Path, error: std::io::Error| ApiError::Io(format!("{}: {error}", path.display()));
    let target = plans_dir.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
    }
    let mut source = File::open(draft).map_err(|error| io(draft, error))?;
    let mut out = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(out) => out,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(ApiError::InvalidArgument(format!(
                "{} already exists",
                target.display()
            )));
        }
        Err(error) => return Err(io(&target, error)),
    };
    // Synced because the draft is deleted once the plan is accepted.
    if let Err(error) = std::io::copy(&mut source, &mut out).and_then(|_| out.sync_all()) {
        drop(out);
        remove_created(&target);
        return Err(io(&target, error));
    }
    Ok(target)
}

/// Removes a target this action created with `create_new`.
// ponytail: path-based; an inode check would cover a same-path swap within the rollback window.
fn remove_created(target: &Path) {
    if let Err(error) = std::fs::remove_file(target) {
        tracing::warn!(%error, target = %target.display(), "could not remove the unaccepted plan copy");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_plan_never_overwrites_or_escapes() {
        let root = std::env::temp_dir().join(format!("axome-plan-{}", uuid::Uuid::new_v4()));
        let plans = root.join("plans");
        std::fs::create_dir_all(&root).unwrap();
        let draft = root.join("draft.md");
        std::fs::write(&draft, "# Plan\n").unwrap();

        for bad in ["../x.md", "/abs.md", "x.txt", "", ".md", "a/../b.md"] {
            assert!(
                matches!(
                    save_plan(&plans, bad, &draft),
                    Err(ApiError::InvalidArgument(_))
                ),
                "{bad:?}"
            );
        }
        assert!(!root.join("x.md").exists());

        let saved = save_plan(&plans, "road/01-step.md", &draft).unwrap();
        assert_eq!(saved, plans.join("road/01-step.md"));
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "# Plan\n");

        std::fs::write(&saved, "edited").unwrap();
        let refused = save_plan(&plans, "road/01-step.md", &draft).unwrap_err();
        assert!(refused.to_string().contains("already exists"), "{refused}");
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "edited");
        assert!(draft.exists());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
