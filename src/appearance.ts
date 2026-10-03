import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppearancePreferences } from "./types";

export const defaultAppearance: AppearancePreferences = { theme: "system", popupOpacity: 96 };

export function normalizeAppearance(value?: Partial<AppearancePreferences>): AppearancePreferences {
  return {
    theme: value && ["system", "light", "dark"].includes(value.theme || "") ? value.theme! : "system",
    popupOpacity: Number.isFinite(value?.popupOpacity) ? Math.round(Math.min(100, Math.max(70, value!.popupOpacity!))) : 96,
  };
}

export function applyAppearance(value?: Partial<AppearancePreferences>): void {
  const appearance = normalizeAppearance(value);
  document.documentElement.dataset.theme = appearance.theme;
  document.documentElement.style.setProperty("--popup-opacity", `${appearance.popupOpacity}%`);
}

export async function initializeAppearance(): Promise<void> {
  // Register first: a concurrent save must not be overwritten by an older read.
  let changed = false;
  await listen<AppearancePreferences>("appearance-changed", ({ payload }) => {
    changed = true;
    applyAppearance(payload);
  }).catch(() => {}); // A missing event bridge must not prevent the window from mounting.
  try {
    const appearance = await invoke<AppearancePreferences>("get_appearance_preferences");
    if (!changed) applyAppearance(appearance);
  } catch {
    if (!changed) applyAppearance(defaultAppearance);
  }
}
