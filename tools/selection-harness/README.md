# 取词原生回归驱动

划词取词依赖真实的键盘输入、窗口焦点和系统剪贴板，单元测试覆盖不到。这个工具把应用里真实的 `selection.rs` 和 `clipboard.rs` 原样编译进来，注册一个备用的 `Alt+<键>` 热键，模拟用户按键，再像应用一样调用 `capture_selected_text`，最后输出一行匿名的 JSON 结果。

它不属于应用构建，也不在 CI 里运行，需要在一台装有 Office 的 Windows 机器上手动执行。

## 使用

在仓库根目录执行，运行期间不要操作键盘和鼠标：

```powershell
./tools/selection-harness/run.ps1 -Stage notepad
./tools/selection-harness/run.ps1 -Stage word
./tools/selection-harness/run.ps1 -Stage excel
```

常用参数：

| 参数 | 含义 |
| --- | --- |
| `-Release alt-first` | 模拟先松开 Alt、后松开字母键（默认是先松字母键） |
| `-Holds 40,150,1500` | 按住热键的毫秒数；超过 1000 应当得到 `CLIPBOARD_ERROR` |
| `-Presses 2` | 连续按两次 |
| `-Orig empty` | 取词前清空剪贴板，检查取词后是否仍为空 |
| `-NoSelection` | 仅记事本：不选中任何文字，应当得到 `NO_SELECTED_TEXT` |
| `-Hotkey k` | 备用热键的字母，默认 `j`；`Alt+J` 被占用时更换 |
| `-Build` | 强制重新编译（改了取词源码之后需要） |

每一行结果包含：是否取到文本、耗时、取词前后键盘焦点所在的控件类名、取词后剪贴板的状态（`original` 表示已恢复为取词前的文本）。脚本最后会汇总有多少次不符合预期，并以非零退出码结束。

## 安全边界

- 只操作脚本自己新建的文档：记事本打开临时文件，Word 和 Excel 新建空白文档并写入固定的合成文本，结束后不保存直接关闭。
- Word 或 Excel 已经在运行时脚本会拒绝执行，不会碰用户已打开的文档。
- 会替换剪贴板内容，并在剪贴板历史里留下以 `QTDIAG`、`QTORIG` 开头的测试文本。
- 输出不包含剪贴板内容、窗口标题或文档路径。
- 使用的是备用热键，不经过 Tauri 应用本身。它验证的是取词代码，不能代替完整应用构建后对真实快捷键的验收。

## 构建说明

`Cargo.toml` 里的 `windows` 版本必须与应用一致。本机没有 MSVC 链接器时，用 GNU 工具链构建：

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
./tools/selection-harness/run.ps1 -Stage notepad -Build
```
