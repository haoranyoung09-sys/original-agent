# Offline installer tests for scripts/install.ps1 (Windows PowerShell 5.1+).
# Uses local fixtures — never hits GitHub, never writes the real user PATH.
$ErrorActionPreference = 'Stop'
$Installer = Join-Path $PSScriptRoot 'install.ps1'
$Pass = 0

function Assert-ThrowsLike {
    param([scriptblock]$Block, [string]$Pattern, [string]$Label)
    try {
        & $Block
        throw "Expected failure for: $Label"
    } catch {
        $msg = [string]$_.Exception.Message
        if ($msg -notlike $Pattern) {
            throw "FAIL $Label`n  expected like: $Pattern`n  got: $msg"
        }
        Write-Output "PASS: $Label"
        $script:Pass++
    }
}

function Invoke-WebRequest {
    param([switch]$UseBasicParsing, $OutFile, $Uri, $TimeoutSec, [Parameter(ValueFromRemainingArguments = $true)]$Rest)
    Set-Content -LiteralPath $OutFile -Value '' -NoNewline
}

Assert-ThrowsLike -Label 'empty checksum rejected without null-method error' -Pattern '*checksum is empty*' -Block {
    & $Installer -NoPathUpdate
}

Assert-ThrowsLike -Label 'invalid version rejected' -Pattern '*Version must be*' -Block {
    & $Installer -Version '../invalid' -NoPathUpdate
}

function Invoke-WebRequest { throw 'Simulated connection closed' }
function curl.exe { $global:LASTEXITCODE = 28 }
Assert-ThrowsLike -Label 'transport failure falls back and reports timeout' -Pattern '*curl download failed (exit 28)*' -Block {
    & $Installer -NoPathUpdate
}

function curl.exe {
    if ($args -notcontains '--retry-all-errors') { throw 'Missing connection reset retry option' }
    $global:LASTEXITCODE = 56
}
Assert-ThrowsLike -Label 'reset retries configured and reset diagnostic preserved' -Pattern '*curl download failed (exit 56)*' -Block {
    & $Installer -NoPathUpdate
}

function curl.exe { $global:LASTEXITCODE = 22 }
Assert-ThrowsLike -Label 'HTTP 404 maps to missing-release guidance' -Pattern '*Release asset not found*' -Block {
    & $Installer -NoPathUpdate
}
Assert-ThrowsLike -Label 'missing-release guidance mentions local-build fallback' -Pattern '*install-from-local-build*' -Block {
    & $Installer -NoPathUpdate
}

Remove-Item Function:Invoke-WebRequest -ErrorAction SilentlyContinue
Remove-Item Function:curl.exe -ErrorAction SilentlyContinue

$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("original-install-fixture-" + [Guid]::NewGuid().ToString('N'))
$BinRoot = Join-Path ([IO.Path]::GetTempPath()) ("original-install-bin-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $FixtureRoot | Out-Null
New-Item -ItemType Directory -Path $BinRoot | Out-Null
try {
    $Asset = 'original-windows-x86_64.exe'
    $Payload = "original-fixture-binary-$([Guid]::NewGuid().ToString('N'))"
    $AssetPath = Join-Path $FixtureRoot $Asset
    [IO.File]::WriteAllText($AssetPath, $Payload)
    $Hash = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $FixtureRoot "$Asset.sha256") -Value "$Hash  $Asset`n" -NoNewline

    $BeforeUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')

    & $Installer -Version v0.3.4 -BaseUrl $FixtureRoot -BinDir $BinRoot -NoPathUpdate
    $Dest = Join-Path $BinRoot 'original.exe'
    if (-not (Test-Path -LiteralPath $Dest)) { throw 'FAIL happy path: original.exe missing' }
    $Installed = [IO.File]::ReadAllText($Dest)
    if ($Installed -ne $Payload) { throw 'FAIL happy path: installed bytes differ from fixture' }
    $AfterUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($AfterUserPath -ne $BeforeUserPath) {
        throw 'FAIL happy path: user PATH changed despite -NoPathUpdate'
    }
    Write-Output 'PASS: local fixture installs with SHA-256 check and skips PATH'
    $Pass++

    & $Installer -Version latest -BaseUrl $FixtureRoot -BinDir $BinRoot -NoPathUpdate
    Write-Output 'PASS: latest + local BaseUrl reinstalls'
    $Pass++

    Set-Content -LiteralPath (Join-Path $FixtureRoot "$Asset.sha256") -Value (("0" * 64) + "  $Asset`n") -NoNewline
    Assert-ThrowsLike -Label 'checksum mismatch rejected' -Pattern '*Checksum mismatch*' -Block {
        & $Installer -BaseUrl $FixtureRoot -BinDir $BinRoot -NoPathUpdate
    }

    Remove-Item -LiteralPath $AssetPath -Force
    Assert-ThrowsLike -Label 'missing local asset rejected' -Pattern '*Local release asset not found*' -Block {
        & $Installer -BaseUrl $FixtureRoot -BinDir $BinRoot -NoPathUpdate
    }
}
finally {
    Remove-Item -LiteralPath $FixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $BinRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output "All installer tests passed ($Pass)."
