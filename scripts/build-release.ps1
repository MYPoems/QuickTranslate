param(
    [string]$OutputDirectory,
    [string]$Target
)

$ErrorActionPreference = "Stop"
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) { throw "必须先配置 TAURI_SIGNING_PRIVATE_KEY（私钥内容或路径）；拒绝构建无签名更新包。" }

$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $projectRoot "artifacts"
}

$package = Get-Content (Join-Path $projectRoot "package.json") -Raw | ConvertFrom-Json
$tauri = Get-Content (Join-Path $projectRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$cargoVersion = Select-String -Path (Join-Path $projectRoot "src-tauri\Cargo.toml") -Pattern '^version = "([^\"]+)"$' | Select-Object -First 1
if (-not $cargoVersion) {
    throw "无法读取 Cargo.toml 版本"
}
$cargoVersion = $cargoVersion.Matches[0].Groups[1].Value
if ($package.version -ne $tauri.version -or $package.version -ne $cargoVersion) {
    throw "版本号不一致：package=$($package.version), tauri=$($tauri.version), cargo=$cargoVersion"
}

Push-Location $projectRoot
try {
    npm run check
    if ($LASTEXITCODE -ne 0) { throw "前端类型检查失败" }
    npm test
    if ($LASTEXITCODE -ne 0) { throw "前端播放队列测试失败" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "前端构建失败" }
    cargo fmt --manifest-path .\src-tauri\Cargo.toml --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "Rust 格式检查失败" }
    cargo clippy --locked --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Rust 静态检查失败" }
    cargo test --locked --manifest-path .\src-tauri\Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw "Rust 测试失败" }
    if ($Target) { npm run tauri build -- --target $Target --bundles nsis } else { npm run tauri build -- --bundles nsis }
    if ($LASTEXITCODE -ne 0) { throw "安装包构建失败" }
} finally {
    Pop-Location
}

$versionOutput = Join-Path $OutputDirectory "v$($package.version)"
New-Item -ItemType Directory -Path $versionOutput -Force | Out-Null
$cargoTarget = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $projectRoot "src-tauri\target"
}
if ($Target) { $cargoTarget = Join-Path $cargoTarget $Target }
$installers = Get-ChildItem (Join-Path $cargoTarget "release\bundle\nsis") -Filter "QuickTranslate_$($package.version)_x64-setup.exe"
if (-not $installers) {
    throw "未找到 NSIS 安装包"
}
foreach ($installer in $installers) {
    if (!(Test-Path -LiteralPath ($installer.FullName + '.sig'))) { throw "缺少安装包签名：$($installer.Name)" }
    $env:QUICKTRANSLATE_VERIFY_ARTIFACT = $installer.FullName
    $env:QUICKTRANSLATE_VERIFY_SIGNATURE = $installer.FullName + '.sig'
    try {
        cargo test --locked --manifest-path (Join-Path $projectRoot 'src-tauri/Cargo.toml') verify_release_artifact -- --ignored
        if ($LASTEXITCODE -ne 0) { throw "安装包签名或签名版本校验失败" }
    } finally {
        Remove-Item Env:QUICKTRANSLATE_VERIFY_ARTIFACT, Env:QUICKTRANSLATE_VERIFY_SIGNATURE -ErrorAction SilentlyContinue
    }
    Copy-Item $installer.FullName (Join-Path $versionOutput $installer.Name) -Force
    Copy-Item ($installer.FullName + '.sig') (Join-Path $versionOutput ($installer.Name + '.sig')) -Force
}
$hashes = Get-ChildItem $versionOutput -Filter "*.exe" | Get-FileHash -Algorithm SHA256
$hashLines = $hashes | ForEach-Object { "$($_.Hash)  $(Split-Path $_.Path -Leaf)" }
$hashLines | Set-Content (Join-Path $versionOutput "SHA256SUMS.txt") -Encoding utf8
$installer = $installers | Where-Object Name -EQ "QuickTranslate_$($package.version)_x64-setup.exe" | Select-Object -First 1
if (!$installer) { throw "未找到对应版本的 x64 NSIS 安装包" }
$notesPath = Join-Path $projectRoot "docs/releases/v$($package.version).md"
$notes = if (Test-Path -LiteralPath $notesPath) { Get-Content -LiteralPath $notesPath -Raw } else { "QuickTranslate v$($package.version)" }
$feed = @{
    version = $package.version
    notes = $notes
    pub_date = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = @{
        "windows-x86_64" = @{
            signature = (Get-Content -LiteralPath ($installer.FullName + '.sig') -Raw).Trim()
            url = "https://github.com/MYPoems/QuickTranslate/releases/download/v$($package.version)/$($installer.Name)"
        }
    }
}
$feed | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $versionOutput 'latest.json') -Encoding utf8NoBOM
Write-Host "Release artifacts: $versionOutput"
$hashes | Format-Table Hash, Path -AutoSize
