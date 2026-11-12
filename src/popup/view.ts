import { splitSpeech, type ReadSide } from "../speech/reader";
import { speakerMarkup } from "../ui/icons";
import type { AppError, TranslationResult } from "../types";

/** What the popup offers next to an error message. */
export type ErrorAction = "open-settings" | "retry" | "manual-input" | "reselect";

const actionLabels: Record<ErrorAction, string> = {
  "open-settings": "打开设置",
  retry: "重试",
  "manual-input": "手动输入",
  reselect: "重新框选",
};

/**
 * The next step for each kind of failure, most useful first. `hasSource` says whether there
 * is text to try again with.
 */
export function errorActions(code: string | undefined, hasSource: boolean): ErrorAction[] {
  switch (code) {
    case "PROVIDER_NOT_CONFIGURED":
    case "INVALID_API_KEY":
      return ["open-settings"];
    case "NO_SELECTED_TEXT":
    case "CLIPBOARD_ERROR":
      return ["manual-input"];
    case "OCR_NO_TEXT":
      return ["reselect"];
    case "OCR_ERROR":
      return ["reselect", "open-settings"];
    case "TEXT_TOO_LONG":
      return [];
    case "NETWORK_ERROR":
      return hasSource ? ["retry"] : [];
    case "PROVIDER_ERROR":
      return hasSource ? ["retry", "open-settings"] : ["open-settings"];
    default:
      return hasSource ? ["retry"] : [];
  }
}

/** A line of advice under the message, for failures the message alone does not explain. */
export function errorHint(code: string | undefined, shortcut: string): string {
  switch (code) {
    case "NO_SELECTED_TEXT":
      return `先在其他程序里选中文字，再按 ${shortcut}。有些程序不支持复制选中的文字，可以手动输入。`;
    case "CLIPBOARD_ERROR":
      return `按下 ${shortcut} 后请尽快松开；也可以手动输入要翻译的文字。`;
    case "PROVIDER_NOT_CONFIGURED":
      return "翻译服务还没有可用的 API Key。";
    case "INVALID_API_KEY":
      return "服务拒绝了当前的 API Key，请在设置里检查。";
    case "NETWORK_ERROR":
      return "没有连上翻译服务，请检查网络或服务地址。";
    default:
      return "";
  }
}

export function buildError(error: AppError | undefined, hasSource: boolean, shortcut: string): HTMLElement {
  const box = document.createElement("div");
  box.className = "error-box";
  const message = document.createElement("p");
  message.className = "error-message";
  message.textContent = error?.message || "翻译失败";
  box.append(message);
  const hint = errorHint(error?.code, shortcut);
  if (hint) {
    const advice = document.createElement("p");
    advice.className = "error-hint";
    advice.textContent = hint;
    box.append(advice);
  }
  const actions = errorActions(error?.code, hasSource);
  if (actions.length) {
    const row = document.createElement("div");
    row.className = "error-actions";
    actions.forEach((action, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = index === 0 ? "text-button primary-button" : "text-button";
      button.dataset.action = action;
      button.textContent = actionLabels[action];
      row.append(button);
    });
    box.append(row);
  }
  return box;
}

/** A label with a read-aloud button, as above the source and above the translation. */
export function buildHeading(label: string, speakLabel: string, id: string, side: ReadSide): HTMLElement {
  const heading = document.createElement("div");
  heading.className = "section-heading";
  heading.innerHTML = `<p class="section-label"></p>${speakerMarkup(speakLabel, id)}`;
  heading.querySelector(".section-label")!.textContent = label;
  heading.querySelector<HTMLButtonElement>("button")!.dataset.readSide = side;
  return heading;
}

/** Text split into the paragraphs the reader highlights one at a time. */
export function appendSegments(container: HTMLElement, text: string, side: ReadSide): void {
  for (const segment of splitSpeech(text, side)) {
    const paragraph = document.createElement("p");
    paragraph.className = "read-segment";
    paragraph.dataset.side = side;
    paragraph.dataset.index = String(segment.index);
    paragraph.textContent = segment.text;
    container.append(paragraph);
  }
}

/** Whether a source text is long enough to push the translation out of view. */
export function isLongSource(text: string): boolean {
  return text.length > 180 || text.split("\n").length > 3;
}

/**
 * The source of a selection: read-only text. A long one starts collapsed to a few lines so
 * that the translation, which is what the popup is for, stays in view.
 */
export function buildSelectionSource(text: string): HTMLElement {
  const section = document.createElement("section");
  section.className = "source-section";
  section.append(buildHeading("原文", "朗读原文", "read-source", "source"));
  const body = document.createElement("div");
  body.className = "original-text";
  appendSegments(body, text, "source");
  section.append(body);
  if (isLongSource(text)) {
    body.dataset.collapsed = "true";
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.id = "toggle-source";
    toggle.className = "text-button inline-retry";
    toggle.setAttribute("aria-expanded", "false");
    toggle.textContent = "展开原文";
    toggle.addEventListener("click", () => {
      const collapsed = body.dataset.collapsed !== "true";
      body.dataset.collapsed = String(collapsed);
      toggle.setAttribute("aria-expanded", String(!collapsed));
      toggle.textContent = collapsed ? "展开原文" : "收起原文";
    });
    section.append(toggle);
  }
  return section;
}

/** A spinner with what is going on and a way to stop it. */
export function buildLoading(text: string): HTMLElement {
  const loading = document.createElement("div");
  loading.className = "inline-loading";
  loading.innerHTML = '<div class="spinner" aria-hidden="true"></div><p></p><button type="button" class="text-button inline-retry" data-action="cancel-request">取消</button>';
  loading.querySelector("p")!.textContent = text;
  return loading;
}

/** The translation with, for a single word, its dictionary details. */
export function appendResult(content: HTMLElement, result: TranslationResult): void {
  const translation = document.createElement("div");
  translation.className = "translation";
  appendSegments(translation, result.translation, "translation");
  content.append(translation);
  const details: Array<[string, string | undefined]> = [
    ["word-detail", [result.partOfSpeech, result.phonetic].filter(Boolean).join(" · ")],
    ["example", result.example],
  ];
  for (const [className, value] of details) {
    if (!value) continue;
    const paragraph = document.createElement("p");
    paragraph.className = className;
    paragraph.textContent = value;
    content.append(paragraph);
  }
  if (result.definitions?.length) {
    const list = document.createElement("ul");
    list.className = "definitions";
    for (const value of result.definitions) {
      const item = document.createElement("li");
      item.textContent = value;
      list.append(item);
    }
    content.append(list);
  }
}

/** The box for typing or pasting text to translate, used when nothing can be selected. */
export function buildManualInput(initial: string): HTMLElement {
  const form = document.createElement("form");
  form.id = "manual-form";
  form.className = "manual-form";
  form.innerHTML = `<label for="manual-text" class="section-label">输入或粘贴要翻译的文字</label>
    <textarea id="manual-text" rows="4" maxlength="5000" spellcheck="false"></textarea>
    <div class="error-actions">
      <button type="submit" class="text-button primary-button" id="manual-submit">翻译 <small>Ctrl+Enter</small></button>
      <button type="button" class="text-button" data-action="cancel-input">取消</button>
    </div>`;
  form.querySelector<HTMLTextAreaElement>("#manual-text")!.value = initial;
  return form;
}
