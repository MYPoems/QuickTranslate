use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    UI::WindowsAndMessaging::GetCursorPos,
};

use crate::{
    errors::AppError,
    platform::{place_popup, PopupPlacement, WorkArea},
};

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

    Ok(place_popup(
        cursor.x,
        cursor.y,
        width,
        height,
        WorkArea {
            left: info.rcWork.left,
            top: info.rcWork.top,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom,
        },
    ))
}
