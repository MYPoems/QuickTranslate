mod placement;
#[cfg(windows)]
mod windows;

use crate::{config::OcrLanguage, errors::AppError};

pub use placement::{place_popup, PopupPlacement, WorkArea};

#[cfg(windows)]
pub use windows::cursor::window_is_foreground;

#[derive(Debug, Clone, Copy)]
pub struct ScreenBounds {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// A summary of the last selection capture: timings and yes/no answers, never content.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDiagnostics {
    pub seconds_ago: u64,
    pub total_ms: u32,
    pub release_wait_ms: u32,
    pub keys_released: bool,
    pub clipboard_changed: bool,
    pub text_found: bool,
    pub written_by_target: bool,
    pub clipboard_restored: Option<bool>,
}

pub fn last_capture_diagnostics() -> Option<CaptureDiagnostics> {
    #[cfg(windows)]
    {
        windows::selection::last_capture_report().map(|report| CaptureDiagnostics {
            seconds_ago: report.finished.elapsed().as_secs(),
            total_ms: report.total_ms,
            release_wait_ms: report.release_wait_ms,
            keys_released: report.keys_released,
            clipboard_changed: report.clipboard_changed,
            text_found: report.text_found,
            written_by_target: report.written_by_target,
            clipboard_restored: report.clipboard_restored,
        })
    }
    #[cfg(not(windows))]
    None
}

/// How far local time is ahead of UTC, in seconds, including daylight saving. Used where
/// "today" has to mean the user's day rather than the UTC one.
pub fn local_utc_offset_seconds() -> i64 {
    #[cfg(windows)]
    {
        use ::windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        const DAYLIGHT: u32 = 2;
        const INVALID: u32 = u32::MAX;
        let mut zone = TIME_ZONE_INFORMATION::default();
        let state = unsafe { GetTimeZoneInformation(&mut zone) };
        if state == INVALID {
            return 0;
        }
        // Windows gives the bias as minutes to add to local time to get UTC.
        let bias = zone.Bias
            + if state == DAYLIGHT {
                zone.DaylightBias
            } else {
                zone.StandardBias
            };
        -i64::from(bias) * 60
    }
    #[cfg(not(windows))]
    0
}

pub async fn get_selected_text() -> Result<String, AppError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(windows::selection::capture_selected_text)
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }
    #[cfg(not(windows))]
    Err(AppError::UnsupportedPlatform)
}

/// Writes `text` to the clipboard and reports whether it did. A copy that came from a
/// keyboard shortcut is skipped (`Ok(false)`) while a selection capture is sending its own
/// Ctrl+C, because that keystroke was not the user's.
pub async fn copy_text(text: String, from_shortcut: bool) -> Result<bool, AppError> {
    #[cfg(windows)]
    {
        if from_shortcut && windows::selection::capture_in_flight() {
            return Ok(false);
        }
        tokio::task::spawn_blocking(move || windows::clipboard::write_text(&text))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))??;
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        let _ = (text, from_shortcut);
        Err(AppError::UnsupportedPlatform)
    }
}

pub fn popup_placement(width: i32, height: i32) -> Option<PopupPlacement> {
    #[cfg(windows)]
    {
        windows::cursor::popup_placement(width, height).ok()
    }
    #[cfg(not(windows))]
    {
        let _ = (width, height);
        None
    }
}

pub fn ocr_monitor_bounds() -> Option<ScreenBounds> {
    #[cfg(windows)]
    {
        windows::cursor::monitor_bounds_at_cursor().ok()
    }
    #[cfg(not(windows))]
    None
}

pub fn recognize_screen_region(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    language: OcrLanguage,
) -> Result<String, AppError> {
    #[cfg(windows)]
    {
        windows::ocr::capture_and_recognize(x, y, width, height, language)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, width, height, language);
        Err(AppError::UnsupportedPlatform)
    }
}

pub fn capture_screen_region_png(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<Vec<u8>, AppError> {
    #[cfg(windows)]
    {
        windows::ocr::capture_png(x, y, width, height)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, width, height);
        Err(AppError::UnsupportedPlatform)
    }
}
