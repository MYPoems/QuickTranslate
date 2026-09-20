import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppError, TranslationEvent, TranslationResult } from "../types";
import "./popup.css";

const root = document.querySelector<HTMLElement>("#app")!;
let currentRequestId = 0;
let currentSource = "";
let currentTranslation = "";
let currentSourceKind: "selection" | "ocr" = "selection";
let pinned = false;

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
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void hide();
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "c" && currentSource) {
      event.preventDefault();
      void copyText(currentSource, "#copy-source");
    } else if (event.ctrlKey && event.key.toLowerCase() === "c" && currentTranslation) {
      event.preventDefault();
      void copyText(currentTranslation, "#copy");
    } else if (event.altKey && event.key.toLowerCase() === "r" && currentSource) {
      event.preventDefault();
      void retranslate();
    }
  });
  void invoke<boolean>("get_popup_pinned").then(updatePin);
  void listen<TranslationEvent>("translation-state", ({ payload }) => render(payload));
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
    currentSourceKind = event.sourceKind || "selection";
    currentSource = event.sourceText || "";
    currentTranslation = "";
    source.textContent = currentSourceKind === "ocr" ? "OCR 识别文字与译文" : currentSource || "选中的文字";
    if (currentSourceKind === "ocr") {
      content.className = "content ocr-content";
      content.replaceChildren();
      appendOcrSource(content, currentSource);
      const loading = document.createElement("div");
      loading.className = "inline-loading";
      loading.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>正在翻译识别文字…</p>`;
      content.append(loading);
    } else {
      content.className = "content loading";
      content.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>正在翻译…</p>`;
    }
    copy.disabled = true;
    copySource.disabled = !currentSource;
    retranslateButton.disabled = true;
    meta.textContent = "正在请求翻译服务";
    badge.hidden = true;
    return;
  }

  if (event.status === "error") {
    currentTranslation = "";
    source.textContent = currentSource || "QuickTranslate";
    content.className = "content error";
    content.innerHTML = `<p class="error-message"></p>`;
    content.querySelector("p")!.textContent = event.error?.message || "翻译失败";
    copy.disabled = true;
    copySource.disabled = !currentSource;
    retranslateButton.disabled = !currentSource;
    meta.textContent = event.error?.code || "ERROR";
    badge.hidden = true;
    return;
  }

  if (event.result) {
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
  const translation = document.createElement("p");
  translation.className = "translation";
  translation.textContent = result.translation;
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
}

function appendOcrSource(content: HTMLElement, text: string): void {
  const section = document.createElement("section");
  section.className = "recognized-section";
  const label = document.createElement("p");
  label.className = "section-label";
  label.textContent = "识别文字";
  const source = document.createElement("p");
  source.className = "recognized-text";
  source.textContent = text;
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
  try {
    const result = await invoke<TranslationResult>("retranslate_text", { text: sourceText });
    if (currentRequestId === requestId) {
      render({ requestId, status: "success", result, sourceKind: currentSourceKind });
    }
  } catch (error) {
    if (currentRequestId === requestId) {
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
  await invoke("hide_translation_window");
}
