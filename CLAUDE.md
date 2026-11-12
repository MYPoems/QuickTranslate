# QuickTranslate — Claude 项目交接

交接快照：2026-11-12。面向接手本仓库的 Claude 或其他开发者。版本、行为和安全边界以当前源码、测试和可核实的产物为准；旧版 README 小节、QA 文档和发布说明保留作历史证据，不代表现行界面。开始任务时先读本文件，再按模块查源码，不必一次加载全部历史文档。

## 1. 从哪里接手

- **产品**：面向 Windows 11 x64 的轻量翻译工具（以中英文为主，源码已支持日、韩、法、德、西、俄），系统托盘常驻。默认 `Alt+Q` 划词翻译，`Alt+W` 框选 OCR；可自定义两组快捷键。应用本地管理设置、历史、词卡和插件，翻译仍取决于配置的模型服务；“本地应用”不意味着默认所有推理都离线。
- **技术栈**：Tauri 2 + Rust/Tokio、Vanilla TypeScript + Vite + 原生 CSS、SQLite（rusqlite bundled）、Windows WebView2、Windows Credential Manager。保持小、快、事件驱动；不要为小修复引入前端框架、长期轮询、全局键盘钩子或常驻服务。
- **版本锚点**：写本文前，本地分支 `codex/collector-ui-fix`、远端 `main` 和 `v1.6.2` 标签指向 `e88e2a0cbcd3e3d936ea38ed6fe378abb481ea36`（标签解引用后）；此后该分支新增了取词修复、代码审查修复和本文件的提交，已推送并在 PR #1 中等待合并。**本地分支 `iteration/plan-2026-11`**（基于 PR #1 的分支顶端，未推送、未开 PR）实现了 [迭代计划](docs/ITERATION-PLAN.md) 的 1.6.3–1.9.0 条目，逐项状态和未验证的部分见该文档第 12 节；本文件第 3、4 节描述的是这个分支的源码，与已发布的 v1.6.2 的差异见 `CHANGELOG.md` 的 Unreleased。五处版本号仍是 1.6.2。远端是 [MYPoems/QuickTranslate](https://github.com/MYPoems/QuickTranslate)。接手时重新运行 `git status --short --branch`、`git log -5 --oneline`、`git tag --points-at HEAD`；不要假设本地 `main` 同步，也不要覆盖工作区已有改动。
- **正式版**：[v1.6.2 Release](https://github.com/MYPoems/QuickTranslate/releases/tag/v1.6.2) 为交接时的最新正式版，非草稿、非预发行。此前已覆盖安装本机版并校验数据保留；本轮只写交接，不代表此刻程序仍在运行，也未重新安装。
- **优先待办**：Word/Excel 划词显示“未检测到选中文字”，但手工复制正常。根因已确认并在源码修复（2026-11-11，提交 `f14b3a6`，在 PR #1 中，未合并、未打包、未安装，已安装的 1.6.2 仍有此问题）。待随下一版本构建后在真机验收，见第 6 节。其次是决定 `iteration/plan-2026-11` 的去向：推送、评审、合并顺序（它包含 PR #1 的全部提交）、版本号和发布，都需要用户授权；该分支的改动同样未打包、未安装，也没有对真实的翻译、OCR、朗读服务测试过。
- **操作授权**：分析请求先诊断；明确要求修复才改源码。改源码、构建、替换本机安装、推送、公开 Release 是不同操作，按当前用户授权执行。不要把过去授权当成持续授权。现有用户数据、凭据和未提交改动都要保留。

## 2. 目录和调用边界

| 任务 | 首先看 |
| --- | --- |
| 运行入口、托盘、快捷键、窗口事件 | `src-tauri/src/main.rs`、`lib.rs`、`tray.rs`；`main.rs` 的 `--speech-worker` 是内部进程入口，`--vocabulary` 可从应用入口打开生词本 |
| 前端窗口路由、类型和样式 | `src/main.ts` 按 Tauri 窗口标签加载 `popup` / `settings` / `history` / `vocabulary` / `ocr`；`src/types.ts`、`src/appearance.ts`、`src/styles/global.css` |
| IPC / 安全 | `src-tauri/src/commands/`、`lib.rs` 的 invoke 注册、`src-tauri/build.rs` 的 `COMMANDS` 清单、`src-tauri/capabilities/<窗口>-commands.json`（每个窗口只授权它实际调用的命令）、`tests/ipc.test.ts`、`src-tauri/tauri.conf.json` 的窗口配置、CSP、资源范围 |
| 划词、翻译与错误 | `src-tauri/src/app.rs`、`platform/mod.rs`、`platform/windows/{selection,clipboard}.rs`、`translation/`、`providers/openai_compatible.rs`、`errors.rs` |
| 悬浮窗行为与定位 | `src/popup/`、`src-tauri/src/window/mod.rs`、`window_state.rs`、`popup_focus.rs`、`platform/{placement.rs,windows/cursor.rs}` |
| OCR | `src/ocr/`、`src-tauri/src/commands/ocr.rs`、`platform/windows/ocr.rs`、`ocr/{cloud,plugin}.rs` |
| 朗读 | `src/speech/{controls,reader,stream}.ts`、`src-tauri/src/commands/speech.rs`、`speech.rs`、`speech_{cloud,plugin,worker,cache}.rs` |
| 历史与生词本 | `src/history/`、`src/vocabulary/`、`src-tauri/src/storage/cache.rs`、`vocabulary.rs`、`vocabulary_model.rs`、`commands/vocabulary.rs`、`vocabulary_tests.rs` |
| 设置、凭据、升级 | `src/settings/`、`src-tauri/src/config/`、`security/secret_store.rs`、`commands/{settings,update}.rs`、`update.rs`、`upgrade.rs` |
| 测试与发布 | `tests/`、各 Rust 模块的测试、`core-tests/`、`tools/selection-harness/`（取词原生回归，手动运行）、`scripts/`、`.github/workflows/`、`docs/` |

路径表中省略的后端路径均相对于 `src-tauri/src/`。增改 IPC 时同时检查 Rust 序列化、TS 类型、调用方和窗口权限：新命令要同时加进 `lib.rs` 的 handler、`build.rs` 的 `COMMANDS` 和调用它的窗口的 capability 文件，否则 `tests/ipc.test.ts` 失败，运行时该窗口的调用也会被拒绝。敏感操作、持久状态和校验留在 Rust，不信任前端提供的状态或时间。

## 3. 当前功能与不得破坏的约束

### 划词、翻译与历史

- 全局快捷键 → `app::trigger_selected_translation` → 阻塞线程取词 → 清洗（最多 5000 字符）、本地判定语言并按设置选目标语言 → SQLite 缓存 → OpenAI-compatible Provider → 结果写入缓存和历史并以 `translation-state` 事件送到悬浮窗。悬浮窗的手动输入（`translate_input`）走同一条路径，只是不取词。
- 句子和段落以流式请求（`stream: true`）翻译，增量经 `translation-delta` 事件（最多每 50 ms 一次，带 `requestId`）送到悬浮窗；超时是“30 秒内开始响应 + 段间 30 秒空闲 + 总上限 600 秒”。服务不支持流式或流在出字前失败时退回一次性请求；单词的结构化释义始终一次性返回。增量只用于显示，写入缓存和历史的是完整结果。
- 语言：`translation/language.rs` 按文字系统和常用词判断中、英、日、韩、法、德、西、俄，短文本可能判错；`TranslationPreferences`（设置里的可选块，Schema 仍为 5）保存“我阅读的语言”、另一种语言、术语表（最多 200 条）和附加要求（最多 600 字）。只把出现在原文里的术语发给模型。悬浮窗菜单可为当前文本临时指定目标语言。没有对应音色的语言不显示朗读。API、取词、翻译空结果应分别诊断。新请求取消旧请求，`request_id` 防止旧结果覆盖新界面；取消异步等待不等于中止已开始的阻塞系统调用。
- 缓存失败不应阻断翻译展示，翻译缓存最多 1000 条；支持重译绕过缓存。历史是独立的表 `translation_history`（最近 5000 条，收藏不计入也不被清理），可搜索、收藏、复制、删除和清空（破坏性操作需确认，清空默认保留收藏）；清理缓存不动历史。从历史打开旧译文只读本地结果，不重新调用模型。旧库首次打开时自动拆分。
- 默认翻译配置是 `https://api.openai.com/v1` + `gpt-4.1-mini`，不是已配置凭据。设置提供 OpenAI、阿里云百炼、DeepSeek、自定义及 Ollama / LM Studio 预设。Base URL 填 API 根路径，不追加完整 `/chat/completions`；实际模型权限和额度由服务商决定。远程使用 HTTPS，普通 HTTP 仅允许本机回环。回环模型可以无 Key；不要把某个用户私有配置写成项目默认。
- Provider 对 429、5xx、连接/超时错误有限重试，401 不应当作网络重试。遇到 429 先区分调用频率、账户额度和计费限制，不要一味增加重试。单个英文词会走结构化词义提示，和持久生词本的词卡生成是两条路径。翻译缓存键含文本、语言方向、Provider 名、Model、服务地址以及术语表和附加要求的摘要；改其中任何一项都不会命中旧译文。

### OCR：云端优先，离线为显式替代

- `Alt+W` 打开鼠标所在显示器的选区。坐标需要考虑显示器原点、DPI 和逻辑/物理像素转换；截图前隐藏遮罩。必须保留 Esc、失焦处理及初始化失败时的退出路径，尤其不能因 OCR 初始化或 WebView 问题把屏幕遮罩留在最上层。
- **云端视觉 OCR** 是新安装默认选择。内置预设为阿里云百炼 `https://dashscope.aliyuncs.com/compatible-mode/v1` + `qwen3.5-ocr`；不与翻译共用 Key，未配置时不上传截图。只有选定该模式并主动框选才上传区域图像。云端 PNG 上限是 7,000,000 字节：超过时自动缩小后再传，需要缩到一半以下则报错并提示改选更小的区域。云端请求超时 90 秒，连接失败重试一次，识别中可取消。设置里的“测试连接”只上传内置的小图片。旧设置的既有引擎选择应保留。
- **Windows OCR** 是内置离线方案，依赖 Windows 语言包，有自动 / 简中 / 英文模式，对小字使用增强与候选比较。**PP-OCRv6 Small** 是约 29.8 MiB 的可选本地插件，用户一键安装；下载核对固定来源、大小和 SHA-256，由前端 WASM 推理。禁止自动下载、静默更换引擎或为离线模式上传截图。离线 OCR 只保证识别图像不上传，后续翻译仍使用配置的 Provider。
- OCR 悬浮窗同时显示识别原文和译文；原文可编辑并重译，菜单里可“重新框选”。识别、重译、取消及迟到结果要保持请求一致性。

### 朗读：独立控件、云端默认、离线可选

- 当前 v1.6.2 的小喇叭跟在“原文”“译文”旁，分别朗读、再点停止；**没有旧 README 所述的底部“双语跟读”按钮**。历史原文/译文、生词本单词、来源句和例句也有独立朗读。新翻译、编辑 OCR 原文、隐藏窗口或新播放应停止旧队列，不能播放迟到音频或叠音。
- 云端是默认模式：百炼 `qwen3-tts-flash-realtime`，默认 `Cherry`，使用独立 TTS Key 和地域对应的 Realtime WebSocket。北京 `wss://dashscope.aliyuncs.com/api-ws/v1/realtime`，新加坡 `wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime`。这是源码预设，不保证服务商端点、账号权限或价格永久不变；不要把翻译 Base URL 填到这里。
- 流式播放、短首段、单段预生成、连接复用和受限内存缓存减少等待；生产延迟需实测，不承诺固定首声时间。云端失败给明确提示，不悄悄切到离线。测试连接也会合成测试句，可能产生费用。
- 离线支持 Windows 系统音色及 Kokoro 中英文插件（约 164 MiB，sherpa-onnx x64 CPU）。插件用工作进程复用模型，经匿名管道传输，空闲退出；不在主 UI 进程加载第三方语音 DLL。校验下载、解压和加载完整性；禁止绕过哈希校验。调整分发前检查 eSpeak NG GPL 及其他第三方许可/源码义务，独立进程并不豁免。见 [音色插件说明](docs/SPEECH-PLUGIN.md)。

### 生词本与悬浮窗体验

- 单词收藏和句内批量收藏（每次最多 20 个），可手动输入。仅在用户选择生成时用当前翻译模型发送词及必要上下文，缓存英美 IPA、词性、释义、例句、搭配和题库；失败保留词条。基本词形由用户确认，大小写去重；AI 词卡不是权威字典，不编造音标。词卡生成并发上限 2。
- 首次学习不计次；默认至少 4 小时间隔，三次“记得”后再等一个间隔才可做释义和拼写测试（最早 16 小时）。测试通过归档“已掌握”，不是删除；失败回到 2/3。后端校验到期、每日上限（按本机时区的自然日）、版本与幂等，离线学习无需调用模型。复习可全程用键盘。JSON 用于合并备份与恢复，CSV 用于阅读；清理翻译历史不应触及生词本。见 [生词本规则](docs/VOCABULARY.md)。
- 悬浮窗默认 `520 × 380` 逻辑像素，最小 `360 × 180`、最大 `960 × 720`；用户拉伸尺寸持久化。显示位置跟随鼠标所在显示器并避让工作区边缘。无外框、透明圆角；顶部**仅中间空白区**原生拖动，普通箭头、无 tooltip；标题、按钮、缩放边缘/角落不能被拖动区覆盖。
- 悬浮窗的模式和语言方向并在标题行；长原文默认折叠，窗口矮时原文缩为一行，保证最小尺寸也能看到译文；错误状态带下一步按钮；固定状态与窗口尺寸一起持久化。改布局后在 360 × 180 下复查。
- 未固定窗真正离开时收起，固定或收藏弹层开启时保留。原生拖动/缩放的 WebView 临时失焦不是离开；v1.6.2 使用 Windows 前台顶层窗口判定，勿改回一失焦就隐藏。收藏弹层在 popup 内用圆角壳裁切遮罩，避免四个暗角；保留生词本独立窗口原有模态行为。
- Memo 浅色/深蓝色主题可跟随系统；悬浮窗只调背景不透明度（70–100%，默认 96%），文字、操作控件及菜单仍可读。底栏保留复制译文和“更多”，避免新增功能挤满底栏。设置按功能分类，含可选登录后自动启动，所有分类共用一个保存；快捷键用录制控件录入。校验错误用 `AppError::InvalidSetting { field, message }` 带字段名返回，设置页显示原因并定位到字段。保存设置失败需回滚配置、快捷键、自启和凭据变更。首次启动且未配置翻译服务时显示引导，完成或跳过后在设置文件旁写 `onboarding-complete` 标记。

## 4. 数据、隐私与迁移

- **应用 ID**：`com.quicktranslate.desktop`。当前每用户安装路径通常为 `%LOCALAPPDATA%\QuickTranslate\quicktranslate.exe`，标准数据/配置目录为 `%APPDATA%\com.quicktranslate.desktop\`（以 Tauri path resolver 与实际脚本为准）。不要在源码、README 或交接文档写死个人盘符或用户名。
- **配置**：`settings.json`（Schema 5）、旧版迁移快照 `settings.pre-v*.json`、`popup-window.json`（窗口尺寸和固定状态）、`onboarding-complete`（引导已完成的标记）。`settings.json` 里的 `translation` 块（语言、术语表、附加要求）是可选字段，旧程序读不到它时按默认值工作，但由旧程序保存会丢掉这个块。未来 Schema 不能被旧程序无声覆盖；升级保留快捷键、OCR 选择、已有音色和尺寸。注册的自启状态由 Windows 管理。
- **数据库**：`translations.sqlite3` 包含翻译缓存（`translations`）和历史（`translation_history`）两张表，旧库首次打开时把原表里的记录拆入历史并保留编号、时间和收藏；`vocabulary.sqlite3` 是独立词库（Schema 1），含来源、复习事件、测试和进度。备份运行中的 SQLite 用 Online Backup，不能只复制主文件；离线手工备份须退出应用并保留同组 WAL/SHM。恢复前先保护当前数据，禁止随意删除侧车文件。
- **资源**：`ocr-plugins/ppocrv6-small/`、`speech-plugins/kokoro-v1.1/` 位于用户数据目录，不随主安装包打入，不应在升级中清除。与数据库快照区分：本地安装脚本不会把大型插件目录复制进备份。
- **凭据**：Windows Credential Manager 的 `QuickTranslate` 服务分别保存翻译、OCR、朗读 Key，并且每个 Key 绑定到服务地址的来源（`<旧账户名>@<协议>://<主机>:<端口>`）：换服务商不会把原来的 Key 发到新地址，换回去不用重填。升级后首次启动把旧的单一 Key 认领到当时配置的地址（只做一次，用 `<旧账户名>#moved` 标记），旧条目保留。代码经 `SecretStore` 抽象访问凭据，测试和隔离 QA 用内存实现，不碰真实凭据管理器。不能读取或导出用户实际 Key 交接，不能把 Key/Authorization、翻译原文、截图、词库、完整用户路径或备份放进 Git、日志、公开 Release 或测试夹具。设置视图和诊断只报告配置状态。词库 JSON 与数据库备份虽无 Key，仍可能包含私人文本。
- **更新备份**：应用内安装前在 `upgrade-backups/` 保存设置、尺寸和两个数据库，校验完成后写 `COMPLETE`。本机覆盖脚本另在 `%LOCALAPPDATA%\QuickTranslateLocalBackups\` 写应用/配置/数据库快照和 `COMPLETE.txt`。两条路径均不迁移凭据，备份失败不应继续安装。详见 [更新说明](docs/UPDATES.md) 和实际 `upgrade.rs`（旧文档可能未列词库）。

## 5. 开发、测试与运行

用 PowerShell 在仓库根目录执行。CI 使用 Node.js 22 和 Windows stable Rust MSVC；建议本地用同等环境及 C++ Build Tools / Windows SDK。克隆、Git/Node/Rust/VS Build Tools 的通用安装命令见 [README](README.md)。已安装用户只需官方 NSIS 包和 WebView2 Runtime，不需要开发工具。

```powershell
git status --short --branch
node --version
npm --version
rustup show active-toolchain
Get-Command link.exe -ErrorAction SilentlyContinue
npm ci
npm run check
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

- `npm run tauri dev` 启动 Vite（1420）和原生应用；`npm run dev` 单独启动 Vite，不等于能运行托盘、截图、IPC 或真实划词。普通原生 Debug 可能使用真实配置/全局热键；优先使用隔离 QA。避免让正式版和非隔离 Debug 同时竞争热键、打开同一数据库。
- MSVC 工具链存在不代表已安装或能找到 `link.exe`；先检查 VS Developer PowerShell 和 SDK。当前机器普通 PowerShell 未找到 `link.exe`，有 GNU Rust 与 `windres.exe`，不要假定 MSVC 已能本机编译。若明确用 GNU，设置会话级 `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`，对资源编译路径问题使用无空格且可写的 `CARGO_TARGET_DIR`。项目随包分发 x64 `WebView2Loader.dll`，Loader 与系统 WebView2 Runtime 是不同组件。
- 受限环境可以运行 `cargo test --locked --manifest-path core-tests/Cargo.toml` 验证复用的语言/缓存/定位等核心代码；它不覆盖完整 Tauri、Windows 键盘、OCR、词库或真实 WebView 行为。
- 浏览器布局：`npm run qa:ui` 使用真实前端代码和 fixture，需已备 Playwright/Chromium（或指定 `QUICKTRANSLATE_PLAYWRIGHT_PATH`）；输出 `artifacts/ui-qa/`。检查深浅主题、窗口最小尺寸、溢出、圆角透明像素、拖动区与边缘互斥。浏览器通过不等于 Windows 窗口原生交互通过。

隔离原生 QA（仅 Debug，不读真实凭据、不注册实际快捷键；故**不能**用于验证全局划词热键）：

```powershell
$env:QUICKTRANSLATE_QA_DIR = Join-Path $env:TEMP ("QuickTranslate-QA-" + [guid]::NewGuid())
# 可选：$env:QUICKTRANSLATE_QA_VOCABULARY = '1'  # 隔离词库和 loopback 模型
try {
    npm run tauri dev
} finally {
    Remove-Item Env:\QUICKTRANSLATE_QA_DIR,Env:\QUICKTRANSLATE_QA_VOCABULARY -ErrorAction SilentlyContinue
}
```

前端有 TS/Node 测试（队列、流式、主题、词库、交互）；Rust 覆盖清洗、缓存、配置迁移、窗口放置、词库规则、升级备份、更新验证等。v1.6.2 发布记录为前端 38 通过、Rust 96 通过 / 5 环境测试忽略，CI 和 Windows 打包均通过；这是发布时结果，不是本次交接重跑。`iteration/plan-2026-11` 在本机 GNU 工具链上的最近一次结果见 [迭代计划](docs/ITERATION-PLAN.md) 第 12 节，未经 CI。`tests/ipc.test.ts` 静态核对命令清单与各窗口授权；流式翻译用本机回环的模拟服务测试（`providers/stream_tests.rs`）。需要时单独运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml native_voice_and_audio_smoke -- --ignored --nocapture`（需系统音色）。验签测试使用指定产物，见第 7 节；不要无差别运行所有 ignored 测试。

## 6. Word / Excel 取不到选区：源码已修复，待随新版本验收

**已确认**：用户按划词键后会看到“未检测到选中文字”；同一选区手按 `Ctrl+C` 可粘贴到记事本。热键至少已触发应用，Office 可以提供文本，此次失败发生在模型翻译之前。2026-11-11 已在空白 Word/Excel/记事本上用合成文本复现并定位，与 API Key、模型额度、权限级别、等待时长都无关。

**根因**：旧链路在 `Ctrl+C` 之前用 `SendInput` 注入“Alt 松开”。热键的字母键被系统吞掉，目标程序只看到“Alt 按下、Alt 松开”，当成单击 Alt：Word/Excel 把焦点从正文切到功能区（`NetUIHWND`），Windows 11 记事本切到菜单层，`Ctrl+C` 到不了文档，剪贴板始终不变。已安装的 1.6.2 在 Word 中按 `Alt+Q` 实测如此。只把 Ctrl 提前按下能救 Office 但救不了记事本；只等用户松手，在用户先松 Alt 时同样失败。旧的 OLE 剪贴板恢复因未调用 `OleInitialize` 从未生效（`0x800401F0`）。

**现行链路（源码）**：`lib.rs` 在 `Pressed` 事件触发 → `platform::get_selected_text` 在 blocking worker 调用 `selection::capture_selected_text`（全局互斥，串行）→ Alt/Win 仍按着时先注入无功能屏蔽键 `0xE8` → 等所有修饰键实际松开（上限 1 s，超时返回 `CLIPBOARD_ERROR`，不发送任何按键）→ 记录原剪贴板文本与序号 → 只发送 `Ctrl+C` → 至多 25 次、间隔 10 ms，序号改变且 `IsClipboardFormatAvailable` 报告有文本后才打开读取 → 复制确实发生、且剪贴板的写入者属于前台窗口的进程时恢复原文本（原为空则清空；原为图片/文件等非文本则不动；写入者是别的程序时不恢复，但取到的文本照常使用）。每次取词的各阶段结果以匿名元数据保存在内存里（`last_capture_report`），显示在设置的诊断信息中，不含窗口标题和文本。不要改回注入修饰键松开：除了触发菜单，仍按着的字母键还会在 Alt 被判为松开后向选区输入字符。没有 UI Automation 或 Office COM 专用读取；UI Automation 兜底已评估过，结论是暂不实现（Word 的焦点元素没有文本接口，Excel 多格选区拿不全，见迭代计划 §7.4）。

**已验证**：把真实的 `selection.rs` / `clipboard.rs` 编进测试驱动（现已收进仓库：`tools/selection-harness/`，用 `run.ps1 -Stage notepad|word|excel -Build` 手动运行，只用合成文档），用 `Alt+J` 模拟按键：Word 正文与表格单元格、Excel 单格/多格/编辑态、记事本，两种松键顺序、按住 40/150 ms，共 48 次取词全部成功，焦点不变，原剪贴板文本恢复；另测连按两次、原剪贴板为空、无选区（`NO_SELECTED_TEXT`）、按住 1.5 s（`CLIPBOARD_ERROR`）。`cargo test` 98 通过 / 5 忽略。**未验证**：完整应用构建后的真实 `Alt+Q`、浏览器及其他程序、非 Alt 的自定义热键、原剪贴板为图片/富文本、长文本、UIPI 场景。正常复制 30–60 ms 完成，取词总耗时约等于用户按键时长加 50–70 ms；不要用增加固定 sleep 来“优化”。

**原建议顺序**（第 1–3 步已完成；第 4 步目前不需要；第 5 步见上面的未验证项）：

1. 用空白 Word/Excel 样例和记事本对照。Word 测正文及表格；Excel 分别测单格、多格、编辑态选中单元格内子串。使用同等普通权限，临时换未占用热键对照，确认前台焦点是否变化。不要降低系统安全配置或改真实文档。
2. 增加仅诊断阶段的匿名元数据：触发时目标是否仍是同一顶层窗、修饰键是否已释放、`SendInput` 发送数量、序号是否变化、是否有文本格式、读失败分类、耗时。不要记录窗口标题、文档路径、剪贴板内容或选区全文。
3. 在确认目标仍有效时等待实际按键释放，再发送复制；设置有界剪贴板就绪等待，避免把旧剪贴板误当成新选区。取词串行化或隔离事务，防止快速多次触发时多个 blocking 任务互相覆盖；所有退出路径正确清理 COM/OLE 资源。仅在确定仍持有本次复制的结果时恢复原剪贴板，避免覆盖用户随后复制的内容。
4. 通用链路仍不稳定时，再评估按前台目标启用的 UI Automation / Office COM 只读取词。分别定义 Word 选区、Excel Range 显示值和 Excel 编辑态文本的处理；不能为了取词启动 Office、保存工作簿、读取整份文档或绕过禁止复制的策略。
5. 原生回归：记事本/浏览器仍可用；Word/Excel 各选择模式、空选区、长文本、快速连按、切换焦点、剪贴板原有图片/富文本或外部新复制都安全。用合成文本、隔离数据和测试端点，不把真实文档内容发给服务商。

参考：[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)、[剪贴板序号与延迟渲染](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber)。

## 7. 本机替换、回滚、正式发布

**v1.6.2 已核实的发布物**：`QuickTranslate_1.6.2_x64-setup.exe`，19,523,314 字节，SHA-256 `C5BBBB22ADA062F56C9677DD391D838C89B76DE26E93AF9EF3791AFF5E1211B8`。Release 同时含 `.exe.sig`、`SHA256SUMS.txt`、`latest.json`。这些是该版固定值，不得用于核对未来新构建。v1.6.2 发布时已匿名重下并验证包/签名/feed，CI [37252980531](https://github.com/MYPoems/QuickTranslate/actions/runs/37252980531)、Windows 构建 [37252980624](https://github.com/MYPoems/QuickTranslate/actions/runs/37252980624) 通过。本次交接重新核对了 GitHub latest 元数据、远端引用和资产摘要，未重新下载或安装。

- **构建**：同步 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 的应用版本；维护 `CHANGELOG.md`、`docs/releases/vX.Y.Z.md` 和 README 安装指引。受控环境使用 `./scripts/build-release.ps1`，脚本要求配置 `TAURI_SIGNING_PRIVATE_KEY`（私钥或文件路径，密码由 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 提供），执行检查、NSIS 打包和验签，产物落在 `artifacts/vX.Y.Z/`。不要复制、打印、提交或上传私钥；不要为发布擅自配置 CI secrets、重置公钥或关闭 `requireSignedVersion`。
- **验签**：`verify_release_artifact` 是 ignored Rust 测试；给当前会话设置 `QUICKTRANSLATE_VERIFY_ARTIFACT` / `QUICKTRANSLATE_VERIFY_SIGNATURE` 指向对应 EXE / SIG，然后运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml verify_release_artifact -- --ignored`，结束后清环境变量。它核验配置公钥、签名绑定版本及篡改包拒绝；Tauri 更新签名**不是** Windows 商业 Authenticode 签名，SmartScreen 仍可能提示未知发布者。
- **本机替换**：只在用户明确要求时运行 `./scripts/install-local.ps1 -Installer <已验证的安装包路径> -ExpectedSha256 <已验证的64位哈希>`（尖括号为占位符，实际路径需引号）。只停止准确的已安装进程路径，备份后才安装。验收重启后的实际路径和版本，并核对设置、窗口尺寸、历史、词库、插件保留。可用 `scripts/audit-history.py`、`scripts/audit-vocabulary.py` 作只读比较，不输出内容。NSIS 可能把二进制内部 Tauri bundle marker 从 `UNK` 改为 `NSS`，安装后 EXE 的哈希不等于原始构建 EXE；先核实差异，不修改二进制来凑哈希。
- **源码回滚**：`./scripts/rollback-to-stable.ps1 -StableVersion v1.6.1` 会拒绝脏工作区并 detached checkout 指定标签；脚本默认参数仍为 `v1.0.0`，必须明确选择。不会还原安装或数据。应用回滚需另外保留当前数据、确认旧 Schema 兼容、验证历史安装包和还原计划；不能靠 git checkout 恢复用户数据库。
- **发布**：只在当前用户授权后，确认远端和凭据状态（本次 `gh` 未登录，公开元数据不需登录）、核对范围与版本、避免强推/移动旧标签。推送目标提交及匹配的 tag，等待 main CI 和 tag Windows 构建都成功，再发布四件套。无 CI 签名配置时，`release.yml` 只做无签名打包验证，不分发无签名产物；可以使用另行验签的受控本机包。工作流不会自动公开 Release。
- **发布后**：确认 Latest、非草稿、非预发行；匿名从公开 URL 重下四项，核验 EXE 大小/哈希/签名，`latest.json` 的版本、Windows 下载 URL 和签名与该包一致，`/releases/latest/download/latest.json` 指向新版本。无法验证的项目明确列出，不能把 GitHub 的构建成功当成安装成功。
- **应用内更新**：设置中检查 → 下载并校验 → 用户确认安装；固定 GitHub HTTPS 入口，仅接受对应版本 NSIS、带版本绑定签名、不允许降级。下载可取消，内存有大小限制（150 MiB），包退出后不保留；安装前备份失败则禁止安装。首个 v1.0.0 老版需手工覆盖升级。参见 [发布检查清单](docs/RELEASE_CHECKLIST.md)。

## 8. 剩余验收、路线与资料

- **发布记录未覆盖**：v1.6.2 真机自动手势确认未固定窗口不会消失，但未量化实际拖动位移或边角缩放；不要引用 v1.6.1 的结果替代这一版验收。还需按实际任务检查干净 Windows 用户安装、不同缩放比例/多屏、真实 Office 矩阵及从旧正式版完整应用内升级。真实云端 OCR/朗读的生产延迟没有在该版发布中重新测量。
- **后续优先顺序建议**：路线、每项的状态和未验证的部分都在 [迭代计划](docs/ITERATION-PLAN.md)（第 12 节是进度）。源码层面计划内的条目已基本做完，剩下的主要是验收和发布：构建后真机验收真实 `Alt+Q` 和悬浮窗原生交互，对真实的翻译、OCR、朗读服务测试流式、缩放后的截图和 Key 迁移，从 v1.6.2 做一次完整的应用内升级（历史拆表、Key 认领），然后才是版本号和发布。未做的工程项：`vocabulary.ts` / `settings.ts` 拆分、界面截图基线进 CI、原生回归扩展、性能度量。保持两种离线 OCR 为用户显式选项，不承诺商业签名或服务商 SLA。
- **阅读索引**：[README（安装与用户入口）](README.md) · [版本演进](CHANGELOG.md) · [迭代计划与进度](docs/ITERATION-PLAN.md) · [v1.6.2 发布说明](docs/releases/v1.6.2.md) · [本机安装记录](docs/V1.6.2-LOCAL-INSTALL.md) · [收藏弹层 QA](docs/COLLECTOR-UI-QA.md) · [未固定拖动 QA](docs/POPUP-UNPINNED-DRAG-QA.md) · [更新迁移](docs/UPDATES.md) · [生词本规则](docs/VOCABULARY.md) · [插件来源与许可](docs/SPEECH-PLUGIN.md)。
- **交付口径**：清楚区分改了什么、运行了哪些测试、用的是哪个构建/窗口、哪些需要用户或目标环境复核。源代码检查、单元测试、浏览器 fixture、原生 UI、覆盖安装、公开下载与真实云端运行是不同证据。
