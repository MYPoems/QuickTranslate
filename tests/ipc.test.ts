import { test } from "node:test";
import assert from "node:assert/strict";
import { build } from "esbuild";
import { readFile } from "node:fs/promises";

// Each window may call only the backend commands its capability file grants. Three lists
// have to agree for that to hold: the handlers registered in lib.rs, the commands put
// under access control in build.rs, and the grants in capabilities/. A command missing
// from a grant only fails at run time, in the window that needed it, so the lists are
// checked here against what each window's code actually calls.

const windows: Record<string, string> = {
  popup: "src/popup/popup.ts",
  settings: "src/settings/settings.ts",
  history: "src/history/history.ts",
  vocabulary: "src/vocabulary/vocabulary.ts",
  ocr: "src/ocr/ocr.ts",
};

async function registeredCommands(): Promise<string[]> {
  const lib = await readFile("src-tauri/src/lib.rs", "utf8");
  const start = lib.indexOf("generate_handler![");
  const handler = lib.slice(start, lib.indexOf("])", start));
  return [...handler.matchAll(/commands::\w+::(\w+)/g)].map(match => match[1]);
}

async function controlledCommands(): Promise<string[]> {
  const script = await readFile("src-tauri/build.rs", "utf8");
  const start = script.indexOf("const COMMANDS");
  const list = script.slice(start, script.indexOf("];", start));
  return [...list.matchAll(/"(\w+)"/g)].map(match => match[1]);
}

async function grantedCommands(label: string): Promise<string[]> {
  const capability = JSON.parse(await readFile(`src-tauri/capabilities/${label}-commands.json`, "utf8"));
  assert.deepEqual(capability.windows, [label], `${label}-commands.json applies to that window only`);
  return (capability.permissions as string[]).map(permission => {
    assert.match(permission, /^allow-[a-z-]+$/, permission);
    return permission.slice("allow-".length).replaceAll("-", "_");
  });
}

/** The commands a window's code can call: every registered command name in its bundle. */
async function calledCommands(label: string, registered: string[]): Promise<string[]> {
  // The overlay deliberately loads without the appearance module; every other window has it.
  const entryPoints = label === "ocr" ? [windows[label]] : [windows[label], "src/appearance.ts"];
  const output = await build({ entryPoints, bundle: true, write: false, format: "esm", platform: "browser", outdir: "unused", loader: { ".css": "empty" }, external: ["@tauri-apps/api/*", "@paddleocr/paddleocr-js"], logLevel: "silent" });
  const text = output.outputFiles.map(file => file.text).join("\n");
  return registered.filter(name => new RegExp(`["'\`]${name}["'\`]`).test(text));
}

test("every registered command is under access control, and nothing else is", async () => {
  const registered = await registeredCommands();
  assert.ok(registered.length > 40, "the handler list was found");
  assert.equal(new Set(registered).size, registered.length, "no command is registered twice");
  assert.deepEqual([...await controlledCommands()].sort(), [...registered].sort());
});

test("each window is granted exactly the commands its code calls", async () => {
  const registered = await registeredCommands();
  for (const label of Object.keys(windows)) {
    const granted = (await grantedCommands(label)).sort();
    const called = (await calledCommands(label, registered)).sort();
    assert.deepEqual(granted.filter(name => !registered.includes(name)), [], `${label}: grants for commands that do not exist`);
    assert.deepEqual(called.filter(name => !granted.includes(name)), [], `${label}: called but not granted, would fail at run time`);
    assert.deepEqual(granted.filter(name => !called.includes(name)), [], `${label}: granted but never called`);
  }
});

test("windows do not inherit commands from a shared capability", async () => {
  const shared = JSON.parse(await readFile("src-tauri/capabilities/default.json", "utf8"));
  assert.deepEqual(shared.permissions.filter((permission: string) => !permission.startsWith("core:")), []);
  const popup = await grantedCommands("popup");
  for (const sensitive of ["save_settings", "get_settings", "install_update", "clear_translation_cache", "export_vocabulary"]) {
    assert.equal(popup.includes(sensitive), false, `the popup cannot call ${sensitive}`);
  }
});
