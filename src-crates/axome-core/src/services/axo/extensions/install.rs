use crate::dirs::create_private_dir;
use crate::errors::ApiError;
use crate::services::git::{git_command, validated_ref_name};
use axo_extensions::{inspect_package, ExtensionPackage};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, Permissions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use url::{Host, Url};

const STAGE_AGE_MS: i64 = 24 * 60 * 60 * 1000;
const STAGE_RANDOM_LEN: usize = 22;
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

pub(in crate::services::axo) enum Source {
    Folder(PathBuf),
    Git {
        url: String,
        git_ref: Option<String>,
    },
}

pub(in crate::services::axo) struct Stage {
    dir: TempDir,
    pub package: ExtensionPackage,
    pub origin: String,
    pub revision: Option<GitRevision>,
}

impl Stage {
    pub fn id(&self) -> &str {
        self.dir
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("generated ASCII stage ID")
    }

    pub fn revision_label(&self) -> Result<Option<String>, ApiError> {
        self.revision
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ApiError::Internal(format!("Cannot encode Git revision: {error}")))
    }

    pub async fn discard(self) -> Result<(), ApiError> {
        discard_dir(self.dir).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::services::axo) struct GitRevision {
    pub commit: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub url: String,
    pub subject: String,
}

/// Both current and previous copies use the same host label grammar.
pub(in crate::services::axo) fn parse_revision_label(
    label: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    match label {
        Some(label) => match serde_json::from_str::<GitRevision>(label) {
            Ok(revision) => (Some(revision.commit), revision.git_ref, Some(revision.url)),
            Err(_) => (Some(label.to_owned()), None, None),
        },
        None => (None, None, None),
    }
}

fn stage_timestamp(id: &str) -> Option<i64> {
    let (time, tail) = id.split_once('-')?;
    if !time.is_empty()
        && time.bytes().all(|byte| byte.is_ascii_digit())
        && tail.len() == STAGE_RANDOM_LEN
        && tail.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        time.parse().ok()
    } else {
        None
    }
}

pub(in crate::services::axo) fn valid_stage_id(id: &str) -> bool {
    stage_timestamp(id).is_some()
}

/// Initialize the shared staging root without disturbing another host's recent stages.
pub(in crate::services::axo) async fn prepare(cache_dir: &Path) -> Result<PathBuf, ApiError> {
    let root = cache_dir.join("axo/extension-fetch");
    tokio::task::spawn_blocking(move || {
        // The configured cache parent may be redirected; the host-owned leaf may not.
        match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.is_symlink() => {
                return Err(ApiError::Io(format!(
                    "Extension staging root must not be a symlink: {}",
                    root.display()
                )));
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        create_private_dir(&root)?;
        let root = root.canonicalize()?;
        let cutoff = chrono::Utc::now().timestamp_millis() - STAGE_AGE_MS;
        for entry in fs::read_dir(&root)? {
            let result = (|| {
                let entry = entry?;
                let old = entry
                    .file_name()
                    .to_str()
                    .and_then(stage_timestamp)
                    .is_some_and(|time| time < cutoff);
                if old && entry.file_type()?.is_dir() {
                    fs::remove_dir_all(entry.path())?;
                }
                Ok::<_, io::Error>(())
            })();
            if let Err(error) = result {
                tracing::warn!(root = %root.display(), %error, "Could not clean stale extension stages");
            }
        }
        Ok(root)
    })
    .await?
}

pub(in crate::services::axo) async fn inspect(
    root: &Path,
    source: Source,
) -> Result<Stage, ApiError> {
    let root = root.to_owned();
    // If the request goes away, finish (or time out) before dropping its private tree.
    tokio::spawn(async move {
        let dir = tempfile::Builder::new()
            .prefix(&format!("{}-", chrono::Utc::now().timestamp_millis()))
            .rand_bytes(STAGE_RANDOM_LEN)
            .permissions(Permissions::from_mode(0o700))
            .tempdir_in(&root)?;
        match acquire(dir.path(), source).await {
            Ok((package, origin, revision)) => Ok(Stage {
                dir,
                package,
                origin,
                revision,
            }),
            Err(error) => {
                if let Err(cleanup) = discard_dir(dir).await {
                    tracing::warn!(%cleanup, "Could not discard failed extension inspection");
                }
                Err(error)
            }
        }
    })
    .await?
}

async fn discard_dir(dir: TempDir) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || dir.close()).await??;
    Ok(())
}

async fn acquire(
    dir: &Path,
    source: Source,
) -> Result<(ExtensionPackage, String, Option<GitRevision>), ApiError> {
    let package = dir.join("package");
    let (origin, revision) = match source {
        Source::Folder(path) => {
            let origin =
                tokio::task::spawn_blocking(move || snapshot_folder(&path, &package)).await??;
            (origin, None)
        }
        Source::Git { url, git_ref } => {
            let origin = git_origin(&url).ok_or_else(|| {
                ApiError::InvalidArgument("Use an HTTPS or SSH Git repository URL".into())
            })?;
            let revision = fetch(dir, url, git_ref).await?;
            (origin, Some(revision))
        }
    };
    let package = inspect_package(&dir.join("package"))
        .await
        .map_err(|problem| ApiError::InvalidArgument(problem.message))?;
    if !package.id.contains('/') {
        return Err(ApiError::InvalidArgument(
            "An installed extension must use a publisher/name id".into(),
        ));
    }
    Ok((package, origin, revision))
}

fn snapshot_folder(path: &Path, package: &Path) -> Result<String, ApiError> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(ApiError::InvalidArgument(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    // Don't recurse into this stage, or copy other hosts' stages into the package.
    if package.starts_with(&path) {
        return Err(ApiError::InvalidArgument(
            "Extension source cannot contain the extension staging directory".into(),
        ));
    }
    let canonical = path.to_str().ok_or_else(|| {
        ApiError::InvalidArgument("Extension source path must be valid UTF-8".into())
    })?;
    let source = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&path)?;
    fs::create_dir(package)?;
    copy_tree(&source, &path, package, true)?;
    Ok(format!("folder:{canonical}"))
}

fn fd_path(file: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

fn copy_tree(source: &File, path: &Path, target: &Path, top: bool) -> Result<(), ApiError> {
    for entry in fs::read_dir(fd_path(source))? {
        let entry = entry?;
        let name = entry.file_name();
        if top && (name == ".git" || name == "node_modules") {
            continue;
        }
        let entry_path = entry.path();
        // An O_PATH descriptor pins the entry without opening a device/FIFO or
        // following a symlink, even if the source changes while the copy runs.
        let input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW)
            .open(&entry_path)?;
        let metadata = input.metadata()?;
        let target = target.join(&name);
        if metadata.is_dir() {
            fs::create_dir(&target)?;
            copy_tree(&input, &path.join(&name), &target, false)?;
        } else if metadata.is_file() {
            let mut input = File::open(fd_path(&input))?;
            let mut output = File::create_new(target)?;
            io::copy(&mut input, &mut output)?;
            output.set_permissions(Permissions::from_mode(
                metadata.permissions().mode() & 0o777,
            ))?;
        } else if metadata.is_symlink() {
            symlink(fs::read_link(entry_path)?, target)?;
        } else {
            return Err(ApiError::InvalidArgument(format!(
                "{} is not a regular file, directory or symlink",
                path.join(&name).display()
            )));
        }
    }
    Ok(())
}

fn git_origin(url: &str) -> Option<String> {
    if url.starts_with('-')
        || url.contains('\\')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    fn host(authority: &str) -> &str {
        authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host)
    }
    let (host, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("ssh://"))
    {
        let parsed = Url::parse(url).ok()?;
        let (authority, path) = rest.split(['?', '#']).next()?.split_once('/')?;
        if authority.is_empty() || parsed.host_str()?.starts_with('-') {
            return None;
        }
        (host(authority), path)
    } else {
        if url.contains("://") {
            return None;
        }
        let split = match url.split_once('[') {
            Some((prefix, ip)) if prefix.is_empty() || prefix.ends_with('@') => {
                prefix.len() + ip.find(']')? + 2
            }
            _ => url.find(':')?,
        };
        let (authority, rest) = url.split_at(split);
        let path = rest.strip_prefix(':')?;
        let host = host(authority);
        if authority.contains('/')
            || host.starts_with('-')
            || Host::parse(host).is_err()
            || path.starts_with(':')
        {
            return None;
        }
        (host, path.split(['?', '#']).next()?)
    };
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!path.is_empty()).then(|| format!("{host}/{path}"))
}

async fn fetch(dir: &Path, url: String, git_ref: Option<String>) -> Result<GitRevision, ApiError> {
    git(dir, "init", &["init", "--quiet", "--template=", "package"]).await?;
    let branch;
    let target = match git_ref.as_deref() {
        None => "HEAD",
        Some(value)
            if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            value
        }
        Some(value) => {
            validated_ref_name(value)?; // Reject @{-N} before Git can expand it.
            let output = run_git(
                dir,
                &["-C", "package", "check-ref-format", "--branch", value],
            )
            .await?;
            if !output.status.success() {
                return Err(ApiError::InvalidArgument(git_error(
                    "check-ref-format",
                    &output,
                )));
            }
            // Fetch the validated branch, not a tag or other ref with the same short name.
            branch = format!("refs/heads/{value}");
            &branch
        }
    };
    git(
        dir,
        "remote add",
        &[
            "-C",
            "package",
            "-c",
            "protocol.file.allow=never",
            "remote",
            "add",
            "origin",
            &url,
        ],
    )
    .await?;
    git(
        dir,
        "fetch",
        &[
            "-C",
            "package",
            "-c",
            "protocol.file.allow=never",
            "fetch",
            "--depth=1",
            "--no-tags",
            "origin",
            target,
        ],
    )
    .await?;
    git(
        dir,
        "checkout",
        &[
            "-C",
            "package",
            "-c",
            "core.hooksPath=/dev/null",
            "checkout",
            "--detach",
            "--force",
            "FETCH_HEAD",
        ],
    )
    .await?;
    let commit = git(dir, "rev-parse", &["-C", "package", "rev-parse", "HEAD"]).await?;
    let subject = git(dir, "log", &["-C", "package", "log", "-1", "--format=%s"]).await?;
    Ok(GitRevision {
        commit,
        git_ref,
        url,
        subject,
    })
}

async fn git(dir: &Path, action: &str, args: &[&str]) -> Result<String, ApiError> {
    let mut output = run_git(dir, args).await?;
    if !output.status.success() {
        return Err(ApiError::Internal(git_error(action, &output)));
    }
    if output.stdout.last() == Some(&b'\n') {
        output.stdout.pop();
    }
    Ok(String::from_utf8(output.stdout)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()))
}

fn git_error(action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        format!("Extension Git {action} failed ({})", output.status)
    } else {
        format!("Extension Git {action} failed: {}", stderr.trim_end())
    }
}

async fn run_git(dir: &Path, args: &[&str]) -> Result<Output, ApiError> {
    let mut command = git_command(dir);
    let inherited: Vec<_> = command
        .get_envs()
        .filter(|(key, _)| key.as_encoded_bytes().starts_with(b"GIT_"))
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in inherited {
        command.env_remove(key);
    }
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/bin/false")
        .env("SSH_ASKPASS", "/bin/false")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -oBatchMode=yes -oStrictHostKeyChecking=yes",
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()?;
    let pid = child.id().expect("Git was just spawned") as libc::pid_t;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut out = child.stdout.take().expect("piped Git stdout");
    let mut err = child.stderr.take().expect("piped Git stderr");
    let result = tokio::time::timeout(GIT_TIMEOUT, async {
        tokio::try_join!(out.read_to_end(&mut stdout), err.read_to_end(&mut stderr))?;
        child.wait().await
    })
    .await;
    let error = match result {
        Ok(Ok(status)) => {
            return Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        Ok(Err(error)) => error.into(),
        Err(_) => ApiError::Internal("Extension Git fetch timed out after 120 seconds".into()),
    };
    // Git can spawn SSH and HTTP helpers. Kill the group before reaping
    // the leader so the stage isn't removed while a helper still uses it.
    // SAFETY: the child was spawned as the leader of a dedicated group.
    if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
        let _ = child.start_kill();
    }
    if let Err(reap) = child.wait().await {
        tracing::warn!(error = %reap, "Could not reap extension Git process");
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_labels_round_trip_and_keep_external_labels() {
        let revision = GitRevision {
            commit: "a123b456c789".into(),
            git_ref: Some("main".into()),
            url: "git@code.example:lab/notes.git".into(),
            subject: "Sync notes".into(),
        };
        let label = serde_json::to_string(&revision).unwrap();
        assert_eq!(
            label,
            r#"{"commit":"a123b456c789","ref":"main","url":"git@code.example:lab/notes.git","subject":"Sync notes"}"#,
        );
        assert_eq!(
            parse_revision_label(Some(&label)),
            (Some(revision.commit), revision.git_ref, Some(revision.url)),
        );
        assert_eq!(parse_revision_label(None), (None, None, None));
        for label in [
            "external-label",
            "{malformed",
            r#"{"commit":"a","ref":null,"url":"u","subject":"s","extra":true}"#,
        ] {
            assert_eq!(
                parse_revision_label(Some(label)),
                (Some(label.into()), None, None)
            );
        }
        let previous: axo_extensions::InstalledCopy = serde_json::from_value(serde_json::json!({
            "folder": "v-earlier",
            "revision": r#"{"commit":"previous","ref":null,"url":"ssh://code.example/notes.git","subject":"Earlier copy"}"#,
            "installedAt": 42,
        }))
        .unwrap();
        assert_eq!(
            parse_revision_label(previous.revision.as_deref()),
            (
                Some("previous".into()),
                None,
                Some("ssh://code.example/notes.git".into()),
            ),
        );
    }

    #[test]
    fn git_origins_keep_only_host_port_and_repository() {
        for (url, origin) in [
            (
                "https://u:p@code.example:443/lab/notes.git?q=1#fragment",
                "code.example:443/lab/notes",
            ),
            (
                "ssh://git@code.example:2222/lab/notes.git",
                "code.example:2222/lab/notes",
            ),
            ("git@code.example:lab/notes.git", "code.example/lab/notes"),
            ("git@[::1]:lab/notes.git", "[::1]/lab/notes"),
        ] {
            assert_eq!(git_origin(url).as_deref(), Some(origin));
        }
        for url in [
            "",
            " ",
            "-q",
            "/tmp/a.git",
            "../a.git",
            "dir/local:a.git",
            "file:///tmp/a.git",
            "http://host/a.git",
            "ext::cmd",
            "https:///tmp/a.git",
        ] {
            assert_eq!(git_origin(url), None, "{url}");
        }
    }
}
