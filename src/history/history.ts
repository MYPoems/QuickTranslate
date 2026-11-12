import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppError, HistoryEntry } from "../types";
import { SpeechControls } from "../speech/controls";
import "./history.css";

/** How many entries the backend returns per request. A full page means there may be more. */
const PAGE_SIZE = 100;

const root = document.querySelector<HTMLElement>("#app")!;
const el = <T extends HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
let searchTimer = 0;
// Counts list requests; a reply to anything but the latest one is dropped.
let revision = 0;
let entries: HistoryEntry[] = [];
let selectedId: number | undefined;
let hasMore = false;
let loadingMore = false;
let mutating = false;
const speech = new SpeechControls("history", (message, error) => setStatus(message, error));

const text = (tag: string, value: string, className = "") => {
  const node = document.createElement(tag);
  node.textContent = value;
  node.className = className;
  return node;
};
const button = (label: string, action: () => void, className = "") => {
  const node = document.createElement("button");
  node.type = "button";
  node.textContent = label;
  node.className = className;
  node.addEventListener("click", action);
  return node;
};

export function mountHistory(): void {
  root.innerHTML = `<main class="history-shell">
    <header class="app-header"><div><p class="eyebrow">QUICKTRANSLATE</p><h1>翻译历史</h1></div><span class="header-note">仅保存在本机</span></header>
    <section class="history-workspace"><aside class="history-sidebar">
      <input id="history-search" type="search" placeholder="搜索原文或译文" aria-label="搜索历史" autocomplete="off" />
      <label class="favorite-filter"><input id="favorite-only" type="checkbox" />只看收藏</label>
      <div id="history-list" class="history-list" aria-label="历史记录"></div>
      <details class="history-maintenance"><summary>数据管理</summary>
        <p>保留最近 5000 条，收藏不计入也不会被清理。清空历史会一并清空翻译缓存，不影响生词本。</p>
        <button id="clear-history" class="danger-text" type="button">清空历史（保留收藏）…</button>
        <button id="clear-history-all" class="danger-text" type="button">清空全部历史…</button>
      </details>
    </aside><article id="history-detail" aria-label="翻译详情"></article></section>
    <p id="history-status" class="history-status" role="status"></p>
    <dialog id="history-confirm"><h2>请确认</h2><p id="history-confirm-text"></p><div><button id="history-confirm-no">取消</button><button id="history-confirm-yes">确认</button></div></dialog>
  </main>`;
  el<HTMLInputElement>("#history-search").addEventListener("input", () => {
    revision++;
    window.clearTimeout(searchTimer);
    speech.stop();
    searchTimer = window.setTimeout(() => void loadHistory(), 180);
  });
  el("#favorite-only").addEventListener("change", () => void loadHistory());
  el("#clear-history").addEventListener("click", () => void clearHistory(true));
  el("#clear-history-all").addEventListener("click", () => void clearHistory(false));

  const list = el("#history-list");
  list.addEventListener("scroll", () => {
    if (list.scrollTop + list.clientHeight >= list.scrollHeight - 80) void loadMore();
  });
  // Arrow keys walk the list; F toggles the favourite; Delete removes after confirming.
  list.addEventListener("keydown", event => {
    const rows = Array.from(list.querySelectorAll<HTMLButtonElement>(".history-row"));
    const current = rows.findIndex(row => row.dataset.id === String(selectedId));
    if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
      if (!rows.length) return;
      const next = event.key === "Home" ? 0
        : event.key === "End" ? rows.length - 1
        : Math.min(rows.length - 1, Math.max(0, current + (event.key === "ArrowDown" ? 1 : -1)));
      select(Number(rows[next].dataset.id), true);
      event.preventDefault();
      return;
    }
    const entry = entries.find(value => value.id === selectedId);
    if (!entry || event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.key.toLowerCase() === "f") { event.preventDefault(); void toggleFavorite(entry); }
    else if (event.key === "Delete") { event.preventDefault(); void deleteEntry(entry); }
  });

  void listen("history-hidden", () => { speech.stop(); el<HTMLDialogElement>("#history-confirm").close(); });
  void getCurrentWindow().onFocusChanged(({ payload }) => { if (payload && !mutating) void loadHistory(); });
  void loadHistory();
}

function query() {
  return {
    query: el<HTMLInputElement>("#history-search").value.trim(),
    favoriteOnly: el<HTMLInputElement>("#favorite-only").checked,
  };
}

/** Loads the first page for the current search, replacing what is listed. */
async function loadHistory(): Promise<void> {
  const request = ++revision;
  setStatus("正在读取…");
  try {
    const next = await invoke<HistoryEntry[]>("list_translation_history", { ...query(), offset: 0 });
    if (request !== revision) return;
    entries = next ?? [];
    hasMore = entries.length >= PAGE_SIZE;
    if (!entries.some(entry => entry.id === selectedId)) selectedId = entries[0]?.id;
    renderEntries();
    renderDetail();
    showCount();
  } catch (error) {
    if (request === revision) setStatus(errorMessage(error), true);
  }
}

/** Appends the next page; runs when the list is scrolled to its end or on request. */
async function loadMore(): Promise<void> {
  if (!hasMore || loadingMore) return;
  loadingMore = true;
  const request = revision;
  try {
    const next = await invoke<HistoryEntry[]>("list_translation_history", { ...query(), offset: entries.length });
    if (request !== revision) return;
    const known = new Set(entries.map(entry => entry.id));
    const fresh = (next ?? []).filter(entry => !known.has(entry.id));
    entries = entries.concat(fresh);
    hasMore = (next ?? []).length >= PAGE_SIZE;
    renderEntries();
    showCount();
  } catch (error) {
    if (request === revision) setStatus(errorMessage(error), true);
  } finally {
    loadingMore = false;
  }
}

function showCount(): void {
  if (!entries.length) setStatus("没有符合条件的记录");
  else setStatus(hasMore ? `已显示 ${entries.length} 条，向下滚动加载更多` : `显示 ${entries.length} 条记录`);
}

/** When an entry was last used, in the words someone would use: 刚刚, 3 小时前, a date. */
export function relativeTime(seconds: number, now = Date.now()): string {
  const elapsed = Math.max(0, Math.floor(now / 1000) - seconds);
  if (elapsed < 60) return "刚刚";
  if (elapsed < 3600) return `${Math.floor(elapsed / 60)} 分钟前`;
  if (elapsed < 86400) return `${Math.floor(elapsed / 3600)} 小时前`;
  const date = new Date(seconds * 1000);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return sameYear ? `${date.getMonth() + 1} 月 ${date.getDate()} 日` : `${date.getFullYear()} 年 ${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

/** Which heading an entry is listed under. Favourites come first whatever their date. */
export function groupOf(entry: Pick<HistoryEntry, "favorite" | "usedAt" | "createdAt">, now = Date.now()): string {
  if (entry.favorite) return "收藏";
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const used = (entry.usedAt ?? entry.createdAt) * 1000;
  if (used >= startOfToday.getTime()) return "今天";
  if (used >= startOfToday.getTime() - 86_400_000) return "昨天";
  return "更早";
}

function renderEntries(): void {
  const list = el("#history-list");
  const hadFocus = list.contains(document.activeElement);
  list.replaceChildren();
  let group = "";
  for (const entry of entries) {
    const heading = groupOf(entry);
    if (heading !== group) {
      group = heading;
      list.append(text("h2", heading, "history-group"));
    }
    const selected = entry.id === selectedId;
    const row = button("", () => select(entry.id), "history-row");
    row.setAttribute("aria-pressed", String(selected));
    row.dataset.id = String(entry.id);
    // One stop in the tab order; the arrow keys move within the list.
    row.tabIndex = selected ? 0 : -1;
    row.append(
      text("strong", `${entry.favorite ? "★ " : ""}${entry.sourceText}`),
      text("span", entry.translation),
      text("small", relativeTime(entry.usedAt ?? entry.createdAt)),
    );
    list.append(row);
  }
  if (!entries.length) list.append(text("p", "完成一次翻译后，记录会出现在这里。", "empty-state"));
  if (hasMore) list.append(button("加载更多", () => void loadMore(), "history-more"));
  if (hadFocus) list.querySelector<HTMLButtonElement>('.history-row[aria-pressed="true"]')?.focus();
}

function select(id: number, focus = false): void {
  selectedId = id;
  renderEntries();
  renderDetail();
  if (focus) {
    const row = el("#history-list").querySelector<HTMLButtonElement>('.history-row[aria-pressed="true"]');
    row?.focus();
    row?.scrollIntoView({ block: "nearest" });
  }
}

function renderDetail(): void {
  speech.stop();
  const detail = el("#history-detail");
  detail.replaceChildren();
  const entry = entries.find(value => value.id === selectedId);
  if (!entry) {
    detail.append(text("h2", "留住每一次理解。"), text("p", "选择左侧记录，查看原文和译文。", "empty-state"));
    return;
  }
  const content = text("div", "", "history-detail-content");
  for (const [label, value, side] of [["原文", entry.sourceText, "source"], ["译文", entry.translation, "translation"]]) {
    const section = text("section", "", `history-${side}`);
    const heading = text("div", "", "section-heading");
    heading.append(text("h2", label), speech.button(`朗读${label}`, value, `history-read-${side}`));
    section.append(heading, text("p", value, "history-text"));
    content.append(section);
  }
  content.append(text("p", `${entry.sourceLanguage.toUpperCase()} → ${entry.targetLanguage.toUpperCase()} · ${entry.provider} · ${entry.model}`, "history-meta"));

  const footer = text("footer", "", "history-actions");
  footer.append(
    button("复制译文", () => void copyText(entry.translation), "primary-button"),
    button(entry.favorite ? "★ 已收藏" : "☆ 收藏", () => void toggleFavorite(entry)),
  );
  const more = document.createElement("details");
  more.className = "detail-more";
  more.append(
    text("summary", "更多"),
    button("复制原文", () => void copyText(entry.sourceText)),
    button("在悬浮窗打开", () => void mutate(async () => {
      speech.stop();
      await invoke("open_history_translation", { id: entry.id });
    })),
    button("删除记录…", () => void deleteEntry(entry), "danger-text"),
  );
  footer.append(more);
  detail.append(content, footer);
}

function toggleFavorite(entry: HistoryEntry): Promise<void> {
  return mutate(async () => {
    await invoke("set_history_favorite", { id: entry.id, favorite: !entry.favorite });
  });
}

function deleteEntry(entry: HistoryEntry): Promise<void> {
  return mutate(async () => {
    if (!await confirmAction("永久删除这条翻译记录及其缓存？生词本不受影响。")) return;
    speech.stop();
    await invoke("delete_history_entry", { id: entry.id });
    selectedId = undefined;
  });
}

function clearHistory(keepFavorites: boolean): Promise<void> {
  return mutate(async () => {
    const question = keepFavorites
      ? "确定清空翻译历史吗？已收藏的条目会保留，翻译缓存会一并清空。此操作不可撤销，生词本不受影响。"
      : "确定清空全部翻译历史吗？已收藏的条目也会删除，翻译缓存会一并清空。此操作不可撤销，生词本不受影响。";
    if (!await confirmAction(question)) return;
    speech.stop();
    await invoke("clear_translation_history", { keepFavorites });
    selectedId = undefined;
  });
}

/** Runs a change, then reloads the list; ignores further changes until that is done. */
async function mutate(action: () => Promise<void>): Promise<void> {
  if (mutating) return;
  mutating = true;
  root.dataset.busy = "true";
  try {
    await action();
    await loadHistory();
  } catch (error) {
    setStatus(errorMessage(error), true);
  } finally {
    mutating = false;
    root.dataset.busy = "false";
  }
}

function confirmAction(message: string): Promise<boolean> {
  const dialog = el<HTMLDialogElement>("#history-confirm");
  el("#history-confirm-text").textContent = message;
  dialog.showModal();
  return new Promise(resolve => {
    const finish = (value: boolean) => {
      dialog.oncancel = null;
      dialog.onclose = null;
      el("#history-confirm-yes").onclick = null;
      el("#history-confirm-no").onclick = null;
      dialog.close();
      resolve(value);
    };
    el("#history-confirm-yes").onclick = () => finish(true);
    el("#history-confirm-no").onclick = () => finish(false);
    dialog.oncancel = () => finish(false);
    dialog.onclose = () => finish(false);
  });
}

async function copyText(value: string): Promise<void> {
  try {
    await invoke("copy_translation", { text: value });
    setStatus("已复制");
  } catch (error) {
    setStatus(errorMessage(error), true);
  }
}

function setStatus(message: string, error = false): void {
  const status = el("#history-status");
  if (!status) return;
  status.textContent = message;
  status.dataset.kind = error ? "error" : "neutral";
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) return String((error as AppError).message);
  return typeof error === "string" ? error : "操作失败";
}
