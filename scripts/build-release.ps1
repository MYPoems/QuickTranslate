param(
    [string]$OutputDirectory
)

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
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "前端构建失败" }
    cargo fmt --manifest-path .\src-tauri\Cargo.toml --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "Rust 格式检查失败" }
    cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Rust 静态检查失败" }
    cargo test --manifest-path .\src-tauri\Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw "Rust 测试失败" }
    npm run tauri build -- --bundles nsis
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
$installers = Get-ChildItem (Join-Path $cargoTarget "release\bundle\nsis") -Filter "*.exe"
if (-not $installers) {
    throw "未找到 NSIS 安装包"
}
foreach ($installer in $installers) {
    Copy-Item $installer.FullName (Join-Path $versionOutput $installer.Name) -Force
}
$hashes = Get-ChildItem $versionOutput -Filter "*.exe" | Get-FileHash -Algorithm SHA256
$hashLines = $hashes | ForEach-Object { "$($_.Hash)  $(Split-Path $_.Path -Leaf)" }
$hashLines | Set-Content (Join-Path $versionOutput "SHA256SUMS.txt") -Encoding utf8
Write-Host "Release artifacts: $versionOutput"
$hashes | Format-Table Hash, Path -AutoSize
