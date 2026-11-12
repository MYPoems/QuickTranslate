import type { DiagnosticsView } from "../types";

// The last selection capture, phase by phase, so a "no selected text" report can be
// narrowed down without asking what was on screen.
export function formatCapture(capture: DiagnosticsView["lastCapture"]): string[] {
  if (!capture) return ["上次划词取词：本次启动后还没有取过词"];
  const yes = (value: boolean) => (value ? "是" : "否");
  const outcome = !capture.keysReleased
    ? "快捷键按住超过 1 秒未松开，没有发送复制"
    : capture.textFound
      ? "取到了文字"
      : capture.clipboardChanged
        ? "剪贴板有变化，但其中没有文字"
        : "目标程序没有响应复制（没有选中文字，或该程序不支持 Ctrl+C）";
  return [
    `上次划词取词：${capture.secondsAgo} 秒前，${outcome}`,
    `  总耗时 ${capture.totalMs} ms，其中等待松开快捷键 ${capture.releaseWaitMs} ms`,
    `  剪贴板有变化：${yes(capture.clipboardChanged)}；由前台程序写入：${yes(capture.writtenByTarget)}`,
    `  恢复原剪贴板：${capture.clipboardRestored == null ? "无需恢复" : capture.clipboardRestored ? "已恢复" : "恢复失败"}`,
  ];
}
