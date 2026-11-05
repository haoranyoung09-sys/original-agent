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
function Get-ReleaseFile {
    param([string]$Url, [string]$OutFile)
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
        & curl.exe --fail --location --retry 2 --connect-timeout 10 --max-time 120 --proto '=https' --proto-redir '=https' --output $OutFile $Url
        $DownloadExit = $LASTEXITCODE
        if ($DownloadExit -ne 0) {
            throw "curl download failed (exit $DownloadExit). Exit 22: HTTP error (check Release assets); 28: timeout; 35/60: TLS/certificate failure. Check network/proxy and https://github.com/$Repo/releases. URL: $Url"
        }
    }
}
$Temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    Get-ReleaseFile -Url "$Base/$Asset" -OutFile (Join-Path $Temp $Asset)
    Get-ReleaseFile -Url "$Base/$Asset.sha256" -OutFile (Join-Path $Temp 'checksum')
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
