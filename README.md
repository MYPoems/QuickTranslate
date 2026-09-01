# QuickTranslate

QuickTranslate 是一个面向 Windows 11 的本地轻量级中英文划词翻译工具。它常驻系统托盘，用户在任意应用中选中文字后按 `Alt + Q`，应用通过一次临时复制获取选区、在本地判断语言、优先查询 SQLite 缓存，再通过 OpenAI-compatible API 翻译并在鼠标附近显示结果。

> 截图占位：`docs/screenshots/popup.png`、`docs/screenshots/settings.png`

## 安装正式版（普通用户）

支持 Windows x64。普通用户不需要安装 Git、Node.js、Rust 或 Visual Studio Build Tools。使用 PowerShell 执行以下命令即可下载、校验并运行 `v0.2.0` 安装程序：

```powershell
$installer = Join-Path $env:TEMP "QuickTranslate_0.2.0_x64-setup.exe"
Invoke-WebRequest "https://github.com/MYPoems/QuickTranslate/releases/download/v0.2.0/QuickTranslate_0.2.0_x64-setup.exe" -OutFile $installer
if ((Get-FileHash $installer -Algorithm SHA256).Hash -ne "A5CFFEF69736BFABECC30DA185C09331FDB023A0202EA2160E2721D575BCECFD") { Remove-Item $installer -Force; throw "安装包校验失败，请勿运行" }
Start-Process $installer -Wait
Remove-Item $installer -Force
```

也可以前往 [Releases](https://github.com/MYPoems/QuickTranslate/releases/latest) 手动下载安装包。安装后从 Windows 开始菜单启动 `QuickTranslate`；应用会常驻系统托盘。当前安装包尚未进行商业代码签名，因此 Windows SmartScreen 可能显示“未知发布者”，请只从本仓库的 Releases 页面下载并核对上述 SHA-256。

## 功能

- 系统托盘：翻译、设置、退出
- 可修改的全局快捷键（默认 `Alt + Q`）
- Windows 临时 `Ctrl + C` 选词，并尽可能恢复原剪贴板全部格式
- 完全本地的中英文检测和文本清洗（最多 5000 字符）
- OpenAI-compatible Provider（Base URL、Model 可配置）
- API Key 保存到 Windows Credential Manager，不写入 JSON 或 SQLite
- SQLite 翻译缓存；缓存失败不影响正常展示
- 鼠标附近的无标题栏、置顶悬浮窗
- 点击悬浮窗以外的位置时自动收起，不打断当前工作流
- 可在设置中启用或关闭 Windows 登录后自动启动
- 单词查询可显示音标、词性、释义与例句
- request ID 并发防护，旧请求不会覆盖新结果
- 浅色/深色自动适配，无前端 UI 框架和轮询

## 开发环境

- Windows 11
- Node.js 20 或更高版本
- Rust stable（MSVC toolchain）
- Visual Studio Build Tools：Desktop development with C++
- WebView2 Runtime（Windows 11 通常已预装）

安装 Tauri 的 Windows 前置依赖可参考 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。

## 开发者：安装前置环境

使用管理员身份打开 PowerShell，依次执行：

```powershell
winget install --id Git.Git -e --source winget --accept-package-agreements --accept-source-agreements
winget install --id OpenJS.NodeJS.LTS -e --source winget --accept-package-agreements --accept-source-agreements
winget install --id Rustlang.Rustup -e --source winget --accept-package-agreements --accept-source-agreements
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --source winget --override "--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" --accept-package-agreements --accept-source-agreements
```

安装完成后，关闭并重新打开 PowerShell。如果安装程序提示需要重启，请先重启 Windows。然后配置 Rust 的 MSVC 工具链并检查环境：

```powershell
rustup default stable-msvc
git --version
node --version
npm --version
rustc --version
cargo --version
```

以上命令应全部输出版本号。如果某个命令仍提示“无法识别”，请再次重开 PowerShell，确认对应程序已加入 `PATH`。

## 从源码安装与运行

确认已安装上述开发环境后，在 PowerShell 中执行：

```powershell
git clone https://github.com/MYPoems/QuickTranslate.git
cd QuickTranslate
npm install
npm run tauri dev
```

如果已经克隆过项目，可在项目目录中执行 `git pull` 获取最新代码，然后运行 `npm install` 和 `npm run tauri dev`。

首次启动后，在系统托盘右键 QuickTranslate → “设置”：

1. 填写 OpenAI-compatible `Base URL`，例如 `https://api.openai.com/v1`。
2. 填写模型名称。
3. 填写 API Key；保存后输入框会清空，Key 仅存在于 Windows Credential Manager。
4. 按需勾选“开机自动启动”。
5. 点击“测试连接”，成功后保存。

然后在 Notepad、Edge/Chrome 或 VS Code 中选中文字，按 `Alt + Q`。

## 检查与构建

```powershell
npm run build
cargo fmt --manifest-path .\src-tauri\Cargo.toml --all -- --check
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path .\src-tauri\Cargo.toml
npm run tauri build
```

安装包位于 `src-tauri\target\release\bundle`。

项目显式随安装包分发 x64 `WebView2Loader.dll`，用于兼容 GNU Windows 构建；正式发布仍推荐使用 Tauri 官方要求的 MSVC 工具链。该 Loader 与 Windows 中的 WebView2 Runtime 是两个不同组件。

如果在没有 MSVC/Windows SDK 的受限环境中使用 GNU 工具链，可用下面的轻量 harness 运行同一批核心源码测试，避免 Tauri 测试进程的 Windows manifest 工具链限制：

```powershell
cargo test --manifest-path .\core-tests\Cargo.toml
```

## 目录

```text
src/
  popup/           悬浮窗 UI
  settings/        设置 UI
  styles/          共享原生 CSS
  main.ts          按 Tauri 窗口标签加载页面
src-tauri/src/
  app.rs           应用状态、翻译触发与并发防护
  commands/        Tauri Commands
  config/          非敏感 JSON 设置
  platform/windows Windows 剪贴板、选词、光标定位
  providers/       OpenAI-compatible Provider
  security/        Windows Credential Manager 抽象
  storage/         SQLite 缓存
  translation/     清洗、语言检测、Prompt、翻译服务和类型
  tray.rs          系统托盘
  window/          悬浮窗和设置窗口生命周期
core-tests/         受限 GNU 环境下复用核心源码测试的 harness
```

运行时数据使用 Tauri 标准应用目录：非敏感设置存为 `settings.json`，缓存存为 `translations.sqlite3`。API Key 不会写入这两个文件。

## 安全与隐私

- 选中文字只会发送给用户配置的 API Provider。
- Release 构建不记录 API Key、Authorization Header 或翻译原文。
- Windows 选词使用 OLE clipboard data object 尝试恢复原始剪贴板格式；如果原应用不再提供延迟渲染数据，恢复仍可能失败。
- 建议只配置可信的 HTTPS Provider。为了支持本机 Ollama/LM Studio，MVP 未强制禁止 HTTP Base URL。

## 已知限制

- 第一版只在 Windows 实现选区读取；macOS/Linux 已保留平台模块边界，但会返回“不支持”。
- 某些管理员权限应用、受保护输入框、游戏或禁用复制的控件无法通过 `Ctrl + C` 读取。
- 全局快捷键冲突时需要在设置中更换组合。
- 悬浮窗高度为可调整的固定初始值，长译文在窗口内部滚动。
- 尚未实现 OCR、历史记录 UI、生词本或离线模型管理。

## Roadmap

1. macOS Accessibility / Linux selection clipboard 平台实现。
2. 可选的本地 Ollama/LM Studio 预设与缓存维护工具。
3. 缓存查看、清理与可选的生词收藏。
