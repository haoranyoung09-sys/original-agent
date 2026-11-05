#Requires -Version 5.1
<#
.SYNOPSIS
  Install prebuilt `original` from GitHub Releases (no Rust required).

.PARAMETER Version
  `latest` or `vX.Y.Z` (must match a published Release tag).

.PARAMETER BaseUrl
  Override download base. HTTPS GitHub URL, or a local directory that already
  contains the asset + `.sha256` (for offline / installer tests).
  Env: ORIGINAL_INSTALL_BASE

.PARAMETER BinDir
  Install directory for `original.exe`. Default: %USERPROFILE%\.original-agent\bin
  Env: ORIGINAL_INSTALL_BIN

.PARAMETER NoPathUpdate
  Do not modify the user PATH. Env: ORIGINAL_INSTALL_NO_PATH=1
#>
param(
    [string]$Version = "latest",
    [string]$BaseUrl = "",
    [string]$BinDir = "",
    [switch]$NoPathUpdate
)

$ErrorActionPreference = "Stop"
$Repo = "haoranyoung09-sys/original-agent"

if ($Version -ne "latest" -and $Version -notmatch '^v\d+\.\d+\.\d+$') {
    throw "Version must be latest or vX.Y.Z"
}

if ([string]::IsNullOrWhiteSpace($BaseUrl) -and $env:ORIGINAL_INSTALL_BASE) {
    $BaseUrl = [string]$env:ORIGINAL_INSTALL_BASE
}
if ([string]::IsNullOrWhiteSpace($BinDir) -and $env:ORIGINAL_INSTALL_BIN) {
    $BinDir = [string]$env:ORIGINAL_INSTALL_BIN
}
if (-not $NoPathUpdate -and $env:ORIGINAL_INSTALL_NO_PATH -eq "1") {
    $NoPathUpdate = $true
}

# Environment variables work on Windows PowerShell 5.1 as well as PowerShell 7.
$Arch = [string]$env:PROCESSOR_ARCHITEW6432
if ([string]::IsNullOrWhiteSpace($Arch)) { $Arch = [string]$env:PROCESSOR_ARCHITECTURE }
if ($Arch -ne "AMD64") { throw "Only Windows x64 releases are available" }

$Asset = "original-windows-x86_64.exe"

if ([string]::IsNullOrWhiteSpace($BaseUrl)) {
    if ($Version -eq "latest") {
        $BaseUrl = "https://github.com/$Repo/releases/latest/download"
    } else {
        $BaseUrl = "https://github.com/$Repo/releases/download/$Version"
    }
} else {
    $BaseUrl = $BaseUrl.TrimEnd('\', '/')
}

if ([string]::IsNullOrWhiteSpace($BinDir)) {
    $BinDir = Join-Path $env:USERPROFILE '.original-agent\bin'
}

function Test-LocalReleaseDir {
    param([string]$Path)
    return (Test-Path -LiteralPath $Path -PathType Container)
}

function Get-ReleaseFile {
    param([string]$Url, [string]$OutFile)

    # Local fixture / offline mirror: BaseUrl is a directory.
    if (Test-LocalReleaseDir $BaseUrl) {
        $name = Split-Path -Leaf $OutFile
        # Allow callers to pass either the asset name or 'checksum' temp name.
        if ($name -eq 'checksum') { $name = "$Asset.sha256" }
        $src = Join-Path $BaseUrl $name
        if (-not (Test-Path -LiteralPath $src)) {
            throw "Local release asset not found: $src"
        }
        Copy-Item -LiteralPath $src -Destination $OutFile -Force
        return
    }

    try {
        Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile -TimeoutSec 30
        return
    } catch {
        $Response = $_.Exception.Response
        if ($null -ne $Response -and [int]$Response.StatusCode -eq 404) {
            throw "Release asset not found (HTTP 404). Publish a completed Release with binary and checksum first: $Url"
        }
        if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue)) {
            throw "GitHub download failed. Check network/proxy access to GitHub. $($_.Exception.Message)"
        }
        Write-Host 'PowerShell download failed; retrying with curl.exe...'
        & curl.exe --fail --location --retry 2 --retry-all-errors --retry-max-time 90 --connect-timeout 10 --max-time 120 --proto '=https' --proto-redir '=https' --output $OutFile $Url
        $DownloadExit = $LASTEXITCODE
        if ($DownloadExit -ne 0) {
            throw "curl download failed (exit $DownloadExit). Exit 22: HTTP error (check Release assets); 28: timeout; 56: connection reset (check proxy/network); 35/60: TLS/certificate failure. Check network/proxy and https://github.com/$Repo/releases. URL: $Url"
        }
    }
}

$Temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $AssetPath = Join-Path $Temp $Asset
    $ChecksumPath = Join-Path $Temp 'checksum'
    if (Test-LocalReleaseDir $BaseUrl) {
        Get-ReleaseFile -Url '' -OutFile $AssetPath
        Get-ReleaseFile -Url '' -OutFile $ChecksumPath
    } else {
        Get-ReleaseFile -Url "$BaseUrl/$Asset" -OutFile $AssetPath
        Get-ReleaseFile -Url "$BaseUrl/$Asset.sha256" -OutFile $ChecksumPath
    }

    $ChecksumText = [string](Get-Content -LiteralPath $ChecksumPath -Raw)
    if ([string]::IsNullOrWhiteSpace($ChecksumText)) {
        throw 'Downloaded checksum is empty; check the Release assets.'
    }
    $Expected = ($ChecksumText.Trim() -split '\s+')[0]
    if ($Expected -notmatch '^[a-fA-F0-9]{64}$') { throw "Invalid checksum" }
    $Actual = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash
    if ($Actual -ne $Expected) {
        throw "Checksum mismatch (expected $Expected, got $Actual)"
    }

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $Dest = Join-Path $BinDir 'original.exe'
    Copy-Item -LiteralPath $AssetPath -Destination $Dest -Force

    if (-not $NoPathUpdate) {
        $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ([string]::IsNullOrEmpty($UserPath)) { $UserPath = '' }
        $parts = @($UserPath -split ';' | Where-Object { $_ -ne '' })
        if ($parts -notcontains $BinDir) {
            [Environment]::SetEnvironmentVariable('Path', (($BinDir + ';' + $UserPath).TrimEnd(';')), 'User')
        }
        $sessionParts = @($env:Path -split ';' | Where-Object { $_ -ne '' })
        if ($sessionParts -notcontains $BinDir) {
            $env:Path = "$BinDir;$env:Path"
        }
    }

    Write-Host "Installed original -> $Dest"
    if ($NoPathUpdate) {
        Write-Host "PATH update skipped (-NoPathUpdate)."
    } else {
        Write-Host "Run: original (or reopen your terminal)."
    }
} catch {
    throw "Installation failed: $($_.Exception.Message) [source: $BaseUrl; line: $($_.InvocationInfo.ScriptLineNumber)]"
} finally {
    Remove-Item -LiteralPath $Temp -Recurse -Force -ErrorAction SilentlyContinue
}
