use std::{
    mem::size_of,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

use crate::errors::AppError;

use super::clipboard::{self, Snapshot};

const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);
/// An unassigned virtual key: applications ignore it, but it still counts as "another key
/// was pressed while Alt was down".
const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_MENU, VK_CONTROL, VK_SHIFT, VK_LWIN, VK_RWIN];

const RELEASE_TIMEOUT: Duration = Duration::from_millis(1_000);
const RELEASE_POLL: Duration = Duration::from_millis(5);
const COPY_POLLS: u32 = 25;
const COPY_POLL: Duration = Duration::from_millis(10);
const CLIPBOARD_ATTEMPTS: u32 = 5;

/// The clipboard is one shared resource: two overlapping captures would read each other's
/// copy and restore the wrong content.
static CAPTURE: Mutex<()> = Mutex::new(());

pub fn capture_selected_text() -> Result<String, AppError> {
    let _serial = CAPTURE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Must happen first, while the hotkey's modifiers are still physically held.
    mask_held_modifiers()?;
    // Never release the user's keys for them: a synthetic Alt-up makes the target see a lone
    // Alt tap (Office then moves focus to the ribbon, Windows 11 Notepad to its menu, and
    // the copy below never reaches the document), and a still-held letter key would start
    // typing into the selection once Alt reads as up.
    wait_for_modifier_release()?;

    let original = with_retries(|| clipboard::snapshot().ok());
    let original_sequence = clipboard::sequence_number();

    send_copy_shortcut()?;

    let mut captured = None;
    let mut copy_landed = false;
    for _ in 0..COPY_POLLS {
        thread::sleep(COPY_POLL);
        if clipboard::sequence_number() == original_sequence {
            continue;
        }
        copy_landed = true;
        // Only open the clipboard once the source has published text; opening it earlier
        // can make an application that copies in several steps fail its own copy.
        if !clipboard::has_text() {
            continue;
        }
        if let Ok(Some(text)) = clipboard::read_text() {
            if !text.trim().is_empty() {
                captured = Some(text);
                break;
            }
        }
    }

    // Decided once, right after the copy was observed. The sequence number cannot be used to
    // detect a later copy by the user here: reading delay-rendered data (ours above, and the
    // clipboard history service's) advances it as well.
    let restored = match plan_restore(original.as_ref(), copy_landed) {
        Restore::Keep => Some(()),
        Restore::Text(text) => with_retries(|| clipboard::write_text(text).ok()),
        Restore::Clear => with_retries(|| clipboard::clear().ok()),
    };
    if cfg!(debug_assertions) && restored.is_none() {
        eprintln!("QuickTranslate: original clipboard content could not be restored");
    }

    captured.ok_or(AppError::NoSelectedText)
}

/// Clipboard listeners (clipboard history, vendor utilities) briefly hold the clipboard open
/// right after every change, so a single attempt to open it is not enough.
fn with_retries<T>(mut attempt: impl FnMut() -> Option<T>) -> Option<T> {
    for index in 0..CLIPBOARD_ATTEMPTS {
        if index > 0 {
            thread::sleep(COPY_POLL);
        }
        if let Some(value) = attempt() {
            return Some(value);
        }
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
enum Restore<'a> {
    /// Leave the clipboard as it is.
    Keep,
    Text(&'a str),
    Clear,
}

fn plan_restore(original: Option<&Snapshot>, copy_landed: bool) -> Restore<'_> {
    // Our copy never reached the clipboard, so there is nothing to undo.
    if !copy_landed {
        return Restore::Keep;
    }
    match original {
        Some(Snapshot {
            text: Some(text), ..
        }) => Restore::Text(text),
        Some(Snapshot {
            text: None,
            is_empty: true,
        }) => Restore::Clear,
        // Images, files and other non-text content cannot be put back from here, and an
        // unreadable clipboard has no known previous state.
        _ => Restore::Keep,
    }
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000) != 0
}

/// A hotkey such as Alt+Q is swallowed by the system, so the focused application only sees
/// "Alt down ... Alt up" and treats it as a tap on Alt or Win. Tapping a no-op key in
/// between defuses that, whichever key the user lets go of first.
fn mask_held_modifiers() -> Result<(), AppError> {
    if ![VK_MENU, VK_LWIN, VK_RWIN].into_iter().any(is_down) {
        return Ok(());
    }
    send_keys(&[
        keyboard_input(VK_MASK, false),
        keyboard_input(VK_MASK, true),
    ])
}

fn wait_for_modifier_release() -> Result<(), AppError> {
    let deadline = Instant::now() + RELEASE_TIMEOUT;
    while MODIFIERS.into_iter().any(is_down) {
        if Instant::now() >= deadline {
            return Err(AppError::Clipboard(
                "shortcut keys were still held when the copy was due".into(),
            ));
        }
        thread::sleep(RELEASE_POLL);
    }
    Ok(())
}

fn send_copy_shortcut() -> Result<(), AppError> {
    send_keys(&[
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_C, false),
        keyboard_input(VK_C, true),
        keyboard_input(VK_CONTROL, true),
    ])
}

fn send_keys(inputs: &[INPUT]) -> Result<(), AppError> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Snapshot {
        Snapshot {
            text: Some(value.into()),
            is_empty: false,
        }
    }

    #[test]
    fn restores_previous_text_only_after_a_copy_replaced_it() {
        let original = text("before");
        assert_eq!(plan_restore(Some(&original), true), Restore::Text("before"));
        // The copy never happened: the clipboard still holds what it held.
        assert_eq!(plan_restore(Some(&original), false), Restore::Keep);
    }

    #[test]
    fn restores_emptiness_but_never_discards_content_it_cannot_put_back() {
        let empty = Snapshot {
            text: None,
            is_empty: true,
        };
        let image = Snapshot {
            text: None,
            is_empty: false,
        };
        assert_eq!(plan_restore(Some(&empty), true), Restore::Clear);
        assert_eq!(plan_restore(Some(&image), true), Restore::Keep);
        assert_eq!(plan_restore(None, true), Restore::Keep);
    }
}
