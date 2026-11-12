import { invoke } from "@tauri-apps/api/core";

const idleHint = "点击输入框后，直接按下新的组合键";

/** Keys that may follow the modifiers, named the way the backend parses a shortcut. */
function keyName(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
    Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", Insert: "Insert", Delete: "Delete",
    Backquote: "Backquote", Minus: "Minus", Equal: "Equal", BracketLeft: "BracketLeft", BracketRight: "BracketRight",
    Backslash: "Backslash", Semicolon: "Semicolon", Quote: "Quote", Comma: "Comma", Period: "Period", Slash: "Slash",
  };
  return named[code] || "";
}

/**
 * The shortcut a key press stands for, such as `Alt+Q`, or a reason it cannot be one.
 * Exported for tests.
 */
export function shortcutFromEvent(event: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): { shortcut?: string; problem?: string } {
  const key = keyName(event.code);
  if (!key) return { problem: "继续按下字母、数字或功能键" };
  if (event.metaKey) return { problem: "不支持包含 Win 键的组合" };
  const modifiers = [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift"].filter(Boolean) as string[];
  const functionKey = /^F\d+$/.test(key);
  if (!modifiers.length && !functionKey) return { problem: "需要同时按住 Ctrl、Alt 或 Shift" };
  // Shift plus a character key is ordinary typing; it would fire on every capital letter.
  if (modifiers.length === 1 && modifiers[0] === "Shift" && !functionKey) return { problem: "Shift 需要与 Ctrl 或 Alt 一起使用" };
  return { shortcut: [...modifiers, key].join("+") };
}

/**
 * Turns a read-only text input into a shortcut recorder: the user presses the combination
 * instead of typing its name. `hint` explains what to do and reports problems as they
 * arise: a combination that cannot be a shortcut, one that equals `other`, or one that
 * another program already holds.
 */
export function attachShortcutRecorder(input: HTMLInputElement, other: HTMLInputElement, hint: HTMLElement): void {
  let check = 0;
  const show = (message: string, invalid: boolean) => {
    hint.textContent = message;
    hint.dataset.kind = invalid ? "error" : "neutral";
    if (invalid) input.setAttribute("aria-invalid", "true"); else input.removeAttribute("aria-invalid");
  };
  const validate = async () => {
    const current = ++check;
    const value = input.value.trim();
    if (!value) { show(idleHint, false); return; }
    if (value.toLowerCase() === other.value.trim().toLowerCase()) { show("与另一个快捷键相同，请换一个组合", true); return; }
    try {
      const state = await invoke<string | undefined>("check_shortcut", { shortcut: value });
      if (current !== check) return;
      if (state === "taken") show("这个组合已被其他程序占用，保存时会失败", true);
      else if (state === "invalid") show("这个组合不能用作快捷键", true);
      else show(idleHint, false);
    } catch { if (current === check) show(idleHint, false); }
  };
  show(idleHint, false);
  input.addEventListener("focus", () => { if (!input.hasAttribute("aria-invalid")) hint.textContent = "请按下新的组合键；按 Esc 取消"; });
  input.addEventListener("blur", () => { if (!input.hasAttribute("aria-invalid")) hint.textContent = idleHint; });
  input.addEventListener("keydown", event => {
    if (event.key === "Tab") return;
    event.preventDefault();
    if (event.key === "Escape") { input.blur(); return; }
    const { shortcut, problem } = shortcutFromEvent(event);
    if (!shortcut) { hint.textContent = problem || idleHint; return; }
    input.value = shortcut;
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
    void validate();
  });
  // Loading settings or a backup sets the value from outside; the other field changing can
  // create or resolve a clash.
  other.addEventListener("input", () => void validate());
}
