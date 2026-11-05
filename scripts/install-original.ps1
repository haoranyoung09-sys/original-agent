# Build and install the current source using Cargo's standard user bin directory.
$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
cargo install --locked --path $ProjectRoot --force
if ($LASTEXITCODE -ne 0) { throw "Installation failed" }
Write-Host "Installed original. Ensure your Cargo bin directory is on PATH."
