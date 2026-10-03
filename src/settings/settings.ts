import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { defaultSpeech } from "../speech/reader";
import { organizeSettings, loadAppearance, appearanceFromForm, selectSettingsPanel } from "./layout";
import type {
  AppError,
  DiagnosticsView,
  PaddleOcrPluginStatus,
  SettingsBackup,
  SettingsView,
  UpdateInfo,
  UpdateSettings,
  SpeechVoice,
  UpdateProgress,
  SpeechPluginStatus,
} from "../types";
import "./settings.css";

const root = document.querySelector<HTMLElement>("#app")!;
let apiKeyConfigured = false;
let cloudOcrApiKeyConfigured = false;
let cloudSpeechApiKeyConfigured = false;
let paddleOcrInstalled = false;
let updateState: UpdateProgress = { phase: "idle", version: "", downloaded: 0, message: "", releaseNotes: "" };
let formBusy = false;
let speechPluginState: SpeechPluginStatus = { installed: false, present: false, phase: "idle", downloaded: 0, downloadBytes: 171837079, version: "", message: "" };
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
            <p>推荐云端视觉 OCR；Windows OCR 与本地插件用于离线或云端不可用时手动切换。</p>
          </div>
          <label>识别方案
            <select name="ocrEngine">
              <option value="cloud">云端视觉 OCR（推荐，自备 API Key）</option>
              <option value="windows">Windows OCR（内置离线替代）</option>
              <option value="paddle">PP-OCRv6 Small（本地离线替代）</option>
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
            <p class="privacy-warning">云端模式会把所选截图发送给你配置的服务商，仅在已配置 Key 且你主动框选截图时发送。不希望上传时请改用离线方案。</p>
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
        <section class="ocr-card speech-card">
          <strong>朗读</strong>
          <label>朗读方案<select name="speechProvider"><option value="cloud">云端流式朗读（默认，需 API Key）</option><option value="offline">离线朗读（Windows / Kokoro 插件）</option></select></label>
          <div id="cloud-speech-options" class="ocr-options">
            <p class="privacy-warning">只有你主动点击朗读时，才会把朗读文字上传至阿里云并产生语音合成费用。未配置 Key 或连接失败时明确提示，不自动切换服务。北京参考价 1 元 / 万字符，以控制台为准。</p>
            <label>云端朗读地域 / WebSocket 地址<select name="cloudSpeechEndpoint"><option value="wss://dashscope.aliyuncs.com/api-ws/v1/realtime">北京（中国内地 Key）</option><option value="wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime">新加坡（国际 Key）</option></select></label>
            <label>云端语音模型<input name="cloudSpeechModel" value="qwen3-tts-flash-realtime" required /></label>
            <label>云端中文音色<input name="cloudChineseVoice" value="Cherry" required /></label>
            <label>云端英文音色<input name="cloudEnglishVoice" value="Cherry" required /></label>
            <p>默认 Cherry，同一音色支持中英文。可填写官方支持的音色 ID。</p>
            <label>云端朗读 API Key<input name="cloudSpeechApiKey" type="password" autocomplete="off" placeholder="独立保存；留空不修改，不自动复用 OCR / 翻译 Key" /></label>
            <p id="speech-key-status" class="key-status"></p>
            <label class="checkbox-row"><input name="clearCloudSpeechApiKey" type="checkbox" />删除已保存的云端朗读 API Key</label>
            <div class="cloud-help"><button id="test-cloud-speech" type="button" class="secondary">测试云端朗读连接</button><button id="copy-speech-key-url" type="button" class="secondary">复制 API 申请网址</button></div>
            <p>测试会合成一条短句并显示首音频耗时，可能产生少量费用；不会保存未保存的表单。</p>
          </div>
          <div id="offline-speech-options" class="ocr-options" hidden>
          <p>系统音色和可选中英文插件均离线运行，不上传文字、不需要 API。插件无需管理员权限。</p>
          <div class="plugin-row"><div><strong>Kokoro 中英文音色插件</strong><p id="speech-plugin-status" role="status">正在读取插件状态…</p><progress id="speech-plugin-progress" hidden></progress></div><button id="toggle-speech-plugin" type="button" class="secondary">一键安装中英文音色</button><button id="cancel-speech-plugin" type="button" class="secondary" hidden>取消下载</button></div>
          <p>首次下载约 164 MiB，含精选中英文音色；完整性验证后启用。离线自动选择优先使用系统音色，不可用时使用插件。插件模型首次加载后复用，闲置 5 分钟释放。</p>
          <label>插件 CPU 线程数<select name="speechThreads"><option value="2">2（较低 CPU 占用）</option><option value="4">4（默认）</option><option value="6">6（需实测性能）</option><option value="8">8（需实测性能）</option></select></label>
          <label>中文音色<select name="chineseVoice"><option value="">自动选择中文音色</option></select></label>
          <label>英文音色<select name="englishVoice"><option value="">自动选择英文音色</option></select></label>
          <p id="voice-status" role="status">正在读取系统音色…</p>
          </div>
          <label>语速<select name="speechRate"><option value="50">0.5×</option><option value="75">0.75×</option><option value="85">0.85×</option><option value="100">1×</option><option value="125">1.25×</option><option value="150">1.5×</option><option value="175">1.75×</option><option value="200">2×</option></select></label>
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
            <progress id="update-progress" hidden></progress>
            <details id="release-notes" hidden><summary>更新说明</summary><p id="release-notes-text"></p></details>
          </div>
          <div class="update-actions">
            <button id="check-update" type="button" class="secondary">检查更新</button>
            <button id="download-update" type="button" class="secondary" hidden>下载并校验</button>
            <button id="cancel-update" type="button" class="secondary" hidden>取消下载</button>
            <button id="install-update" type="button" class="primary" hidden>安装更新…</button>
          </div>
        </div>
        <dialog id="install-dialog">
          <h2>确认安装更新</h2><p id="install-confirm-text"></p>
          <p>签名校验已通过。安装前会自动备份设置、悬浮窗尺寸及翻译历史。安装时应用会退出，完成后重新启动。API Key 保留在 Windows 凭据管理器，不写入备份。</p>
          <div class="backup-actions"><button id="defer-install" type="button" class="secondary">暂不安装</button><button id="confirm-install" type="button" class="primary">确认安装</button></div>
        </dialog>
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
  organizeSettings(root, form);
  (form.elements.namedItem("speechProvider") as HTMLSelectElement).addEventListener("change", () => refreshSpeechPanels(form));
  root.querySelector("#copy-speech-key-url")!.addEventListener("click", () => void copyCloudKeyUrl());
  root.querySelector("#test-cloud-speech")!.addEventListener("click", () => void testCloudSpeech(form));
  root.querySelector("#toggle-speech-plugin")!.addEventListener("click", () => void toggleSpeechPlugin(form));
  root.querySelector("#cancel-speech-plugin")!.addEventListener("click", () => void invoke("cancel_speech_plugin_install").catch(error => setStatus(errorMessage(error), "error")));
  void listen<SpeechPluginStatus>("speech-plugin-progress", ({ payload }) => renderSpeechPlugin(payload)).then(() => invoke<SpeechPluginStatus>("get_speech_plugin_status")).then(renderSpeechPlugin).catch(error => { root.querySelector("#speech-plugin-status")!.textContent = errorMessage(error); });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const invalid = Array.from(form.querySelectorAll<HTMLInputElement>("input, select, textarea")).find(input => !input.closest('#settings-learning') && !input.validity.valid);
    if (invalid) { selectSettingsPanel(form, invalid.closest<HTMLElement>("[data-settings-panel]")?.dataset.settingsPanel || "translation"); form.reportValidity(); return; }
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
  root.querySelector("#download-update")!.addEventListener("click", () => void downloadUpdate());
  root.querySelector("#cancel-update")!.addEventListener("click", () => void invoke("cancel_update_download").catch(error => setStatus(errorMessage(error), "error")));
  root.querySelector("#install-update")!.addEventListener("click", () => {
    root.querySelector("#install-confirm-text")!.textContent = `即将从当前版本升级到 v${updateState.version}。是否现在安装？未保存的表单更改不会进入备份，请先保存。`;
    root.querySelector<HTMLDialogElement>("#install-dialog")!.showModal();
  });
  root.querySelector("#defer-install")!.addEventListener("click", () => root.querySelector<HTMLDialogElement>("#install-dialog")!.close());
  root.querySelector("#confirm-install")!.addEventListener("click", () => void confirmInstall());
  void listen<UpdateProgress>("update-progress", ({ payload }) => renderUpdate(payload)).then(() => invoke<UpdateProgress>("get_update_state").then(renderUpdate)).catch(error => setStatus(errorMessage(error), "error"));
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
    loadAppearance(form, settings.appearance);
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
    await loadVoices(form, settings.speech);
    apiKeyConfigured = settings.apiKeyConfigured;
    cloudOcrApiKeyConfigured = settings.cloudOcrApiKeyConfigured;
    cloudSpeechApiKeyConfigured = settings.cloudSpeechApiKeyConfigured;
    refreshSpeechPanels(form);
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
    loadAppearance(form, settings.appearance);
    (form.elements.namedItem("apiKey") as HTMLInputElement).value = "";
    (form.elements.namedItem("clearApiKey") as HTMLInputElement).checked = false;
    (form.elements.namedItem("cloudOcrApiKey") as HTMLInputElement).value = "";
    (form.elements.namedItem("clearCloudOcrApiKey") as HTMLInputElement).checked = false;
    (form.elements.namedItem("cloudSpeechApiKey") as HTMLInputElement).value = "";
    setCheckbox(form, "clearCloudSpeechApiKey", false);
    setCheckbox(form, "autoStartEnabled", settings.autoStartEnabled);
    apiKeyConfigured = settings.apiKeyConfigured;
    cloudOcrApiKeyConfigured = settings.cloudOcrApiKeyConfigured;
    cloudSpeechApiKeyConfigured = settings.cloudSpeechApiKeyConfigured;
    refreshSpeechPanels(form);
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
      output.textContent = `发现 v${update.latestVersion}，可下载并校验后安装。`;
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
    loadAppearance(form, backup.appearance);
    await loadVoices(form, backup.speech || defaultSpeech);
    refreshSpeechPanels(form);
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
    appearance: appearanceFromForm(form),
    speech: { provider: String(data.get("speechProvider") || "cloud") as "cloud" | "offline", cloudEndpoint: String(data.get("cloudSpeechEndpoint") || defaultSpeech.cloudEndpoint), cloudModel: String(data.get("cloudSpeechModel") || defaultSpeech.cloudModel).trim(), cloudChineseVoice: String(data.get("cloudChineseVoice") || "Cherry").trim(), cloudEnglishVoice: String(data.get("cloudEnglishVoice") || "Cherry").trim(), threads: Number(data.get("speechThreads") || 4), rate: Number(data.get("speechRate") || 100), chineseVoice: String(data.get("chineseVoice") || ""), englishVoice: String(data.get("englishVoice") || ""), bilingual: data.get("bilingual") === "on" },
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
    cloudSpeechApiKey: String(data.get("cloudSpeechApiKey") || "").trim() || undefined,
    clearCloudSpeechApiKey: data.get("clearCloudSpeechApiKey") === "on",
    autoStartEnabled: data.get("autoStartEnabled") === "on",
  };
}

function setInput(form: HTMLFormElement, name: string, value: string): void {
  (form.elements.namedItem(name) as HTMLInputElement | HTMLSelectElement).value = value;
}

function setCheckbox(form: HTMLFormElement, name: string, checked: boolean): void {
  const input = form.elements.namedItem(name) as HTMLInputElement | null;
  if (input) input.checked = checked;
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
  formBusy = busy;
  root.querySelectorAll<HTMLButtonElement>("button").forEach((button) => (button.disabled = busy));
  renderUpdate(updateState);
  renderSpeechPlugin(speechPluginState);
}

function renderSpeechPlugin(state: SpeechPluginStatus): void {
  speechPluginState = state;
  const busy = state.phase === "downloading" || state.phase === "verifying";
  const toggle = root.querySelector<HTMLButtonElement>("#toggle-speech-plugin")!;
  toggle.disabled = busy || formBusy;
  toggle.textContent = state.installed ? "卸载音色插件" : state.present ? "卸载损坏插件后重装" : "一键安装中英文音色";
  const cancel = root.querySelector<HTMLButtonElement>("#cancel-speech-plugin")!;
  cancel.hidden = !busy; cancel.disabled = false;
  const meter = root.querySelector<HTMLProgressElement>("#speech-plugin-progress")!;
  meter.hidden = !busy; meter.max = state.downloadBytes; meter.value = state.downloaded;
  root.querySelector("#speech-plugin-status")!.textContent = state.phase === "downloading" ? `正在下载 · ${formatBytes(state.downloaded)} / ${formatBytes(state.downloadBytes)}` : state.phase === "verifying" ? "正在安全解压与校验…" : state.message || (state.installed ? `已安装 · ${state.version}` : "未安装，Windows 系统音色仍可使用");
}
async function toggleSpeechPlugin(form: HTMLFormElement): Promise<void> {
  try {
    renderSpeechPlugin({ ...speechPluginState, phase: "verifying", message: "" });
    const state = await invoke<SpeechPluginStatus>(speechPluginState.present || speechPluginState.installed ? "uninstall_speech_plugin" : "install_speech_plugin");
    renderSpeechPlugin(state);
    await loadVoices(form, formValue(form).speech);
  } catch (error) {
    renderSpeechPlugin({ ...speechPluginState, phase: speechPluginState.phase === "cancelled" ? "cancelled" : "error", message: errorMessage(error) });
  }
}

async function loadVoices(form: HTMLFormElement, preferences = defaultSpeech): Promise<void> {
  preferences = { ...defaultSpeech, ...preferences };
  setInput(form, "speechProvider", preferences.provider);
  setInput(form, "cloudSpeechEndpoint", preferences.cloudEndpoint);
  setInput(form, "cloudSpeechModel", preferences.cloudModel);
  setInput(form, "cloudChineseVoice", preferences.cloudChineseVoice);
  setInput(form, "cloudEnglishVoice", preferences.cloudEnglishVoice);
  setInput(form, "speechThreads", String(preferences.threads));
  const threads = form.elements.namedItem("speechThreads") as HTMLSelectElement;
  if (!threads.value) { threads.add(new Option(String(preferences.threads), String(preferences.threads))); threads.value = String(preferences.threads); }
  setInput(form, "speechRate", String(preferences.rate));
  const rates = form.elements.namedItem("speechRate") as HTMLSelectElement;
  if (!rates.value) { rates.add(new Option(`${preferences.rate / 100}×`, String(preferences.rate))); rates.value = String(preferences.rate); }
  setCheckbox(form, "bilingual", preferences.bilingual);
  let voices: SpeechVoice[] = [];
  try { voices = await invoke<SpeechVoice[]>("list_speech_voices"); root.querySelector("#voice-status")!.textContent = `检测到 ${voices.length} 个离线音色；每段文字自动匹配中英文。`; }
  catch (error) { root.querySelector("#voice-status")!.textContent = errorMessage(error); }
  for (const [name, language, saved] of [["chineseVoice", "zh", preferences.chineseVoice], ["englishVoice", "en", preferences.englishVoice]]) {
    const select = form.elements.namedItem(name) as HTMLSelectElement;
    select.replaceChildren(new Option(`自动选择${language === "zh" ? "中文" : "英文"}音色`, ""));
    for (const voice of voices.filter(voice => voice.language.toLowerCase().startsWith(language))) select.add(new Option(`${voice.name} · ${voice.language}`, voice.id));
    if (saved && !Array.from(select.options).some(option => option.value === saved)) select.add(new Option("已保存的音色（当前不可用，请重新选择）", saved));
    select.value = saved;
  }
}
function refreshSpeechPanels(form: HTMLFormElement): void {
  const cloud = (form.elements.namedItem("speechProvider") as HTMLSelectElement).value === "cloud";
  root.querySelector<HTMLElement>("#cloud-speech-options")!.hidden = !cloud;
  root.querySelector<HTMLElement>("#offline-speech-options")!.hidden = cloud;
  root.querySelector("#speech-key-status")!.textContent = cloudSpeechApiKeyConfigured ? "已安全保存云端朗读 API Key" : "尚未配置云端朗读 API Key；朗读前需配置，或选择离线方案";
}
async function testCloudSpeech(form: HTMLFormElement): Promise<void> {
  setBusy(true); setStatus("正在测试云端朗读（会合成一条短句）…", "neutral");
  try {
    const timing = await invoke<{ firstAudioMs: number; totalMs: number; cached: boolean }>("test_cloud_speech", { update: formValue(form) });
    setStatus(`云端朗读连接成功，首音频 ${timing.firstAudioMs} ms，合成总耗时 ${timing.totalMs} ms（不等于实际播放延迟）`, "success");
  } catch (error) { setStatus(errorMessage(error), "error"); }
  finally { setBusy(false); }
}
function renderUpdate(progress: UpdateProgress): void {
  updateState = progress;
  const downloading = progress.phase === "downloading" || progress.phase === "verifying";
  const busy = downloading || progress.phase === "checking" || progress.phase === "installing";
  const check = root.querySelector<HTMLButtonElement>("#check-update")!;
  check.disabled = busy || formBusy;
  const download = root.querySelector<HTMLButtonElement>("#download-update")!;
  download.hidden = !["available", "cancelled", "error"].includes(progress.phase) || !progress.version;
  download.disabled = formBusy;
  const cancel = root.querySelector<HTMLButtonElement>("#cancel-update")!;
  cancel.hidden = !downloading; cancel.disabled = false;
  const install = root.querySelector<HTMLButtonElement>("#install-update")!;
  install.hidden = progress.phase !== "ready"; install.disabled = formBusy;
  const meter = root.querySelector<HTMLProgressElement>("#update-progress")!;
  meter.hidden = !downloading;
  if (progress.total) { meter.max = progress.total; meter.value = progress.downloaded; } else meter.removeAttribute("value");
  const output = root.querySelector("#update-status")!;
  output.textContent = progress.phase === "downloading" ? `下载 v${progress.version} · ${formatBytes(progress.downloaded)}${progress.total ? ` / ${formatBytes(progress.total)}` : ""}` : progress.phase === "verifying" ? "下载完成，正在校验签名与版本…" : progress.phase === "checking" ? "正在检查 GitHub Releases…" : progress.message || "从 GitHub Releases 检查正式版本。";
  root.querySelector<HTMLElement>("#release-notes")!.hidden = !progress.releaseNotes;
  root.querySelector("#release-notes-text")!.textContent = progress.releaseNotes;
}
async function downloadUpdate(): Promise<void> {
  try { await invoke("download_update"); }
  catch (error) { setStatus(errorMessage(error), "error"); }
}
async function confirmInstall(): Promise<void> {
  const button = root.querySelector<HTMLButtonElement>("#confirm-install")!;
  button.disabled = true;
  try { await invoke("install_update", { version: updateState.version, confirmed: true }); }
  catch (error) { setStatus(errorMessage(error), "error"); }
  finally { button.disabled = false; root.querySelector<HTMLDialogElement>("#install-dialog")!.close(); }
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
