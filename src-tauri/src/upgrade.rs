use crate::{config::AppSettings, errors::AppError, window_state::PopupSize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn backup_error(error: impl std::fmt::Display) -> AppError {
    AppError::Update(format!("升级备份失败，已阻止安装：{error}"))
}
pub fn begin_backup(
    root: &Path,
    config_dir: &Path,
    settings: &AppSettings,
    size: PopupSize,
    target: &str,
) -> Result<PathBuf, AppError> {
    // Never let a remote version become a path component.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(backup_error)?
        .as_nanos();
    fs::create_dir_all(root).map_err(backup_error)?;
    let snapshot = root.join(format!("upgrade-{nonce}"));
    fs::create_dir(&snapshot).map_err(backup_error)?;
    fs::write(
        snapshot.join("settings.json"),
        serde_json::to_vec_pretty(settings).map_err(backup_error)?,
    )
    .map_err(backup_error)?;
    fs::write(snapshot.join("popup-window.json"), serde_json::to_vec_pretty(&serde_json::json!({ "schemaVersion": 1, "popupWidth": size.width, "popupHeight": size.height })).map_err(backup_error)?).map_err(backup_error)?;
    // Preserve the original schema as well, if settings were migrated in this session.
    for schema in 0..crate::config::CURRENT_SETTINGS_SCHEMA {
        let name = format!("settings.pre-v{schema}.json");
        let source = config_dir.join(&name);
        if source.exists() {
            fs::copy(source, snapshot.join(name)).map_err(backup_error)?;
        }
    }
    fs::write(snapshot.join("manifest.json"), serde_json::to_vec_pretty(&serde_json::json!({ "fromVersion": env!("CARGO_PKG_VERSION"), "toVersion": target, "containsApiKeys": false, "createdAt": nonce.to_string() })).map_err(backup_error)?).map_err(backup_error)?;
    Ok(snapshot)
}
pub fn finish_backup(snapshot: &Path) -> Result<(), AppError> {
    for name in [
        "settings.json",
        "popup-window.json",
        "translations.sqlite3",
        "manifest.json",
    ] {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(snapshot.join(name))
            .and_then(|file| file.sync_all())
            .map_err(backup_error)?;
    }
    fs::write(
        snapshot.join("COMPLETE"),
        b"Backup completed before installer launch\n",
    )
    .map_err(backup_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SettingsStore, storage::TranslationCache, window_state::PopupSizeStore};

    #[test]
    fn upgrade_snapshot_preserves_settings_window_history_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("backups");
        let config = directory.path().join("config");
        fs::create_dir(&config).unwrap();
        let old = include_str!("../../tests/fixtures/settings-v1.0.json");
        fs::write(config.join("settings.json"), old).unwrap();
        let settings = SettingsStore::load(config.join("settings.json"))
            .unwrap()
            .get()
            .unwrap();
        let size = PopupSize::new(700.0, 550.0);
        let snapshot = begin_backup(&root, &config, &settings, size, "1.3.0").unwrap();
        let cache = TranslationCache::open(&directory.path().join("history.sqlite3")).unwrap();
        cache
            .backup_to(&snapshot.join("translations.sqlite3"))
            .unwrap();
        finish_backup(&snapshot).unwrap();
        assert!(snapshot.join("COMPLETE").exists());
        assert_eq!(
            SettingsStore::load(snapshot.join("settings.json"))
                .unwrap()
                .get()
                .unwrap(),
            settings
        );
        assert_eq!(
            PopupSizeStore::load(snapshot.join("popup-window.json")).current(),
            size
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("settings.pre-v2.json")).unwrap(),
            old
        );
        assert_eq!(
            TranslationCache::open(&snapshot.join("translations.sqlite3"))
                .unwrap()
                .len()
                .unwrap(),
            0
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(snapshot.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["containsApiKeys"], false);
        let second = begin_backup(&root, &config, &settings, size, "../../untrusted").unwrap();
        assert_eq!(second.parent().unwrap(), root);
        assert_ne!(snapshot, second);
    }

    #[test]
    fn partial_or_failed_backup_never_marked_complete() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = begin_backup(
            &directory.path().join("backups"),
            directory.path(),
            &AppSettings::default(),
            PopupSize::default(),
            "1.3.0",
        )
        .unwrap();
        assert!(finish_backup(&snapshot).is_err());
        assert!(!snapshot.join("COMPLETE").exists());
        let blocked = directory.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        assert!(begin_backup(
            &blocked,
            directory.path(),
            &AppSettings::default(),
            PopupSize::default(),
            "1.3.0"
        )
        .is_err());
    }
}
