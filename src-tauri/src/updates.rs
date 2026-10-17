//! Checking for and installing new versions from the project's GitHub releases.
//!
//! Nothing here runs unless the person asks: either by clicking "Check for
//! updates" or by turning on "Check for updates when the app opens", which is
//! off by default. A check asks GitHub for a small file describing the newest
//! release. No footage, project or personal information is sent.
//!
//! Updates are signed. The public half of the signing key is built into the
//! app from the UPDATER_PUBLIC_KEY build setting; a build made without it
//! cannot update itself and says so plainly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Public key for verifying updates, set when the release build is made.
pub const PUBLIC_KEY: Option<&str> = option_env!("UPDATER_PUBLIC_KEY");

/// True when this build carries an update signing key.
pub fn configured() -> bool {
    PUBLIC_KEY.map(|k| !k.trim().is_empty()).unwrap_or(false)
}

/// The updater plugin, only when this build can verify updates.
pub fn plugin<R: Runtime>() -> Option<tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config>> {
    if !configured() {
        return None;
    }
    Some(tauri_plugin_updater::Builder::new().pubkey(PUBLIC_KEY.unwrap().trim()).build())
}

/// Settings kept between sessions, in the app's own settings folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Check for a new version each time the app opens. Off unless turned on.
    pub check_updates_on_open: bool,
    /// Picture profile for clips whose camera does not say, used for new projects.
    pub fallback_profile: Option<selects_engine::tone::ToneProfile>,
}

impl Settings {
    pub fn load(path: &std::path::Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}

pub fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

/// An update found by the last check, waiting for the person to install it.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<Update>>);

#[derive(Serialize)]
pub struct UpdateStatus {
    /// False when this build cannot update itself.
    pub configured: bool,
    pub current_version: String,
    /// The newer version, if one was found.
    pub available: Option<String>,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateSettings {
    pub configured: bool,
    pub check_updates_on_open: bool,
}

/// Turn an updater error into something a person can act on.
pub fn explain(error: &str) -> String {
    let e = error.to_lowercase();
    if e.contains("404") || e.contains("not found") {
        "No published update information was found on GitHub, so the app could not tell whether a newer version exists. Nothing was changed. If the release page needs a GitHub sign-in to open, the app cannot reach it either.".into()
    } else if e.contains("signature") || e.contains("pubkey") || e.contains("public key") {
        "A newer version was found, but its signature did not match this app, so it was not installed. Nothing was changed. Download the newest version from the release page instead.".into()
    } else if e.contains("dns") || e.contains("connect") || e.contains("network") || e.contains("timed out") || e.contains("timeout") || e.contains("request") {
        "The app could not reach GitHub to check for a newer version. Nothing was changed. Check your internet connection and try again.".into()
    } else {
        format!("The update check did not finish ({error}). Nothing was changed. You can try again, or download the newest version from the release page.")
    }
}

const NOT_CONFIGURED: &str = "This copy of the app was built without update signing, so it cannot update itself. Download newer versions from the release page.";

#[tauri::command]
pub fn update_settings<R: Runtime>(app: AppHandle<R>) -> UpdateSettings {
    let s = settings_path(&app).map(|p| Settings::load(&p)).unwrap_or_default();
    UpdateSettings { configured: configured(), check_updates_on_open: s.check_updates_on_open }
}

#[tauri::command]
pub fn set_check_updates_on_open<R: Runtime>(app: AppHandle<R>, on: bool) -> Result<UpdateSettings, String> {
    let path = settings_path(&app).ok_or("The app's settings folder could not be found.")?;
    let mut s = Settings::load(&path);
    s.check_updates_on_open = on;
    s.save(&path)
        .map_err(|e| format!("The setting could not be saved ({e}). It stays as it was."))?;
    Ok(UpdateSettings { configured: configured(), check_updates_on_open: on })
}

#[tauri::command]
pub async fn check_for_update<R: Runtime>(app: AppHandle<R>, pending: State<'_, Pending>) -> Result<UpdateStatus, String> {
    let current_version = app.package_info().version.to_string();
    if !configured() {
        return Err(NOT_CONFIGURED.into());
    }
    let updater = app.updater().map_err(|e| explain(&e.to_string()))?;
    let found = updater.check().await.map_err(|e| explain(&e.to_string()))?;
    let status = UpdateStatus {
        configured: true,
        current_version,
        available: found.as_ref().map(|u| u.version.clone()),
        notes: found.as_ref().and_then(|u| u.body.clone()),
    };
    *pending.0.lock().unwrap() = found;
    Ok(status)
}

#[tauri::command]
pub async fn install_update<R: Runtime>(app: AppHandle<R>, pending: State<'_, Pending>) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("There is no update waiting. Click Check for updates first.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| explain(&e.to_string()))?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_checks_are_off_by_default() {
        assert!(!Settings::default().check_updates_on_open);
        let dir = std::env::temp_dir().join(format!("updates-test-{}", std::process::id()));
        let missing = dir.join("nothing-here.json");
        assert_eq!(Settings::load(&missing), Settings::default());
    }

    #[test]
    fn settings_round_trip_and_survive_damage() {
        let dir = std::env::temp_dir().join(format!("updates-test-rt-{}", std::process::id()));
        let path = dir.join("settings.json");
        Settings { check_updates_on_open: true, ..Default::default() }.save(&path).unwrap();
        assert!(Settings::load(&path).check_updates_on_open);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn errors_are_explained_in_plain_language() {
        for raw in ["error sending request: dns error", "Network error", "HTTP 404 Not Found", "invalid signature", "weird"] {
            let msg = explain(raw);
            assert!(msg.contains("Nothing was changed"), "{msg}");
        }
        assert!(explain("dns error: failed to lookup").contains("internet connection"));
        assert!(explain("404").contains("No published update"));
    }
}
