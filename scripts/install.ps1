# Install prebuilt `original` from GitHub Releases (no Rust required).
# Do NOT put #Requires here: interactive / stdin / some irm|iex paths treat it as a
# top-level command and fail with "在创建管道时出错" (pipeline creation error), then
# dump the rest of the file into the prompt (>> after incomplete param().
# Prefer:  iex (irm 'https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install.ps1')
# Avoid:   irm ... | iex   (works for a single string in PS 5.1, but less reliable)
# File:    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1
# Params:  -Version latest|vX.Y.Z  -BaseUrl <url|dir>  -BinDir <dir>  -NoPathUpdate
# Env:     ORIGINAL_INSTALL_BASE  ORIGINAL_INSTALL_BIN  ORIGINAL_INSTALL_NO_PATH=1
param(
    [string]$Version = "latest",
    [string]$BaseUrl = "",
    [string]$BinDir = "",
    [switch]$NoPathUpdate
)
$ErrorActionPreference = "Stop"
$Repo = "haoranyoung09-sys/original-agent"

function Get-MissingReleaseHelp {
    param([string]$Url)
    return @"
Release asset not found (HTTP 404): $Url

No usable GitHub Release assets yet (empty Releases page or missing original-windows-x86_64.exe).
The no-Rust remote installer (iex (irm ...)) cannot succeed until a Release with attachments is published.

What to do now:
  1) Publish a Release: push a tag matching Cargo.toml (e.g. v0.3.5), wait for Actions "Release binaries", then retry: iex (irm <raw-url>)
  2) If this repo already has target\release\original.exe:
       powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install-from-local-build.ps1
  3) If you have Rust:
       cargo install --locked --path . --force

See: https://github.com/$Repo/releases
"@
}
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

function Get-HttpStatusCode {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        if ($ex.Response -and $ex.Response.StatusCode) {
            try { return [int]$ex.Response.StatusCode } catch { }
        }
        $ex = $ex.InnerException
    }
    $msg = [string]$ErrorRecord.Exception.Message
    if ($msg -match '\(404\)' -or $msg -match '404') { return 404 }
    return $null
}

function Get-ReleaseFile {
    param([string]$Url, [string]$OutFile)

    if (Test-LocalReleaseDir $BaseUrl) {
        $name = Split-Path -Leaf $OutFile
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
        $code = Get-HttpStatusCode $_
        if ($code -eq 404) {
            throw (Get-MissingReleaseHelp -Url $Url)
        }
        if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue)) {
            throw "GitHub download failed. Check network/proxy access to GitHub. $($_.Exception.Message)"
        }
        Write-Host "PowerShell download failed (http=$code); retrying with curl.exe..."
        & curl.exe --fail --location --retry 2 --retry-all-errors --retry-max-time 90 --connect-timeout 10 --max-time 120 --proto '=https' --proto-redir '=https' --silent --show-error --output $OutFile $Url
        $DownloadExit = $LASTEXITCODE
        if ($DownloadExit -eq 22) {
            throw (Get-MissingReleaseHelp -Url $Url)
        }
        if ($DownloadExit -ne 0) {
            throw "curl download failed (exit $DownloadExit). Exit 28: timeout; 56: connection reset (check proxy/network); 35/60: TLS/certificate failure. Check network/proxy and https://github.com/$Repo/releases. URL: $Url"
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
