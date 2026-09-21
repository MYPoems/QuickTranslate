import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { OcrRegionResult } from "../types";
import "./ocr.css";

type PaddleModule = typeof import("@paddleocr/paddleocr-js");
type PaddleEngine = Awaited<ReturnType<PaddleModule["PaddleOCR"]["create"]>>;

const root = document.querySelector<HTMLElement>("#app")!;
let startX = 0;
let startY = 0;
let dragging = false;
let submitting = false;
let paddleOcr: Promise<PaddleEngine> | undefined;
let paddleModelKey = "";

export function mountOcr(): void {
  root.innerHTML = `
    <main id="ocr-surface" class="ocr-surface">
      <div class="ocr-instructions">
        <strong>拖动鼠标选择要识别的文字区域</strong>
        <span>松开后自动识别并翻译 · Esc 取消</span>
      </div>
      <div id="ocr-selection" class="ocr-selection" hidden></div>
      <p id="ocr-status" class="ocr-status" hidden>正在识别…</p>
    </main>`;
  const surface = root.querySelector<HTMLElement>("#ocr-surface")!;
  surface.addEventListener("pointerdown", startSelection);
  surface.addEventListener("pointermove", moveSelection);
  surface.addEventListener("pointerup", finishSelection);
  surface.addEventListener("pointercancel", cancelSelection);
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void hide();
  });
  void getCurrentWindow().onFocusChanged(({ payload }) => {
    if (payload) reset();
  });
}

function startSelection(event: PointerEvent): void {
  if (event.button !== 0 || submitting) return;
  startX = event.clientX;
  startY = event.clientY;
  dragging = true;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  updateSelection(startX, startY, startX, startY);
}

function moveSelection(event: PointerEvent): void {
  if (!dragging) return;
  updateSelection(startX, startY, event.clientX, event.clientY);
}

function finishSelection(event: PointerEvent): void {
  if (!dragging) return;
  dragging = false;
  const left = Math.max(0, Math.min(startX, event.clientX));
  const top = Math.max(0, Math.min(startY, event.clientY));
  const right = Math.min(window.innerWidth, Math.max(startX, event.clientX));
  const bottom = Math.min(window.innerHeight, Math.max(startY, event.clientY));
  const width = right - left;
  const height = bottom - top;
  if (width < 8 || height < 8) {
    root.querySelector<HTMLElement>("#ocr-selection")!.hidden = true;
    return;
  }
  submitting = true;
  root.querySelector<HTMLElement>(".ocr-instructions")!.hidden = true;
  root.querySelector<HTMLElement>("#ocr-status")!.hidden = false;
  void recognize({ x: left, y: top, width, height })
    .catch(async () => {
      // Never leave the full-screen input-capturing overlay open after an IPC failure.
      await hide();
    })
    .finally(() => {
      submitting = false;
    });
}

async function recognize(region: {
  x: number;
  y: number;
  width: number;
  height: number;
}): Promise<void> {
  const result = await invoke<OcrRegionResult>("recognize_ocr_region", { region });
  if (result.kind !== "paddle") return;

  try {
    const engine = await getPaddleEngine(
      result.detectionModelPath,
      result.recognitionModelPath,
    );
    const response = await fetch(result.imageDataUrl);
    const image = await response.blob();
    const [prediction] = await engine.predict(image, { textRecScoreThresh: 0.35 });
    const text = prediction.items
      .filter((item) => item.score >= 0.35 && item.text.trim())
      .map((item) => item.text.trim())
      .join("\n");
    await invoke("complete_paddle_ocr", { requestId: result.requestId, text });
  } catch (error) {
    await invoke("fail_paddle_ocr", {
      requestId: result.requestId,
      message: error instanceof Error ? error.message : "本地模型初始化失败",
    });
  }
}

function getPaddleEngine(
  detectionModelPath: string,
  recognitionModelPath: string,
): Promise<PaddleEngine> {
  const key = `${detectionModelPath}\n${recognitionModelPath}`;
  if (!paddleOcr || paddleModelKey !== key) {
    paddleModelKey = key;
    paddleOcr = createPaddleEngine(detectionModelPath, recognitionModelPath);
  }
  return paddleOcr;
}

async function createPaddleEngine(
  detectionModelPath: string,
  recognitionModelPath: string,
): Promise<PaddleEngine> {
  // Keep the heavy OpenCV/ONNX runtime out of the default Windows OCR overlay path.
  const { PaddleOCR } = await import("@paddleocr/paddleocr-js");
  return PaddleOCR.create({
    textDetectionModelName: "PP-OCRv6_small_det",
    textDetectionModelAsset: { url: convertFileSrc(detectionModelPath) },
    textRecognitionModelName: "PP-OCRv6_small_rec",
    textRecognitionModelAsset: { url: convertFileSrc(recognitionModelPath) },
    textRecognitionBatchSize: 6,
    ortOptions: {
      backend: "wasm",
      numThreads: 1,
      simd: true,
    },
  });
}

function cancelSelection(): void {
  dragging = false;
  root.querySelector<HTMLElement>("#ocr-selection")!.hidden = true;
}

function updateSelection(x1: number, y1: number, x2: number, y2: number): void {
  const selection = root.querySelector<HTMLElement>("#ocr-selection")!;
  selection.hidden = false;
  selection.style.left = `${Math.min(x1, x2)}px`;
  selection.style.top = `${Math.min(y1, y2)}px`;
  selection.style.width = `${Math.abs(x2 - x1)}px`;
  selection.style.height = `${Math.abs(y2 - y1)}px`;
}

function reset(): void {
  dragging = false;
  submitting = false;
  root.querySelector<HTMLElement>("#ocr-selection")!.hidden = true;
  root.querySelector<HTMLElement>(".ocr-instructions")!.hidden = false;
  root.querySelector<HTMLElement>("#ocr-status")!.hidden = true;
}

async function hide(): Promise<void> {
  dragging = false;
  await invoke("hide_ocr_window");
}
