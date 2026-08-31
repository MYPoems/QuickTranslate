import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/global.css";

const label = getCurrentWindow().label;

if (label === "settings") {
  document.body.classList.add("settings-window");
  void import("./settings/settings").then(({ mountSettings }) => mountSettings());
} else {
  document.body.classList.add("popup-window");
  void import("./popup/popup").then(({ mountPopup }) => mountPopup());
}
