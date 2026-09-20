[CmdletBinding()]
param(
    [string]$StableVersion = "v0.2.0"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repositoryRoot
try {
    git rev-parse --is-inside-work-tree | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "当前目录不是 Git 仓库。"
    }

    $pendingChanges = @(git status --porcelain)
    if ($LASTEXITCODE -ne 0) {
        throw "无法读取 Git 工作区状态。"
    }
    if ($pendingChanges.Count -gt 0) {
        throw "检测到未提交的改动。请先提交或备份改动，再执行回滚。"
    }

    git fetch origin "refs/tags/${StableVersion}:refs/tags/${StableVersion}"
    if ($LASTEXITCODE -ne 0) {
        throw "无法获取稳定版本标签 $StableVersion。"
    }

    git switch --detach $StableVersion
    if ($LASTEXITCODE -ne 0) {
        throw "无法切换到稳定版本 $StableVersion。"
    }

    Write-Host "已安全回滚到 $StableVersion。恢复开发分支时运行：git switch main" -ForegroundColor Green
}
finally {
    Pop-Location
}
