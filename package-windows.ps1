#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$ConfigPath,
    [string]$CollectionPath,
    [string]$OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$target = "x86_64-pc-windows-msvc"
$script:TempRoot = $null

function Fail([string]$Message) {
    throw "PostUI 打包失败: $Message"
}

function Require-File([string]$Path, [string]$Description) {
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {
        Fail "找不到${Description}: $Path"
    }
}

function Require-Directory([string]$Path, [string]$Description) {
    if (!(Test-Path -LiteralPath $Path -PathType Container)) {
        Fail "找不到${Description}: $Path"
    }
}

try {
    $projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
    if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
        $ConfigPath = Join-Path $projectRoot "config.yaml"
    }
    if ([string]::IsNullOrWhiteSpace($CollectionPath)) {
        $CollectionPath = Join-Path $projectRoot ".postui"
    }
    if ([string]::IsNullOrWhiteSpace($OutputDir)) {
        $OutputDir = Join-Path $projectRoot "打包区"
    }

    $ConfigPath = [IO.Path]::GetFullPath($ConfigPath)
    $CollectionPath = [IO.Path]::GetFullPath($CollectionPath)
    $OutputDir = [IO.Path]::GetFullPath($OutputDir)
    Require-File $ConfigPath "全局配置文件"
    Require-File (Join-Path $CollectionPath "config.yaml") "请求集合配置文件"
    Require-Directory (Join-Path $CollectionPath "requests") "请求集合目录"
    Require-File (Join-Path $projectRoot "install.ps1") "PowerShell 安装脚本"

    Write-Host "构建 Windows amd64 release: $target"
    & cargo build --release --target $target
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo build 执行失败"
    }

    $binaryPath = Join-Path $projectRoot "target\$target\release\postui.exe"
    Require-File $binaryPath "Windows release 二进制"

    $script:TempRoot = Join-Path ([IO.Path]::GetTempPath()) ("postui-package-{0}" -f [Guid]::NewGuid())
    $packageRoot = Join-Path $script:TempRoot "postui-windows-amd64"
    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $packageRoot ".postui") -Force | Out-Null

    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $packageRoot "postui.exe") -Force
    Copy-Item -LiteralPath $ConfigPath -Destination (Join-Path $packageRoot "config.yaml") -Force
    Copy-Item -LiteralPath (Join-Path $CollectionPath "config.yaml") `
        -Destination (Join-Path $packageRoot ".postui\config.yaml") -Force
    Copy-Item -LiteralPath (Join-Path $CollectionPath "requests") `
        -Destination (Join-Path $packageRoot ".postui\requests") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "install.ps1") -Destination (Join-Path $packageRoot "install.ps1") -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "README.md") -Destination (Join-Path $packageRoot "README.md") -Force

    foreach ($optionalDirectory in @("files", "themes")) {
        $sourceDirectory = Join-Path $projectRoot $optionalDirectory
        if (Test-Path -LiteralPath $sourceDirectory -PathType Container) {
            Copy-Item -LiteralPath $sourceDirectory -Destination (Join-Path $packageRoot $optionalDirectory) -Recurse -Force
        }
    }

    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    $archivePath = Join-Path $OutputDir "postui-windows-amd64.zip"
    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    Compress-Archive -LiteralPath $packageRoot -DestinationPath $archivePath -CompressionLevel Optimal
    Write-Host "Windows 发布包已生成: $archivePath"
} finally {
    if ($null -ne $script:TempRoot -and (Test-Path -LiteralPath $script:TempRoot)) {
        Remove-Item -LiteralPath $script:TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
