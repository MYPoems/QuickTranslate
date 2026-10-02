import { test } from "node:test";
import assert from "node:assert/strict";
import { PcmPlayer } from "../src/speech/stream";

class Node {
  onended: (() => void) | null = null; playbackRate = { value: 1 }; buffer: unknown;
  stopped = false; at = 0; offset = 0;
  connect() {} disconnect() {}
  start(at: number, offset: number) { this.at = at; this.offset = offset; }
  stop() { this.stopped = true; }
}
const nodes: Node[] = [];
class Context {
  currentTime = 0; destination = {};
  async resume() {}
  createBuffer(_channels: number, length: number, rate: number) { return { length, rate, copyToChannel() {} }; }
  createBufferSource() { const node = new Node(); nodes.push(node); return node; }
}
(globalThis as unknown as { AudioContext: unknown }).AudioContext = Context;
const pcm = Buffer.alloc(480, 0).toString("base64");
test("PCM starts before generation finishes and waits for actual playback end", async () => {
  nodes.length = 0; const player = new PcmPlayer(); let ended = false;
  player.onended = () => { ended = true; };
  player.push(pcm, 24000); assert.equal(nodes.length, 0);
  await player.play(); assert.equal(nodes.length, 1); assert.equal(ended, false);
  player.push(pcm, 24000); assert.equal(nodes.length, 2); assert.ok(nodes[1].at > nodes[0].at);
  player.finish(); assert.equal(ended, false);
  nodes[0].onended?.(); assert.equal(ended, false); nodes[1].onended?.(); assert.equal(ended, true);
  player.dispose();
});
test("pause buffers arriving audio; resume schedules it; stop discards all late audio", async () => {
  nodes.length = 0; const player = new PcmPlayer(); player.push(pcm, 24000); await player.play();
  player.pause(); assert.equal(nodes[0].stopped, true); player.push(pcm, 24000); assert.equal(nodes.length, 1);
  await player.play(); assert.equal(nodes.length, 3); player.dispose();
  assert.ok(nodes.every(node => node.stopped)); player.push(pcm, 24000); assert.equal(nodes.length, 3);
});
test("invalid PCM is rejected and service errors survive prefetched playback", async () => {
  const player = new PcmPlayer(); assert.throws(() => player.push("AA==", 24000));
  assert.throws(() => player.push(pcm, 22050)); player.fail({ message: "认证失败，请检查地域" });
  await assert.rejects(player.play(), /认证失败/); player.dispose();
});
