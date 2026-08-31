use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    UI::WindowsAndMessaging::GetCursorPos,
};

use crate::{errors::AppError, platform::PopupPlacement};

pub fn popup_placement(width: i32, height: i32) -> Result<PopupPlacement, AppError> {
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }.map_err(|error| AppError::Internal(error.to_string()))?;
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(AppError::Internal("GetMonitorInfoW failed".into()));
    }

    let x = (cursor.x + 12).clamp(info.rcWork.left, info.rcWork.right - width);
    let preferred_y = cursor.y + 18;
    let y = if preferred_y + height <= info.rcWork.bottom {
        preferred_y
    } else {
        (cursor.y - height - 12).max(info.rcWork.top)
    };
    Ok(PopupPlacement { x, y })
}
