#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$ArchiveUrl,
    [string]$Version,
    [switch]$SkipInit
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:TempRoot = $null

function Fail([string]$Message) {
    throw "PostUI 安装失败: $Message"
}

function Get-DefaultInstallDir {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        Fail "无法确定 LOCALAPPDATA"
    }
    return (Join-Path $env:LOCALAPPDATA "Programs\PostUI")
}

function Find-PackageDirectory([string]$Root) {
    $directBinary = Join-Path $Root "postui.exe"
    if (Test-Path -LiteralPath $directBinary -PathType Leaf) {
        return (Get-Item -LiteralPath $directBinary).DirectoryName
    }

    $nestedBinary = Get-ChildItem -LiteralPath $Root -Filter "postui.exe" -File -Recurse |
        Select-Object -First 1
    if ($null -ne $nestedBinary) {
        return $nestedBinary.DirectoryName
    }
    return $null
}

function Download-Package([string]$Url) {
    $script:TempRoot = Join-Path ([IO.Path]::GetTempPath()) ("postui-install-{0}" -f [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $script:TempRoot -Force | Out-Null

    $archivePath = Join-Path $script:TempRoot "postui.zip"
    $extractPath = Join-Path $script:TempRoot "package"
    New-Item -ItemType Directory -Path $extractPath -Force | Out-Null

    Write-Host "正在下载 Windows amd64 发布包: $Url"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $archivePath
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath -Force

    $packageDirectory = Find-PackageDirectory $extractPath
    if ([string]::IsNullOrWhiteSpace($packageDirectory)) {
        Fail "发布包中没有 postui.exe"
    }
    return $packageDirectory
}

try {
    if ($env:OS -ne "Windows_NT") {
        Fail "此脚本仅支持 Windows；Linux/macOS 请使用 install.sh"
    }
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($architecture)) {
        $architecture = $env:PROCESSOR_ARCHITECTURE
    }
    if ($architecture -ne "AMD64") {
        Fail "当前发布包仅支持 Windows amd64，当前架构: $architecture"
    }
    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Get-DefaultInstallDir
    }
    $InstallDir = [IO.Path]::GetFullPath($InstallDir)

    $localPackageDirectory = $null
    if (![string]::IsNullOrWhiteSpace($PSScriptRoot) -and
        [string]::IsNullOrWhiteSpace($ArchiveUrl) -and
        [string]::IsNullOrWhiteSpace($Version)) {
        $localBinary = Join-Path $PSScriptRoot "postui.exe"
        if (Test-Path -LiteralPath $localBinary -PathType Leaf) {
            $localPackageDirectory = $PSScriptRoot
        }
    }
    if ([string]::IsNullOrWhiteSpace($localPackageDirectory)) {
        if ([string]::IsNullOrWhiteSpace($ArchiveUrl)) {
            $releasePath = "latest/download"
            if (![string]::IsNullOrWhiteSpace($Version)) {
                $Version = $Version -replace '^v', ''
                if ($Version -notmatch '^[0-9A-Za-z.-]+$') {
                    Fail "无效版本号: $Version"
                }
                $releasePath = "download/v$Version"
            }
            $ArchiveUrl = "https://github.com/refiget/postui/releases/$releasePath/postui-windows-amd64.zip"
        }
        $packageDirectory = Download-Package $ArchiveUrl
    } else {
        $packageDirectory = $localPackageDirectory
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    if ([IO.Path]::GetFullPath($packageDirectory) -ne $InstallDir) {
        Copy-Item -LiteralPath (Join-Path $packageDirectory "postui.exe") `
            -Destination (Join-Path $InstallDir "postui.exe") -Force
    }
    if (!$SkipInit) {
        & (Join-Path $InstallDir "postui.exe") init
        if ($LASTEXITCODE -ne 0) {
            Fail "postui init 执行失败，已完成文件安装；可使用 -SkipInit 跳过后手动执行"
        }
    }

    Write-Host "PostUI 已安装到: $InstallDir"
    if (!$SkipInit) {
        Write-Host "当前 PowerShell 请重新打开后使用 postui。"
    }
} finally {
    if ($null -ne $script:TempRoot -and (Test-Path -LiteralPath $script:TempRoot)) {
        Remove-Item -LiteralPath $script:TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
