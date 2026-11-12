import { invoke } from "@tauri-apps/api/core";
import type { AppError, TranslationResult, UpdateSettings } from "../types";

/** What the guide needs from the settings form it sits on top of. */
export interface OnboardingHooks {
  /** Provider presets by name, in display order. */
  providers: string[];
  /** Whether the named provider runs on this machine and needs no key. */
  isLocal(provider: string): boolean;
  /** Puts the provider's address and model into the form. */
  chooseProvider(provider: string): void;
  /** Puts the typed key into the form. */
  enterKey(key: string): void;
  formValue(): UpdateSettings;
  /** Saves the form; resolves to whether it succeeded. */
  save(): Promise<boolean>;
}

const sample = "Stay curious. Keep learning.";
const message = (error: unknown) => String((error as AppError)?.message || error || "操作失败");

/**
 * The first-run guide: choose a service, enter and test its key, try one translation.
 * It appears only when the backend says nothing is configured yet, works through the real
 * settings form, and never shows again once finished or skipped.
 */
export async function mountOnboarding(root: HTMLElement, hooks: OnboardingHooks): Promise<void> {
  let wanted = false;
  try { wanted = (await invoke<boolean | undefined>("get_onboarding")) === true; } catch { /* No guide; the settings page still works. */ }
  if (!wanted) return;

  const dialog = document.createElement("dialog");
  dialog.id = "onboarding";
  dialog.setAttribute("aria-labelledby", "onboarding-title");
  dialog.innerHTML = `
    <h2 id="onboarding-title">三步开始使用</h2>
    <ol class="onboarding-steps"><li>选择翻译服务</li><li>填写 API Key</li><li>试一次翻译</li></ol>
    <section data-onboarding-step="1">
      <p>QuickTranslate 使用你自己的模型服务进行翻译，文字只发送给你选择的服务商。</p>
      <label>翻译服务<select id="onboarding-provider"></select></label>
      <p class="settings-note">不确定选哪个时，可以先用阿里云百炼；已经在本机运行 Ollama 或 LM Studio 的话，可以选择它们，不需要 API Key。服务地址和模型之后都能在设置里修改。</p>
      <div class="onboarding-actions"><button id="onboarding-skip" type="button" class="secondary">跳过，稍后自己设置</button><button id="onboarding-next" type="button" class="primary">下一步</button></div>
    </section>
    <section data-onboarding-step="2" hidden>
      <label>API Key<input id="onboarding-key" type="password" autocomplete="off" /></label>
      <p id="onboarding-key-note" class="settings-note"></p>
      <p id="onboarding-status" class="status" role="status"></p>
      <div class="onboarding-actions"><button id="onboarding-back" type="button" class="secondary">上一步</button><button id="onboarding-key-page" type="button" class="secondary">打开 API Key 申请页面</button><button id="onboarding-test" type="button" class="primary">测试并保存</button></div>
    </section>
    <section data-onboarding-step="3" hidden>
      <p>连接成功，设置已保存。现在试一次：</p>
      <blockquote id="onboarding-sample"></blockquote>
      <p id="onboarding-result" class="onboarding-result" role="status"></p>
      <p class="settings-note">之后在任何程序里选中文字，按 <kbd id="onboarding-shortcut"></kbd> 就会弹出译文。</p>
      <div class="onboarding-actions"><button id="onboarding-try" type="button" class="secondary">翻译这句话</button><button id="onboarding-done" type="button" class="primary">完成</button></div>
    </section>`;
  root.append(dialog);
  const find = <T extends HTMLElement>(selector: string) => dialog.querySelector<T>(selector)!;
  const provider = find<HTMLSelectElement>("#onboarding-provider");
  for (const name of hooks.providers) provider.add(new Option(name, name));
  provider.value = hooks.providers.includes("阿里云百炼") ? "阿里云百炼" : hooks.providers[0];
  find("#onboarding-sample").textContent = sample;

  const steps = Array.from(dialog.querySelectorAll<HTMLElement>(".onboarding-steps li"));
  const show = (step: number) => {
    dialog.querySelectorAll<HTMLElement>("[data-onboarding-step]").forEach(section => section.hidden = Number(section.dataset.onboardingStep) !== step);
    steps.forEach((item, index) => { if (index + 1 === step) item.setAttribute("aria-current", "step"); else item.removeAttribute("aria-current"); item.dataset.done = String(index + 1 < step); });
    dialog.querySelector<HTMLElement>(`[data-onboarding-step="${step}"] input, [data-onboarding-step="${step}"] select, [data-onboarding-step="${step}"] .primary`)?.focus();
  };
  const status = (text: string, kind: "neutral" | "error" | "success" = "neutral") => { const line = find("#onboarding-status"); line.textContent = text; line.dataset.kind = kind; };
  const busy = (value: boolean) => dialog.querySelectorAll<HTMLButtonElement>("button").forEach(button => button.disabled = value);
  const finish = async () => { try { await invoke("complete_onboarding"); } catch { /* Shown again next time at worst. */ } dialog.close(); };

  find("#onboarding-skip").addEventListener("click", () => void finish());
  find("#onboarding-done").addEventListener("click", () => void finish());
  find("#onboarding-back").addEventListener("click", () => show(1));
  find("#onboarding-key-page").addEventListener("click", () => void invoke("open_api_key_page").catch(error => status(message(error), "error")));
  find("#onboarding-next").addEventListener("click", () => {
    hooks.chooseProvider(provider.value);
    const local = hooks.isLocal(provider.value);
    find("#onboarding-key-note").textContent = local
      ? "本机模型不需要 API Key，可以留空。请确认它已经启动。"
      : "API Key 只保存在这台电脑的 Windows 凭据管理器里，并且只会发送给这个服务地址。测试会发送一个短句，可能产生极少量费用。";
    find<HTMLButtonElement>("#onboarding-key-page").hidden = provider.value !== "阿里云百炼";
    status("");
    show(2);
  });
  find("#onboarding-test").addEventListener("click", async () => {
    const key = find<HTMLInputElement>("#onboarding-key").value.trim();
    if (!key && !hooks.isLocal(provider.value)) { status("请先填写 API Key", "error"); find("#onboarding-key").focus(); return; }
    hooks.enterKey(key);
    busy(true);
    status("正在测试连接…");
    try {
      await invoke("test_provider", { update: hooks.formValue() });
      if (!(await hooks.save())) { status("连接成功，但保存设置失败。请关闭引导，在设置页查看原因。", "error"); return; }
      const shortcut = hooks.formValue().globalShortcut.split("+").map(part => part.trim()).filter(Boolean).join(" + ");
      find("#onboarding-shortcut").textContent = shortcut || "Alt + Q";
      find("#onboarding-result").textContent = "";
      show(3);
    } catch (error) {
      status(message(error), "error");
    } finally {
      busy(false);
    }
  });
  find("#onboarding-try").addEventListener("click", async () => {
    const result = find("#onboarding-result");
    busy(true);
    result.dataset.kind = "neutral";
    result.textContent = "正在翻译…";
    try {
      const translated = await invoke<TranslationResult>("translate_text", { text: sample });
      result.textContent = translated?.translation || "没有收到译文";
      result.dataset.kind = "success";
    } catch (error) {
      result.textContent = message(error);
      result.dataset.kind = "error";
    } finally {
      busy(false);
    }
  });
  // Esc closes a dialog; treat it as "skip" so the guide does not come back.
  dialog.addEventListener("cancel", event => { event.preventDefault(); void finish(); });
  dialog.showModal();
  show(1);
}
