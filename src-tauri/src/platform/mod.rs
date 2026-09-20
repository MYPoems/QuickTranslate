mod placement;
#[cfg(windows)]
mod windows;

use crate::errors::AppError;

pub use placement::{place_popup, PopupPlacement, WorkArea};

#[derive(Debug, Clone, Copy)]
pub struct ScreenBounds {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
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

pub async fn copy_text(text: String) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || windows::clipboard::write_text(&text))
            .await
            .map_err(|error| AppError::Internal(error.to_string()))?
    }
    #[cfg(not(windows))]
    {
        let _ = text;
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
) -> Result<String, AppError> {
    #[cfg(windows)]
    {
        windows::ocr::capture_and_recognize(x, y, width, height)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, width, height);
        Err(AppError::UnsupportedPlatform)
    }
}
