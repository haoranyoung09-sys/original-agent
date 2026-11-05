#Requires -Version 5.1
<#
.SYNOPSIS
  Temporary no-GitHub path: install `original.exe` from this repo's
  `target/release/original.exe` into %USERPROFILE%\.original-agent\bin.

  Use when GitHub Releases have no assets yet. Requires a prior
  `cargo build --locked --release` (or CI artifact copied here).
#>
param(
    [string]$RepoRoot = "",
    [string]$BinDir = "",
    [switch]$NoPathUpdate
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($BinDir)) {
    if ($env:ORIGINAL_INSTALL_BIN) { $BinDir = [string]$env:ORIGINAL_INSTALL_BIN }
    else { $BinDir = Join-Path $env:USERPROFILE '.original-agent\bin' }
}
if (-not $NoPathUpdate -and $env:ORIGINAL_INSTALL_NO_PATH -eq "1") {
    $NoPathUpdate = $true
}

$Src = Join-Path $RepoRoot 'target\release\original.exe'
if (-not (Test-Path -LiteralPath $Src)) {
    throw @"
Local release binary not found: $Src

Build first (needs Rust):
  cargo build --locked --release
Or publish a GitHub Release, then use scripts/install.ps1 / irm|iex.
"@
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
$Dest = Join-Path $BinDir 'original.exe'
Copy-Item -LiteralPath $Src -Destination $Dest -Force

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

Write-Host "Installed original -> $Dest (from local build)"
if ($NoPathUpdate) {
    Write-Host "PATH update skipped."
} else {
    Write-Host "Run: original (reopen terminal if command not found)."
}
