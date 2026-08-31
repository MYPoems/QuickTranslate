use std::{mem::size_of, thread, time::Duration};

use windows::Win32::{
    System::{
        Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        DataExchange::GetClipboardSequenceNumber,
        Ole::{OleGetClipboard, OleSetClipboard},
    },
    UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL, VK_MENU,
    },
};

use crate::errors::AppError;

use super::clipboard;

const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

pub fn capture_selected_text() -> Result<String, AppError> {
    let com_initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let original_data = if com_initialized {
        unsafe { OleGetClipboard() }.ok()
    } else {
        None
    };
    let original_text = clipboard::read_text().ok().flatten();
    let original_sequence = unsafe { GetClipboardSequenceNumber() };

    send_copy_shortcut()?;

    let mut captured = None;
    for _ in 0..25 {
        thread::sleep(Duration::from_millis(10));
        if unsafe { GetClipboardSequenceNumber() } == original_sequence {
            continue;
        }
        match clipboard::read_text() {
            Ok(Some(text)) if !text.trim().is_empty() => {
                captured = Some(text);
                break;
            }
            Ok(_) | Err(_) => continue,
        }
    }

    let restored = if let Some(data) = original_data.as_ref() {
        unsafe { OleSetClipboard(data) }.is_ok()
    } else if let Some(text) = original_text.as_deref() {
        clipboard::write_text(text).is_ok()
    } else {
        clipboard::clear().is_ok()
    };

    if cfg!(debug_assertions) && !restored {
        eprintln!("QuickTranslate: original clipboard content could not be restored");
    }
    if com_initialized {
        unsafe { CoUninitialize() };
    }

    captured.ok_or(AppError::NoSelectedText)
}

fn send_copy_shortcut() -> Result<(), AppError> {
    let inputs = [
        keyboard_input(VK_MENU, true),
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_C, false),
        keyboard_input(VK_C, true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(AppError::Clipboard(
            "SendInput did not send all key events".into(),
        ));
    }
    Ok(())
}

fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
