import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/global.css";

const currentWindow = getCurrentWindow();
const label = currentWindow.label;

if (label === "settings") {
  document.body.classList.add("settings-window");
  void import("./settings/settings").then(({ mountSettings }) => mountSettings());
} else if (label === "history") {
  document.body.classList.add("history-window");
  void import("./history/history").then(({ mountHistory }) => mountHistory());
} else if (label === "ocr") {
  document.body.classList.add("ocr-window");
  // Register the escape hatch before loading any OCR-specific code. If a future
  // optional engine fails during module initialization, the desktop remains usable.
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void currentWindow.hide();
  });
  void import("./ocr/ocr")
    .then(({ mountOcr }) => mountOcr())
    .catch(() => currentWindow.hide());
} else {
  document.body.classList.add("popup-window");
  void import("./popup/popup").then(({ mountPopup }) => mountPopup());
}
