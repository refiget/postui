#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$ArchiveUrl,
    [string]$Version,
    [string]$Proxy,
    [Alias("NoModifyPath")]
    [switch]$SkipInit
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:Repository = "refiget/postui"
$script:TempRoot = $null
$script:InstallStage = $null
$script:PackageVersion = $null

function Fail([string]$Message) {
    throw "安装失败: $Message"
}

function Get-DefaultInstallDir {
    $localAppData = $env:LOCALAPPDATA
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        $localAppData = [Environment]::GetFolderPath("LocalApplicationData")
    }
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        Fail "无法确定 LOCALAPPDATA"
    }
    return (Join-Path $localAppData "Programs\PostUI")
}

function Get-PackageDirectory([string]$Root) {
    $directBinary = Join-Path $Root "postui.exe"
    if (Test-Path -LiteralPath $directBinary -PathType Leaf) {
        return (Get-Item -LiteralPath $directBinary).DirectoryName
    }

    $nestedBinary = Get-ChildItem -LiteralPath $Root -Filter "postui.exe" -File -Recurse |
        Select-Object -First 1
    if ($null -eq $nestedBinary) {
        Fail "发布包缺少 postui.exe"
    }
    return $nestedBinary.DirectoryName
}

function Download-Package([string]$Url) {
    $script:TempRoot = Join-Path ([IO.Path]::GetTempPath()) ("postui-install-{0}" -f [Guid]::NewGuid())
    $extractPath = Join-Path $script:TempRoot "package"
    $archivePath = Join-Path $script:TempRoot "postui.zip"
    New-Item -ItemType Directory -Path $extractPath -Force | Out-Null

    Write-Host "  url: $Url"
    $curl = Get-Command -Name "curl.exe" -CommandType Application -ErrorAction SilentlyContinue
    if ($null -ne $curl) {
        $arguments = @(
            "--fail",
            "--location",
            "--retry", "3",
            "--retry-delay", "1",
            "--connect-timeout", "15",
            "--progress-bar",
            "--output", $archivePath
        )
        if (![string]::IsNullOrWhiteSpace($Proxy)) {
            $arguments = @("--proxy", $Proxy) + $arguments
        }
        & curl.exe @arguments $Url
        if ($LASTEXITCODE -ne 0) {
            Fail "下载失败"
        }
    } else {
        $request = @{
            Uri             = $Url
            OutFile         = $archivePath
            UseBasicParsing = $true
        }
        if (![string]::IsNullOrWhiteSpace($Proxy)) {
            $request.Proxy = New-Object System.Net.WebProxy($Proxy)
        }
        Invoke-WebRequest @request
    }

    $tar = Get-Command -Name "tar.exe" -CommandType Application -ErrorAction SilentlyContinue
    if ($null -ne $tar) {
        & tar.exe -xf $archivePath -C $extractPath
        if ($LASTEXITCODE -ne 0) {
            Fail "解压失败"
        }
    } else {
        $previousProgressPreference = $ProgressPreference
        try {
            $ProgressPreference = "SilentlyContinue"
            Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath -Force | Out-Null
        } catch {
            Fail "解压失败"
        } finally {
            $ProgressPreference = $previousProgressPreference
        }
    }
    return (Get-PackageDirectory $extractPath)
}

function Get-PackageVersion([string]$BinaryPath) {
    $output = @(& $BinaryPath --version 2>&1)
    if ($LASTEXITCODE -ne 0) {
        Fail "版本校验失败"
    }
    $line = $output | Select-Object -First 1
    if ($null -eq $line) {
        Fail "版本校验失败"
    }
    $packageVersion = ([string]$line).Trim()
    if ($packageVersion -notmatch '^postui\s+') {
        Fail "版本校验失败"
    }
    return $packageVersion
}

function Install-Package([string]$PackageDirectory) {
    $binaryPath = Join-Path $PackageDirectory "postui.exe"
    if (!(Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
        Fail "发布包缺少 postui.exe"
    }
    if (Test-Path -LiteralPath $InstallDir -PathType Leaf) {
        Fail "安装目录无效"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $script:InstallStage = Join-Path $InstallDir (".postui-install-{0}" -f [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $script:InstallStage -Force | Out-Null

    $stagedBinary = Join-Path $script:InstallStage "postui.exe"
    Copy-Item -LiteralPath $binaryPath -Destination $stagedBinary -Force
    $null = Get-PackageVersion $stagedBinary

    $destination = Join-Path $InstallDir "postui.exe"
    if (Test-Path -LiteralPath $destination -PathType Container) {
        Fail "安装目标无效"
    }
    Move-Item -LiteralPath $stagedBinary -Destination $destination -Force
    Remove-Item -LiteralPath $script:InstallStage -Recurse -Force
    $script:InstallStage = $null
}

function Set-UserPath {
    $installedBinary = Join-Path $InstallDir "postui.exe"
    & $installedBinary init *> $null
    if ($LASTEXITCODE -ne 0) {
        Fail "PATH 配置失败"
    }
}

function Remove-TemporaryPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        if (!(Test-Path -LiteralPath $Path)) {
            return
        }
        Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
        if (!(Test-Path -LiteralPath $Path)) {
            return
        }
        Start-Sleep -Milliseconds 200
    }
}

function Read-EnvironmentValue([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        return $null
    }
    return $value
}

try {
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($architecture)) {
        $architecture = $env:PROCESSOR_ARCHITECTURE
    }
    if ($architecture -ne "AMD64") {
        Fail "仅支持 Windows amd64"
    }

    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Read-EnvironmentValue "POSTUI_INSTALL_DIR"
    }
    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Get-DefaultInstallDir
    }
    $InstallDir = [IO.Path]::GetFullPath($InstallDir)

    if ([string]::IsNullOrWhiteSpace($ArchiveUrl)) {
        $ArchiveUrl = Read-EnvironmentValue "POSTUI_ARCHIVE_URL"
    }
    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = Read-EnvironmentValue "POSTUI_VERSION"
    }
    if ([string]::IsNullOrWhiteSpace($Proxy)) {
        $Proxy = Read-EnvironmentValue "POSTUI_PROXY"
    }

    $skipInitValue = Read-EnvironmentValue "POSTUI_SKIP_INIT"
    if ([string]::IsNullOrWhiteSpace($skipInitValue)) {
        $skipInitValue = Read-EnvironmentValue "POSTUI_NO_MODIFY_PATH"
    }
    if (![string]::IsNullOrWhiteSpace($skipInitValue)) {
        if ($skipInitValue -match '^(1|true|yes)$') {
            $SkipInit = $true
        } elseif ($skipInitValue -notmatch '^(0|false|no)$') {
            Fail "POSTUI_SKIP_INIT 只能是 0 或 1"
        }
    }

    if (![string]::IsNullOrWhiteSpace($Version)) {
        $Version = $Version -replace '^v', ''
        if ($Version -notmatch '^[0-9A-Za-z.-]+$') {
            Fail "无效版本号"
        }
    }

    $packageDirectory = $null
    $scriptDirectory = $PSScriptRoot
    if ([string]::IsNullOrWhiteSpace($ArchiveUrl) -and
        [string]::IsNullOrWhiteSpace($Version) -and
        ![string]::IsNullOrWhiteSpace($scriptDirectory)) {
        $localBinary = Join-Path $scriptDirectory "postui.exe"
        if (Test-Path -LiteralPath $localBinary -PathType Leaf) {
            $packageDirectory = $scriptDirectory
        }
    }

    if ($null -eq $packageDirectory) {
        if ([string]::IsNullOrWhiteSpace($ArchiveUrl)) {
            if ([string]::IsNullOrWhiteSpace($Version)) {
                $releasePath = "latest/download"
                $releaseLabel = "latest"
            } else {
                $releasePath = "download/v$Version"
                $releaseLabel = $Version
            }
            $ArchiveUrl = "$($script:Repository)/releases/$releasePath/postui-windows-amd64.zip"
            $ArchiveUrl = "https://github.com/$ArchiveUrl"
        } else {
            $releaseLabel = "custom"
        }
        Write-Host "downloading postui $releaseLabel x86_64-pc-windows-msvc"
        $packageDirectory = Download-Package $ArchiveUrl
    } else {
        Write-Host "using local package $packageDirectory"
    }

    Write-Host "installing to $InstallDir"
    $script:PackageVersion = Get-PackageVersion (Join-Path $packageDirectory "postui.exe")
    if (![string]::IsNullOrWhiteSpace($Version)) {
        $versionPattern = '^postui\s+' + [regex]::Escape($Version) + '(\s|$)'
        if ($script:PackageVersion -notmatch $versionPattern) {
            Fail "版本不匹配"
        }
    }
    Write-Host "  version: $($script:PackageVersion)"
    Install-Package $packageDirectory

    $installedVersion = Get-PackageVersion (Join-Path $InstallDir "postui.exe")
    if ($installedVersion -ne $script:PackageVersion) {
        Fail "安装校验失败"
    }
    Write-Host "everything's installed!"

    if (!$SkipInit) {
        Set-UserPath
        Write-Host "To add $InstallDir to your PATH, either restart your shell or run:"
        Write-Host ('  $env:Path = "{0};$env:Path"' -f $InstallDir)
    } else {
        Write-Host "PATH modification skipped"
    }
} catch {
    Write-Error $_.Exception.Message
    exit 1
} finally {
    Remove-TemporaryPath $script:InstallStage
    Remove-TemporaryPath $script:TempRoot
}
