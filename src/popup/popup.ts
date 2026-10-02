import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { AppError, TranslationEvent, TranslationResult, SettingsView, SpeechAudio } from "../types";
import { Reader, browserAudio, splitSpeech, defaultSpeech, type ReadSide, type ReaderState } from "../speech/reader";
import "./popup.css";

const root = document.querySelector<HTMLElement>("#app")!;
let currentRequestId = 0;
let localGeneration = 0;
let currentSource = "";
let currentTranslation = "";
let currentSourceKind: "selection" | "ocr" = "selection";
let pinned = false;
let translationStale = false;
const reader = new Reader((segment, voiceId) => invoke<SpeechAudio>("synthesize_speech", { text: segment.text, language: segment.language, voiceId }), browserAudio, renderReading);
let readStart = 0;

export function mountPopup(): void {
  root.innerHTML = `
    <section class="popup-shell" aria-live="polite">
      <header class="source-row">
        <p id="source" class="source">选择文字后按 Alt + Q</p>
        <span id="badge" class="badge" hidden></span>
      </header>
      <div id="content" class="content idle">
        <p class="hint">QuickTranslate 将在这里显示译文</p>
      </div>
      <section class="reading-tools" aria-label="朗读控制">
        <div class="read-buttons">
          <button id="read-source" class="text-button" type="button" disabled>朗读原文</button>
          <button id="read-translation" class="text-button" type="button" disabled>朗读译文</button>
          <button id="read-both" class="text-button" type="button" disabled>双语跟读</button>
        </div>
        <div id="playback" hidden>
          <span id="reading-status" role="status"></span>
          <button id="pause-reading" class="text-button" type="button">暂停</button>
          <button id="stop-reading" class="text-button" type="button">停止</button>
        </div>
      </section>
      <footer class="actions">
        <span id="meta" class="meta">就绪</span>
        <div class="action-buttons">
          <button id="pin" class="icon-button pin-button" type="button" title="固定悬浮窗" aria-label="固定悬浮窗" aria-pressed="false">⌖</button>
          <button id="retranslate" class="text-button" type="button" disabled title="Alt+R">重译</button>
          <button id="copy-source" class="text-button" type="button" disabled title="Ctrl+Shift+C">原文</button>
          <button id="copy" class="text-button" type="button" disabled title="Ctrl+C">译文</button>
          <button id="close" class="icon-button" type="button" aria-label="关闭">×</button>
        </div>
      </footer>
    </section>`;

  root.querySelector<HTMLButtonElement>("#copy")!.addEventListener("click", () => {
    void copyText(currentTranslation, "#copy");
  });
  root.querySelector<HTMLButtonElement>("#copy-source")!.addEventListener("click", () => {
    void copyText(currentSource, "#copy-source");
  });
  root.querySelector<HTMLButtonElement>("#retranslate")!.addEventListener("click", () => {
    void retranslate();
  });
  root.querySelector<HTMLButtonElement>("#pin")!.addEventListener("click", () => {
    void togglePin();
  });
  root.querySelector<HTMLButtonElement>("#close")!.addEventListener("click", () => void hide());
  for (const [id, side] of [["read-source", "source"], ["read-translation", "translation"], ["read-both", "both"]] as const) {
    root.querySelector<HTMLButtonElement>(`#${id}`)!.addEventListener("click", () => void startReading(side));
  }
  root.querySelector<HTMLButtonElement>("#stop-reading")!.addEventListener("click", stopReading);
  root.querySelector<HTMLButtonElement>("#pause-reading")!.addEventListener("click", () => void reader.togglePause());
  document.addEventListener("visibilitychange", () => { if (document.hidden) stopReading(); });
  void listen("popup-hidden", stopReading);
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void hide();
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c" && currentSource) {
      event.preventDefault();
      void copyText(currentSource, "#copy-source");
    } else if (event.ctrlKey && event.key.toLowerCase() === "c" && currentTranslation && !window.getSelection()?.toString() && !(event.target instanceof HTMLTextAreaElement)) {
      event.preventDefault();
      void copyText(currentTranslation, "#copy");
    } else if (event.altKey && event.key.toLowerCase() === "r" && currentSource) {
      event.preventDefault();
      void retranslate();
    }
  });
  void invoke<boolean>("get_popup_pinned").then(updatePin);
  void listen<TranslationEvent>("translation-state", ({ payload }) => render(payload)).then(() => emit("popup-ready"));
}

function render(event: TranslationEvent): void {
  if (event.requestId < currentRequestId) return;
  currentRequestId = event.requestId;
  const source = root.querySelector<HTMLElement>("#source")!;
  const content = root.querySelector<HTMLElement>("#content")!;
  const copy = root.querySelector<HTMLButtonElement>("#copy")!;
  const copySource = root.querySelector<HTMLButtonElement>("#copy-source")!;
  const retranslateButton = root.querySelector<HTMLButtonElement>("#retranslate")!;
  const meta = root.querySelector<HTMLElement>("#meta")!;
  const badge = root.querySelector<HTMLElement>("#badge")!;

  if (event.status === "loading") {
    localGeneration++;
    translationStale = false;
    stopReading();
    currentSourceKind = event.sourceKind || "selection";
    currentSource = event.sourceText || "";
    currentTranslation = "";
    source.textContent = currentSourceKind === "ocr" ? "OCR 识别文字与译文" : currentSource || "选中的文字";
    if (currentSourceKind === "ocr") {
      content.className = "content ocr-content";
      content.replaceChildren();
      if (currentSource) appendOcrSource(content, currentSource);
      const loading = document.createElement("div");
      loading.className = "inline-loading";
      loading.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>${currentSource ? "正在翻译识别文字…" : "正在识别图片文字…"}</p>`;
      content.append(loading);
    } else {
      content.className = "content loading";
      content.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>正在翻译…</p>`;
    }
    copy.disabled = true;
    copySource.disabled = !currentSource;
    retranslateButton.disabled = true;
    meta.textContent = currentSourceKind === "ocr" && !currentSource ? "正在运行 OCR" : "正在请求翻译服务";
    badge.hidden = true;
    refreshReadingButtons();
    return;
  }

  if (event.status === "error") {
    stopReading();
    currentTranslation = "";
    source.textContent = currentSource || "QuickTranslate";
    content.className = "content success";
    content.replaceChildren();
    if (currentSourceKind === "ocr" && currentSource) appendOcrSource(content, currentSource);
    const error = document.createElement("p");
    error.className = "error-message";
    error.textContent = event.error?.message || "翻译失败";
    content.append(error);
    copy.disabled = true;
    copySource.disabled = !currentSource;
    retranslateButton.disabled = !currentSource;
    meta.textContent = event.error?.code || "ERROR";
    badge.hidden = true;
    refreshReadingButtons();
    return;
  }

  if (event.result) {
    stopReading();
    currentSourceKind = event.sourceKind || currentSourceKind;
    renderResult(event.result, source, content, copy, meta, badge);
  }
}

function renderResult(
  result: TranslationResult,
  source: HTMLElement,
  content: HTMLElement,
  copy: HTMLButtonElement,
  meta: HTMLElement,
  badge: HTMLElement,
): void {
  currentSource = result.sourceText;
  currentTranslation = result.translation;
  translationStale = false;
  source.textContent = currentSourceKind === "ocr" ? "OCR 识别文字与译文" : result.sourceText;
  content.className = currentSourceKind === "ocr" ? "content success ocr-content" : "content success";
  content.replaceChildren();
  if (currentSourceKind === "ocr") appendOcrSource(content, result.sourceText);
  if (currentSourceKind === "ocr") {
    const label = document.createElement("p");
    label.className = "section-label";
    label.textContent = "译文";
    content.append(label);
  }
  const translation = document.createElement("div");
  translation.className = "translation";
  appendReadingSegments(translation, result.translation, "translation");
  content.append(translation);

  if (result.partOfSpeech || result.phonetic) {
    const detail = document.createElement("p");
    detail.className = "word-detail";
    detail.textContent = [result.partOfSpeech, result.phonetic].filter(Boolean).join(" · ");
    content.append(detail);
  }
  if (result.definitions?.length) {
    const list = document.createElement("ul");
    list.className = "definitions";
    for (const definition of result.definitions) {
      const item = document.createElement("li");
      item.textContent = definition;
      list.append(item);
    }
    content.append(list);
  }
  if (result.example) {
    const example = document.createElement("p");
    example.className = "example";
    example.textContent = result.example;
    content.append(example);
  }
  copy.disabled = false;
  root.querySelector<HTMLButtonElement>("#copy-source")!.disabled = false;
  root.querySelector<HTMLButtonElement>("#retranslate")!.disabled = false;
  meta.textContent = `${result.provider} · ${result.model}`;
  badge.textContent = result.cached ? "缓存" : "已翻译";
  badge.hidden = false;
  refreshReadingButtons();
}

function appendOcrSource(content: HTMLElement, text: string): void {
  const section = document.createElement("section");
  section.className = "recognized-section";
  const label = document.createElement("p");
  label.className = "section-label";
  label.textContent = "识别文字";
  const source = document.createElement("textarea");
  source.className = "recognized-text";
  source.value = text;
  source.rows = Math.min(6, Math.max(2, text.split("\n").length));
  source.setAttribute("aria-label", "OCR 识别文字，可编辑后重新翻译");
  source.addEventListener("input", () => {
    stopReading();
    currentSource = source.value;
    translationStale = true;
    const badge = root.querySelector<HTMLElement>("#badge")!;
    badge.textContent = "原文已修改，请重译"; badge.hidden = false;
    root.querySelector<HTMLButtonElement>("#copy-source")!.disabled = !currentSource.trim();
    root.querySelector<HTMLButtonElement>("#retranslate")!.disabled = !currentSource.trim();
    refreshReadingButtons();
  });
  section.append(label, source);
  content.append(section);
}

async function copyText(text: string, selector: string): Promise<void> {
  if (!text) return;
  const button = root.querySelector<HTMLButtonElement>(selector)!;
  const original = button.textContent || "复制";
  try {
    await invoke("copy_translation", { text });
    button.textContent = "已复制";
    window.setTimeout(() => (button.textContent = original), 1200);
  } catch {
    button.textContent = "失败";
    window.setTimeout(() => (button.textContent = original), 1200);
  }
}

async function retranslate(): Promise<void> {
  if (!currentSource) return;
  const sourceText = currentSource;
  const requestId = currentRequestId;
  render({ requestId, status: "loading", sourceText, sourceKind: currentSourceKind });
  const generation = localGeneration;
  try {
    const result = await invoke<TranslationResult>("retranslate_text", { text: sourceText });
    if (currentRequestId === requestId && localGeneration === generation) {
      render({ requestId, status: "success", result, sourceKind: currentSourceKind });
    }
  } catch (error) {
    if (currentRequestId === requestId && localGeneration === generation) {
      render({ requestId, status: "error", error: normalizeError(error) });
    }
  }
}

async function togglePin(): Promise<void> {
  try {
    updatePin(await invoke<boolean>("set_popup_pinned", { pinned: !pinned }));
  } catch {
    // Keep the previous visual state when persistence fails.
  }
}

function updatePin(value: boolean): void {
  pinned = value;
  const button = root.querySelector<HTMLButtonElement>("#pin")!;
  button.setAttribute("aria-pressed", String(pinned));
  button.title = pinned ? "取消固定" : "固定悬浮窗";
  button.setAttribute("aria-label", button.title);
}

function normalizeError(error: unknown): AppError {
  if (typeof error === "object" && error !== null && "message" in error) {
    return error as AppError;
  }
  return { code: "ERROR", message: typeof error === "string" ? error : "翻译失败" };
}

async function hide(): Promise<void> {
  stopReading();
  await invoke("hide_translation_window");
}

function refreshReadingButtons(): void {
  root.querySelector<HTMLButtonElement>("#read-source")!.disabled = !currentSource.trim();
  root.querySelector<HTMLButtonElement>("#read-translation")!.disabled = !currentTranslation.trim();
  root.querySelector<HTMLButtonElement>("#read-both")!.disabled = !currentSource.trim() || !currentTranslation.trim() || translationStale;
}
function stopReading(): void { readStart++; reader.stop(); }
async function startReading(side: ReadSide | "both"): Promise<void> {
  stopReading();
  const start = readStart;
  let preferences = defaultSpeech;
  try { preferences = (await invoke<SettingsView>("get_settings")).speech; }
  catch (error) { if (start === readStart) renderReading({ status: "error", message: normalizeError(error).message }); return; }
  if (start !== readStart) return;
  const source = splitSpeech(currentSource, "source");
  const translation = splitSpeech(currentTranslation, "translation");
  const both = !translationStale && (side === "both" || (side === "source" && preferences.bilingual && !!currentTranslation));
  const sequence = both ? [...source, ...translation] : side === "source" ? source : translation;
  void reader.read(sequence, preferences);
}
function appendReadingSegments(container: HTMLElement, text: string, side: ReadSide): void {
  for (const segment of splitSpeech(text, side)) {
    const paragraph = document.createElement("p");
    paragraph.className = "read-segment";
    paragraph.dataset.side = side; paragraph.dataset.index = String(segment.index);
    paragraph.textContent = segment.text;
    container.append(paragraph);
  }
}
function renderReading(state: ReaderState): void {
  const playback = root.querySelector<HTMLElement>("#playback");
  if (!playback) return;
  playback.hidden = state.status === "idle";
  root.querySelectorAll(".reading-active").forEach(element => element.classList.remove("reading-active"));
  const mirror = root.querySelector<HTMLElement>("#source-reading");
  const editor = root.querySelector<HTMLTextAreaElement>(".recognized-text");
  if (!state.segment || state.segment.side !== "source") {
    mirror?.remove(); if (editor) editor.hidden = false;
  }
  const status = root.querySelector<HTMLElement>("#reading-status")!;
  status.textContent = state.status === "error" ? state.message || "朗读失败" : `${state.status === "loading" ? "生成语音" : state.status === "paused" ? "已暂停" : "正在朗读"} · ${state.position}/${state.total}`;
  status.title = state.message || "";
  const pause = root.querySelector<HTMLButtonElement>("#pause-reading")!;
  pause.hidden = state.status === "error"; pause.textContent = state.status === "paused" ? "继续" : "暂停";
  if (state.segment?.side === "source" && !mirror) {
    const sourceReading = document.createElement("div");
    sourceReading.id = "source-reading"; sourceReading.className = "source-reading";
    appendReadingSegments(sourceReading, currentSource, "source");
    if (editor) { editor.hidden = true; editor.after(sourceReading); }
    else root.querySelector("#content")!.prepend(sourceReading);
  }
  if (state.segment) {
    const active = root.querySelector<HTMLElement>(`[data-side="${state.segment.side}"][data-index="${state.segment.index}"]`);
    active?.classList.add("reading-active");
    active?.scrollIntoView({ block: "nearest" });
  }
}
