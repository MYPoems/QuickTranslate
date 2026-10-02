import { test } from "node:test";
import assert from "node:assert/strict";
import { Reader, splitSpeech, defaultSpeech, type AudioPlayer, type ReaderState } from "../src/speech/reader";

class FakeAudio implements AudioPlayer {
  playbackRate = 1; onended: (() => void) | null = null; onerror: (() => void) | null = null;
  playing = false; disposed = false; plays = 0;
  async play() { this.playing = true; this.plays++; }
  pause() { this.playing = false; }
  dispose() { this.disposed = true; }
  end() { this.playing = false; this.onended?.(); }
}
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
function setup() {
  const players: FakeAudio[] = [], states: ReaderState[] = [], voices: string[] = [];
  const reader = new Reader(async (_segment, voice) => { voices.push(voice); return { audioDataUrl: "data:audio/wav;base64,test", voiceName: "test" }; }, () => { const player = new FakeAudio(); players.push(player); return player; }, state => states.push(state));
  return { reader, players, states, voices };
}
test("splits paragraphs, CRLF, long Unicode and punctuation without losing text", () => {
  const text = "Hello world!\r\n你好世界。\n" + "😀".repeat(900);
  const segments = splitSpeech(text, "source");
  assert.equal(segments[0].language, "en"); assert.equal(segments[1].language, "zh");
  assert.ok(segments.every(segment => Array.from(segment.text).length <= 400));
  assert.equal(segments.map(segment => segment.text).join(""), text.replace(/\r?\n/g, ""));
  assert.deepEqual(splitSpeech(" \n ", "source"), []);
});
test("sequential source and translation, language voices and persisted rate", async () => {
  const { reader, players, states, voices } = setup();
  const completion = reader.read([...splitSpeech("Hello", "source"), ...splitSpeech("你好", "translation")], { ...defaultSpeech, rate: 85, chineseVoice: "zh", englishVoice: "en" });
  await tick(); assert.equal(players.length, 1); assert.equal(players[0].playbackRate, .85);
  players[0].end(); await tick(); assert.equal(players.length, 2); assert.ok(players[0].disposed);
  assert.equal(states.at(-1)?.segment?.side, "translation");
  players[1].end(); await completion;
  assert.deepEqual(voices, ["en", "zh"]); assert.equal(reader.active, false);
});
test("pause resumes same segment; stop cleans audio handlers and source", async () => {
  const { reader, players, states } = setup();
  const completion = reader.read(splitSpeech("test", "source"), defaultSpeech);
  await tick(); await reader.togglePause(); assert.equal(players[0].playing, false); assert.equal(states.at(-1)?.status, "paused");
  await reader.togglePause(); assert.equal(players[0].plays, 2);
  reader.stop(); await completion; assert.ok(players[0].disposed); assert.equal(players[0].onended, null);
});
test("new reading cancels previous queue and never overlaps", async () => {
  const { reader, players } = setup();
  const old = reader.read(splitSpeech("one\ntwo", "source"), defaultSpeech); await tick();
  const next = reader.read(splitSpeech("new", "translation"), defaultSpeech); await tick(); await old;
  assert.equal(players.length, 2); assert.ok(players[0].disposed); assert.equal(players.filter(player => player.playing).length, 1);
  players[1].end(); await next;
});
test("stopped synthesis cannot play late audio or overwrite next state", async () => {
  let resolve!: (value: { audioDataUrl: string; voiceName: string }) => void;
  const states: ReaderState[] = []; let played = false;
  const reader = new Reader(() => new Promise(done => { resolve = done; }), () => { played = true; return new FakeAudio(); }, state => states.push(state));
  const completion = reader.read(splitSpeech("old", "source"), defaultSpeech);
  reader.stop(); resolve({ audioDataUrl: "old", voiceName: "old" }); await completion;
  assert.equal(played, false); assert.equal(states.at(-1)?.status, "idle");
});
test("pause during synthesis waits before playing", async () => {
  let resolve!: (value: { audioDataUrl: string; voiceName: string }) => void; const player = new FakeAudio();
  const reader = new Reader(() => new Promise(done => { resolve = done; }), () => player, () => {});
  const completion = reader.read(splitSpeech("test", "source"), defaultSpeech); await reader.togglePause();
  resolve({ audioDataUrl: "audio", voiceName: "test" }); await tick(); assert.equal(player.plays, 0);
  await reader.togglePause(); assert.equal(player.plays, 1); player.end(); await completion;
});
test("synthesis and playback errors stop queue with actionable message", async () => {
  const states: ReaderState[] = [];
  const reader = new Reader(async () => { throw { message: "missing voice" }; }, () => new FakeAudio(), state => states.push(state));
  await reader.read(splitSpeech("one\ntwo", "source"), defaultSpeech); assert.equal(states.at(-1)?.message, "missing voice");
  const { reader: playback, players, states: playbackStates } = setup();
  const completion = playback.read(splitSpeech("test\nnext", "source"), defaultSpeech); await tick(); players[0].onerror?.(); await completion;
  assert.equal(players.length, 1); assert.equal(playbackStates.at(-1)?.status, "error"); assert.ok(players[0].disposed);
});
