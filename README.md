# QuickTranslate

QuickTranslate 是一个面向 Windows 11 的本地轻量级中英文划词与 OCR 翻译工具。它常驻系统托盘：选中文字后按 `Alt + Q` 可划词翻译，按 `Alt + W` 可框选屏幕区域并识别后翻译。OCR 默认使用 Windows 内置能力，也可切换到可选的 PP-OCRv6 Small 本地插件或自备 API Key 的云端视觉模型。

> 截图占位：`docs/screenshots/popup.png`、`docs/screenshots/settings.png`

## 安装正式版（普通用户）

支持 Windows x64。普通用户不需要安装 Git、Node.js、Rust 或 Visual Studio Build Tools。使用 PowerShell 执行以下命令即可下载、校验并运行 `v1.0.0` 安装程序：

```powershell
$installer = Join-Path $env:TEMP "QuickTranslate_1.0.0_x64-setup.exe"
Invoke-WebRequest "https://github.com/MYPoems/QuickTranslate/releases/download/v1.0.0/QuickTranslate_1.0.0_x64-setup.exe" -OutFile $installer
if ((Get-FileHash $installer -Algorithm SHA256).Hash -ne "F8ED1532E7F49BDCC5D84FE7061E04DA6B09E7F8F694A6770D98CDC90280838D") { Remove-Item $installer -Force; throw "安装包校验失败，请勿运行" }
Start-Process $installer -Wait
Remove-Item $installer -Force
```

也可以前往 [Releases](https://github.com/MYPoems/QuickTranslate/releases/latest) 手动下载安装包。安装后从 Windows 开始菜单启动 `QuickTranslate`；应用会常驻系统托盘。当前安装包尚未进行商业代码签名，因此 Windows SmartScreen 可能显示“未知发布者”，请只从本仓库的 Releases 页面下载并核对上述 SHA-256。

## 功能

- 系统托盘：翻译、设置、退出
- 可修改的全局快捷键（默认 `Alt + Q`）
- 独立 OCR 快捷键（默认 `Alt + W`），支持 Windows OCR、PP-OCRv6 Small 和云端视觉 OCR 三种方案
- Windows OCR 默认启用：无需额外下载，并自动放大小字、比较原图与增强图的识别结果
- PP-OCRv6 Small 为可选本地插件：设置页一键下载/卸载约 29.8 MiB 官方模型，并在启用前校验固定大小和 SHA-256
- 云端视觉 OCR 为可选 BYOK 配置：独立 Base URL、模型和 API Key，不与翻译 API Key 混用
- Windows 临时 `Ctrl + C` 选词，并尽可能恢复原剪贴板全部格式
- 完全本地的中英文检测和文本清洗（最多 5000 字符）
- OpenAI-compatible Provider（OpenAI、阿里云百炼、DeepSeek 与自定义端点预设）
- Ollama、LM Studio 本地模型预设，本机连接可不配置 API Key
- API Key 保存到 Windows Credential Manager，不写入 JSON 或 SQLite
- SQLite 翻译缓存；缓存失败不影响正常展示
- 本地翻译历史支持搜索、收藏、复制与删除
- 鼠标附近的无标题栏、置顶悬浮窗；自动避让当前显示器工作区边缘
- 悬浮窗默认尺寸为 `520 × 380`，手动调整后会在下次弹出及重启应用后保持该尺寸
- 点击悬浮窗以外的位置时自动收起，不打断当前工作流
- 悬浮窗可固定显示，并可复制原文、复制译文或绕过缓存重新翻译
- OCR 结果在悬浮窗中同时显示识别文字和译文
- 可在设置中启用或关闭 Windows 登录后自动启动
- 单词查询可显示音标、词性、释义与例句
- request ID 并发防护，旧请求不会覆盖新结果
- 新请求会主动取消仍在进行的旧网络请求
- API 限流、超时及服务端临时故障自动短暂重试
- 翻译缓存最多保留 1000 条，并可在设置中一键清理
- 设置保存失败时自动恢复快捷键、开机启动、凭据和旧配置
- 设置页可复制不包含 API Key 的诊断信息
- 设置页可导出/恢复不含 API Key 的 JSON 备份，并检查 GitHub 正式更新
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

## 回滚到当前稳定版

仓库使用 Git 管理版本，当前稳定基线是 `v1.0.0`。请先保存或提交自己的改动，然后在仓库目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\rollback-to-stable.ps1
```

脚本检测到未提交改动时会拒绝执行，避免误删文件；回滚后处于稳定标签的只读检出状态。需要回到开发主分支时运行：

```powershell
git switch main
```

首次启动后，在系统托盘右键 QuickTranslate → “设置”：

1. 填写 OpenAI-compatible `Base URL`，例如 `https://api.openai.com/v1`。
2. 填写模型名称。
3. 填写 API Key；保存后输入框会清空，Key 仅存在于 Windows Credential Manager。
4. 按需勾选“开机自动启动”。
5. 点击“测试连接”，成功后保存。

然后在 Notepad、Edge/Chrome 或 VS Code 中选中文字，按 `Alt + Q`。对于图片、视频或无法复制的界面，按 `Alt + W` 后拖动框选文字区域。

### OCR 方案

| 方案 | 是否联网 | 配置方式 | 适合场景 |
| --- | --- | --- | --- |
| Windows OCR（默认） | 否 | 无需安装；可选择自动、简体中文或英文 | 日常文字、追求最轻量 |
| PP-OCRv6 Small | 否 | 在设置页选择后点击“一键安装” | 小字、复杂排版、希望图片留在本机 |
| 云端视觉 OCR | 是 | 用户自行填写 Base URL、模型和独立 API Key | 对准确率要求最高、可接受上传所选截图 |

云端方案推荐阿里云百炼 `qwen3.5-ocr`：

- Base URL：`https://dashscope.aliyuncs.com/compatible-mode/v1`
- Model：`qwen3.5-ocr`
- API Key：[前往阿里云百炼控制台申请](https://bailian.console.aliyun.com/?tab=model#/api-key)

云端 OCR 配置完全可选。只有明确选择“云端视觉 OCR”并按 `Alt + W` 框选后，所选截图才会发送到用户配置的服务商；Cloud OCR API Key 单独保存在 Windows Credential Manager。Windows OCR 若提示语言不可用，请在 Windows“语言和区域”中安装对应语言包。

## 检查与构建

每次推送到 `main` 或提交 Pull Request 时，GitHub Actions 会在 Windows 环境自动执行前端构建、Rust 格式检查、Clippy 和测试。本地可运行同一组核心命令：

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
  history/         本地翻译历史 UI
  ocr/             OCR 屏幕选区 UI
  settings/        设置 UI
  styles/          共享原生 CSS
  main.ts          按 Tauri 窗口标签加载页面
src-tauri/src/
  app.rs           应用状态、翻译触发与并发防护
  commands/        Tauri Commands
  config/          非敏感 JSON 设置
  ocr/             云端 OCR 与 PP-OCRv6 Small 插件下载、校验和生命周期
  platform/windows Windows 剪贴板、选词、光标定位、截图和本地 OCR
  providers/       OpenAI-compatible Provider
  security/        Windows Credential Manager 抽象
  storage/         SQLite 缓存
  translation/     清洗、语言检测、Prompt、翻译服务和类型
  tray.rs          系统托盘
  window/          悬浮窗和设置窗口生命周期
core-tests/         受限 GNU 环境下复用核心源码测试的 harness
scripts/            本地发布与安全回滚脚本
docs/               发布检查清单与维护文档
```

运行时数据使用 Tauri 标准应用目录：非敏感设置存为 `settings.json`，缓存存为 `translations.sqlite3`。API Key 不会写入这两个文件。

## 安全与隐私

- 选中文字只会发送给用户配置的 API Provider。
- Windows OCR 与 PP-OCRv6 Small 的截图只在本机内存中处理且不落盘；识别后的文字会发送给用户配置的翻译 Provider。
- 只有用户主动选择云端视觉 OCR 时，所选截图才会发送到其配置的云端 OCR 服务；云端 OCR Key 与翻译 Key 分开保存。
- PP-OCRv6 Small 只从 PaddleOCR 官方模型地址下载，安装前校验文件大小和固定 SHA-256；模型可在设置页一键卸载。
- Release 构建不记录 API Key、Authorization Header 或翻译原文。
- Windows 选词使用 OLE clipboard data object 尝试恢复原始剪贴板格式；如果原应用不再提供延迟渲染数据，恢复仍可能失败。
- 远程 Provider 强制使用 HTTPS；HTTP 仅允许本机回环地址上的 Ollama/LM Studio 等服务。

## 已知限制

- 第一版只在 Windows 实现选区读取；macOS/Linux 已保留平台模块边界，但会返回“不支持”。
- 某些管理员权限应用、受保护输入框、游戏或禁用复制的控件无法通过 `Ctrl + C` 读取。
- 全局快捷键冲突时需要在设置中更换组合。
- Windows OCR 质量取决于已安装语言包、截图清晰度和文字排版；复杂图片可切换 PP-OCRv6 Small 或云端视觉 OCR。
- PP-OCRv6 Small 首次启用需要下载约 29.8 MiB 模型，首次推理还需初始化本地 WebAssembly 运行时，因此会比后续识别慢。
- 云端视觉 OCR 的可用性、费用、数据处理和限额由用户选择的服务商决定。
- 悬浮窗高度为可调整的固定初始值，长原文或译文在窗口内部滚动。
- 当前“检查更新”会定位并复制 GitHub Releases 下载页，不会绕过签名校验静默安装。

## Roadmap

1. macOS Accessibility / Linux selection clipboard 平台实现。
2. 自动更新、设置迁移备份与发布签名。
3. PP-OCRv6 Small 性能基准、更多语言模型及断点续传。
