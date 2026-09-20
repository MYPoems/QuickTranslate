import { invoke } from "@tauri-apps/api/core";
import type { AppError, DiagnosticsView, SettingsView, UpdateSettings } from "../types";
import "./settings.css";

const root = document.querySelector<HTMLElement>("#app")!;

export function mountSettings(): void {
  root.innerHTML = `
    <section class="settings-shell">
      <header>
        <p class="eyebrow">QUICKTRANSLATE</p>
        <h1>设置</h1>
        <p class="subtitle">配置 OpenAI-compatible 翻译服务。API Key 仅保存到 Windows 凭据管理器。</p>
      </header>
      <form id="settings-form">
        <label>Provider<input name="provider" value="OpenAI Compatible" readonly /></label>
        <label>Base URL<input name="baseUrl" type="url" required placeholder="https://api.openai.com/v1" /></label>
        <label>Model<input name="model" required placeholder="gpt-4.1-mini" /></label>
        <label>API Key<input name="apiKey" type="password" autocomplete="off" placeholder="保持为空则不修改" /></label>
        <label>全局快捷键<input name="globalShortcut" required placeholder="Alt+Q" /></label>
        <div class="preference-card">
          <label class="checkbox-row"><input name="autoStartEnabled" type="checkbox" />开机自动启动</label>
          <p>登录 Windows 后在后台启动 QuickTranslate，不主动显示窗口。</p>
        </div>
        <div class="maintenance-card">
          <div>
            <strong>本地翻译缓存</strong>
            <p>最多保留 1000 条常用译文，可随时清理。</p>
          </div>
          <button id="clear-cache" type="button" class="secondary">清理缓存</button>
        </div>
        <details class="diagnostics-card">
          <summary>诊断信息（不包含 API Key）</summary>
          <pre id="diagnostics">展开后读取诊断信息</pre>
          <button id="copy-diagnostics" type="button" class="secondary">复制诊断信息</button>
        </details>
        <label class="checkbox-row"><input name="clearApiKey" type="checkbox" />删除已保存的 API Key</label>
        <p id="key-status" class="key-status"></p>
        <p id="status" class="status" role="status"></p>
        <div class="form-actions">
          <button id="test" type="button" class="secondary">测试连接</button>
          <button id="save" type="submit" class="primary">保存</button>
        </div>
      </form>
    </section>`;

  const form = root.querySelector<HTMLFormElement>("#settings-form")!;
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void save(form);
  });
  root.querySelector<HTMLButtonElement>("#test")!.addEventListener("click", () => void test(form));
  root
    .querySelector<HTMLButtonElement>("#clear-cache")!
    .addEventListener("click", () => void clearCache());
  root
    .querySelector<HTMLDetailsElement>(".diagnostics-card")!
    .addEventListener("toggle", (event) => {
      if ((event.currentTarget as HTMLDetailsElement).open) void loadDiagnostics();
    });
  root
    .querySelector<HTMLButtonElement>("#copy-diagnostics")!
    .addEventListener("click", () => void copyDiagnostics());
  void load(form);
}

async function load(form: HTMLFormElement): Promise<void> {
  setStatus("正在读取设置…", "neutral");
  try {
    const settings = await invoke<SettingsView>("get_settings");
    setInput(form, "provider", settings.provider);
    setInput(form, "baseUrl", settings.baseUrl);
    setInput(form, "model", settings.model);
    setInput(form, "globalShortcut", settings.globalShortcut);
    setCheckbox(form, "autoStartEnabled", settings.autoStartEnabled);
    updateKeyStatus(settings.apiKeyConfigured);
    setStatus("", "neutral");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  }
}

async function save(form: HTMLFormElement): Promise<void> {
  setBusy(true);
  setStatus("正在保存…", "neutral");
  try {
    const settings = await invoke<SettingsView>("save_settings", { update: formValue(form) });
    (form.elements.namedItem("apiKey") as HTMLInputElement).value = "";
    (form.elements.namedItem("clearApiKey") as HTMLInputElement).checked = false;
    setCheckbox(form, "autoStartEnabled", settings.autoStartEnabled);
    updateKeyStatus(settings.apiKeyConfigured);
    setStatus("设置已保存，快捷键和开机启动立即生效", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

async function test(form: HTMLFormElement): Promise<void> {
  setBusy(true);
  setStatus("正在测试连接…", "neutral");
  try {
    await invoke<string>("test_provider", { update: formValue(form) });
    setStatus("连接成功", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

async function clearCache(): Promise<void> {
  setBusy(true);
  setStatus("正在清理本地缓存…", "neutral");
  try {
    const removed = await invoke<number>("clear_translation_cache");
    setStatus(`已清理 ${removed} 条缓存`, "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

async function loadDiagnostics(): Promise<string> {
  const output = root.querySelector<HTMLElement>("#diagnostics")!;
  output.textContent = "正在读取…";
  try {
    const diagnostics = await invoke<DiagnosticsView>("get_diagnostics");
    const text = formatDiagnostics(diagnostics);
    output.textContent = text;
    return text;
  } catch (error) {
    const message = errorMessage(error);
    output.textContent = message;
    throw error;
  }
}

async function copyDiagnostics(): Promise<void> {
  setBusy(true);
  try {
    const text = await loadDiagnostics();
    await invoke("copy_translation", { text });
    setStatus("诊断信息已复制（不包含 API Key）", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

function formatDiagnostics(value: DiagnosticsView): string {
  const lines = [
    `QuickTranslate ${value.appVersion}`,
    `Provider: ${value.provider}`,
    `Base URL: ${value.baseUrl}`,
    `Model: ${value.model}`,
    `API Key configured: ${value.apiKeyConfigured ? "yes" : "no"}`,
    `Cache entries: ${value.cacheEntries}`,
    `Settings: ${value.settingsPath}`,
    `Cache: ${value.cachePath}`,
  ];
  if (value.lastError) lines.push(`Last error: ${value.lastError.code} - ${value.lastError.message}`);
  return lines.join("\n");
}

function formValue(form: HTMLFormElement): UpdateSettings {
  const data = new FormData(form);
  const apiKey = String(data.get("apiKey") || "").trim();
  return {
    provider: String(data.get("provider") || "OpenAI Compatible"),
    baseUrl: String(data.get("baseUrl") || "").trim(),
    model: String(data.get("model") || "").trim(),
    globalShortcut: String(data.get("globalShortcut") || "").trim(),
    apiKey: apiKey || undefined,
    clearApiKey: data.get("clearApiKey") === "on",
    autoStartEnabled: data.get("autoStartEnabled") === "on",
  };
}

function setInput(form: HTMLFormElement, name: string, value: string): void {
  (form.elements.namedItem(name) as HTMLInputElement).value = value;
}

function setCheckbox(form: HTMLFormElement, name: string, checked: boolean): void {
  (form.elements.namedItem(name) as HTMLInputElement).checked = checked;
}

function updateKeyStatus(configured: boolean): void {
  root.querySelector<HTMLElement>("#key-status")!.textContent = configured
    ? "已安全保存 API Key"
    : "尚未配置 API Key";
}

function setBusy(busy: boolean): void {
  root.querySelectorAll<HTMLButtonElement>("button").forEach((button) => (button.disabled = busy));
}

function setStatus(message: string, kind: "neutral" | "success" | "error"): void {
  const status = root.querySelector<HTMLElement>("#status")!;
  status.textContent = message;
  status.dataset.kind = kind;
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as AppError).message);
  }
  return typeof error === "string" ? error : "操作失败";
}
