# QuickTranslate 发布检查清单

## 代码与版本

- `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 版本一致。
- `main` 工作区干净，CI 全部通过。
- `CHANGELOG.md` 包含目标版本和发布日期。
- 执行 `./scripts/build-release.ps1`，保留安装包和 `SHA256SUMS.txt`。

## Windows 人工验收

- 全新安装与从上一正式版覆盖安装均能启动。
- `Alt + Q` 可翻译中英文选区，剪贴板原内容尽可能恢复。
- 鼠标位于屏幕四角和副显示器时，悬浮窗不会超出工作区。
- 点击悬浮窗外自动收起；固定后不会自动收起。
- `Alt + W` 在鼠标所在显示器打开选区，OCR 悬浮窗同时显示识别文字和译文。
- 历史搜索、收藏、删除、清空以及设置备份/恢复正常。
- 开机启动可启用、关闭，并在重新登录后验证。

## 安全与发布

- 安装包只来自本仓库的 GitHub Actions 或受控本机构建。
- 对照 `SHA256SUMS.txt` 核验待发布安装包。
- 确认 Release 不包含 API Key、`settings.json`、SQLite 数据库或用户日志。
- 商业代码签名证书尚未配置时，Release 明确说明 SmartScreen 可能显示“未知发布者”。
- 创建公开 GitHub Release 前再次获得仓库所有者确认。
- 发布后在干净 Windows 用户环境下载、验签、安装并完成一次划词与 OCR 冒烟测试。
