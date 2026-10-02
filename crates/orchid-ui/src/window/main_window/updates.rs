//! Startup update check and opt-in telemetry.

use std::sync::Arc;
use std::time::Duration;

use tracing::warn;

use crate::release::{
    append_telemetry_line, interpret_release, os_family, telemetry_json, telemetry_target,
    ReleaseOffer, TelemetryEvent, TelemetryTarget, RELEASES_LATEST,
};

use super::MainWindowController;
use crate::window::spawn;

impl MainWindowController {
    /// Quiet GitHub check when auto-update is on, and a local telemetry line
    /// when telemetry is on.
    pub(super) fn begin_session_reports(self: &Arc<Self>) {
        if self.config.read().general.auto_update {
            self.spawn_update_check(false);
        }
        if self.config.read().general.telemetry {
            self.spawn_telemetry(false);
        }
    }

    /// Look up the latest release and, when it is newer, open the release page.
    pub(super) fn check_for_updates(self: &Arc<Self>) {
        self.spawn_update_check(true);
    }

    /// Record an app-start event and say whether it stayed local or was sent.
    pub(super) fn report_telemetry_now(self: &Arc<Self>) {
        self.spawn_telemetry(true);
    }

    fn spawn_update_check(self: &Arc<Self>, announce: bool) {
        let this = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let client = crate::http::shared_client();
            let result = match client
                .get(RELEASES_LATEST)
                .header("accept", "application/vnd.github+json")
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    interpret_release(status, &body, env!("CARGO_PKG_VERSION"))
                }
                Err(e) => {
                    warn!(error = %e, "update check");
                    Err(())
                }
            };
            let Some(c) = this.upgrade() else {
                return;
            };
            let audit_detail = match &result {
                Ok(ReleaseOffer::Newer { .. }) => "newer",
                Ok(ReleaseOffer::Current) => "current",
                Ok(ReleaseOffer::None) => "none",
                Err(()) => "failed",
            };
            if let Ok(paths) = orchid_storage::OrchidPaths::resolve() {
                if let Err(err) = orchid_storage::append_audit(
                    &orchid_storage::audit_path(&paths.config_file),
                    "update-check",
                    audit_detail,
                ) {
                    warn!(error = %err, "audit update check");
                }
            }
            let title = c.locale.tr("update-check-title");
            match result {
                Ok(ReleaseOffer::Newer { version, page }) => {
                    let body = c.locale.tr_args(
                        "update-check-newer",
                        &orchid_i18n::FluentArgs::new().with("version", version),
                    );
                    c.push_notification(&title, &body, 0);
                    if announce {
                        if let Some(page) = page {
                            if let Err(e) = opener::open(&page) {
                                warn!(error = %e, "open release page");
                            }
                        }
                    }
                }
                Ok(ReleaseOffer::Current) if announce => {
                    let body = c.locale.tr_args(
                        "update-check-current",
                        &orchid_i18n::FluentArgs::new().with("version", env!("CARGO_PKG_VERSION")),
                    );
                    c.push_notification(&title, &body, 1);
                }
                Ok(ReleaseOffer::None) if announce => {
                    c.push_notification(&title, &c.locale.tr("update-check-none"), 0);
                }
                Ok(_) => {}
                Err(()) if announce => {
                    c.push_notification(&title, &c.locale.tr("update-check-failed"), 2);
                }
                Err(()) => {}
            }
        });
    }

    fn spawn_telemetry(self: &Arc<Self>, announce: bool) {
        let (language, endpoint) = {
            let cfg = self.config.read();
            if !cfg.general.telemetry {
                return;
            }
            (
                cfg.locale.language.clone(),
                cfg.general.telemetry_endpoint.clone(),
            )
        };
        let this = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let event = TelemetryEvent {
                event: "app-start",
                version: env!("CARGO_PKG_VERSION"),
                os: os_family(),
                language: &language,
            };
            let Ok(line) = telemetry_json(&event) else {
                return;
            };
            if let Ok(paths) = orchid_storage::OrchidPaths::resolve() {
                let path = paths.data_dir.join("telemetry.jsonl");
                if let Err(e) = append_telemetry_line(&path, &line) {
                    warn!(error = %e, "telemetry journal");
                }
            }
            let target = telemetry_target(&endpoint);
            let outcome = match &target {
                TelemetryTarget::LocalOnly => TelemetryOutcome::Local,
                TelemetryTarget::Rejected => TelemetryOutcome::Rejected,
                TelemetryTarget::Https(url) => {
                    if post_telemetry(url, &line).await {
                        TelemetryOutcome::Sent
                    } else {
                        TelemetryOutcome::Failed
                    }
                }
            };
            let Some(c) = this.upgrade() else {
                return;
            };
            let tell = match outcome {
                TelemetryOutcome::Local => announce,
                TelemetryOutcome::Sent => announce,
                TelemetryOutcome::Rejected => true,
                TelemetryOutcome::Failed => announce,
            };
            if !tell {
                return;
            }
            let key = match outcome {
                TelemetryOutcome::Local => "telemetry-local",
                TelemetryOutcome::Sent => "telemetry-sent",
                TelemetryOutcome::Rejected => "telemetry-rejected",
                TelemetryOutcome::Failed => "telemetry-failed",
            };
            let severity = match outcome {
                TelemetryOutcome::Rejected | TelemetryOutcome::Failed => 2,
                _ => 0,
            };
            c.push_notification(&c.locale.tr("telemetry-title"), &c.locale.tr(key), severity);
        });
    }
}

enum TelemetryOutcome {
    Local,
    Sent,
    Rejected,
    Failed,
}

async fn post_telemetry(url: &str, body: &str) -> bool {
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            warn!(error = %e, "telemetry client");
            return false;
        }
    };
    match client
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => true,
        Ok(resp) => {
            warn!(status = %resp.status(), "telemetry send");
            false
        }
        Err(e) => {
            warn!(error = %e, "telemetry send");
            false
        }
    }
}
