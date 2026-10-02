import { test } from "node:test";
import assert from "node:assert/strict";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import { defaultSpeech } from "../src/speech/reader";
import type { SettingsView, UpdateProgress, TranslationEvent } from "../src/types";

const settings: SettingsView = { provider: "阿里云百炼", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-turbo", globalShortcut: "Alt+Q", ocrShortcut: "Alt+W", ocrEngine: "cloud", ocrLanguage: "auto", cloudOcrBaseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", cloudOcrModel: "qwen3.5-ocr", apiKeyConfigured: false, cloudOcrApiKeyConfigured: false, paddleOcrInstalled: false, autoStartEnabled: false, speech: { ...defaultSpeech, rate: 85, chineseVoice: "zh-test", englishVoice: "removed-voice" } };
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
async function settle() { for (let i = 0; i < 4; i++) await tick(); }

async function mount(entry: "settings" | "popup") {
  const dom = new JSDOM('<main id="app"></main>', { url: "http://localhost/" });
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of ["window", "document", "HTMLElement", "HTMLInputElement", "HTMLSelectElement", "HTMLFormElement", "HTMLTextAreaElement", "FormData", "Option"]) globals[name] = (dom.window as unknown as Record<string, unknown>)[name];
  dom.window.HTMLElement.prototype.scrollIntoView = () => {};
  dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; };
  const events = new Map<string, (event: { payload: unknown }) => void>();
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  let update: UpdateProgress = { phase: "idle", version: "", downloaded: 0, message: "", releaseNotes: "" };
  const qa = {
    emit(name: string, payload: unknown) { events.get(name)?.({ payload }); },
    async listen(name: string, handler: (event: { payload: unknown }) => void) { events.set(name, handler); return () => events.delete(name); },
    async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
      calls.push({ command, args });
      if (command === "get_settings") return settings;
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
    builder.onLoad({ filter: /.*/, namespace: "qa" }, () => ({ contents: "export const invoke = (...args) => globalThis.__qa.invoke(...args); export const listen = (...args) => globalThis.__qa.listen(...args); export const emit = (...args) => globalThis.__qa.emit(...args);" }));
  } }] });
  const module = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text + `\n//# sourceURL=quicktranslate-${entry}-test-${Math.random()}.js`).toString("base64")}`);
  module[entry === "settings" ? "mountSettings" : "mountPopup"]();
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
    element<HTMLInputElement>('[name="bilingual"]').checked = true;
    element<HTMLFormElement>("form").dispatchEvent(new window.Event("submit", { cancelable: true }));
    await settle();
    const save = calls.find(call => call.command === "save_settings")!;
    assert.deepEqual((save.args.update as { speech: unknown }).speech, { ...settings.speech, bilingual: true });
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
    assert.equal(element("#read-both").hidden, false);
    assert.equal(element<HTMLButtonElement>("#read-both").disabled, false);
    const editor = element<HTMLTextAreaElement>(".recognized-text"); editor.value = "这是用户修正后的文字。"; editor.dispatchEvent(new window.Event("input"));
    assert.equal(element<HTMLButtonElement>("#read-both").disabled, true);
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
    assert.equal(element("#playback").hidden, true);
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
