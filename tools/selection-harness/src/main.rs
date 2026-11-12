//! Native regression driver for the selection capture.
//!
//! Compiles the application's real `selection.rs` and `clipboard.rs`, registers a spare
//! Alt+<key> hotkey, plays the key presses a user would make and calls
//! `capture_selected_text` exactly as the application does. It prints one JSON line of
//! anonymous metadata: whether text was captured, how long it took, where keyboard focus
//! went and what the clipboard holds afterwards. It never prints clipboard content, window
//! titles or document paths.
//!
//! Only run it against documents created for the purpose (see `run.ps1`).
#![allow(non_snake_case)]

use std::{
    env,
    ffi::c_void,
    mem::size_of,
    ptr, slice, thread,
    time::{Duration, Instant},
};

mod errors {
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub enum AppError {
        NoSelectedText,
        Clipboard(String),
    }
}

#[allow(dead_code)]
mod platform {
    // Paths are relative to this inline module's directory, `src/platform/`.
    #[path = "../../../../src-tauri/src/platform/windows/clipboard.rs"]
    pub mod clipboard;
    #[path = "../../../../src-tauri/src/platform/windows/selection.rs"]
    pub mod selection;
}

/// What the clipboard holds before each capture, to check that it is put back.
const ORIGINAL_CLIPBOARD: &str = "QTORIG text that was on the clipboard before the capture";

#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    ty: u32,
    ki: KeybdInput,
    pad: [u8; 8],
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    x: i32,
    y: i32,
    private: u32,
}

#[repr(C)]
struct GuiThreadInfo {
    cb: u32,
    flags: u32,
    active: isize,
    focus: isize,
    capture: isize,
    menu_owner: isize,
    move_size: isize,
    caret: isize,
    rc: [i32; 4],
}

type EnumProc = unsafe extern "system" fn(isize, isize) -> i32;

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn GetClassNameW(hwnd: isize, buf: *mut u16, n: i32) -> i32;
    fn GetAsyncKeyState(vk: i32) -> i16;
    fn RegisterHotKey(hwnd: isize, id: i32, mods: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
    fn PeekMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
    fn SendInput(n: u32, inputs: *const Input, cb: i32) -> u32;
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn GetClipboardData(format: u32) -> isize;
    fn SetClipboardData(format: u32, handle: isize) -> isize;
    fn CountClipboardFormats() -> i32;
    fn AttachThreadInput(a: u32, b: u32, attach: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn BringWindowToTop(hwnd: isize) -> i32;
    fn EnumWindows(cb: EnumProc, lparam: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: isize) -> i32;
    fn GetWindow(hwnd: isize, cmd: u32) -> isize;
    fn GetGUIThreadInfo(thread: u32, info: *mut GuiThreadInfo) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalFree(h: isize) -> isize;
    fn GlobalLock(h: isize) -> *mut c_void;
    fn GlobalUnlock(h: isize) -> i32;
    fn GlobalSize(h: isize) -> usize;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn QueryFullProcessImageNameW(h: isize, flags: u32, buf: *mut u16, size: *mut u32) -> i32;
    fn CloseHandle(h: isize) -> i32;
    fn GetCurrentThreadId() -> u32;
}

const CF_UNICODETEXT: u32 = 13;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const WM_HOTKEY: u32 = 0x0312;

fn key(vk: u16, up: bool) -> Input {
    let extended = matches!(vk, 0x21..=0x28 | 0x2D | 0x2E);
    let mut flags = 0u32;
    if up {
        flags |= 0x0002;
    }
    if extended {
        flags |= 0x0001;
    }
    Input {
        ty: 1,
        ki: KeybdInput {
            vk,
            scan: 0,
            flags,
            time: 0,
            extra: 0,
        },
        pad: [0; 8],
    }
}

fn send(inputs: &[Input]) -> u32 {
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<Input>() as i32,
        )
    }
}

fn down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn vk_of(name: &str) -> Option<u16> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "ctrl" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "esc" => 0x1B,
        "enter" => 0x0D,
        "tab" => 0x09,
        "home" => 0x24,
        "end" => 0x23,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ if n.len() == 1 && n.as_bytes()[0].is_ascii_alphanumeric() => {
            n.to_ascii_uppercase().as_bytes()[0] as u16
        }
        _ if n.starts_with('f') => 0x6F + n[1..].parse::<u16>().ok()?,
        _ => return None,
    })
}

/// Plays a key sequence such as `esc,ctrl+a,shift+left*5`.
fn play(sequence: &str) {
    for combo in sequence.split(',').filter(|c| !c.is_empty()) {
        let (combo, repeat) = match combo.split_once('*') {
            Some((c, n)) => (c, n.parse::<u32>().unwrap_or(1)),
            None => (combo, 1),
        };
        let keys: Vec<u16> = combo.split('+').filter_map(vk_of).collect();
        for _ in 0..repeat {
            let mut inputs: Vec<Input> = keys.iter().map(|vk| key(*vk, false)).collect();
            inputs.extend(keys.iter().rev().map(|vk| key(*vk, true)));
            send(&inputs);
            thread::sleep(Duration::from_millis(40));
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn process_name(pid: u32) -> String {
    unsafe {
        let handle = OpenProcess(0x1000, 0, pid);
        if handle == 0 {
            return "?".into();
        }
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return "?".into();
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        path.rsplit('\\').next().unwrap_or("?").to_string()
    }
}

/// (thread id, process name, window class) of a window.
fn window_info(hwnd: isize) -> (u32, String, String) {
    if hwnd == 0 {
        return (0, "none".into(), "none".into());
    }
    unsafe {
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(hwnd, &mut pid);
        let mut class = [0u16; 128];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        (tid, process_name(pid), String::from_utf16_lossy(&class[..n]))
    }
}

/// Window class of the control that has keyboard focus in the foreground application.
fn focused_class() -> String {
    unsafe {
        let (tid, process, _) = window_info(GetForegroundWindow());
        let mut info: GuiThreadInfo = std::mem::zeroed();
        info.cb = size_of::<GuiThreadInfo>() as u32;
        if GetGUIThreadInfo(tid, &mut info) == 0 {
            return format!("{process}/?");
        }
        format!("{process}/{}", window_info(info.focus).2)
    }
}

struct Search {
    target: String,
    found: isize,
}

unsafe extern "system" fn find_window(hwnd: isize, lparam: isize) -> i32 {
    let search = &mut *(lparam as *mut Search);
    if IsWindowVisible(hwnd) == 0 || GetWindowTextLengthW(hwnd) == 0 || GetWindow(hwnd, 4) != 0 {
        return 1;
    }
    if window_info(hwnd).1.eq_ignore_ascii_case(&search.target) {
        search.found = hwnd;
        return 0;
    }
    1
}

/// Brings the first visible top-level window of `target` (a process name) to the front.
fn activate(target: &str) -> bool {
    unsafe {
        let mut search = Search {
            target: target.to_string(),
            found: 0,
        };
        EnumWindows(find_window, &mut search as *mut Search as isize);
        if search.found == 0 {
            return false;
        }
        let hwnd = search.found;
        for _ in 0..5 {
            let foreground = GetForegroundWindow();
            if foreground == hwnd {
                return true;
            }
            let foreground_thread = GetWindowThreadProcessId(foreground, ptr::null_mut());
            let me = GetCurrentThreadId();
            AttachThreadInput(me, foreground_thread, 1);
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, 9);
            }
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            AttachThreadInput(me, foreground_thread, 0);
            thread::sleep(Duration::from_millis(250));
        }
        GetForegroundWindow() == hwnd
    }
}

/// How the clipboard looks, without revealing what is on it.
fn clipboard_state(marker: &str) -> &'static str {
    unsafe {
        if OpenClipboard(0) == 0 {
            return "unreadable";
        }
        let formats = CountClipboardFormats();
        let handle = GetClipboardData(CF_UNICODETEXT);
        let mut text = None;
        if handle != 0 {
            let pointer = GlobalLock(handle) as *const u16;
            if !pointer.is_null() {
                let max = GlobalSize(handle) / 2;
                let buffer = slice::from_raw_parts(pointer, max);
                let len = buffer.iter().position(|v| *v == 0).unwrap_or(max);
                text = Some(String::from_utf16_lossy(&buffer[..len]));
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        match text.as_deref() {
            Some(ORIGINAL_CLIPBOARD) => "original",
            Some(text) if !marker.is_empty() && text.contains(marker) => "selection",
            Some(_) => "other-text",
            None if formats == 0 => "empty",
            None => "non-text",
        }
    }
}

fn write_clipboard(text: &str) -> bool {
    unsafe {
        let mut utf16: Vec<u16> = text.encode_utf16().collect();
        utf16.push(0);
        let global = GlobalAlloc(2, utf16.len() * 2);
        if global == 0 {
            return false;
        }
        let pointer = GlobalLock(global) as *mut u16;
        if pointer.is_null() {
            GlobalFree(global);
            return false;
        }
        ptr::copy_nonoverlapping(utf16.as_ptr(), pointer, utf16.len());
        GlobalUnlock(global);
        if OpenClipboard(0) == 0 {
            GlobalFree(global);
            return false;
        }
        let ok = EmptyClipboard() != 0 && SetClipboardData(CF_UNICODETEXT, global) != 0;
        CloseClipboard();
        ok
    }
}

fn clear_clipboard() {
    unsafe {
        if OpenClipboard(0) != 0 {
            EmptyClipboard();
            CloseClipboard();
        }
    }
}

fn arg(name: &str, default: &str) -> String {
    let args: Vec<String> = env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| default.to_string())
}

fn main() {
    let target = arg("--target", "");
    let label = arg("--label", "");
    let marker = arg("--marker", "");
    let hotkey = vk_of(&arg("--hotkey", "j")).unwrap_or(0x4A);
    let hold_ms: u64 = arg("--hold-ms", "120").parse().unwrap_or(120);
    let presses: u32 = arg("--presses", "1").parse().unwrap_or(1);
    let gap_ms: u64 = arg("--gap-ms", "60").parse().unwrap_or(60);
    let alt_first = arg("--release", "key-first") == "alt-first";
    let original = arg("--orig", "text");
    let pre_keys = arg("--pre-keys", "");
    let post_keys = arg("--post-keys", "");

    let head = format!(
        "{{\"label\":\"{label}\",\"target\":\"{target}\",\"hold_ms\":{hold_ms},\"release\":\"{}\",\"orig\":\"{original}\",\"presses\":{presses}",
        if alt_first { "alt-first" } else { "key-first" }
    );
    let fail = |reason: &str| -> ! {
        println!("{head},\"abort\":\"{reason}\"}}");
        std::process::exit(2);
    };

    if target.is_empty() {
        fail("no --target given");
    }
    if !activate(&target) {
        fail("cannot bring the target to the front");
    }
    thread::sleep(Duration::from_millis(300));
    if !pre_keys.is_empty() {
        play(&pre_keys);
        thread::sleep(Duration::from_millis(250));
    }
    if !window_info(unsafe { GetForegroundWindow() })
        .1
        .eq_ignore_ascii_case(&target)
    {
        fail("another window came to the front before the trigger");
    }
    if down(VK_MENU) || down(VK_CONTROL) || down(VK_SHIFT) {
        fail("a modifier key is physically held; someone is using the keyboard");
    }
    // MOD_ALT | MOD_NOREPEAT, the same shape as the application's Alt+<letter> shortcut.
    if unsafe { RegisterHotKey(0, 1, 0x0001 | 0x4000, hotkey as u32) } == 0 {
        fail("RegisterHotKey failed; pick another --hotkey");
    }

    match original.as_str() {
        "text" => {
            write_clipboard(ORIGINAL_CLIPBOARD);
        }
        "empty" => clear_clipboard(),
        _ => {}
    }
    thread::sleep(Duration::from_millis(200));
    let focus_before = focused_class();

    let user = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        for _ in 0..presses {
            send(&[key(VK_MENU, false)]);
            thread::sleep(Duration::from_millis(30));
            send(&[key(hotkey, false)]);
            thread::sleep(Duration::from_millis(hold_ms));
            let (first, second) = if alt_first {
                (VK_MENU, hotkey)
            } else {
                (hotkey, VK_MENU)
            };
            send(&[key(first, true)]);
            thread::sleep(Duration::from_millis(15));
            send(&[key(second, true)]);
            thread::sleep(Duration::from_millis(gap_ms));
        }
    });

    let mut captures = Vec::new();
    let mut message: Msg = unsafe { std::mem::zeroed() };
    let wait = Instant::now();
    while wait.elapsed() < Duration::from_millis(8000) {
        if unsafe { PeekMessageW(&mut message, 0, 0, 0, 1) } != 0 && message.message == WM_HOTKEY {
            captures.push(thread::spawn(|| {
                let started = Instant::now();
                let result = platform::selection::capture_selected_text();
                (started.elapsed().as_secs_f64() * 1000.0, result)
            }));
        }
        if user.is_finished() && captures.len() as u32 >= presses {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    let _ = user.join();
    let fired = captures.len();
    let results: Vec<String> = captures
        .into_iter()
        .map(|handle| match handle.join().unwrap() {
            (ms, Ok(text)) => format!(
                "{{\"result\":\"ok\",\"ms\":{ms:.0},\"len\":{},\"marker\":{}}}",
                text.chars().count(),
                !marker.is_empty() && text.contains(&marker)
            ),
            (ms, Err(errors::AppError::NoSelectedText)) => {
                format!("{{\"result\":\"NO_SELECTED_TEXT\",\"ms\":{ms:.0}}}")
            }
            (ms, Err(errors::AppError::Clipboard(_))) => {
                format!("{{\"result\":\"CLIPBOARD_ERROR\",\"ms\":{ms:.0}}}")
            }
        })
        .collect();
    unsafe { UnregisterHotKey(0, 1) };

    thread::sleep(Duration::from_millis(400));
    let focus_after = focused_class();
    let clipboard_after = clipboard_state(&marker);
    if !post_keys.is_empty() {
        play(&post_keys);
    }
    println!(
        "{head},\"fired\":{fired},\"focus_before\":\"{focus_before}\",\"focus_after\":\"{focus_after}\",\"clipboard_after\":\"{clipboard_after}\",\"results\":[{}]}}",
        results.join(",")
    );
}
