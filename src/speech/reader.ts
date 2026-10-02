import type { SpeechPreferences, SpeechAudio } from "../types";

export type ReadSide = "source" | "translation";
export interface ReadSegment { text: string; side: ReadSide; index: number; language: "zh" | "en" }
export interface ReaderState {
  status: "idle" | "loading" | "playing" | "paused" | "error";
  segment?: ReadSegment;
  position?: number;
  total?: number;
  message?: string;
}
export interface AudioPlayer {
  errorMessage?: string;
  playbackRate: number;
  onended: (() => void) | null;
  onerror: (() => void) | null;
  play(): Promise<void>;
  pause(): void;
  dispose(): void;
}
export const defaultSpeech: SpeechPreferences = { provider: "cloud", cloudEndpoint: "wss://dashscope.aliyuncs.com/api-ws/v1/realtime", cloudModel: "qwen3-tts-flash-realtime", cloudChineseVoice: "Cherry", cloudEnglishVoice: "Cherry", threads: 4, rate: 100, chineseVoice: "", englishVoice: "", bilingual: false };
export interface PreparedSpeech { player: AudioPlayer; voiceName: string }

// Short, Unicode-safe segments keep synthesis responsive even for long OCR results.
export function splitSpeech(text: string, side: ReadSide): ReadSegment[] {
  const pieces: string[] = [];
  for (const paragraph of text.replace(/\r\n?/g, "\n").split(/\n+/)) {
    const sentences = paragraph.trim().match(/[^。！？.!?]+[。！？.!?]*|[。！？.!?]+/gu) || [];
    let chunk = "";
    for (const sentence of sentences) {
      const chars = Array.from(sentence);
      const limit = () => pieces.length === 0 ? (/[\u3400-\u9fff]/u.test(sentence) ? 40 : 120) : 160;
      if (Array.from(chunk + sentence).length > limit() && chunk.trim()) {
        pieces.push(chunk.trim()); chunk = "";
      }
      while (chars.length > limit()) {
        const max = limit();
        // Prefer a word/clause boundary; hard split only unbroken Unicode-safe text.
        let boundary = max;
        for (let i = max - 1; i >= Math.floor(max / 2); i--) { if (/[\s，,；;：:]/u.test(chars[i])) { boundary = i + 1; break; } }
        pieces.push(chars.splice(0, boundary).join("").trim());
      }
      chunk += chars.join("");
      if (pieces.length === 0 && chunk.trim()) { pieces.push(chunk.trim()); chunk = ""; }
    }
    if (chunk.trim()) pieces.push(chunk.trim());
  }
  return pieces.filter(Boolean).map((text, index) => ({
    text, side, index, language: /[\u3400-\u9fff]/u.test(text) ? "zh" : "en",
  }));
}

export class Reader {
  private generation = 0;
  private player?: AudioPlayer;
  private finish?: () => void;
  private state: ReaderState = { status: "idle" };
  private paused = false;
  private cancellation = new AbortController();
  constructor(
    private synthesize: (segment: ReadSegment, voiceId: string, preferences: SpeechPreferences, signal: AbortSignal) => Promise<SpeechAudio | PreparedSpeech>,
    private createAudio: (url: string) => AudioPlayer,
    private changed: (state: ReaderState) => void,
  ) {}
  get active(): boolean { return this.state.status !== "idle" && this.state.status !== "error"; }
  stop(): void {
    this.generation++;
    this.cancellation.abort(); this.cancellation = new AbortController();
    this.paused = false;
    this.cleanup();
    this.emit({ status: "idle" });
  }
  async read(segments: ReadSegment[], preferences: SpeechPreferences): Promise<void> {
    this.stop();
    const generation = this.generation;
    const signal = this.cancellation.signal;
    const prepare = (index: number) => {
      const segment = segments[index];
      return this.synthesize(segment, segment.language === "zh" ? preferences.chineseVoice : preferences.englishVoice, preferences, signal)
        .then(audio => { if (signal.aborted && "player" in audio) audio.player.dispose(); return { audio }; }, error => ({ error }));
    };
    let pending = segments.length ? prepare(0) : undefined;
    for (let position = 0; position < segments.length; position++) {
      if (generation !== this.generation) return;
      const segment = segments[position];
      this.emit({ status: "loading", segment, position: position + 1, total: segments.length });
      try {
        const prepared = await pending!;
        if ("error" in prepared) throw prepared.error;
        const audio = prepared.audio;
        if (generation !== this.generation) return;
        const player = "player" in audio ? audio.player : this.createAudio(audio.audioDataUrl);
        this.player = player;
        player.playbackRate = preferences.rate / 100;
        const done = new Promise<void>((resolve, reject) => {
          this.finish = resolve;
          player.onended = resolve;
          player.onerror = () => reject(new Error(player.errorMessage || "无法播放语音，请检查系统音频设备"));
        });
        // Attach the error handler before starting playback.
        const settled = done.then(() => undefined, (error: unknown) => error);
        this.emit({ ...this.state, status: this.paused ? "paused" : "playing", message: audio.voiceName });
        if (!this.paused) await player.play();
        // Only one lookahead: synthesis and playback overlap, never two audible players.
        pending = position + 1 < segments.length ? prepare(position + 1) : undefined;
        const playbackError = await settled;
        if (generation !== this.generation) return;
        if (playbackError) throw playbackError;
        this.cleanup();
      } catch (error) {
        if (generation !== this.generation) return;
        this.cleanup();
        this.cancellation.abort();
        this.emit({ status: "error", message: error instanceof Error ? error.message : String((error as { message?: string })?.message || error) });
        return;
      }
    }
    if (generation === this.generation) this.stop();
  }
  async togglePause(): Promise<void> {
    if (!this.active) return;
    const generation = this.generation;
    this.paused = !this.paused;
    if (this.paused) this.player?.pause();
    else if (this.player) {
      try { await this.player.play(); }
      catch { if (generation === this.generation) { this.cleanup(); this.emit({ status: "error", message: "无法恢复播放，请重新朗读" }); } return; }
    }
    if (generation !== this.generation) return;
    this.emit({ ...this.state, status: this.paused ? "paused" : this.player ? "playing" : "loading" });
  }
  private cleanup(): void {
    if (this.player) {
      this.player.onended = null; this.player.onerror = null;
      this.player.pause(); this.player.dispose(); this.player = undefined;
    }
    this.finish?.(); this.finish = undefined;
  }
  private emit(state: ReaderState): void { this.state = state; this.changed(state); }
}

export function browserAudio(url: string): AudioPlayer {
  const audio = new Audio(url);
  return {
    get playbackRate() { return audio.playbackRate; },
    set playbackRate(value) { audio.playbackRate = value; },
    get onended() { return audio.onended as (() => void) | null; },
    set onended(value) { audio.onended = value; },
    get onerror() { return audio.onerror as (() => void) | null; },
    set onerror(value) { audio.onerror = value; },
    play: () => audio.play(), pause: () => audio.pause(),
    dispose: () => { audio.removeAttribute("src"); audio.load(); },
  };
}
