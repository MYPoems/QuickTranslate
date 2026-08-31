#[cfg(windows)]
mod windows;

use crate::errors::AppError;

#[derive(Debug, Clone, Copy)]
pub struct PopupPlacement {
    pub x: i32,
    pub y: i32,
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
