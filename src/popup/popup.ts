import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { TranslationEvent, TranslationResult } from "../types";
import "./popup.css";

const root = document.querySelector<HTMLElement>("#app")!;
let currentRequestId = 0;
let currentTranslation = "";

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
          <button id="copy" class="text-button" type="button" disabled>复制</button>
          <button id="close" class="icon-button" type="button" aria-label="关闭">×</button>
        </div>
      </footer>
    </section>`;

  root.querySelector<HTMLButtonElement>("#copy")!.addEventListener("click", () => void copyResult());
  root.querySelector<HTMLButtonElement>("#close")!.addEventListener("click", () => void hide());
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void hide();
  });
  void listen<TranslationEvent>("translation-state", ({ payload }) => render(payload));
}

function render(event: TranslationEvent): void {
  if (event.requestId < currentRequestId) return;
  currentRequestId = event.requestId;
  const source = root.querySelector<HTMLElement>("#source")!;
  const content = root.querySelector<HTMLElement>("#content")!;
  const copy = root.querySelector<HTMLButtonElement>("#copy")!;
  const meta = root.querySelector<HTMLElement>("#meta")!;
  const badge = root.querySelector<HTMLElement>("#badge")!;

  if (event.status === "loading") {
    currentTranslation = "";
    source.textContent = event.sourceText || "选中的文字";
    content.className = "content loading";
    content.innerHTML = `<div class="spinner" aria-hidden="true"></div><p>正在翻译…</p>`;
    copy.disabled = true;
    meta.textContent = "正在请求翻译服务";
    badge.hidden = true;
    return;
  }

  if (event.status === "error") {
    currentTranslation = "";
    source.textContent = "QuickTranslate";
    content.className = "content error";
    content.innerHTML = `<p class="error-message"></p>`;
    content.querySelector("p")!.textContent = event.error?.message || "翻译失败";
    copy.disabled = true;
    meta.textContent = event.error?.code || "ERROR";
    badge.hidden = true;
    return;
  }

  if (event.result) renderResult(event.result, source, content, copy, meta, badge);
}

function renderResult(
  result: TranslationResult,
  source: HTMLElement,
  content: HTMLElement,
  copy: HTMLButtonElement,
  meta: HTMLElement,
  badge: HTMLElement,
): void {
  currentTranslation = result.translation;
  source.textContent = result.sourceText;
  content.className = "content success";
  content.replaceChildren();
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
  meta.textContent = `${result.provider} · ${result.model}`;
  badge.textContent = result.cached ? "缓存" : "已翻译";
  badge.hidden = false;
}

async function copyResult(): Promise<void> {
  if (!currentTranslation) return;
  const button = root.querySelector<HTMLButtonElement>("#copy")!;
  try {
    await invoke("copy_translation", { text: currentTranslation });
    button.textContent = "已复制";
    window.setTimeout(() => (button.textContent = "复制"), 1200);
  } catch {
    button.textContent = "复制失败";
  }
}

async function hide(): Promise<void> {
  await invoke("hide_translation_window");
}
