use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};

const WINDOW_STATE_SCHEMA: u32 = 1;
pub const DEFAULT_POPUP_WIDTH: f64 = 520.0;
pub const DEFAULT_POPUP_HEIGHT: f64 = 380.0;
pub const MIN_POPUP_WIDTH: f64 = 360.0;
pub const MIN_POPUP_HEIGHT: f64 = 180.0;
pub const MAX_POPUP_WIDTH: f64 = 960.0;
pub const MAX_POPUP_HEIGHT: f64 = 720.0;
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(450);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupSize {
    pub width: f64,
    pub height: f64,
}

impl Default for PopupSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_POPUP_WIDTH,
            height: DEFAULT_POPUP_HEIGHT,
        }
    }
}

impl PopupSize {
    pub fn new(width: f64, height: f64) -> Self {
        let default = Self::default();
        Self {
            width: normalize_dimension(width, MIN_POPUP_WIDTH, MAX_POPUP_WIDTH, default.width),
            height: normalize_dimension(height, MIN_POPUP_HEIGHT, MAX_POPUP_HEIGHT, default.height),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedWindowState {
    schema_version: u32,
    popup_width: f64,
    popup_height: f64,
}

pub struct PopupSizeStore {
    path: PathBuf,
    current: RwLock<PopupSize>,
    revision: AtomicU64,
}

impl PopupSizeStore {
    pub fn load(path: PathBuf) -> Self {
        let current = read_size(&path).unwrap_or_default();
        Self {
            path,
            current: RwLock::new(current),
            revision: AtomicU64::new(0),
        }
    }

    pub fn current(&self) -> PopupSize {
        self.current.read().map(|size| *size).unwrap_or_default()
    }

    pub fn update(&self, width: f64, height: f64) -> Option<u64> {
        let next = PopupSize::new(width, height);
        let changed = self
            .current
            .write()
            .map(|mut current| {
                if *current == next {
                    false
                } else {
                    *current = next;
                    true
                }
            })
            .unwrap_or(false);
        if !changed {
            return None;
        }

        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Some(revision)
    }

    pub fn persist_if_current(&self, revision: u64) -> std::io::Result<()> {
        if self.revision.load(Ordering::Relaxed) != revision {
            return Ok(());
        }
        self.persist_current()
    }

    fn persist_current(&self) -> std::io::Result<()> {
        let size = self.current();
        let state = PersistedWindowState {
            schema_version: WINDOW_STATE_SCHEMA,
            popup_width: size.width,
            popup_height: size.height,
        };
        let bytes = serde_json::to_vec_pretty(&state).map_err(std::io::Error::other)?;
        persist_atomically(&self.path, &bytes)
    }
}

fn normalize_dimension(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.round().clamp(min, max)
    } else {
        fallback
    }
}

fn read_size(path: &Path) -> Option<PopupSize> {
    let bytes = fs::read(path).ok()?;
    let state: PersistedWindowState = serde_json::from_slice(&bytes).ok()?;
    (state.schema_version == WINDOW_STATE_SCHEMA)
        .then(|| PopupSize::new(state.popup_width, state.popup_height))
}

fn persist_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("window state path has no parent"))?;
    fs::create_dir_all(parent)?;

    let temp = parent.join(format!(
        ".popup-window-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&temp, bytes)?;

    let backup = path.with_extension("json.bak");
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "quicktranslate-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn uses_larger_default_popup_size() {
        assert_eq!(
            PopupSize::default(),
            PopupSize {
                width: 520.0,
                height: 380.0,
            }
        );
    }

    #[test]
    fn clamps_invalid_or_unsupported_sizes() {
        assert_eq!(
            PopupSize::new(120.0, 2_000.0),
            PopupSize {
                width: MIN_POPUP_WIDTH,
                height: MAX_POPUP_HEIGHT,
            }
        );
        assert_eq!(
            PopupSize::new(f64::NAN, f64::INFINITY),
            PopupSize::default()
        );
    }

    #[test]
    fn persists_and_restores_popup_size() {
        let path = temp_path("popup-size");
        let store = PopupSizeStore::load(path.clone());
        *store.current.write().unwrap() = PopupSize::new(744.0, 512.0);
        store.persist_current().unwrap();

        assert_eq!(
            PopupSizeStore::load(path.clone()).current(),
            PopupSize::new(744.0, 512.0)
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn only_persists_the_latest_resize_revision() {
        let path = temp_path("debounced-popup-size");
        let store = PopupSizeStore::load(path.clone());
        let first = store.update(640.0, 440.0).unwrap();
        let latest = store.update(760.0, 540.0).unwrap();

        store.persist_if_current(first).unwrap();
        assert!(!path.exists());
        store.persist_if_current(latest).unwrap();
        assert_eq!(
            PopupSizeStore::load(path.clone()).current(),
            PopupSize::new(760.0, 540.0)
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn ignores_corrupted_state_file() {
        let path = temp_path("corrupted-popup-size");
        fs::write(&path, b"not-json").unwrap();

        assert_eq!(
            PopupSizeStore::load(path.clone()).current(),
            PopupSize::default()
        );
        let _ = fs::remove_file(path);
    }
}
