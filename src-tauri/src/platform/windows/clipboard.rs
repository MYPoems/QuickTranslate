use std::{ptr, slice};

use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
    },
};

use crate::errors::AppError;

struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Result<Self, AppError> {
        unsafe { OpenClipboard(None) }.map_err(|error| AppError::Clipboard(error.to_string()))?;
        Ok(Self)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn read_text() -> Result<Option<String>, AppError> {
    let _guard = ClipboardGuard::open()?;
    let handle = match unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) } {
        Ok(handle) => handle,
        Err(_) => return Ok(None),
    };
    let global = HGLOBAL(handle.0);
    let pointer = unsafe { GlobalLock(global) } as *const u16;
    if pointer.is_null() {
        return Err(AppError::Clipboard("GlobalLock returned null".into()));
    }
    let byte_size = unsafe { GlobalSize(global) };
    let max_len = byte_size / size_of::<u16>();
    let buffer = unsafe { slice::from_raw_parts(pointer, max_len) };
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(max_len);
    let text = String::from_utf16_lossy(&buffer[..length]);
    let _ = unsafe { GlobalUnlock(global) };
    Ok(Some(text))
}

pub fn write_text(text: &str) -> Result<(), AppError> {
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let byte_size = utf16.len() * size_of::<u16>();

    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_size) }
        .map_err(|error| AppError::Clipboard(error.to_string()))?;
    let pointer = unsafe { GlobalLock(global) } as *mut u16;
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err(AppError::Clipboard("GlobalLock returned null".into()));
    }
    unsafe { ptr::copy_nonoverlapping(utf16.as_ptr(), pointer, utf16.len()) };
    let _ = unsafe { GlobalUnlock(global) };

    let _guard = ClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.map_err(|error| AppError::Clipboard(error.to_string()))?;
    if let Err(error) = unsafe { SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(global.0))) }
    {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err(AppError::Clipboard(error.to_string()));
    }
    Ok(())
}

pub fn clear() -> Result<(), AppError> {
    let _guard = ClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.map_err(|error| AppError::Clipboard(error.to_string()))
}
