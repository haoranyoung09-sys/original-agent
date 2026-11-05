$ErrorActionPreference = 'Stop'
# Mock downloads only; neither test reaches installation or modifies PATH.
$Installer = Join-Path $PSScriptRoot 'install.ps1'
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, $OutFile, $Uri, $TimeoutSec, [Parameter(ValueFromRemainingArguments=$true)]$Rest)
    Set-Content -LiteralPath $OutFile -Value '' -NoNewline
}
try {
    & $Installer
    throw 'Expected empty checksum rejection'
} catch {
    if ($_.Exception.Message -notlike '*checksum is empty*') { throw }
    Write-Output 'PASS: empty checksum rejected without null-method error'
}
try {
    & $Installer -Version '../invalid'
    throw 'Expected invalid version rejection'
} catch {
    if ($_.Exception.Message -notlike '*Version must be*') { throw }
    Write-Output 'PASS: invalid version rejected'
}

# Simulate a failed PowerShell transport and a failed curl fallback.
function Invoke-WebRequest { throw 'Simulated connection closed' }
function curl.exe { $global:LASTEXITCODE = 28 }
try {
    & $Installer
    throw 'Expected download failure'
} catch {
    if ($_.Exception.Message -notlike '*curl download failed (exit 28)*') { throw }
    Write-Output 'PASS: transport failure falls back and reports timeout'
}
