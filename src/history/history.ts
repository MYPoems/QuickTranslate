import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppError, HistoryEntry } from "../types";
import "./history.css";

const root = document.querySelector<HTMLElement>("#app")!;
let searchTimer = 0;

export function mountHistory(): void {
  root.innerHTML = `
    <main class="history-shell">
      <header class="history-header">
        <div>
          <p class="eyebrow">QUICKTRANSLATE</p>
          <h1>翻译历史</h1>
          <p class="subtitle">最近 200 条记录仅保存在本机；收藏会固定显示在顶部。</p>
        </div>
        <button id="clear-history" class="danger-button" type="button">清空历史</button>
      </header>
      <section class="history-toolbar" aria-label="筛选历史">
        <input id="history-search" type="search" placeholder="搜索原文或译文" autocomplete="off" />
        <label class="favorite-filter"><input id="favorite-only" type="checkbox" />只看收藏</label>
      </section>
      <p id="history-status" class="history-status" role="status"></p>
      <section id="history-list" class="history-list"></section>
    </main>`;

  root.querySelector<HTMLInputElement>("#history-search")!.addEventListener("input", () => {
    window.clearTimeout(searchTimer);
    searchTimer = window.setTimeout(() => void loadHistory(), 180);
  });
  root.querySelector<HTMLInputElement>("#favorite-only")!.addEventListener("change", () => {
    void loadHistory();
  });
  root.querySelector<HTMLButtonElement>("#clear-history")!.addEventListener("click", () => {
    void clearHistory();
  });
  void getCurrentWindow().onFocusChanged(({ payload }) => {
    if (payload) void loadHistory();
  });
  void loadHistory();
}

async function loadHistory(): Promise<void> {
  const query = root.querySelector<HTMLInputElement>("#history-search")!.value.trim();
  const favoriteOnly = root.querySelector<HTMLInputElement>("#favorite-only")!.checked;
  setStatus("正在读取…");
  try {
    const entries = await invoke<HistoryEntry[]>("list_translation_history", {
      query,
      favoriteOnly,
    });
    renderEntries(entries);
    setStatus(entries.length ? `显示 ${entries.length} 条记录` : "没有符合条件的记录");
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

function renderEntries(entries: HistoryEntry[]): void {
  const list = root.querySelector<HTMLElement>("#history-list")!;
  list.replaceChildren();
  if (!entries.length) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "完成一次翻译后，记录会出现在这里。";
    list.append(empty);
    return;
  }

  for (const entry of entries) {
    const card = document.createElement("article");
    card.className = "history-card";
    const top = document.createElement("div");
    top.className = "card-top";
    const date = document.createElement("span");
    date.textContent = new Date(entry.createdAt * 1000).toLocaleString();
    const favorite = document.createElement("button");
    favorite.type = "button";
    favorite.className = "favorite-button";
    favorite.textContent = entry.favorite ? "★" : "☆";
    favorite.title = entry.favorite ? "取消收藏" : "收藏";
    favorite.setAttribute("aria-label", favorite.title);
    favorite.addEventListener("click", () => void toggleFavorite(entry));
    top.append(date, favorite);

    const source = document.createElement("p");
    source.className = "history-source";
    source.textContent = entry.sourceText;
    const translation = document.createElement("p");
    translation.className = "history-translation";
    translation.textContent = entry.translation;

    const footer = document.createElement("footer");
    const meta = document.createElement("span");
    meta.textContent = `${entry.sourceLanguage.toUpperCase()} → ${entry.targetLanguage.toUpperCase()} · ${entry.provider} · ${entry.model}`;
    const actions = document.createElement("div");
    actions.className = "card-actions";
    actions.append(
      actionButton("复制原文", () => copyText(entry.sourceText)),
      actionButton("复制译文", () => copyText(entry.translation)),
      actionButton("删除", () => deleteEntry(entry.id), true),
    );
    footer.append(meta, actions);
    card.append(top, source, translation, footer);
    list.append(card);
  }
}

function actionButton(label: string, action: () => void, danger = false): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  if (danger) button.classList.add("danger-text");
  button.addEventListener("click", action);
  return button;
}

async function toggleFavorite(entry: HistoryEntry): Promise<void> {
  try {
    await invoke("set_history_favorite", { id: entry.id, favorite: !entry.favorite });
    await loadHistory();
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

async function deleteEntry(id: number): Promise<void> {
  try {
    await invoke("delete_history_entry", { id });
    await loadHistory();
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

async function clearHistory(): Promise<void> {
  if (!window.confirm("确定清空全部翻译历史和缓存吗？此操作不可撤销。")) return;
  try {
    const removed = await invoke<number>("clear_translation_cache");
    await loadHistory();
    setStatus(`已清空 ${removed} 条记录`);
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

async function copyText(text: string): Promise<void> {
  try {
    await invoke("copy_translation", { text });
    setStatus("已复制");
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

function setStatus(message: string, error = false): void {
  const status = root.querySelector<HTMLElement>("#history-status")!;
  status.textContent = message;
  status.dataset.kind = error ? "error" : "neutral";
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as AppError).message);
  }
  return typeof error === "string" ? error : "操作失败";
}
