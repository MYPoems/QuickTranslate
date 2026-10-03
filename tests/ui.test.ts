import { test } from "node:test";
import assert from "node:assert/strict";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import { defaultSpeech } from "../src/speech/reader";
import type { SettingsView, UpdateProgress, TranslationEvent } from "../src/types";

const settings: SettingsView = { provider: "阿里云百炼", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-turbo", globalShortcut: "Alt+Q", ocrShortcut: "Alt+W", ocrEngine: "cloud", ocrLanguage: "auto", cloudOcrBaseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", cloudOcrModel: "qwen3.5-ocr", apiKeyConfigured: false, cloudOcrApiKeyConfigured: false, paddleOcrInstalled: false, autoStartEnabled: false, speech: { ...defaultSpeech, rate: 85, chineseVoice: "zh-test", englishVoice: "removed-voice" } };
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
settings.appearance = { theme: "system", popupOpacity: 96 };
settings.cloudSpeechApiKeyConfigured = false;
async function settle() { for (let i = 0; i < 4; i++) await tick(); }

async function mount(entry: "settings" | "popup" | "history") {
  const dom = new JSDOM('<main id="app"></main>', { url: "http://localhost/" });
  const globals = globalThis as unknown as Record<string, unknown>;
  globals.AudioContext = class { async resume() {} };
  for (const name of ["window", "document", "HTMLElement", "HTMLInputElement", "HTMLSelectElement", "HTMLFormElement", "HTMLTextAreaElement", "FormData", "Option"]) globals[name] = (dom.window as unknown as Record<string, unknown>)[name];
  dom.window.HTMLElement.prototype.scrollIntoView = () => {};
  dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; };
  const events = new Map<string, Set<(event: { payload: unknown }) => void>>();
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  let update: UpdateProgress = { phase: "idle", version: "", downloaded: 0, message: "", releaseNotes: "" };
  const qa = {
    emit(name: string, payload: unknown) { events.get(name)?.forEach(handler => handler({ payload })); },
    async listen(name: string, handler: (event: { payload: unknown }) => void) { if (!events.has(name)) events.set(name, new Set()); events.get(name)!.add(handler); return () => events.get(name)?.delete(handler); },
    async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
      calls.push({ command, args });
      if (command === "get_settings") return settings;
      if (command === "get_appearance_preferences") return { theme: "system", popupOpacity: 96 };
      if (command === "list_translation_history") return [{ id: 1, sourceText: "Stay curious.", translation: "保持好奇。", sourceLanguage: "en", targetLanguage: "zh", provider: "QA", model: "fixture", createdAt: 1, favorite: false }];
      if (command === "get_speech_preferences") return { ...settings.speech, provider: "offline" };
      if (command === "get_speech_plugin_status") return { installed: false, phase: "idle", downloaded: 0, downloadBytes: 171837079, version: "test", message: "" };
      if (command === "list_speech_voices") return [{ id: "zh-test", name: "Test Chinese", language: "zh-CN" }];
      if (command === "get_update_state") return update;
      if (command === "get_popup_pinned") return false;
      if (command === "save_settings") return { ...settings, ...args.update as object };
      if (command === "check_for_updates") {
        update = { phase: "available", version: "1.3.0", downloaded: 0, message: "发现新版本", releaseNotes: "Test release" };
        qa.emit("update-progress", update);
        return { updateAvailable: true, latestVersion: "1.3.0", currentVersion: "1.2.0", releaseUrl: "https://github.com/MYPoems/QuickTranslate/releases" };
      }
      if (command === "download_update") { update = { ...update, phase: "downloading", downloaded: 1234, total: 5000 }; qa.emit("update-progress", update); }
      if (command === "cancel_update_download") { update = { ...update, phase: "cancelled", message: "已取消" }; qa.emit("update-progress", update); }
      if (command === "synthesize_speech") throw { code: "SPEECH_ERROR", message: "missing voice" };
      return undefined;
    },
  };
  globals.__qa = qa;
  const result = await build({ entryPoints: [`src/${entry}/${entry}.ts`], bundle: true, write: false, format: "esm", platform: "browser", loader: { ".css": "empty" }, plugins: [{ name: "tauri-test-stub", setup(builder) {
    builder.onResolve({ filter: /^@tauri-apps\/api\// }, args => ({ path: args.path, namespace: "qa" }));
    builder.onLoad({ filter: /.*/, namespace: "qa" }, () => ({ contents: "export class Channel { onmessage = () => {}; } export const invoke = (...args) => globalThis.__qa.invoke(...args); export const listen = (...args) => globalThis.__qa.listen(...args); export const emit = (...args) => globalThis.__qa.emit(...args); export const getCurrentWindow = () => ({onFocusChanged: async () => () => {}});" }));
  } }] });
  const module = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text + `\n//# sourceURL=quicktranslate-${entry}-test-${Math.random()}.js`).toString("base64")}`);
  module[{ settings: "mountSettings", popup: "mountPopup", history: "mountHistory" }[entry]]();
  await settle();
  const element = <T extends HTMLElement>(selector: string) => dom.window.document.querySelector<T>(selector)!;
  return { dom, qa, calls, element, close: () => dom.window.close() };
}

test("settings preserve installed/missing voices, rate and preference on save", async () => {
  const { element, calls, close } = await mount("settings");
  try {
    assert.equal(element<HTMLSelectElement>('[name="speechRate"]').value, "85");
    assert.equal(element<HTMLSelectElement>('[name="chineseVoice"]').value, "zh-test");
    assert.equal(element<HTMLSelectElement>('[name="englishVoice"]').value, "removed-voice");
    assert.match(element('[name="englishVoice"]').textContent || "", /不可用/);
    assert.equal(element('[name="bilingual"]'), null);
    element<HTMLFormElement>("form").dispatchEvent(new window.Event("submit", { cancelable: true }));
    await settle();
    const save = calls.find(call => call.command === "save_settings")!;
    assert.deepEqual((save.args.update as { speech: unknown }).speech, { ...settings.speech, bilingual: false });
  } finally { close(); }
});
test("cloud is default with explicit privacy notice, independent key and manual offline switch", async () => {
  const { element, calls, close } = await mount("settings");
  try {
    assert.equal(element<HTMLSelectElement>('[name="speechProvider"]').value, "cloud");
    assert.equal(element("#cloud-speech-options").hidden, false); assert.equal(element("#offline-speech-options").hidden, true);
    assert.match(element("#cloud-speech-options").textContent || "", /上传.*费用/);
    assert.match(element("#speech-key-status").textContent || "", /尚未配置/);
    assert.equal(calls.some(call => call.command === "stream_cloud_speech" || call.command === "test_cloud_speech"), false);
    const provider = element<HTMLSelectElement>('[name="speechProvider"]'); provider.value = "offline"; provider.dispatchEvent(new window.Event("change"));
    assert.equal(element("#offline-speech-options").hidden, false);
    element<HTMLFormElement>("form").dispatchEvent(new window.Event("submit", { cancelable: true })); await settle();
    const update = calls.find(call => call.command === "save_settings")!.args.update as { speech: { provider: string; chineseVoice: string }; cloudSpeechApiKey?: string };
    assert.equal(update.speech.provider, "offline"); assert.equal(update.speech.chineseVoice, "zh-test"); assert.equal(update.cloudSpeechApiKey, undefined);
  } finally { close(); }
});

test("update progress/cancel, no silent install, defer and exact-version confirmation", async () => {
  const { element, qa, calls, close } = await mount("settings");
  try {
    element("#check-update").click(); await settle();
    assert.equal(element("#download-update").hidden, false);
    element("#download-update").click(); await settle();
    assert.equal(element<HTMLProgressElement>("#update-progress").value, 1234);
    assert.equal(element<HTMLButtonElement>("#cancel-update").disabled, false);
    assert.equal(element<HTMLButtonElement>("#check-update").disabled, true);
    element("#cancel-update").click(); await settle();
    assert.equal(element("#update-progress").hidden, true);
    assert.equal(calls.some(call => call.command === "install_update"), false);
    qa.emit("update-progress", { phase: "error", version: "1.3.0", downloaded: 0, message: "签名错误", releaseNotes: "" });
    assert.equal(element("#install-update").hidden, true);
    qa.emit("update-progress", { phase: "ready", version: "1.3.0", downloaded: 5000, message: "校验通过", releaseNotes: "notes" });
    element("#install-update").click(); assert.equal(element<HTMLDialogElement>("#install-dialog").open, true);
    element("#defer-install").click(); assert.equal(element<HTMLDialogElement>("#install-dialog").open, false);
    assert.equal(calls.some(call => call.command === "install_update"), false);
    element("#install-update").click(); element("#confirm-install").click(); await settle();
    assert.deepEqual(calls.find(call => call.command === "install_update")?.args, { version: "1.3.0", confirmed: true });
  } finally { close(); }
});

test("popup retains OCR after errors, speaks edits, marks stale bilingual output and hides safely", async () => {
  const { element, qa, calls, close } = await mount("popup");
  try {
    const payload: TranslationEvent = { requestId: 1, status: "success", sourceKind: "ocr", result: { sourceText: "原文第一段。\n原文第二段。", translation: "First.\nSecond.", detectedLanguage: "chinese", targetLanguage: "english", provider: "QA", model: "test", cached: false } };
    qa.emit("translation-state", payload);
    assert.equal(element("#read-both"), null);
    assert.equal(element<HTMLButtonElement>("#read-translation").disabled, false);
    const editor = element<HTMLTextAreaElement>(".recognized-text"); editor.value = "这是用户修正后的文字。"; editor.dispatchEvent(new window.Event("input"));
    assert.equal(element<HTMLButtonElement>("#read-translation").disabled, true);
    assert.equal(element("#retranslate-edits").hidden, false);
    assert.match(element("#badge").textContent || "", /修改/);
    element("#read-source").click(); await settle();
    assert.equal(calls.find(call => call.command === "synthesize_speech")?.args.text, editor.value);
    assert.match(element("#reading-status").textContent || "", /missing voice/);
    qa.emit("translation-state", { requestId: 2, status: "loading", sourceText: "识别内容仍需保留", sourceKind: "ocr" });
    qa.emit("translation-state", { requestId: 2, status: "error", error: { code: "NETWORK_ERROR", message: "network" } });
    assert.equal(element<HTMLTextAreaElement>(".recognized-text").value, "识别内容仍需保留");
    assert.equal(element<HTMLButtonElement>("#read-source").disabled, false);
    assert.equal(element<HTMLButtonElement>("#read-translation").disabled, true);
    element("#close").click(); await settle(); assert.ok(calls.some(call => call.command === "hide_translation_window"));
    assert.equal(element("#reading-status").hidden, true);
  } finally { close(); }
});

test("late retranslation cannot replace a newer retry or screenshot", async () => {
  const { element, qa, dom, close } = await mount("popup");
  try {
    const result = (translation: string) => ({ sourceText: "原文", translation, detectedLanguage: "chinese", targetLanguage: "english", provider: "QA", model: "test", cached: false });
    qa.emit("translation-state", { requestId: 1, status: "success", sourceKind: "ocr", result: result("Initial") });
    const pending: Array<(result: unknown) => void> = [];
    const originalInvoke = qa.invoke.bind(qa);
    qa.invoke = async (command, args = {}) => command === "retranslate_text"
      ? new Promise(resolve => pending.push(resolve)) : originalInvoke(command, args);
    const retry = () => dom.window.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: "r", altKey: true }));
    retry(); retry();
    assert.equal(pending.length, 2);
    pending[1](result("Newest retry")); await settle();
    pending[0](result("Obsolete retry")); await settle();
    assert.match(element("#content").textContent || "", /Newest retry/);
    assert.doesNotMatch(element("#content").textContent || "", /Obsolete retry/);
    retry();
    qa.emit("translation-state", { requestId: 2, status: "loading", sourceKind: "ocr", sourceText: "新的截图" });
    pending[2](result("Previous screenshot")); await settle();
    assert.equal(element<HTMLTextAreaElement>(".recognized-text").value, "新的截图");
    assert.doesNotMatch(element("#content").textContent || "", /Previous screenshot/);
  } finally { close(); }
});

test("voice plugin progress, cancellation, installed voices and uninstall preserve preferences", async () => {
  const { element, qa, calls, close } = await mount("settings");
  try {
    const original = qa.invoke.bind(qa);
    let finish: (state: unknown) => void = () => {};
    qa.invoke = async (command, args = {}) => {
      if (command === "install_speech_plugin") {
        qa.emit("speech-plugin-progress", { installed: false, phase: "downloading", downloaded: 1234, downloadBytes: 171837079, version: "test", message: "" });
        return new Promise(resolve => { finish = resolve; });
      }
      if (command === "list_speech_voices") return [{ id: "zh-test", name: "Test Chinese", language: "zh-CN" }, { id: "plugin:kokoro:0", name: "Maple", language: "en-US" }];
      if (command === "uninstall_speech_plugin") { calls.push({ command, args }); return { installed: false, phase: "idle", downloaded: 0, downloadBytes: 171837079, version: "test", message: "已卸载" }; }
      return original(command, args);
    };
    element("#toggle-speech-plugin").click(); await settle();
    assert.equal(element<HTMLButtonElement>("#toggle-speech-plugin").disabled, true);
    assert.equal(element<HTMLProgressElement>("#speech-plugin-progress").value, 1234);
    element("#cancel-speech-plugin").click(); await settle();
    assert.ok(calls.some(call => call.command === "cancel_speech_plugin_install"));
    finish({ installed: true, phase: "ready", downloaded: 171837079, downloadBytes: 171837079, version: "test", message: "已安装" }); await settle();
    assert.equal(element("#toggle-speech-plugin").textContent, "卸载音色插件");
    assert.equal(element<HTMLSelectElement>('[name="englishVoice"]').value, "removed-voice");
    assert.match(element('[name="englishVoice"]').textContent || "", /Maple/);
    element("#toggle-speech-plugin").click(); await settle();
    assert.ok(calls.some(call => call.command === "uninstall_speech_plugin"));
    assert.equal(element<HTMLSelectElement>('[name="speechRate"]').value, "85");
    assert.equal(element("#speech-plugin-progress").hidden, true);
  } finally { close(); }
});

test("appearance saves independently of learning rules and invalid hidden fields reveal their category", async () => {
  const { element, calls, close } = await mount("settings");
  try {
    element('[data-settings-tab="general"]').click();
    const theme = element<HTMLSelectElement>('[name="appearanceTheme"]'); theme.value = "dark"; theme.dispatchEvent(new window.Event("change"));
    const range = element<HTMLInputElement>('[name="popupTransparency"]'); range.value = "20"; range.dispatchEvent(new window.Event("input"));
    assert.equal(document.documentElement.dataset.theme, "dark");
    assert.equal(document.documentElement.style.getPropertyValue("--popup-opacity"), "80%");
    assert.equal(element("#opacity-value").textContent, "20%");
    element<HTMLInputElement>('[name="dailyLimit"]').value = "0"; // An unsaved learning draft must not block appearance.
    element<HTMLFormElement>("#settings-form").dispatchEvent(new window.Event("submit", { cancelable: true })); await settle();
    assert.deepEqual((calls.find(c => c.command === "save_settings")!.args.update as {appearance: unknown}).appearance, { theme: "dark", popupOpacity: 80 });
    assert.equal(calls.some(c => c.command === "set_vocabulary_rules"), false);
    const model = element<HTMLInputElement>('[name="model"]'); model.value = "";
    element<HTMLFormElement>("#settings-form").dispatchEvent(new window.Event("submit", { cancelable: true }));
    assert.equal(element("#settings-translation").hidden, false);
    assert.equal(calls.filter(c => c.command === "save_settings").length, 1);
  } finally { close(); }
});

test("popup menu closes before window, preserves selections and leaves editable Ctrl+C alone", async () => {
  const { element, qa, calls, dom, close } = await mount("popup");
  try {
    qa.emit("translation-state", { requestId: 1, status: "success", sourceKind: "ocr", result: { sourceText: "Stay curious.", translation: "保持好奇。", detectedLanguage: "english", targetLanguage: "chinese", provider: "QA", model: "fixture", cached: false } });
    element("#more").click(); assert.equal(element("#popup-menu").hidden, false);
    dom.window.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: "Escape", cancelable: true })); await settle();
    assert.equal(element("#popup-menu").hidden, true);
    assert.equal(calls.some(c => c.command === "hide_translation_window"), false);
    const editor = element<HTMLTextAreaElement>(".recognized-text"); editor.focus(); editor.setSelectionRange(5,12);
    dom.window.document.dispatchEvent(new dom.window.Event("selectionchange"));
    assert.match(element("#collect-selected").textContent || "", /curious/);
    const copy = new dom.window.KeyboardEvent("keydown", { key: "c", ctrlKey: true, bubbles: true, cancelable: true }); editor.dispatchEvent(copy);
    assert.equal(copy.defaultPrevented, false); assert.equal(calls.some(c => c.command === "copy_translation"), false);
    element("#collect-selected").click(); await settle();
    assert.equal(element<HTMLInputElement>("#collect-word").value, "curious");
    assert.equal(calls.some(c => c.command === "collect_vocabulary"), false);
  } finally { close(); }
});

test("history has independent speech, local popup opening and confirmed deletion", async () => {
  const { element, calls, close } = await mount("history");
  try {
    assert.match(element("#history-detail").textContent || "", /Stay curious/);
    element("#history-read-source").click(); await settle();
    element("#history-read-translation").click(); await settle();
    assert.deepEqual(calls.filter(c => c.command === "synthesize_speech").map(c => c.args.text), ["Stay curious.", "保持好奇。"]);
    const actions = Array.from(element(".detail-more").querySelectorAll("button"));
    actions.find(b => b.textContent === "在悬浮窗打开")!.click(); await settle();
    assert.deepEqual(calls.find(c => c.command === "open_history_translation")!.args, { id: 1 });
    const remove = Array.from(element(".detail-more").querySelectorAll("button")).find(b => b.textContent === "删除记录…")!;
    remove.click(); await settle(); assert.equal(calls.some(c => c.command === "delete_history_entry"), false);
    element("#history-confirm-no").click(); await settle(); assert.equal(calls.some(c => c.command === "delete_history_entry"), false);
    Array.from(element(".detail-more").querySelectorAll("button")).find(b => b.textContent === "删除记录…")!.click(); await settle();
    element("#history-confirm-yes").click(); await settle();
    assert.deepEqual(calls.find(c => c.command === "delete_history_entry")!.args, { id: 1 });
    assert.equal(calls.some(c => c.command === "translate_text" || c.command === "retranslate_text"), false);
  } finally { close(); }
});

test("switching history during delayed speech preferences cancels stale audio", async () => {
  const { element, qa, calls, close } = await mount("history");
  try {
    const original = qa.invoke.bind(qa); let finish: (value: unknown) => void = () => {};
    qa.invoke = async (command, args = {}) => command === "get_speech_preferences" ? new Promise(resolve => { finish = resolve; }) : original(command, args);
    element("#history-read-source").click(); await settle();
    element(".history-row").click(); await settle(); finish({ ...defaultSpeech, provider: "offline" }); await settle();
    assert.equal(calls.some(c => c.command === "synthesize_speech"), false);
    assert.equal(element("#history-read-source").getAttribute("aria-pressed"), "false");
  } finally { close(); }
});

test("history ignores older filtered responses arriving after a new query", async () => {
  const { element, qa, dom, close } = await mount("history");
  try {
    const original = qa.invoke.bind(qa); const pending: Array<(value: unknown) => void> = [];
    qa.invoke = async (command, args = {}) => command === "list_translation_history" ? new Promise(resolve => pending.push(resolve)) : original(command, args);
    const filter = element<HTMLInputElement>("#favorite-only"); filter.checked = true; filter.dispatchEvent(new dom.window.Event("change"));
    filter.checked = false; filter.dispatchEvent(new dom.window.Event("change"));
    const entry = (id: number, value: string) => ({ id, sourceText: value, translation: value, sourceLanguage: "en", targetLanguage: "zh", provider: "QA", model: "fixture", createdAt: 1, favorite: false });
    pending[1]([entry(2, "Current")]); await settle(); pending[0]([entry(1, "Obsolete")]); await settle();
    assert.match(element("#history-list").textContent || "", /Current/);
    assert.doesNotMatch(element("#history-list").textContent || "", /Obsolete/);
  } finally { close(); }
});
