#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$ArchiveUrl = "https://github.com/refiget/postui/releases/latest/download/postui-windows-amd64.zip",
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

function Require-PackageFiles([string]$PackageDirectory) {
    $requiredFiles = @((Join-Path $PackageDirectory "postui.exe"))
    foreach ($path in $requiredFiles) {
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) {
            Fail "发布目录缺少文件: $path"
        }
    }
}

try {
    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Get-DefaultInstallDir
    }
    $InstallDir = [IO.Path]::GetFullPath($InstallDir)

    $localPackageDirectory = Find-PackageDirectory (Split-Path -Parent $MyInvocation.MyCommand.Path)
    if ([string]::IsNullOrWhiteSpace($localPackageDirectory)) {
        $packageDirectory = Download-Package $ArchiveUrl
    } else {
        $packageDirectory = $localPackageDirectory
    }
    Require-PackageFiles $packageDirectory

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $packageDirectory "postui.exe") `
        -Destination (Join-Path $InstallDir "postui.exe") -Force
    if (!$SkipInit) {
        & (Join-Path $InstallDir "postui.exe") init
        if ($LASTEXITCODE -ne 0) {
            Fail "postui init 执行失败，已完成文件安装；可使用 -SkipInit 跳过后手动执行"
        }
    }

    Write-Host "PostUI 已安装到: $InstallDir"
    Write-Host "当前 PowerShell 请重新打开后使用 postui。"
} finally {
    if ($null -ne $script:TempRoot -and (Test-Path -LiteralPath $script:TempRoot)) {
        Remove-Item -LiteralPath $script:TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
