param([string]$Version = "latest")
$ErrorActionPreference = "Stop"
$Repo = "haoranyoung09-sys/original-agent"
if ($Version -ne "latest" -and $Version -notmatch '^v\d+\.\d+\.\d+$') { throw "Version must be latest or vX.Y.Z" }
# Environment variables work on Windows PowerShell 5.1 as well as PowerShell 7.
$Arch = [string]$env:PROCESSOR_ARCHITEW6432
if ([string]::IsNullOrWhiteSpace($Arch)) { $Arch = [string]$env:PROCESSOR_ARCHITECTURE }
if ($Arch -ne "AMD64") { throw "Only Windows x64 releases are available" }
$Asset = "original-windows-x86_64.exe"
$Base = if ($Version -eq "latest") { "https://github.com/$Repo/releases/latest/download" } else { "https://github.com/$Repo/releases/download/$Version" }
$Temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing "$Base/$Asset" -OutFile (Join-Path $Temp $Asset)
    Invoke-WebRequest -UseBasicParsing "$Base/$Asset.sha256" -OutFile (Join-Path $Temp 'checksum')
    $ChecksumText = [string](Get-Content (Join-Path $Temp 'checksum') -Raw)
    if ([string]::IsNullOrWhiteSpace($ChecksumText)) { throw 'Downloaded checksum is empty; check the Release assets.' }
    $Expected = ($ChecksumText.Trim() -split '\s+')[0]
    if ($Expected -notmatch '^[a-fA-F0-9]{64}$') { throw "Invalid checksum" }
    if ((Get-FileHash (Join-Path $Temp $Asset) -Algorithm SHA256).Hash -ne $Expected) { throw "Checksum mismatch" }
    $Bin = Join-Path $env:USERPROFILE '.original-agent\bin'
    New-Item -ItemType Directory -Force -Path $Bin | Out-Null
    Copy-Item -LiteralPath (Join-Path $Temp $Asset) -Destination (Join-Path $Bin 'original.exe') -Force
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($UserPath -split ';') -notcontains $Bin) {
        [Environment]::SetEnvironmentVariable('Path', "$Bin;$UserPath", 'User')
    }
    if (($env:Path -split ';') -notcontains $Bin) { $env:Path = "$Bin;$env:Path" }
    Write-Host "Installed original. Run: original (or reopen your terminal)."
} catch {
    throw "Installation failed: $($_.Exception.Message) [source: $Base; line: $($_.InvocationInfo.ScriptLineNumber)]"
} finally {
    Remove-Item -LiteralPath $Temp -Recurse -Force
}
