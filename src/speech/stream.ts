import { Channel, invoke } from "@tauri-apps/api/core";
import type { SpeechAudio, SpeechPreferences } from "../types";
import type { AudioPlayer, PreparedSpeech, ReadSegment } from "./reader";

let context: AudioContext | undefined;
export function unlockSpeechAudio(): void {
  context ??= new AudioContext();
  void context.resume().catch(() => {});
}
interface Packet { kind: "audio" | "done"; audio: string; sampleRate: number; voiceName: string }
interface Chunk { samples: Float32Array; offset: number; node?: AudioBufferSourceNode; start?: number }

/** Bounded PCM queue. Pause preserves unplayed samples, stop discards all buffers. */
export class PcmPlayer implements AudioPlayer {
  errorMessage?: string;
  playbackRate = 1;
  onended: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private chunks: Chunk[] = [];
  private playing = false;
  private disposed = false;
  private completed = false;
  private failed = false;
  private totalSamples = 0;
  private epoch = 0;
  constructor(private sampleRate = 24000) {}
  push(base64: string, sampleRate: number): void {
    if (this.disposed) return;
    if (sampleRate !== this.sampleRate) throw new Error("云端语音采样率不匹配");
    const binary = atob(base64);
    if (!binary.length || binary.length % 2 || this.totalSamples + binary.length / 2 > 8 * 1024 * 1024) throw new Error("云端语音数据超出限制或格式无效");
    const samples = new Float32Array(binary.length / 2);
    for (let i = 0; i < samples.length; i++) { const value = binary.charCodeAt(i * 2) | binary.charCodeAt(i * 2 + 1) << 8; samples[i] = (value >= 32768 ? value - 65536 : value) / 32768; }
    this.totalSamples += samples.length; this.chunks.push({ samples, offset: 0 });
    if (this.playing) this.schedule();
  }
  finish(): void { this.completed = true; this.checkEnded(); }
  fail(error?: unknown): void {
    this.errorMessage = error instanceof Error ? error.message : typeof error === "object" && error !== null && "message" in error ? String(error.message) : typeof error === "string" ? error : "云端语音生成失败";
    this.failed = true; this.pause(); this.onerror?.();
  }
  async play(): Promise<void> {
    if (this.disposed || this.failed) throw new Error(this.errorMessage || "云端语音已停止或生成失败");
    context ??= new AudioContext(); await context.resume();
    if (this.disposed) return;
    this.playing = true; this.schedule(); this.checkEnded();
  }
  pause(): void {
    this.playing = false; this.epoch++;
    for (const chunk of this.chunks) {
      if (chunk.node) {
        const elapsed = Math.max(0, (context?.currentTime ?? 0) - (chunk.start ?? 0));
        chunk.offset = Math.min(chunk.samples.length, chunk.offset + elapsed * this.playbackRate * this.sampleRate);
        chunk.node.onended = null; chunk.node.stop(); chunk.node.disconnect(); chunk.node = undefined;
      }
    }
    this.chunks = this.chunks.filter(chunk => chunk.offset < chunk.samples.length);
  }
  dispose(): void { this.pause(); this.disposed = true; this.chunks = []; this.onended = null; this.onerror = null; }
  private schedule(): void {
    if (!context || !this.playing || this.disposed) return;
    let cursor = context.currentTime + .025;
    const epoch = this.epoch;
    for (const chunk of this.chunks) {
      const duration = (chunk.samples.length - chunk.offset) / this.sampleRate / this.playbackRate;
      if (chunk.node) { cursor = Math.max(cursor, (chunk.start ?? 0) + duration); continue; }
      const buffer = context.createBuffer(1, chunk.samples.length, this.sampleRate); buffer.copyToChannel(chunk.samples, 0);
      const node = context.createBufferSource(); node.buffer = buffer; node.playbackRate.value = this.playbackRate;
      node.connect(context.destination); chunk.node = node; chunk.start = cursor;
      node.onended = () => {
        if (epoch !== this.epoch || this.disposed) return;
        node.disconnect(); this.chunks = this.chunks.filter(value => value !== chunk); this.checkEnded();
      };
      node.start(cursor, chunk.offset / this.sampleRate); cursor += duration;
    }
  }
  private checkEnded(): void {
    if (this.playing && this.completed && !this.chunks.length && !this.disposed) { this.playing = false; this.onended?.(); }
  }
}

export async function prepareSpeech(segment: ReadSegment, voiceId: string, preferences: SpeechPreferences, signal: AbortSignal): Promise<SpeechAudio | PreparedSpeech> {
  if (signal.aborted) throw new Error("朗读已取消");
  if (preferences.provider === "offline") return invoke<SpeechAudio>("synthesize_speech", { text: segment.text, language: segment.language, voiceId });
  const player = new PcmPlayer();
  return new Promise<PreparedSpeech>((resolve, reject) => {
    let ready = false;
    const cancel = () => { player.dispose(); reject(new Error("朗读已取消")); };
    signal.addEventListener("abort", cancel, { once: true });
    const channel = new Channel<Packet>();
    channel.onmessage = packet => {
      if (signal.aborted) return;
      try {
        if (packet.kind === "audio") {
          player.push(packet.audio, packet.sampleRate);
          if (!ready) { ready = true; resolve({ player, voiceName: packet.voiceName }); }
        } else if (packet.kind === "done") player.finish();
      } catch (error) { player.fail(error); reject(error); }
    };
    void invoke("stream_cloud_speech", { text: segment.text, language: segment.language, onAudio: channel })
      .catch(error => { if (!signal.aborted) { player.fail(error); reject(error); } });
    // Listener survives synthesis completion until playback/queue cancellation.
    // Reader.stop aborts this session and frees even prefetched, unplayed audio.
  });
}
