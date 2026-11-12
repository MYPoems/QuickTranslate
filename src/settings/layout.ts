import { invoke } from "@tauri-apps/api/core";
import { applyAppearance, normalizeAppearance } from "../appearance";
import type { AppearancePreferences, VocabularyRules, VocabularyView } from "../types";

const groups = [
  ["translation", "翻译服务"], ["ocr", "截图与 OCR"], ["speech", "朗读"],
  ["learning", "学习"], ["general", "通用"], ["maintenance", "更新与数据"],
] as const;

/** Rearranges the flat settings form into a tab list on the left and one panel per category. */
export function organizeSettings(root: HTMLElement, form: HTMLFormElement): void {
  const shell = root.querySelector<HTMLElement>(".settings-shell")!;
  const header = shell.querySelector<HTMLElement>("header")!;
  header.className = "app-header";
  header.innerHTML = '<h1>设置</h1><span class="settings-brand">QuickTranslate</span>';
  const navigation = document.createElement("nav");
  navigation.className = "settings-nav";
  navigation.setAttribute("role", "tablist");
  navigation.setAttribute("aria-orientation", "vertical");
  navigation.setAttribute("aria-label", "设置分类");
  const body = document.createElement("div");
  body.className = "settings-body";
  const panels = document.createElement("div");
  panels.className = "settings-panels";
  const panelMap = new Map<string, HTMLElement>();
  for (const [id, name] of groups) {
    const selected = id === "translation";
    const tab = document.createElement("button");
    tab.type = "button";
    tab.id = `settings-tab-${id}`;
    tab.textContent = name;
    tab.dataset.settingsTab = id;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", String(selected));
    tab.setAttribute("aria-controls", `settings-${id}`);
    tab.tabIndex = selected ? 0 : -1;
    tab.addEventListener("click", () => selectSettingsPanel(form, id));
    navigation.append(tab);
    const panel = document.createElement("section");
    panel.id = `settings-${id}`;
    panel.dataset.settingsPanel = id;
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", tab.id);
    panel.hidden = !selected;
    const heading = document.createElement("h2");
    heading.textContent = name;
    panel.append(heading);
    panels.append(panel);
    panelMap.set(id, panel);
  }
  // Arrow keys move between categories, as in any tab list.
  navigation.addEventListener("keydown", event => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const tabs = Array.from(navigation.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
    const current = tabs.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0
      : event.key === "End" ? tabs.length - 1
      : (current + (event.key === "ArrowDown" ? 1 : -1) + tabs.length) % tabs.length;
    selectSettingsPanel(form, tabs[next].dataset.settingsTab!);
    tabs[next].focus();
    event.preventDefault();
  });

  const move = (selector: string, target: string) => {
    const element = form.querySelector<HTMLElement>(selector);
    if (element) panelMap.get(target)!.append(element);
  };
  const privacyNote = document.createElement("p");
  privacyNote.className = "settings-note";
  privacyNote.textContent = "API Key 仅保存在 Windows 凭据管理器，并且只用于保存时的服务地址；换到另一个地址需要为它单独填写，换回来时原来的 Key 仍然可用。测试连接会向所选服务发送短句，可能产生少量费用。";
  panelMap.get("translation")!.append(privacyNote);
  for (const name of ["provider", "baseUrl", "model", "apiKey", "clearApiKey"]) {
    const label = form.querySelector(`[name="${name}"]`)?.closest("label");
    if (label) panelMap.get("translation")!.append(label);
  }
  move("#key-status", "translation");
  move("#translation-actions", "translation");
  move(".translation-card", "translation");
  const globalShortcut = form.querySelector('[name="globalShortcut"]')!.closest(".shortcut-field")!;
  const ocrShortcut = form.querySelector('[name="ocrShortcut"]')!.closest(".shortcut-field")!;
  panelMap.get("ocr")!.append(ocrShortcut);
  move(".ocr-card:not(.speech-card):not(.translation-card)", "ocr");
  move(".speech-card", "speech");
  form.querySelector('[name="bilingual"]')?.closest("label")?.remove();
  const appearance = document.createElement("section");
  appearance.className = "appearance-card";
  appearance.innerHTML = `<h3>外观</h3><label>主题<select name="appearanceTheme"><option value="system">跟随系统</option><option value="light">浅色</option><option value="dark">深色 · Memo 配色</option></select></label>
    <div class="opacity-label"><label for="popup-transparency">悬浮窗透明度</label><output id="opacity-value" for="popup-transparency">4%</output></div>
    <input id="popup-transparency" name="popupTransparency" type="range" min="0" max="30" step="1" value="4" aria-describedby="opacity-hint">
    <div class="opacity-range"><span>清晰 · 0%</span><span>通透 · 30%</span></div><p id="opacity-hint">仅调整背景，文字、按钮和展开菜单保持清晰。</p>
    <div class="appearance-preview"><div class="appearance-sample"><p class="sample-label">原文</p><p>Stay curious. Keep learning.</p><p class="sample-label">译文</p><p>保持好奇，持续学习。</p></div></div>`;
  panelMap.get("general")!.append(appearance, globalShortcut);
  move(".preference-card", "general");
  panelMap.get("learning")!.insertAdjacentHTML("beforeend", `<p class="settings-note">首次学习不计复习次数；三次有效复习后，间隔到期再进行小测试。通过后归档至已掌握，不自动永久删除。</p>
    <label>最短复习间隔（小时）<input name="intervalHours" type="number" min="4" max="168" value="4" required disabled></label>
    <label>每日复习上限<input name="dailyLimit" type="number" min="1" max="100" value="20" required disabled></label>
    <p class="settings-note">降低间隔不会提前解锁已有计划；延长间隔会延后尚未到期的计划。</p>
    <p id="learning-status" class="settings-note" role="status">正在读取复习规则…</p>`);
  for (const selector of [".maintenance-card", ".diagnostics-card", ".backup-card"]) {
    for (const element of Array.from(form.querySelectorAll<HTMLElement>(selector))) panelMap.get("maintenance")!.append(element);
  }
  const footer = document.createElement("footer");
  footer.className = "settings-footer";
  const status = form.querySelector<HTMLElement>("#status")!;
  const actions = form.querySelector<HTMLElement>(".form-actions")!;
  footer.append(status, actions);
  body.append(navigation, panels);
  form.prepend(body);
  form.append(footer);
  form.noValidate = true;
  const preview = () => {
    const value = appearanceFromForm(form);
    applyAppearance(value);
    root.querySelector("#opacity-value")!.textContent = `${100 - value.popupOpacity}%`;
  };
  form.querySelector('[name="appearanceTheme"]')!.addEventListener("change", preview);
  form.querySelector('[name="popupTransparency"]')!.addEventListener("input", preview);
}

export function selectSettingsPanel(form: HTMLFormElement, id: string): void {
  form.querySelectorAll<HTMLElement>("[data-settings-panel]").forEach(panel => panel.hidden = panel.dataset.settingsPanel !== id);
  form.querySelectorAll<HTMLElement>("[data-settings-tab]").forEach(tab => {
    const selected = tab.dataset.settingsTab === id;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  });
  form.querySelector<HTMLElement>(".settings-panels")!.scrollTop = 0;
}

export function appearanceFromForm(form: HTMLFormElement): AppearancePreferences {
  return normalizeAppearance({ theme: (form.elements.namedItem("appearanceTheme") as HTMLSelectElement).value as AppearancePreferences["theme"], popupOpacity: 100 - Number((form.elements.namedItem("popupTransparency") as HTMLInputElement).value) });
}

export function loadAppearance(form: HTMLFormElement, value?: AppearancePreferences): void {
  const appearance = normalizeAppearance(value);
  (form.elements.namedItem("appearanceTheme") as HTMLSelectElement).value = appearance.theme;
  (form.elements.namedItem("popupTransparency") as HTMLInputElement).value = String(100 - appearance.popupOpacity);
  form.querySelector("#opacity-value")!.textContent = `${100 - appearance.popupOpacity}%`;
  applyAppearance(appearance);
}

const learningInputs = (form: HTMLFormElement) => [
  form.elements.namedItem("intervalHours") as HTMLInputElement,
  form.elements.namedItem("dailyLimit") as HTMLInputElement,
];

/**
 * Reads the review rules into the form. They live with the vocabulary, not in the settings
 * file, so they load separately; until they have, the two fields stay disabled and the rules
 * are left out of saving.
 */
export async function loadLearningRules(form: HTMLFormElement): Promise<VocabularyRules | undefined> {
  const [interval, daily] = learningInputs(form);
  const status = form.querySelector<HTMLElement>("#learning-status")!;
  try {
    const view = await invoke<VocabularyView>("list_vocabulary", { query: "", filter: "learning", offset: 0 });
    if (!view?.rules) throw new Error("复习规则暂时无法读取");
    interval.value = String(view.rules.intervalHours);
    daily.value = String(view.rules.dailyLimit);
    interval.disabled = daily.disabled = false;
    status.textContent = "";
    return view.rules;
  } catch (error) {
    interval.disabled = daily.disabled = true;
    status.textContent = `复习规则暂时无法读取：${String((error as { message?: string })?.message || error)}`;
    return undefined;
  }
}

/** The review rules in the form, or `undefined` while they are not loaded. */
export function learningRulesFromForm(form: HTMLFormElement): VocabularyRules | undefined {
  const [interval, daily] = learningInputs(form);
  if (interval.disabled) return undefined;
  return { intervalHours: Number(interval.value), dailyLimit: Number(daily.value) };
}

/** The first review-rule field with a value outside its range, if any. */
export function invalidLearningField(form: HTMLFormElement): HTMLInputElement | undefined {
  return learningInputs(form).find(input => !input.disabled && !input.validity.valid);
}
