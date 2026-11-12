import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { AppError, Language, TranslationEvent, TranslationResult, SpeechPreferences } from "../types";
import { directionLabel, languageNames, languages } from "../languages";
import { prepareSpeech, unlockSpeechAudio } from "../speech/stream";
import { Reader, browserAudio, splitSpeech, type ReadSide, type ReaderState } from "../speech/reader";
import { mountCollector } from "../vocabulary/collect";
import { icon } from "../ui/icons";
import { appendResult, buildError, buildHeading, buildLoading, buildManualInput, buildSelectionSource } from "./view";
import "./popup.css";

const root = document.querySelector<HTMLElement>("#app")!;
const el = <T extends HTMLElement>(selector: string) => root.querySelector<T>(selector)!;

// The request on screen. Events for an older request are ignored; `localGeneration`
// separates the popup's own retranslations of the same request.
let currentRequestId = 0;
let localGeneration = 0;
let readStart = 0;
let currentSource = "";
let currentTranslation = "";
let currentSourceKind: "selection" | "ocr" = "selection";
let translationStale = false;
// What the content area shows: nothing yet, a request's state, or the manual input box.
let lastEvent: TranslationEvent | undefined;
let loading = false;
let manualInputOpen = false;
// Word collection follows the text selected inside the popup.
let selectedWord = "";
let selectionSource = "";
let pinned = false;
let shortcutLabel = "Alt + Q";
// A language picked in the menu for the text on screen, instead of the usual direction.
let targetOverride: Language | "" = "";
// The languages of the result on screen. Reading aloud has voices for Chinese and English only.
let sourceLanguage: Language | undefined;
let translationLanguage: Language | undefined;
const speakable = (language: Language | undefined) => !language || language === "chinese" || language === "english";
let toastTimer = 0;
let readingSide: ReadSide | undefined;
const reader = new Reader(prepareSpeech, browserAudio, renderReading);

export function mountPopup(): void {
  root.innerHTML = `<section class="popup-shell">
    <header class="popup-header"><span id="source" class="popup-title"></span><span id="badge" class="badge" hidden></span>
      <div class="popup-drag-region" data-tauri-drag-region aria-label="拖动悬浮窗"></div><div class="popup-window-controls">
      <button id="pin" class="icon-button" type="button" aria-label="固定悬浮窗" aria-pressed="false" title="固定悬浮窗">${icon("pin")}</button>
      <button id="close" class="icon-button" type="button" aria-label="关闭">${icon("close")}</button></div></header>
    <div id="content" class="content idle"></div>
    <p id="reading-status" class="reading-status" role="status" hidden></p>
    <p id="popup-toast" class="popup-toast" role="status" hidden></p>
    <footer class="actions">
      <button id="copy" class="text-button primary-button" type="button" disabled title="Ctrl+C">${icon("copy")}<span>复制译文</span></button>
      <button id="collect-selected" class="text-button collect-context" type="button" hidden>收藏单词</button>
      <button id="more" class="text-button" type="button" aria-expanded="false" aria-controls="popup-menu" aria-haspopup="menu">${icon("more")}更多</button>
      <div id="popup-menu" class="popup-menu" role="menu" aria-label="更多操作" hidden>
        <button id="retranslate" role="menuitem" type="button" disabled>重新翻译 <small>Alt+R</small></button>
        <button id="copy-source" role="menuitem" type="button" disabled>复制原文 <small>Ctrl+Shift+C</small></button>
        <button id="manual-input" role="menuitem" type="button">手动输入翻译 <small>Alt+I</small></button>
        <label class="menu-select">译成<select id="target-language" disabled><option value="">自动</option></select></label>
        <button id="reselect" role="menuitem" type="button" hidden>重新框选截图</button>
        <hr><button id="collect-batch" role="menuitem" type="button">选取生词 / 批量收藏</button>
        <button id="open-book" role="menuitem" type="button">打开生词本</button>
        <hr><details><summary>模型与请求信息</summary><p id="meta">就绪</p></details>
      </div>
    </footer></section>`;
  renderIdle();

  const collector = mountCollector(root, () => ({
    source: selectionSource || [currentSource, currentTranslation].join("\n"),
    translation: selectionSource === currentTranslation ? currentSource : currentTranslation,
  }), true);

  el("#copy").addEventListener("click", () => void copyText(currentTranslation, "已复制译文"));
  el("#copy-source").addEventListener("click", () => { closeMenu(); void copyText(currentSource, "已复制原文"); });
  el("#retranslate").addEventListener("click", () => { closeMenu(); void retranslate(); });
  el("#manual-input").addEventListener("click", () => { closeMenu(); openManualInput(); });
  const targetSelect = el<HTMLSelectElement>("#target-language");
  for (const language of languages) targetSelect.add(new Option(languageNames[language], language));
  targetSelect.addEventListener("change", () => {
    targetOverride = targetSelect.value as Language | "";
    closeMenu();
    void retranslate();
  });
  el("#reselect").addEventListener("click", () => { closeMenu(); void invoke("start_ocr_selection"); });
  el("#pin").addEventListener("click", () => void togglePin());
  el("#close").addEventListener("click", () => void hide());
  el("#more").addEventListener("mousedown", event => event.preventDefault());
  el("#more").addEventListener("click", () => {
    const open = el("#popup-menu").hidden;
    el("#popup-menu").hidden = !open;
    el("#more").setAttribute("aria-expanded", String(open));
  });

  root.addEventListener("click", event => {
    const target = event.target as HTMLElement;
    const read = target.closest<HTMLButtonElement>("[data-read-side]");
    if (read) void startReading(read.dataset.readSide as ReadSide);
    const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
    if (action === "open-settings") void invoke("open_settings");
    else if (action === "retry") void retranslate();
    else if (action === "manual-input") openManualInput();
    else if (action === "cancel-input") closeManualInput();
    else if (action === "reselect") void invoke("start_ocr_selection");
    else if (action === "cancel-request") void cancelRequest();
    if (!target.closest("#popup-menu, #more")) closeMenu();
  });
  root.addEventListener("submit", event => {
    if ((event.target as HTMLElement).id !== "manual-form") return;
    event.preventDefault();
    submitManualInput();
  });

  el("#popup-menu").addEventListener("keydown", event => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const buttons = Array.from(el("#popup-menu").querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
    let index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    index = event.key === "Home" ? 0
      : event.key === "End" ? buttons.length - 1
      : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
    buttons[index]?.focus();
    event.preventDefault();
  });
  el("#more").addEventListener("keydown", event => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    el("#popup-menu").hidden = false;
    el("#more").setAttribute("aria-expanded", "true");
    const buttons = el("#popup-menu").querySelectorAll<HTMLButtonElement>("button:not(:disabled)");
    (event.key === "ArrowDown" ? buttons[0] : buttons[buttons.length - 1])?.focus();
    event.preventDefault();
  });

  document.addEventListener("selectionchange", () => {
    if (collector.dialog.open) return;
    const active = document.activeElement;
    if (active instanceof HTMLTextAreaElement && active.classList.contains("recognized-text")) {
      selectedWord = active.value.slice(active.selectionStart, active.selectionEnd).trim();
      selectionSource = currentSource;
    } else {
      const selection = window.getSelection();
      const parent = selection?.anchorNode?.parentElement;
      selectedWord = parent && root.contains(parent) ? selection!.toString().trim() : "";
      selectionSource = parent?.closest(".translation") ? currentTranslation : currentSource;
    }
    refreshCollection();
  });
  el("#collect-selected").addEventListener("mousedown", event => event.preventDefault());
  el("#collect-selected").addEventListener("click", () => void collector.open(selectedWord || currentSource.trim()).catch(() => {}));
  el("#collect-batch").addEventListener("click", () => { closeMenu(); selectionSource = ""; void collector.open("", true).catch(() => {}); });
  el("#open-book").addEventListener("click", () => { closeMenu(); void invoke("open_vocabulary"); });

  void listen<string>("speech-stop", ({ payload }) => { if (payload !== "popup") stopReading(false); });
  void listen("popup-hidden", () => { collector.close(); closeMenu(); stopReading(); });
  document.addEventListener("visibilitychange", () => { if (document.hidden) stopReading(); });

  window.addEventListener("keydown", event => {
    if (event.key === "Escape") {
      if (collector.dialog.open) collector.close();
      else if (!el("#popup-menu").hidden) { closeMenu(); el("#more").focus(); }
      else if (manualInputOpen) closeManualInput();
      else void hide();
      event.preventDefault();
      return;
    }
    const target = event.target instanceof HTMLElement ? event.target : undefined;
    if (collector.dialog.open) return;
    if (target?.id === "manual-text") {
      if (event.ctrlKey && event.key === "Enter") { event.preventDefault(); submitManualInput(); }
      return;
    }
    if (target?.closest('input, textarea, select, [contenteditable="true"]')) return;
    const key = event.key.toLowerCase();
    if (event.ctrlKey && event.shiftKey && key === "c" && currentSource) {
      event.preventDefault();
      void copyText(currentSource, "已复制原文");
    } else if (event.ctrlKey && key === "c" && currentTranslation && !window.getSelection()?.toString()) {
      event.preventDefault();
      void copyText(currentTranslation, "已复制译文", true);
    } else if (event.altKey && key === "r" && currentSource) {
      event.preventDefault();
      void retranslate();
    } else if (event.altKey && key === "i") {
      event.preventDefault();
      openManualInput();
    }
  });

  const applyShortcuts = (value?: { globalShortcut?: string }) => {
    if (!value?.globalShortcut) return;
    shortcutLabel = value.globalShortcut.split("+").map(part => part.trim()).filter(Boolean).join(" + ");
    if (!lastEvent && !manualInputOpen) renderIdle();
  };
  void invoke<{ globalShortcut: string }>("get_shortcuts").then(applyShortcuts).catch(() => {});
  void listen<{ globalShortcut: string }>("shortcuts-changed", ({ payload }) => applyShortcuts(payload));
  void invoke<boolean>("get_popup_pinned").then(updatePin).catch(() => {});
  void listen<{ requestId: number; generation?: number; text: string }>("translation-delta", ({ payload }) => renderDelta(payload));
  void listen<TranslationEvent>("translation-state", ({ payload }) => {
    if (payload.status === "loading" && payload.requestId >= currentRequestId) {
      collector.close();
      closeMenu();
      selectedWord = "";
      selectionSource = "";
    }
    render(payload);
  }).then(() => emit("popup-ready"));
}

function closeMenu(): void {
  el("#popup-menu").hidden = true;
  el("#more").setAttribute("aria-expanded", "false");
}

// Before anything has been translated: how to start, by shortcut or by typing.
function renderIdle(): void {
  el("#source").textContent = `选择文字后按 ${shortcutLabel}`;
  const content = el("#content");
  content.className = "content idle";
  content.replaceChildren();
  const hint = document.createElement("p");
  hint.className = "hint";
  hint.textContent = "译文会显示在这里";
  const manual = document.createElement("button");
  manual.type = "button";
  manual.className = "text-button";
  manual.dataset.action = "manual-input";
  manual.textContent = "或者手动输入要翻译的文字";
  content.append(hint, manual);
}

function refreshCollection(): void {
  const word = selectedWord || currentSource.trim();
  const valid = word.length <= 48 && /^[A-Za-z]+(?:['’-][A-Za-z]+)*$/.test(word);
  const button = el("#collect-selected");
  button.hidden = !valid;
  button.textContent = valid ? `收藏 ${word}` : "收藏单词";
  button.title = button.textContent;
}

// The recognised text of a screenshot can be corrected and translated again.
function buildOcrSource(): HTMLElement {
  const section = document.createElement("section");
  section.className = "source-section";
  section.append(buildHeading("原文 · 识别文字可编辑", "朗读原文", "read-source", "source"));
  const editor = document.createElement("textarea");
  editor.className = "recognized-text";
  editor.value = currentSource;
  editor.rows = Math.min(6, Math.max(2, currentSource.split("\n").length));
  editor.setAttribute("aria-label", "OCR 识别文字，可编辑后重新翻译");
  const retry = document.createElement("button");
  retry.id = "retranslate-edits";
  retry.type = "button";
  retry.className = "text-button inline-retry";
  retry.textContent = "重新翻译修改后的文字";
  retry.hidden = true;
  retry.addEventListener("click", () => void retranslate());
  editor.addEventListener("input", () => {
    stopReading();
    currentSource = editor.value;
    translationStale = true;
    el("#badge").textContent = "原文已修改";
    el("#badge").hidden = false;
    const empty = !currentSource.trim();
    el<HTMLButtonElement>("#copy-source").disabled = empty;
    el<HTMLButtonElement>("#retranslate").disabled = empty;
    retry.hidden = false;
    retry.disabled = empty;
    refreshReadingButtons();
    refreshCollection();
  });
  section.append(editor, retry);
  return section;
}

function render(event: TranslationEvent): void {
  if (event.requestId < currentRequestId) return;
  if (event.requestId > currentRequestId) {
    selectedWord = "";
    selectionSource = "";
    // A language chosen for the previous text does not carry over to the next one.
    targetOverride = "";
    el<HTMLSelectElement>("#target-language").value = "";
    // A new request that fails before any "loading" event (no selection found) has no source
    // of its own; do not leave the previous request's text on screen and behind the buttons.
    if (event.status === "error") currentSource = "";
  }
  currentRequestId = event.requestId;
  lastEvent = event;
  manualInputOpen = false;
  loading = event.status === "loading";
  stopReading();
  if (event.status === "loading") {
    localGeneration++;
    translationStale = false;
    currentSourceKind = event.sourceKind || "selection";
    currentSource = event.sourceText || "";
    currentTranslation = "";
    sourceLanguage = translationLanguage = undefined;
  } else if (event.result) {
    currentSourceKind = event.sourceKind || currentSourceKind;
    currentSource = event.result.sourceText;
    currentTranslation = event.result.translation;
    sourceLanguage = event.result.detectedLanguage;
    translationLanguage = event.result.targetLanguage;
    translationStale = false;
  } else if (event.status === "error") {
    currentTranslation = "";
  }

  const content = el("#content");
  content.className = event.status === "error" ? "content failed" : "content";
  content.replaceChildren();
  const direction = event.result ? ` · ${directionLabel(event.result.detectedLanguage, event.result.targetLanguage)}` : "";
  el("#source").textContent = `${currentSourceKind === "ocr" ? "截图翻译 · OCR" : "划词翻译"}${direction}`;
  if (currentSource) content.append(currentSourceKind === "ocr" ? buildOcrSource() : buildSelectionSource(currentSource));
  content.append(buildHeading("译文", "朗读译文", "read-translation", "translation"));
  if (event.status === "error") content.append(buildError(event.error, !!currentSource.trim(), shortcutLabel));
  else if (event.status === "loading") content.append(buildLoading(currentSourceKind === "ocr" && !currentSource ? "正在识别图片文字…" : "正在翻译…"));
  else if (event.result) appendResult(content, event.result);

  el<HTMLButtonElement>("#copy").disabled = !currentTranslation;
  el<HTMLButtonElement>("#copy-source").disabled = !currentSource;
  el<HTMLButtonElement>("#retranslate").disabled = !currentSource || loading;
  el<HTMLSelectElement>("#target-language").disabled = !currentSource || loading;
  el("#reselect").hidden = currentSourceKind !== "ocr";
  el("#meta").textContent = event.result ? `${event.result.provider} · ${event.result.model}` : event.error?.code || "正在请求服务";
  el("#badge").textContent = event.result?.cached ? "缓存" : "";
  el("#badge").hidden = !event.result?.cached;
  refreshReadingButtons();
  refreshCollection();
}

// Stops whatever is in progress and returns to the idle view. The backend hands back the
// id that now counts as current, so anything the abandoned request still sends is ignored.
async function cancelRequest(): Promise<void> {
  if (!loading) return;
  try {
    const next = await invoke<number>("cancel_request");
    if (typeof next === "number") currentRequestId = Math.max(currentRequestId, next);
  } catch { /* The request finishes on its own; the view below is still right. */ }
  loading = false;
  lastEvent = undefined;
  currentSource = "";
  currentTranslation = "";
  el<HTMLButtonElement>("#copy").disabled = true;
  el<HTMLButtonElement>("#copy-source").disabled = true;
  el<HTMLButtonElement>("#retranslate").disabled = true;
  el<HTMLSelectElement>("#target-language").disabled = true;
  el("#badge").hidden = true;
  renderIdle();
  refreshCollection();
}

// Text that has arrived so far while the translation is still being written. It replaces
// the spinner; the finished result replaces it in turn.
function renderDelta(delta: { requestId: number; generation?: number; text: string }): void {
  if (!loading || manualInputOpen || delta.requestId !== currentRequestId) return;
  if (delta.generation !== undefined && delta.generation !== localGeneration) return;
  const content = el("#content");
  let partial = content.querySelector<HTMLElement>(".translation.streaming");
  if (!partial) {
    content.querySelector(".inline-loading")?.remove();
    partial = document.createElement("div");
    partial.className = "translation streaming";
    partial.setAttribute("aria-busy", "true");
    content.append(partial);
  }
  partial.textContent = delta.text;
}

// A short confirmation that does not resize or relabel the button that was pressed.
function toast(message: string, error = false): void {
  const line = el("#popup-toast");
  line.textContent = message;
  line.dataset.error = String(error);
  line.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { line.hidden = true; }, 1400);
}

// `shortcut` marks a copy triggered by Ctrl+C. While this window is in front, the selection
// hotkey's own synthetic Ctrl+C lands here too; the backend skips that one and returns false.
async function copyText(text: string, done: string, shortcut = false): Promise<void> {
  if (!text) return;
  try {
    if (await invoke<boolean>("copy_translation", { text, shortcut }) === false) return;
    toast(done);
  } catch {
    toast("复制失败", true);
  }
}

async function retranslate(): Promise<void> {
  if (!currentSource.trim()) return;
  const sourceText = currentSource;
  const requestId = currentRequestId;
  const sourceKind = currentSourceKind;
  render({ requestId, status: "loading", sourceText, sourceKind });
  const generation = localGeneration;
  const current = () => currentRequestId === requestId && generation === localGeneration;
  try {
    const result = await invoke<TranslationResult>("retranslate_text", { text: sourceText, requestId, generation, target: targetOverride || undefined });
    if (current()) render({ requestId, status: "success", result, sourceKind });
  } catch (error) {
    if (current()) render({ requestId, status: "error", error: normalizeError(error) });
  }
}

function openManualInput(): void {
  stopReading();
  manualInputOpen = true;
  const content = el("#content");
  content.className = "content";
  content.replaceChildren(buildManualInput(""));
  el("#source").textContent = "手动输入";
  el("#badge").hidden = true;
  el<HTMLTextAreaElement>("#manual-text").focus();
}

function closeManualInput(): void {
  manualInputOpen = false;
  if (lastEvent) render(lastEvent);
  else renderIdle();
}

function submitManualInput(): void {
  const text = el<HTMLTextAreaElement>("#manual-text").value.trim();
  if (!text) { el<HTMLTextAreaElement>("#manual-text").focus(); return; }
  // The backend answers with the usual loading and result events for a new request.
  void invoke("translate_input", { text }).catch(error => toast(normalizeError(error).message, true));
}

async function togglePin(): Promise<void> {
  try { updatePin(await invoke<boolean>("set_popup_pinned", { pinned: !pinned })); }
  catch { /* Preserve the persisted state. */ }
}

function updatePin(value: boolean): void {
  pinned = value;
  const button = el("#pin");
  button.setAttribute("aria-pressed", String(value));
  button.title = value ? "取消固定" : "固定悬浮窗";
  button.setAttribute("aria-label", button.title);
}

function normalizeError(error: unknown): AppError {
  const source = error as AppError | undefined;
  return { code: source?.code || "ERROR", message: String(source?.message || error || "操作失败") };
}

async function hide(): Promise<void> {
  stopReading();
  closeMenu();
  await invoke("hide_translation_window");
}

function refreshReadingButtons(): void {
  const unsupported = "暂不支持朗读这种语言";
  const source = root.querySelector<HTMLButtonElement>("#read-source");
  if (source) {
    source.disabled = !currentSource.trim() || !speakable(sourceLanguage);
    if (!speakable(sourceLanguage)) source.title = unsupported;
  }
  const translation = root.querySelector<HTMLButtonElement>("#read-translation");
  if (translation) {
    translation.disabled = !currentTranslation.trim() || translationStale || !speakable(translationLanguage);
    if (!speakable(translationLanguage)) translation.title = unsupported;
  }
}

function stopReading(backend = true): void {
  readStart++;
  readingSide = undefined;
  reader.stop();
  if (backend) void invoke("stop_speech").catch(() => {});
}

async function startReading(side: ReadSide): Promise<void> {
  unlockSpeechAudio();
  const toggleOff = readingSide === side;
  stopReading();
  if (toggleOff) return;
  readingSide = side;
  const start = readStart;
  renderReading({ status: "loading" });
  try {
    await invoke("stop_speech");
    const preferences = await invoke<SpeechPreferences>("get_speech_preferences");
    if (start !== readStart) return;
    await reader.read(splitSpeech(side === "source" ? currentSource : currentTranslation, side), preferences);
    if (start === readStart) readingSide = undefined;
  } catch (error) {
    if (start === readStart) renderReading({ status: "error", message: normalizeError(error).message });
  }
}

function renderReading(state: ReaderState): void {
  const status = root.querySelector<HTMLElement>("#reading-status");
  if (!status) return;
  status.hidden = state.status === "idle";
  status.dataset.error = String(state.status === "error");
  status.textContent = state.status === "error"
    ? state.message || "朗读失败"
    : `${state.status === "loading" ? "正在准备朗读…" : "正在朗读"}${state.total ? ` · ${state.position}/${state.total}` : ""}`;
  const active = state.status !== "idle" && state.status !== "error";
  root.querySelectorAll<HTMLButtonElement>("[data-read-side]").forEach(button => {
    const mine = button.dataset.readSide === readingSide;
    button.setAttribute("aria-pressed", String(active && mine));
    button.setAttribute("aria-busy", String(state.status === "loading" && mine));
  });
  root.querySelectorAll(".reading-active").forEach(element => element.classList.remove("reading-active"));
  if (state.segment) {
    const segment = root.querySelector<HTMLElement>(`[data-side="${state.segment.side}"][data-index="${state.segment.index}"]`);
    segment?.classList.add("reading-active");
    segment?.scrollIntoView({ block: "nearest" });
  }
  if (state.status === "error") {
    readingSide = undefined;
    void invoke("stop_speech").catch(() => {});
  }
}
