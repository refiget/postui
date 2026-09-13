#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$target = "x86_64-pc-windows-msvc"
$script:TempRoot = $null
$script:HadRustFlags = $false
$script:PreviousRustFlags = $null
$script:RustFlagsConfigured = $false

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

function Get-CargoPath {
    $cargoCommand = Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue
    if ($null -ne $cargoCommand) {
        return $cargoCommand.Path
    }

    if (![string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $cargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
        if (Test-Path -LiteralPath $cargoPath -PathType Leaf) {
            return $cargoPath
        }
    }

    Fail "找不到 cargo，请先安装 Rust 并将 cargo 加入 PATH"
}

try {
    $projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
    if ([string]::IsNullOrWhiteSpace($OutputDir)) {
        $OutputDir = Join-Path $projectRoot "打包区"
    }

    $OutputDir = [IO.Path]::GetFullPath($OutputDir)
    Require-File (Join-Path $projectRoot "install.ps1") "PowerShell 安装脚本"

    $cargoPath = Get-CargoPath
    $script:HadRustFlags = Test-Path Env:RUSTFLAGS
    $script:PreviousRustFlags = $env:RUSTFLAGS
    $staticCrtFlag = "-C target-feature=+crt-static"
    if ([string]::IsNullOrWhiteSpace($script:PreviousRustFlags)) {
        $env:RUSTFLAGS = $staticCrtFlag
    } elseif ($script:PreviousRustFlags -notmatch [regex]::Escape($staticCrtFlag)) {
        $env:RUSTFLAGS = "$($script:PreviousRustFlags) $staticCrtFlag"
    }
    $script:RustFlagsConfigured = $true

    Write-Host "构建 Windows amd64 release: $target"
    & $cargoPath build --locked --manifest-path (Join-Path $projectRoot "Cargo.toml") --release --target $target
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo build 执行失败"
    }

    $binaryPath = Join-Path $projectRoot "target\$target\release\postui.exe"
    Require-File $binaryPath "Windows release 二进制"

    $script:TempRoot = Join-Path ([IO.Path]::GetTempPath()) ("postui-package-{0}" -f [Guid]::NewGuid())
    $packageRoot = Join-Path $script:TempRoot "postui-windows-amd64"
    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $packageRoot "postui.exe") -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "install.ps1") -Destination (Join-Path $packageRoot "install.ps1") -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot "README.md") -Destination (Join-Path $packageRoot "README.md") -Force

    foreach ($optionalDirectory in @("docs")) {
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
    if ($script:RustFlagsConfigured) {
        if ($script:HadRustFlags) {
            $env:RUSTFLAGS = $script:PreviousRustFlags
        } else {
            Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        }
    }
    if ($null -ne $script:TempRoot -and (Test-Path -LiteralPath $script:TempRoot)) {
        Remove-Item -LiteralPath $script:TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
