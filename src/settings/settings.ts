import { invoke } from "@tauri-apps/api/core";
import type {
  AppError,
  DiagnosticsView,
  PaddleOcrPluginStatus,
  SettingsBackup,
  SettingsView,
  UpdateInfo,
  UpdateSettings,
} from "../types";
import "./settings.css";

const root = document.querySelector<HTMLElement>("#app")!;
let apiKeyConfigured = false;
let cloudOcrApiKeyConfigured = false;
let paddleOcrInstalled = false;
const cloudOcrApiKeyUrl = "https://bailian.console.aliyun.com/?tab=model#/api-key";
const providerPresets: Record<string, { baseUrl: string; model: string }> = {
  "OpenAI Compatible": { baseUrl: "https://api.openai.com/v1", model: "gpt-4.1-mini" },
  "阿里云百炼": {
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-turbo",
  },
  DeepSeek: { baseUrl: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  Ollama: { baseUrl: "http://localhost:11434/v1", model: "qwen2.5:3b" },
  "LM Studio": { baseUrl: "http://localhost:1234/v1", model: "local-model" },
};

export function mountSettings(): void {
  root.innerHTML = `
    <section class="settings-shell">
      <header>
        <p class="eyebrow">QUICKTRANSLATE</p>
        <h1>设置</h1>
        <p class="subtitle">配置 OpenAI-compatible 翻译服务。API Key 仅保存到 Windows 凭据管理器。</p>
      </header>
      <form id="settings-form">
        <label>Provider
          <select name="provider">
            <option>OpenAI Compatible</option>
            <option>阿里云百炼</option>
            <option>DeepSeek</option>
            <option>Ollama</option>
            <option>LM Studio</option>
            <option>自定义</option>
          </select>
        </label>
        <label>Base URL<input name="baseUrl" type="url" required placeholder="https://api.openai.com/v1" /></label>
        <label>Model<input name="model" required placeholder="gpt-4.1-mini" /></label>
        <label>API Key<input name="apiKey" type="password" autocomplete="off" placeholder="保持为空则不修改；本地模型可留空" /></label>
        <label>全局快捷键<input name="globalShortcut" required placeholder="Alt+Q" /></label>
        <label>OCR 截图翻译快捷键<input name="ocrShortcut" required placeholder="Alt+W" /></label>
        <section class="ocr-card">
          <div>
            <strong>OCR 识别引擎</strong>
            <p>Windows OCR 默认可用；高精度本地插件和云端视觉模型均为可选配置。</p>
          </div>
          <label>识别方案
            <select name="ocrEngine">
              <option value="windows">Windows OCR（内置，默认）</option>
              <option value="paddle">PP-OCRv6 Small（本地高精度插件）</option>
              <option value="cloud">云端视觉 OCR（自备 API Key）</option>
            </select>
          </label>
          <div id="windows-ocr-options" class="ocr-options">
            <label>Windows OCR 语言
              <select name="ocrLanguage">
                <option value="auto">自动（跟随 Windows 语言）</option>
                <option value="chinese">简体中文 + 英文</option>
                <option value="english">英文</option>
              </select>
            </label>
            <p>应用会自动放大小字，并对原图与增强图进行双路识别。</p>
          </div>
          <div id="paddle-ocr-options" class="ocr-options" hidden>
            <div class="plugin-row">
              <div>
                <strong>PP-OCRv6 Small 插件</strong>
                <p id="paddle-plugin-status">正在读取插件状态…</p>
              </div>
              <button id="toggle-paddle-plugin" type="button" class="secondary">安装插件</button>
            </div>
            <p>模型在本机运行，截图不会上传；首次初始化可能需要数秒。</p>
          </div>
          <div id="cloud-ocr-options" class="ocr-options" hidden>
            <p class="privacy-warning">云端模式会把所选截图发送给你配置的服务商，仅在你主动选择该模式后启用。</p>
            <label>Cloud OCR Base URL<input name="cloudOcrBaseUrl" type="url" placeholder="https://dashscope.aliyuncs.com/compatible-mode/v1" /></label>
            <label>Cloud OCR Model<input name="cloudOcrModel" placeholder="qwen3.5-ocr" /></label>
            <label>Cloud OCR API Key<input name="cloudOcrApiKey" type="password" autocomplete="off" placeholder="保持为空则不修改" /></label>
            <p id="cloud-key-status" class="key-status"></p>
            <label class="checkbox-row"><input name="clearCloudOcrApiKey" type="checkbox" />删除已保存的云端 OCR API Key</label>
            <div class="cloud-help">
              <span>推荐：阿里云百炼 qwen3.5-ocr</span>
              <button id="copy-cloud-key-url" type="button" class="secondary">复制 API 申请网址</button>
            </div>
          </div>
        </section>
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
        <div class="maintenance-card">
          <div>
            <strong>应用更新</strong>
            <p id="update-status">从 GitHub Releases 检查正式版本。</p>
          </div>
          <button id="check-update" type="button" class="secondary">检查更新</button>
        </div>
        <details class="backup-card">
          <summary>设置备份与恢复（不包含 API Key）</summary>
          <textarea id="settings-backup" rows="8" spellcheck="false" placeholder="导出的 JSON 会显示在这里；也可粘贴备份后载入表单。"></textarea>
          <div class="backup-actions">
            <button id="export-settings" type="button" class="secondary">导出并复制</button>
            <button id="import-settings" type="button" class="secondary">载入到表单</button>
          </div>
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
  (form.elements.namedItem("provider") as HTMLSelectElement).addEventListener("change", () =>
    applyProviderPreset(form),
  );
  (form.elements.namedItem("baseUrl") as HTMLInputElement).addEventListener("input", () =>
    refreshKeyStatus(form),
  );
  (form.elements.namedItem("ocrEngine") as HTMLSelectElement).addEventListener("change", () =>
    refreshOcrPanels(form),
  );
  root
    .querySelector<HTMLButtonElement>("#toggle-paddle-plugin")!
    .addEventListener("click", () => void togglePaddlePlugin());
  root
    .querySelector<HTMLButtonElement>("#copy-cloud-key-url")!
    .addEventListener("click", () => void copyCloudKeyUrl());
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
  root
    .querySelector<HTMLButtonElement>("#check-update")!
    .addEventListener("click", () => void checkUpdates());
  root
    .querySelector<HTMLButtonElement>("#export-settings")!
    .addEventListener("click", () => void exportSettings());
  root
    .querySelector<HTMLButtonElement>("#import-settings")!
    .addEventListener("click", () => void importSettings(form));
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
    setInput(form, "ocrShortcut", settings.ocrShortcut);
    setInput(form, "ocrEngine", settings.ocrEngine);
    setInput(form, "ocrLanguage", settings.ocrLanguage);
    setInput(form, "cloudOcrBaseUrl", settings.cloudOcrBaseUrl);
    setInput(form, "cloudOcrModel", settings.cloudOcrModel);
    setCheckbox(form, "autoStartEnabled", settings.autoStartEnabled);
    apiKeyConfigured = settings.apiKeyConfigured;
    cloudOcrApiKeyConfigured = settings.cloudOcrApiKeyConfigured;
    paddleOcrInstalled = settings.paddleOcrInstalled;
    updateKeyStatus(settings.apiKeyConfigured, isLocalBaseUrl(settings.baseUrl));
    updateCloudKeyStatus();
    updatePaddlePluginStatus({
      installed: settings.paddleOcrInstalled,
      version: "PP-OCRv6 Small",
      installedBytes: settings.paddleOcrInstalled ? 31_211_520 : 0,
      downloadBytes: 31_211_520,
    });
    refreshOcrPanels(form);
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
    (form.elements.namedItem("cloudOcrApiKey") as HTMLInputElement).value = "";
    (form.elements.namedItem("clearCloudOcrApiKey") as HTMLInputElement).checked = false;
    setCheckbox(form, "autoStartEnabled", settings.autoStartEnabled);
    apiKeyConfigured = settings.apiKeyConfigured;
    cloudOcrApiKeyConfigured = settings.cloudOcrApiKeyConfigured;
    paddleOcrInstalled = settings.paddleOcrInstalled;
    updateKeyStatus(settings.apiKeyConfigured, isLocalBaseUrl(settings.baseUrl));
    updateCloudKeyStatus();
    refreshOcrPanels(form);
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

async function checkUpdates(): Promise<void> {
  const output = root.querySelector<HTMLElement>("#update-status")!;
  setBusy(true);
  output.textContent = "正在检查 GitHub Releases…";
  try {
    const update = await invoke<UpdateInfo>("check_for_updates");
    if (update.updateAvailable) {
      output.textContent = `发现 v${update.latestVersion}，下载页已复制。`;
      await invoke("copy_translation", { text: update.releaseUrl });
    } else {
      output.textContent = `当前 v${update.currentVersion} 已是最新正式版。`;
    }
  } catch (error) {
    output.textContent = errorMessage(error);
  } finally {
    setBusy(false);
  }
}

async function exportSettings(): Promise<void> {
  setBusy(true);
  try {
    const backup = await invoke<string>("export_settings_backup");
    root.querySelector<HTMLTextAreaElement>("#settings-backup")!.value = backup;
    await invoke("copy_translation", { text: backup });
    setStatus("设置备份已复制（不包含 API Key）", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

async function importSettings(form: HTMLFormElement): Promise<void> {
  const contents = root.querySelector<HTMLTextAreaElement>("#settings-backup")!.value.trim();
  if (!contents) {
    setStatus("请先粘贴设置备份 JSON", "error");
    return;
  }
  setBusy(true);
  try {
    const backup = await invoke<SettingsBackup>("import_settings_backup", { contents });
    setInput(form, "provider", backup.provider);
    setInput(form, "baseUrl", backup.baseUrl);
    setInput(form, "model", backup.model);
    setInput(form, "globalShortcut", backup.globalShortcut);
    setInput(form, "ocrShortcut", backup.ocrShortcut);
    setInput(form, "ocrEngine", backup.ocrEngine);
    setInput(form, "ocrLanguage", backup.ocrLanguage);
    setInput(form, "cloudOcrBaseUrl", backup.cloudOcrBaseUrl);
    setInput(form, "cloudOcrModel", backup.cloudOcrModel);
    setCheckbox(form, "autoStartEnabled", backup.autoStartEnabled);
    refreshKeyStatus(form);
    refreshOcrPanels(form);
    setStatus("备份已载入表单，请确认后点击保存", "success");
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
    `OCR engine: ${value.ocrEngine}`,
    `OCR language: ${value.ocrLanguage}`,
    `PaddleOCR installed: ${value.paddleOcrInstalled ? "yes" : "no"}`,
    `Cloud OCR model: ${value.cloudOcrModel}`,
    `Cloud OCR API Key configured: ${value.cloudOcrApiKeyConfigured ? "yes" : "no"}`,
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
  const cloudOcrApiKey = String(data.get("cloudOcrApiKey") || "").trim();
  return {
    provider: String(data.get("provider") || "OpenAI Compatible"),
    baseUrl: String(data.get("baseUrl") || "").trim(),
    model: String(data.get("model") || "").trim(),
    globalShortcut: String(data.get("globalShortcut") || "").trim(),
    ocrShortcut: String(data.get("ocrShortcut") || "").trim(),
    ocrEngine: String(data.get("ocrEngine") || "windows") as UpdateSettings["ocrEngine"],
    ocrLanguage: String(data.get("ocrLanguage") || "auto") as UpdateSettings["ocrLanguage"],
    cloudOcrBaseUrl: String(data.get("cloudOcrBaseUrl") || "").trim(),
    cloudOcrModel: String(data.get("cloudOcrModel") || "").trim(),
    apiKey: apiKey || undefined,
    clearApiKey: data.get("clearApiKey") === "on",
    cloudOcrApiKey: cloudOcrApiKey || undefined,
    clearCloudOcrApiKey: data.get("clearCloudOcrApiKey") === "on",
    autoStartEnabled: data.get("autoStartEnabled") === "on",
  };
}

function setInput(form: HTMLFormElement, name: string, value: string): void {
  (form.elements.namedItem(name) as HTMLInputElement | HTMLSelectElement).value = value;
}

function setCheckbox(form: HTMLFormElement, name: string, checked: boolean): void {
  (form.elements.namedItem(name) as HTMLInputElement).checked = checked;
}

function updateKeyStatus(configured: boolean, localProvider: boolean): void {
  root.querySelector<HTMLElement>("#key-status")!.textContent = configured
    ? "已安全保存 API Key"
    : localProvider
      ? "本地 Provider 无需 API Key"
      : "尚未配置 API Key";
}

function applyProviderPreset(form: HTMLFormElement): void {
  const provider = (form.elements.namedItem("provider") as HTMLSelectElement).value;
  const preset = providerPresets[provider];
  if (preset) {
    setInput(form, "baseUrl", preset.baseUrl);
    setInput(form, "model", preset.model);
  }
  refreshKeyStatus(form);
}

function refreshKeyStatus(form: HTMLFormElement): void {
  const baseUrl = (form.elements.namedItem("baseUrl") as HTMLInputElement).value;
  updateKeyStatus(apiKeyConfigured, isLocalBaseUrl(baseUrl));
}

function isLocalBaseUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.replace(/^\[|\]$/g, "");
    return host === "localhost" || host === "127.0.0.1" || host === "::1";
  } catch {
    return false;
  }
}

function refreshOcrPanels(form: HTMLFormElement): void {
  const engine = (form.elements.namedItem("ocrEngine") as HTMLSelectElement).value;
  root.querySelector<HTMLElement>("#windows-ocr-options")!.hidden = engine !== "windows";
  root.querySelector<HTMLElement>("#paddle-ocr-options")!.hidden = engine !== "paddle";
  root.querySelector<HTMLElement>("#cloud-ocr-options")!.hidden = engine !== "cloud";
  updateCloudKeyStatus();
}

function updateCloudKeyStatus(): void {
  root.querySelector<HTMLElement>("#cloud-key-status")!.textContent = cloudOcrApiKeyConfigured
    ? "已安全保存云端 OCR API Key"
    : "尚未配置云端 OCR API Key";
}

async function togglePaddlePlugin(): Promise<void> {
  setBusy(true);
  setStatus(
    paddleOcrInstalled ? "正在卸载 PP-OCRv6 Small…" : "正在下载并校验约 31.2 MB 模型…",
    "neutral",
  );
  try {
    const command = paddleOcrInstalled
      ? "uninstall_paddle_ocr_plugin"
      : "install_paddle_ocr_plugin";
    const status = await invoke<PaddleOcrPluginStatus>(command);
    paddleOcrInstalled = status.installed;
    updatePaddlePluginStatus(status);
    setStatus(status.installed ? "PP-OCRv6 Small 已安装，可离线使用" : "PP-OCRv6 Small 已卸载", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  } finally {
    setBusy(false);
  }
}

function updatePaddlePluginStatus(status: PaddleOcrPluginStatus): void {
  paddleOcrInstalled = status.installed;
  root.querySelector<HTMLElement>("#paddle-plugin-status")!.textContent = status.installed
    ? `已安装并通过完整性校验 · ${formatBytes(status.installedBytes)}`
    : `未安装 · 下载大小约 ${formatBytes(status.downloadBytes)}`;
  root.querySelector<HTMLButtonElement>("#toggle-paddle-plugin")!.textContent = status.installed
    ? "卸载插件"
    : "一键安装";
}

async function copyCloudKeyUrl(): Promise<void> {
  try {
    await invoke("copy_translation", { text: cloudOcrApiKeyUrl });
    setStatus("阿里云百炼 API Key 申请网址已复制", "success");
  } catch (error) {
    setStatus(errorMessage(error), "error");
  }
}

function formatBytes(value: number): string {
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
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
