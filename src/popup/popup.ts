import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { AppError, TranslationEvent, TranslationResult, SpeechPreferences } from "../types";
import { prepareSpeech, unlockSpeechAudio } from "../speech/stream";
import { Reader, browserAudio, splitSpeech, type ReadSide, type ReaderState } from "../speech/reader";
import { mountCollector } from "../vocabulary/collect";
import { icon, speakerMarkup } from "../ui/icons";
import "./popup.css";

const root = document.querySelector<HTMLElement>("#app")!;
const el = <T extends HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
let currentRequestId = 0, localGeneration = 0, readStart = 0;
let currentSource = "", currentTranslation = "", selectedWord = "", selectionSource = "";
let currentSourceKind: "selection" | "ocr" = "selection";
let pinned = false, translationStale = false;
let readingSide: ReadSide | undefined;
const reader = new Reader(prepareSpeech, browserAudio, renderReading);

export function mountPopup(): void {
  root.innerHTML = `<section class="popup-shell">
    <header class="popup-header" data-tauri-drag-region><span data-tauri-drag-region>QuickTranslate</span><div>
      <button id="pin" class="icon-button" type="button" aria-label="固定悬浮窗" aria-pressed="false" title="固定悬浮窗">${icon("pin")}</button>
      <button id="close" class="icon-button" type="button" aria-label="关闭">${icon("close")}</button></div></header>
    <div class="source-row"><p id="source" class="source">选择文字后按 Alt + Q</p><span id="badge" class="badge" hidden></span></div>
    <div id="content" class="content idle"><p class="hint">QuickTranslate 将在这里显示译文</p></div>
    <p id="reading-status" class="reading-status" role="status" hidden></p>
    <footer class="actions">
      <button id="copy" class="text-button primary-button" type="button" disabled title="Ctrl+C">${icon("copy")}<span>复制译文</span></button>
      <button id="collect-selected" class="text-button collect-context" type="button" hidden>收藏单词</button>
      <button id="more" class="text-button" type="button" aria-expanded="false" aria-controls="popup-menu" aria-haspopup="menu">${icon("more")}更多</button>
      <div id="popup-menu" class="popup-menu" role="menu" aria-label="更多操作" hidden>
        <button id="retranslate" role="menuitem" type="button" disabled>重新翻译 <small>Alt+R</small></button>
        <button id="copy-source" role="menuitem" type="button" disabled>复制原文 <small>Ctrl+Shift+C</small></button>
        <hr><button id="collect-batch" role="menuitem" type="button">选取生词 / 批量收藏</button>
        <button id="open-book" role="menuitem" type="button">打开生词本</button>
        <hr><details><summary>模型与请求信息</summary><p id="meta">就绪</p></details>
      </div>
    </footer></section>`;
  const collector = mountCollector(root, () => ({ source: selectionSource || [currentSource, currentTranslation].join("\n"), translation: selectionSource === currentTranslation ? currentSource : currentTranslation }), true);
  el("#copy").addEventListener("click", () => void copyText(currentTranslation, "#copy"));
  el("#copy-source").addEventListener("click", () => { closeMenu(); void copyText(currentSource, "#copy-source"); });
  el("#retranslate").addEventListener("click", () => { closeMenu(); void retranslate(); });
  el("#pin").addEventListener("click", () => void togglePin());
  el("#close").addEventListener("click", () => void hide());
  el("#more").addEventListener("mousedown", event => event.preventDefault());
  el("#more").addEventListener("click", () => { const open = el("#popup-menu").hidden; el("#popup-menu").hidden = !open; el("#more").setAttribute("aria-expanded", String(open)); });
  root.addEventListener("click", event => { const target = event.target as HTMLElement; const read = target.closest<HTMLButtonElement>("[data-read-side]"); if (read) void startReading(read.dataset.readSide as ReadSide); if (!target.closest("#popup-menu, #more")) closeMenu(); });
  el("#popup-menu").addEventListener("keydown", event => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const buttons = Array.from(el("#popup-menu").querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    let index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    index = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
    buttons[index]?.focus(); event.preventDefault();
  });
  el("#more").addEventListener("keydown", event => { if (event.key === "ArrowDown" || event.key === "ArrowUp") { el("#popup-menu").hidden = false; el("#more").setAttribute("aria-expanded", "true"); const buttons = el("#popup-menu").querySelectorAll<HTMLButtonElement>('button:not(:disabled)'); (event.key === "ArrowDown" ? buttons[0] : buttons[buttons.length - 1])?.focus(); event.preventDefault(); } });
  document.addEventListener("selectionchange", () => {
    if (collector.dialog.open) return;
    const active = document.activeElement;
    if (active instanceof HTMLTextAreaElement && active.classList.contains("recognized-text")) { selectedWord = active.value.slice(active.selectionStart, active.selectionEnd).trim(); selectionSource = currentSource; }
    else { const selection = window.getSelection(); const parent = selection?.anchorNode?.parentElement; selectedWord = parent && root.contains(parent) ? selection!.toString().trim() : ""; selectionSource = parent?.closest(".translation") ? currentTranslation : currentSource; }
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
    if (event.key === "Escape") { if (collector.dialog.open) collector.close(); else if (!el("#popup-menu").hidden) { closeMenu(); el("#more").focus(); } else void hide(); event.preventDefault(); return; }
    const target = event.target instanceof HTMLElement ? event.target : undefined;
    if (collector.dialog.open || target?.closest('input, textarea, select, [contenteditable="true"]')) return;
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c" && currentSource) { event.preventDefault(); void copyText(currentSource, "#copy-source"); }
    else if (event.ctrlKey && event.key.toLowerCase() === "c" && currentTranslation && !window.getSelection()?.toString()) { event.preventDefault(); void copyText(currentTranslation, "#copy"); }
    else if (event.altKey && event.key.toLowerCase() === "r" && currentSource) { event.preventDefault(); void retranslate(); }
  });
  void invoke<boolean>("get_popup_pinned").then(updatePin).catch(() => {});
  void listen<TranslationEvent>("translation-state", ({ payload }) => { if (payload.status === "loading" && payload.requestId >= currentRequestId) { collector.close(); closeMenu(); selectedWord = ""; selectionSource = ""; } render(payload); }).then(() => emit("popup-ready"));
}
function closeMenu(): void { el("#popup-menu").hidden = true; el("#more").setAttribute("aria-expanded", "false"); }
function refreshCollection(): void {
  const word = selectedWord || currentSource.trim(); const valid = word.length <= 48 && /^[A-Za-z]+(?:['’-][A-Za-z]+)*$/.test(word);
  el("#collect-selected").hidden = !valid; el("#collect-selected").textContent = valid ? `收藏 ${word}` : "收藏单词"; el("#collect-selected").title = valid ? `收藏 ${word}` : "收藏单词";
}
function sourceSection(content: HTMLElement): void {
  const section = document.createElement("section"); section.className = "source-section";
  const heading = document.createElement("div"); heading.className = "section-heading";
  heading.innerHTML = `<p class="section-label">${currentSourceKind === "ocr" ? "原文 · 识别文字可编辑" : "原文"}</p>${speakerMarkup("朗读原文", "read-source")}`;
  heading.querySelector<HTMLButtonElement>("button")!.dataset.readSide = "source"; section.append(heading);
  if (currentSourceKind === "ocr") {
    const editor = document.createElement("textarea"); editor.className = "recognized-text"; editor.value = currentSource; editor.rows = Math.min(6, Math.max(2, currentSource.split("\n").length)); editor.setAttribute("aria-label", "OCR 识别文字，可编辑后重新翻译");
    editor.addEventListener("input", () => { stopReading(); currentSource = editor.value; translationStale = true; el("#badge").textContent = "原文已修改"; el("#badge").hidden = false; el<HTMLButtonElement>("#copy-source").disabled = !currentSource.trim(); el<HTMLButtonElement>("#retranslate").disabled = !currentSource.trim(); el<HTMLButtonElement>("#retranslate-edits").hidden = false; el<HTMLButtonElement>("#retranslate-edits").disabled = !currentSource.trim(); refreshReadingButtons(); refreshCollection(); });
    const retry = document.createElement("button"); retry.id = "retranslate-edits"; retry.type = "button"; retry.className = "text-button inline-retry"; retry.textContent = "重新翻译修改后的文字"; retry.hidden = true; retry.addEventListener("click", () => void retranslate()); section.append(editor, retry);
  } else { const text = document.createElement("div"); text.className = "original-text"; appendReadingSegments(text, currentSource, "source"); section.append(text); }
  content.append(section);
}
function render(event: TranslationEvent): void {
  if (event.requestId < currentRequestId) return;
  if (event.requestId > currentRequestId) { selectedWord = ""; selectionSource = ""; }
  currentRequestId = event.requestId; stopReading();
  if (event.status === "loading") { localGeneration++; translationStale = false; currentSourceKind = event.sourceKind || "selection"; currentSource = event.sourceText || ""; currentTranslation = ""; }
  else if (event.result) { currentSourceKind = event.sourceKind || currentSourceKind; currentSource = event.result.sourceText; currentTranslation = event.result.translation; translationStale = false; }
  else if (event.status === "error") currentTranslation = "";
  const content = el("#content"); content.className = "content"; content.replaceChildren();
  el("#source").textContent = `${currentSourceKind === "ocr" ? "截图翻译 · OCR" : "划词翻译"}${event.result ? ` · ${event.result.detectedLanguage === "english" ? "EN → 中文" : "中文 → EN"}` : ""}`;
  if (currentSource) sourceSection(content);
  const heading = document.createElement("div"); heading.className = "section-heading"; heading.innerHTML = `<p class="section-label">译文</p>${speakerMarkup("朗读译文", "read-translation")}`; heading.querySelector<HTMLButtonElement>("button")!.dataset.readSide = "translation"; content.append(heading);
  if (event.status === "loading") { const loading = document.createElement("div"); loading.className = "inline-loading"; loading.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>${currentSourceKind === "ocr" && !currentSource ? "正在识别图片文字…" : "正在翻译…"}</p>`; content.append(loading); }
  else if (event.status === "error") { const error = document.createElement("p"); error.className = "error-message"; error.textContent = event.error?.message || "翻译失败"; content.append(error); }
  else if (event.result) appendResult(content, event.result);
  el<HTMLButtonElement>("#copy").disabled = !currentTranslation; el<HTMLButtonElement>("#copy-source").disabled = !currentSource; el<HTMLButtonElement>("#retranslate").disabled = !currentSource || event.status === "loading";
  el("#meta").textContent = event.result ? `${event.result.provider} · ${event.result.model}` : event.error?.code || "正在请求服务";
  el("#badge").textContent = event.result?.cached ? "缓存" : ""; el("#badge").hidden = !event.result?.cached; refreshReadingButtons(); refreshCollection();
}
function appendResult(content: HTMLElement, result: TranslationResult): void {
  const translation = document.createElement("div"); translation.className = "translation"; appendReadingSegments(translation, result.translation, "translation"); content.append(translation);
  for (const [className, value] of [["word-detail", [result.partOfSpeech, result.phonetic].filter(Boolean).join(" · ")], ["example", result.example]]) { if (value) { const paragraph = document.createElement("p"); paragraph.className = className!; paragraph.textContent = value; content.append(paragraph); } }
  if (result.definitions?.length) { const list = document.createElement("ul"); list.className = "definitions"; for (const value of result.definitions) { const item = document.createElement("li"); item.textContent = value; list.append(item); } content.append(list); }
}
async function copyText(text: string, selector: string): Promise<void> {
  if (!text) return; const button = el<HTMLButtonElement>(selector), original = button.innerHTML;
  try { await invoke("copy_translation", { text }); button.textContent = "已复制"; } catch { button.textContent = "复制失败"; }
  window.setTimeout(() => { if (button.isConnected) button.innerHTML = original; }, 1200);
}
async function retranslate(): Promise<void> {
  if (!currentSource) return; const sourceText = currentSource, requestId = currentRequestId, sourceKind = currentSourceKind;
  render({ requestId, status: "loading", sourceText, sourceKind }); const generation = localGeneration;
  try { const result = await invoke<TranslationResult>("retranslate_text", { text: sourceText }); if (currentRequestId === requestId && generation === localGeneration) render({ requestId, status: "success", result, sourceKind }); }
  catch (error) { if (currentRequestId === requestId && generation === localGeneration) render({ requestId, status: "error", error: normalizeError(error) }); }
}
async function togglePin(): Promise<void> { try { updatePin(await invoke<boolean>("set_popup_pinned", { pinned: !pinned })); } catch { /* Preserve the persisted state. */ } }
function updatePin(value: boolean): void { pinned = value; const button = el("#pin"); button.setAttribute("aria-pressed", String(value)); button.title = value ? "取消固定" : "固定悬浮窗"; button.setAttribute("aria-label", button.title); }
function normalizeError(error: unknown): AppError { return { code: "ERROR", message: String((error as AppError)?.message || error || "操作失败") }; }
async function hide(): Promise<void> { stopReading(); closeMenu(); await invoke("hide_translation_window"); }
function refreshReadingButtons(): void { const source = root.querySelector<HTMLButtonElement>("#read-source"); if (source) source.disabled = !currentSource.trim(); const translation = root.querySelector<HTMLButtonElement>("#read-translation"); if (translation) translation.disabled = !currentTranslation.trim() || translationStale; }
function stopReading(backend = true): void { readStart++; readingSide = undefined; reader.stop(); if (backend) void invoke("stop_speech").catch(() => {}); }
async function startReading(side: ReadSide): Promise<void> {
  unlockSpeechAudio(); const toggleOff = readingSide === side; stopReading(); if (toggleOff) return; readingSide = side; const start = readStart; renderReading({ status: "loading" });
  try { await invoke("stop_speech"); const preferences = await invoke<SpeechPreferences>("get_speech_preferences"); if (start !== readStart) return; await reader.read(splitSpeech(side === "source" ? currentSource : currentTranslation, side), preferences); if (start === readStart) readingSide = undefined; }
  catch (error) { if (start === readStart) renderReading({ status: "error", message: normalizeError(error).message }); }
}
function appendReadingSegments(container: HTMLElement, text: string, side: ReadSide): void { for (const segment of splitSpeech(text, side)) { const paragraph = document.createElement("p"); paragraph.className = "read-segment"; paragraph.dataset.side = side; paragraph.dataset.index = String(segment.index); paragraph.textContent = segment.text; container.append(paragraph); } }
function renderReading(state: ReaderState): void {
  const status = root.querySelector<HTMLElement>("#reading-status"); if (!status) return; status.hidden = state.status === "idle"; status.dataset.error = String(state.status === "error");
  status.textContent = state.status === "error" ? state.message || "朗读失败" : `${state.status === "loading" ? "正在准备朗读…" : "正在朗读"}${state.total ? ` · ${state.position}/${state.total}` : ""}`;
  root.querySelectorAll<HTMLButtonElement>("[data-read-side]").forEach(button => { button.setAttribute("aria-pressed", String(state.status !== "idle" && state.status !== "error" && button.dataset.readSide === readingSide)); button.setAttribute("aria-busy", String(state.status === "loading" && button.dataset.readSide === readingSide)); });
  root.querySelectorAll(".reading-active").forEach(element => element.classList.remove("reading-active"));
  if (state.segment) { const active = root.querySelector<HTMLElement>(`[data-side="${state.segment.side}"][data-index="${state.segment.index}"]`); active?.classList.add("reading-active"); active?.scrollIntoView({ block: "nearest" }); }
  if (state.status === "error") { readingSide = undefined; void invoke("stop_speech").catch(() => {}); }
}
