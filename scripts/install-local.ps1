param(
    [string]$Installer,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Installer) { $Installer = Join-Path $projectRoot 'artifacts/v1.2.0/QuickTranslate_1.2.0_x64-setup.exe' }
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
if ((Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash -ne $ExpectedSha256.ToUpperInvariant()) { throw '安装包哈希不符，未执行安装' }
$installedRoot = Join-Path $env:LOCALAPPDATA 'QuickTranslate'
$installedExe = Join-Path $installedRoot 'quicktranslate.exe'
$dataRoot = Join-Path $env:APPDATA 'com.quicktranslate.desktop'
$backupRoot = Join-Path $env:LOCALAPPDATA ('QuickTranslateLocalBackups/' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $backupRoot | Out-Null
# Stop only this exact installation, never all applications or all Rust processes.
$running = @(Get-Process quicktranslate -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installedExe })
foreach ($process in $running) { Stop-Process -Id $process.Id; Wait-Process -Id $process.Id -ErrorAction SilentlyContinue }
try {
    if (Test-Path -LiteralPath $installedRoot) { Copy-Item -LiteralPath $installedRoot -Destination (Join-Path $backupRoot 'application') -Recurse }
    $dataBackup = Join-Path $backupRoot 'data'
    New-Item -ItemType Directory -Path $dataBackup | Out-Null
    if (Test-Path -LiteralPath $dataRoot) {
        # Include SQLite WAL/SHM as a closed-process snapshot; leave keys in Credential Manager.
        Get-ChildItem -LiteralPath $dataRoot -File | Where-Object { $_.Name -like 'settings*.json*' -or $_.Name -eq 'popup-window.json' -or $_.Name -like 'translations.sqlite3*' } | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $dataBackup }
    }
    'Closed-process application/settings/history backup' | Set-Content -LiteralPath (Join-Path $backupRoot 'COMPLETE.txt')
} catch {
    if ($running.Count -gt 0 -and (Test-Path -LiteralPath $installedExe)) { Start-Process -FilePath $installedExe -WindowStyle Hidden }
    throw "备份失败，未运行安装器：$($_.Exception.Message)"
}
$installerProcess = Start-Process -FilePath $installerPath -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
if ($installerProcess.ExitCode -ne 0) { throw "安装失败，退出码 $($installerProcess.ExitCode)。保留备份：$backupRoot" }
if (!(Test-Path -LiteralPath $installedExe)) { throw "安装器返回成功但未找到预期应用。保留备份：$backupRoot" }
if (!(Get-Process quicktranslate -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installedExe })) { Start-Process -FilePath $installedExe -WindowStyle Hidden }
Write-Host "Installed: $installedExe"
Write-Host "Backup: $backupRoot"
