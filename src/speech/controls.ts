import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SpeechPreferences } from "../types";
import { icon } from "../ui/icons";
import { browserAudio, Reader, splitSpeech, type ReaderState } from "./reader";
import { prepareSpeech, unlockSpeechAudio } from "./stream";

export class SpeechControls {
  private revision = 0;
  private active?: HTMLButtonElement;
  private reader: Reader;

  constructor(owner: string, private readonly status: (message: string, error?: boolean) => void) {
    this.reader = new Reader(prepareSpeech, browserAudio, state => this.render(state));
    void listen<string>("speech-stop", ({ payload }) => {
      if (payload !== owner) this.stop(false);
    });
    document.addEventListener("visibilitychange", () => { if (document.hidden) this.stop(); });
    window.addEventListener("pagehide", () => this.stop());
  }

  button(label: string, value: string, id?: string): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "speaker-button";
    button.innerHTML = icon("speaker");
    if (id) button.id = id;
    button.title = `${label}，再次点击停止`;
    button.setAttribute("aria-label", label);
    button.setAttribute("aria-pressed", "false");
    button.disabled = !value.trim();
    button.addEventListener("click", () => void this.read(button, value));
    return button;
  }

  stop(backend = true): void {
    const wasActive = !!this.active;
    this.revision++;
    this.reader.stop();
    this.active?.setAttribute("aria-pressed", "false");
    this.active?.removeAttribute("aria-busy");
    this.active = undefined;
    if (wasActive) this.status("");
    if (backend && wasActive) void invoke("stop_speech").catch(() => {});
  }

  private async read(button: HTMLButtonElement, value: string): Promise<void> {
    unlockSpeechAudio();
    const toggleOff = this.active === button;
    this.stop();
    if (toggleOff) return;
    this.active = button;
    button.setAttribute("aria-pressed", "true");
    button.setAttribute("aria-busy", "true");
    const revision = this.revision;
    try {
      await invoke("stop_speech");
      const preferences = await invoke<SpeechPreferences>("get_speech_preferences");
      if (revision !== this.revision || !button.isConnected) return;
      await this.reader.read(splitSpeech(value, "source"), preferences);
      if (revision === this.revision) {
        button.setAttribute("aria-pressed", "false");
        button.removeAttribute("aria-busy");
        this.active = undefined;
        this.status("朗读完成");
      }
    } catch (error) {
      if (revision === this.revision) {
        this.stop();
        this.status(String((error as { message?: string })?.message || error), true);
      }
    }
  }

  private render(state: ReaderState): void {
    if (state.status === "idle") return; // Reader.read first clears its own previous queue.
    this.active?.setAttribute("aria-busy", String(state.status === "loading"));
    if (state.status === "error") {
      this.stop();
      this.status(state.message || "朗读失败", true);
    } else {
      this.status(state.status === "loading" ? "正在准备朗读…" : "正在朗读，再次点击小喇叭停止");
    }
  }
}
