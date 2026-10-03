import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppError, HistoryEntry } from "../types";
import { SpeechControls } from "../speech/controls";
import "./history.css";

const root = document.querySelector<HTMLElement>("#app")!;
const el = <T extends HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
let searchTimer = 0, revision = 0;
let entries: HistoryEntry[] = [], selectedId: number | undefined;
let mutating = false;
const speech = new SpeechControls("history", (message, error) => setStatus(message, error));
const text = (tag: string, value: string, className = "") => {
  const node = document.createElement(tag); node.textContent = value; node.className = className; return node;
};
const button = (label: string, action: () => void, className = "") => {
  const node = document.createElement("button"); node.type = "button"; node.textContent = label;
  node.className = className; node.addEventListener("click", action); return node;
};

export function mountHistory(): void {
  root.innerHTML = `<main class="history-shell">
    <header class="app-header"><div><p class="eyebrow">QUICKTRANSLATE</p><h1>翻译历史</h1></div><span class="header-note">仅保存在本机</span></header>
    <section class="history-workspace"><aside class="history-sidebar">
      <input id="history-search" type="search" placeholder="搜索原文或译文" aria-label="搜索历史" autocomplete="off" />
      <label class="favorite-filter"><input id="favorite-only" type="checkbox" />只看收藏</label>
      <div id="history-list" class="history-list" aria-label="历史记录"></div>
      <details class="history-maintenance"><summary>数据管理</summary><p>最近 200 条；收藏置顶。清空不会影响生词本。</p><button id="clear-history" class="danger-text" type="button">清空历史与缓存…</button></details>
    </aside><article id="history-detail" aria-label="翻译详情"></article></section>
    <p id="history-status" class="history-status" role="status"></p>
    <dialog id="history-confirm"><h2>请确认</h2><p id="history-confirm-text"></p><div><button id="history-confirm-no">取消</button><button id="history-confirm-yes">确认</button></div></dialog>
  </main>`;
  el<HTMLInputElement>("#history-search").addEventListener("input", () => {
    revision++; window.clearTimeout(searchTimer); speech.stop();
    searchTimer = window.setTimeout(() => void loadHistory(), 180);
  });
  el("#favorite-only").addEventListener("change", () => void loadHistory());
  el("#clear-history").addEventListener("click", () => void mutate(async () => {
    if (!await confirmAction("确定清空全部翻译历史和缓存吗？此操作不可撤销。生词本不受影响。")) return;
    speech.stop(); await invoke("clear_translation_cache"); selectedId = undefined;
  }));
  void listen("history-hidden", () => { speech.stop(); el<HTMLDialogElement>("#history-confirm").close(); });
  void getCurrentWindow().onFocusChanged(({ payload }) => { if (payload && !mutating) void loadHistory(); });
  void loadHistory();
}

async function loadHistory(): Promise<void> {
  const request = ++revision;
  setStatus("正在读取…");
  try {
    const next = await invoke<HistoryEntry[]>("list_translation_history", {
      query: el<HTMLInputElement>("#history-search").value.trim(), favoriteOnly: el<HTMLInputElement>("#favorite-only").checked,
    });
    if (request !== revision) return;
    entries = next;
    if (!entries.some(entry => entry.id === selectedId)) selectedId = entries[0]?.id;
    renderEntries(); renderDetail();
    setStatus(entries.length ? `显示 ${entries.length} 条记录` : "没有符合条件的记录");
  } catch (error) { if (request === revision) setStatus(errorMessage(error), true); }
}

function renderEntries(): void {
  const list = el("#history-list"); list.replaceChildren();
  for (const entry of entries) {
    const row = button("", () => { selectedId = entry.id; renderEntries(); renderDetail(); }, "history-row");
    row.setAttribute("aria-pressed", String(entry.id === selectedId)); row.dataset.id = String(entry.id);
    row.append(text("strong", `${entry.favorite ? "★ " : ""}${entry.sourceText}`), text("span", entry.translation), text("small", new Date(entry.createdAt * 1000).toLocaleString()));
    list.append(row);
  }
  if (!entries.length) list.append(text("p", "完成一次翻译后，记录会出现在这里。", "empty-state"));
}

function renderDetail(): void {
  speech.stop();
  const detail = el("#history-detail"); detail.replaceChildren();
  const entry = entries.find(value => value.id === selectedId);
  if (!entry) { detail.append(text("h2", "留住每一次理解。"), text("p", "选择左侧记录，查看原文和译文。", "empty-state")); return; }
  const content = text("div", "", "history-detail-content");
  for (const [label, value, side] of [["原文", entry.sourceText, "source"], ["译文", entry.translation, "translation"]]) {
    const section = text("section", "", `history-${side}`), heading = text("div", "", "section-heading");
    heading.append(text("h2", label), speech.button(`朗读${label}`, value, `history-read-${side}`));
    section.append(heading, text("p", value, "history-text")); content.append(section);
  }
  content.append(text("p", `${entry.sourceLanguage.toUpperCase()} → ${entry.targetLanguage.toUpperCase()} · ${entry.provider} · ${entry.model}`, "history-meta"));
  const footer = text("footer", "", "history-actions");
  footer.append(button("复制译文", () => void copyText(entry.translation), "primary-button"), button(entry.favorite ? "★ 已收藏" : "☆ 收藏", () => void mutate(async () => {
    await invoke("set_history_favorite", { id: entry.id, favorite: !entry.favorite });
  })));
  const more = document.createElement("details"); more.className = "detail-more";
  more.append(text("summary", "更多"), button("复制原文", () => void copyText(entry.sourceText)), button("在悬浮窗打开", () => void mutate(async () => {
    speech.stop(); await invoke("open_history_translation", { id: entry.id });
  })), button("删除记录…", () => void mutate(async () => {
    if (!await confirmAction("永久删除这条翻译记录及其缓存？生词本不受影响。")) return;
    speech.stop(); await invoke("delete_history_entry", { id: entry.id }); selectedId = undefined;
  }), "danger-text"));
  footer.append(more); detail.append(content, footer);
}

async function mutate(action: () => Promise<void>): Promise<void> {
  if (mutating) return; mutating = true; root.dataset.busy = "true";
  try { await action(); await loadHistory(); } catch (error) { setStatus(errorMessage(error), true); }
  finally { mutating = false; root.dataset.busy = "false"; }
}

function confirmAction(message: string): Promise<boolean> {
  const dialog = el<HTMLDialogElement>("#history-confirm"); el("#history-confirm-text").textContent = message; dialog.showModal();
  return new Promise(resolve => {
    const finish = (value: boolean) => { dialog.oncancel = null; dialog.onclose = null; el("#history-confirm-yes").onclick = null; el("#history-confirm-no").onclick = null; dialog.close(); resolve(value); };
    el("#history-confirm-yes").onclick = () => finish(true); el("#history-confirm-no").onclick = () => finish(false);
    dialog.oncancel = () => finish(false); dialog.onclose = () => finish(false);
  });
}

async function copyText(value: string): Promise<void> {
  try { await invoke("copy_translation", { text: value }); setStatus("已复制"); } catch (error) { setStatus(errorMessage(error), true); }
}
function setStatus(message: string, error = false): void { const status = el("#history-status"); if (!status) return; status.textContent = message; status.dataset.kind = error ? "error" : "neutral"; }
function errorMessage(error: unknown): string { return typeof error === "object" && error !== null && "message" in error ? String((error as AppError).message) : typeof error === "string" ? error : "操作失败"; }
