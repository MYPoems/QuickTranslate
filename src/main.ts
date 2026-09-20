import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/global.css";

const label = getCurrentWindow().label;

if (label === "settings") {
  document.body.classList.add("settings-window");
  void import("./settings/settings").then(({ mountSettings }) => mountSettings());
} else if (label === "history") {
  document.body.classList.add("history-window");
  void import("./history/history").then(({ mountHistory }) => mountHistory());
} else if (label === "ocr") {
  document.body.classList.add("ocr-window");
  void import("./ocr/ocr").then(({ mountOcr }) => mountOcr());
} else {
  document.body.classList.add("popup-window");
  void import("./popup/popup").then(({ mountPopup }) => mountPopup());
}
