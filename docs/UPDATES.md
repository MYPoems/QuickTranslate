# 签名更新与数据迁移

## 用户流程

设置 → 应用更新 → 检查更新 → 下载并校验 → 安装更新 → 确认安装。

没有校验通过的安装包、用户未确认、版本不匹配、下载取消、签名错误或备份失败时，不运行安装程序。下载包仅留在当前应用的内存中；关闭应用后需要重新下载。关闭设置窗口不会中断下载，再打开时读取同一更新任务的状态。安装会退出应用，并由 NSIS 完成更新后重新启动。

新版本只接受本仓库固定 HTTPS 更新清单和对应版本的 GitHub NSIS 下载地址。Tauri 官方 updater 校验 Minisign 签名和签名中绑定的版本，不允许降级。下载上限为 150 MiB。

## 自动备份

安装前在应用数据目录 `upgrade-backups/upgrade-<timestamp>/` 写入：

- `settings.json`：当前已保存设置，包含朗读偏好，不包含 API Key。
- `settings.pre-v*.json`：存在时保留旧设置 Schema 的原始副本。
- `popup-window.json`：当前悬浮窗尺寸。
- `translations.sqlite3`：使用 SQLite Online Backup，包含未 checkpoint 的 WAL 数据；完成后执行 `integrity_check`。
- `manifest.json`：来源、目标版本和备份时间。
- `COMPLETE`：仅所有文件落盘成功后生成。没有此标记的备份不能视为完整。

API Key 保留在 Windows Credential Manager，不导出到备份。开机启动由 Windows 注册状态管理，覆盖安装不清除该项。升级不会改变旧用户的 OCR 引擎与自定义快捷键。新用户默认云端 OCR，但需自行配置 API Key 才能使用。

回退旧应用前先退出应用，保留全部现有数据；将备份复制到单独目录检查。设置中可导入兼容当前版本的备份 JSON 并确认保存。手动回退数据库需先备份当前数据库与 `-wal`、`-shm` 文件，关闭所有 QuickTranslate 进程再恢复。v1.2.0 会拒绝覆盖更高 Schema；已发布的 v1.0.0 没有这项保护，回退时必须主动恢复原始 `settings.pre-v2.json` 或升级前自己的备份，不要让旧程序直接保存新 Schema 的设置。

## 发布者流程

私钥必须保存在仓库外并限制 ACL；公钥写在 `src-tauri/tauri.conf.json`。不要重新生成或替换公钥，否则已安装应用会无法验证未来更新。备份私钥到受控的安全位置，不能提交、上传到 Release 或写入日志。

在 GitHub 仓库 Actions secrets 中配置 `TAURI_SIGNING_PRIVATE_KEY`，有密码时再配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。本机构建可将私钥文件路径放入环境变量：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "C:\安全目录\updater.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
./scripts/build-release.ps1
```

脚本会拒绝缺少签名配置的构建，验证生成包的真实签名、签名绑定版本，并修改一个字节验证篡改包被拒绝。打包工具至少使用 Tauri CLI 2.12.1，旧版签名若缺少版本字段会被拒绝；不要关闭 `requireSignedVersion` 来绕过失败。产物在 `artifacts/v<version>/`：NSIS `.exe`、`.exe.sig`、`SHA256SUMS.txt`、`latest.json`。必须把四项一起上传至对应版本 Release；不要只上传安装包。CI 只构建并上传工作流 artifacts，不自动创建公开 Release。

## 回归验证

仅本机覆盖安装（不发布 Release）：先完成签名构建并记录本轮安装包哈希，使用 `./scripts/install-local.ps1 -ExpectedSha256 "已验证的64位SHA256"`。脚本只停止当前用户安装路径的应用，备份旧程序、设置和关闭进程后的 SQLite/WAL/SHM，校验成功后运行本地安装器并启动应用。备份失败会阻止安装，不修改 Windows 凭据。`scripts/audit-history.py <升级前数据库> <升级后数据库>` 可只读比较历史全文摘要、记录数、收藏数与完整性，只输出统计/摘要，不输出历史内容。

```powershell
npm test
cargo test --locked --manifest-path ./src-tauri/Cargo.toml
```

自动验证覆盖：v1.0 设置和离线引擎选择、快捷键、语速音色、备份恢复、未来 Schema 拒绝覆盖、旧历史释义与收藏、WAL 在线备份、幂等迁移、备份失败、安装确认与版本匹配、可信下载地址。

真实 Windows 音频生成（需已安装至少一种中英文音色）：

```powershell
cargo test --locked --manifest-path ./src-tauri/Cargo.toml native_voice_and_audio_smoke -- --ignored --nocapture
```

隔离的原生 UI 验收仅编译进 Debug，Release 不含此模式，不读取真实凭据或修改真实数据、快捷键：

```powershell
$env:QUICKTRANSLATE_QA_DIR = Join-Path $env:TEMP ("QuickTranslate-QA-" + [guid]::NewGuid())
npm run tauri dev
```

使用合成的中英文 OCR 样例测试朗读、编辑后朗读、段落高亮、关闭停播、语音包缺失提示和设置表单；退出后移除该环境变量。公开 Release 下载、取消和安装全过程需要在下一次受控正式版本升级或隔离 Windows 测试机上验收，不能用虚假的未来版本清单冒充正式发布。
