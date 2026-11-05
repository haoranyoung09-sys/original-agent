#Requires -Version 5.1
<#
.SYNOPSIS
  Package target/release/original.exe as a local fake Release directory
  (asset + .sha256) for install.ps1 -BaseUrl testing or manual upload.
#>
param(
    [string]$RepoRoot = "",
    [string]$OutDir = ""
)
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $RepoRoot 'dist\local-release'
}
$Src = Join-Path $RepoRoot 'target\release\original.exe'
if (-not (Test-Path -LiteralPath $Src)) {
    throw "Missing $Src — run cargo build --locked --release first"
}
$Asset = 'original-windows-x86_64.exe'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Dest = Join-Path $OutDir $Asset
Copy-Item -LiteralPath $Src -Destination $Dest -Force
$Hash = (Get-FileHash -LiteralPath $Dest -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $OutDir "$Asset.sha256") -Value "$Hash  $Asset`n" -NoNewline
Write-Host "Packaged $OutDir"
Write-Host "Install with:"
Write-Host "  powershell -File .\scripts\install.ps1 -BaseUrl `"$OutDir`""
