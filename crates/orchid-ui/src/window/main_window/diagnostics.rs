//! Support-bundle and profile-backup commands.

use std::sync::Arc;

use tracing::warn;

use super::MainWindowController;
use crate::window::spawn;

impl MainWindowController {
    pub(super) fn on_export_support_bundle(self: &Arc<Self>) {
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let dest = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .set_file_name(&orchid_storage::default_support_filename())
                    .add_filter("Zip", &["zip"])
                    .save_file()
            })
            .await
            .ok()
            .flatten();
            let Some(c) = t.upgrade() else {
                return;
            };
            let Some(dest) = dest else {
                c.push_notification(
                    &c.locale.tr("command-diagnostics-export-name"),
                    &c.locale.tr("backup-export-cancelled"),
                    0,
                );
                return;
            };
            let extras = orchid_storage::SupportBundleExtras {
                exe_dir: std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf())),
                app_version: env!("CARGO_PKG_VERSION").into(),
            };
            match tokio::task::spawn_blocking(move || {
                let paths = orchid_storage::OrchidPaths::resolve()?;
                orchid_storage::write_support_bundle(&paths, &dest, &extras)
            })
            .await
            {
                Ok(Ok(path)) => {
                    let body = c.locale.tr_args(
                        "diagnostics-export-ok",
                        &orchid_i18n::FluentArgs::new().with("path", path.display().to_string()),
                    );
                    c.push_notification(&c.locale.tr("command-diagnostics-export-name"), &body, 1);
                }
                Ok(Err(e)) => {
                    warn!(?e, "support bundle");
                    let body = c.locale.tr_args(
                        "diagnostics-export-failed",
                        &orchid_i18n::FluentArgs::new().with("reason", e.to_string()),
                    );
                    c.push_notification(&c.locale.tr("command-diagnostics-export-name"), &body, 2);
                }
                Err(e) => warn!(?e, "support bundle join"),
            }
        });
    }

    pub(super) fn on_export_backup(self: &Arc<Self>) {
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let dest = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .set_file_name(&orchid_storage::default_backup_filename())
                    .add_filter("Zip", &["zip"])
                    .save_file()
            })
            .await
            .ok()
            .flatten();
            let Some(c) = t.upgrade() else {
                return;
            };
            let Some(dest) = dest else {
                c.push_notification(
                    &c.locale.tr("command-data-export-backup-name"),
                    &c.locale.tr("backup-export-cancelled"),
                    0,
                );
                return;
            };
            match tokio::task::spawn_blocking(move || {
                let paths = orchid_storage::OrchidPaths::resolve()?;
                orchid_storage::write_backup_zip(&paths, &dest)
            })
            .await
            {
                Ok(Ok(path)) => {
                    let body = c.locale.tr_args(
                        "backup-export-ok",
                        &orchid_i18n::FluentArgs::new().with("path", path.display().to_string()),
                    );
                    c.push_notification(&c.locale.tr("command-data-export-backup-name"), &body, 1);
                }
                Ok(Err(e)) => {
                    warn!(?e, "backup export");
                    let body = c.locale.tr_args(
                        "backup-export-failed",
                        &orchid_i18n::FluentArgs::new().with("reason", e.to_string()),
                    );
                    c.push_notification(&c.locale.tr("command-data-export-backup-name"), &body, 2);
                }
                Err(e) => warn!(?e, "backup export join"),
            }
        });
    }
}
