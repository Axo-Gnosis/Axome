use crate::common;

use axome_core::{errors::ApiError, Host};
use axome_lib::router::WorkspaceRouter;
use axome_protocol::settings::PatchSettingsSectionInput;
use axome_remote::Identity;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tempfile::tempdir;

/// The retry loop resolves a stop's server by name and reads an unknown name
/// as "run gone". A rename must carry queued stops along, and a removal must
/// wait for them, or the remote process is orphaned.
#[test]
fn pending_stops_follow_remote_renames() -> anyhow::Result<()> {
    let Some(_) =
        common::isolated_home("remote_pending_stops::pending_stops_follow_remote_renames")
    else {
        return Ok(());
    };
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempdir()?;

            let fingerprint = Identity::load_or_generate(temporary.path(), "server")?.fingerprint();
            let host = Arc::new(Host::new(
                axome_core::HostOptions::new(
                    axome_core::HostDirs::under(temporary.path()),
                    "test.db",
                ),
                Arc::new(|_| Ok(())),
            )?);
            let router = WorkspaceRouter::new(Arc::clone(&host));
            router
                .settings_patch_global_section(
                    None,
                    PatchSettingsSectionInput {
                        section: "remotes".into(),
                        value: json!({
                            "old": {
                                "address": "axome-unreachable.invalid:7420",
                                "fingerprint": fingerprint.to_string(),
                            }
                        }),
                    },
                )
                .await?;

            router.queue_stop_for_test("old", "orphan-run");
            router.change_remote("old", Some("new")).await?;
            let expected = HashMap::from([("orphan-run".to_owned(), "new".to_owned())]);
            assert_eq!(router.pending_stops_for_test(), expected);
            assert_eq!(
                WorkspaceRouter::new(Arc::clone(&host)).pending_stops_for_test(),
                expected,
                "the rename must reach the persisted stops"
            );

            let removed = router.change_remote("new", None).await;
            assert!(
                matches!(removed, Err(ApiError::InvalidArgument(_))),
                "removing a remote with a pending stop must be refused, got {removed:?}"
            );
            assert_eq!(router.pending_stops_for_test(), expected);
            Ok(())
        })
}
