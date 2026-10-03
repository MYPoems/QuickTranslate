import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/global.css";
import { initializeAppearance } from "./appearance";

const currentWindow = getCurrentWindow();
const label = currentWindow.label;

async function mountWindow(): Promise<void> {
  if (label !== "ocr") await initializeAppearance();

  if (label === "vocabulary") {
    document.body.classList.add("vocabulary-window");
    void import("./vocabulary/vocabulary").then(({ mountVocabulary }) => mountVocabulary());
  } else if (label === "settings") {
    document.body.classList.add("settings-window");
    void import("./settings/settings").then(({ mountSettings }) => mountSettings());
  } else if (label === "history") {
    document.body.classList.add("history-window");
    void import("./history/history").then(({ mountHistory }) => mountHistory());
  } else if (label === "ocr") {
    document.body.classList.add("ocr-window");
    // Keep the escape hatch independent of appearance and optional OCR engines.
    window.addEventListener("keydown", event => {
      if (event.key === "Escape") void currentWindow.hide();
    });
    void import("./ocr/ocr")
      .then(({ mountOcr }) => mountOcr())
      .catch(() => currentWindow.hide());
  } else {
    document.body.classList.add("popup-window");
    void import("./popup/popup").then(({ mountPopup }) => mountPopup());
  }
}

void mountWindow();
