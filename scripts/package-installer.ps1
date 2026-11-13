<#
.SYNOPSIS
    Build the NØVA Flash Studio Windows installer (setup wizard + uninstaller).

.DESCRIPTION
    Produces dist/windows/NOVA-Flash-Studio-<version>-windows-x64-Setup.exe, a
    self-contained single-file executable that embeds the portable ZIP produced
    by scripts/package-windows.ps1. The setup exe doubles as the uninstaller:
    it is copied into the install directory during setup and removes the
    installation when invoked with --uninstall.

    Requires the .NET 8 SDK (dotnet) on PATH. No WiX/Inno/NSIS dependency.

.PARAMETER SkipTests
    Forwarded to package-windows.ps1 when the payload is rebuilt.

.PARAMETER UseExistingPayload
    Reuse the newest NOVA-Flash-Studio-*-windows-x64.zip in dist/windows instead
    of rebuilding the portable package first.

.PARAMETER NoInstaller
    Only (re)build the portable ZIP payload; do not compile the installer.
#>
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$UseExistingPayload,
    [switch]$NoInstaller
)

function Invoke-Native {
    # WinExe output is unreliable through PowerShell's & operator; capture via
    # ProcessStartInfo like package-windows.ps1 does for the app smoke tests.
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$Arguments
    )
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $Executable
    $startInfo.Arguments = $Arguments
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $startInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $process = [System.Diagnostics.Process]::Start($startInfo)
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        Output = $stdout.Trim()
        Error = $stderr.Trim()
    }
}

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$distDir = Join-Path $root 'dist\windows'

# --- Resolve version ---------------------------------------------------------
$manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
$versionMatch = [regex]::Match(
    $manifest,
    '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"'
)
if (-not $versionMatch.Success) {
    throw 'Workspace version not found in Cargo.toml.'
}
$version = $versionMatch.Groups[1].Value

# --- Payload ZIP --------------------------------------------------------------
$payloadZip = $null
if (-not $UseExistingPayload) {
    Write-Host '== Building portable payload ==' -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot 'package-windows.ps1') -SkipTests:$SkipTests
    if ($LASTEXITCODE -ne 0) { throw "package-windows.ps1 failed with exit code $LASTEXITCODE" }
}

$payloadZip = Get-ChildItem -LiteralPath $distDir -Filter "NOVA-Flash-Studio-$version-windows-x64.zip" -File |
    Select-Object -First 1
if ($null -eq $payloadZip) {
    $payloadZip = Get-ChildItem -LiteralPath $distDir -Filter 'NOVA-Flash-Studio-*-windows-x64.zip' -File |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if ($null -eq $payloadZip) {
    throw "No portable payload ZIP found in $distDir. Run package-windows.ps1 first."
}
Write-Host "Payload: $($payloadZip.FullName)" -ForegroundColor Cyan

if ($NoInstaller) {
    Write-Host 'Payload rebuilt; installer skipped (-NoInstaller).' -ForegroundColor Green
    return
}

# --- dotnet publish ------------------------------------------------------------
$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($null -eq $dotnet) {
    throw 'dotnet SDK not found on PATH. Install .NET 8 SDK to build the installer.'
}

$publishDir = Join-Path $distDir 'installer-publish'
if (Test-Path -LiteralPath $publishDir) {
    Remove-Item -LiteralPath $publishDir -Recurse -Force
}
New-Item -ItemType Directory -Path $publishDir -Force | Out-Null

$csproj = Join-Path $root 'packaging\windows\installer\NovaFlashStudio.Setup.csproj'
Write-Host '== dotnet publish (single-file, self-contained) ==' -ForegroundColor Cyan
& dotnet publish $csproj `
    -c Release `
    -r win-x64 `
    --self-contained true `
    -p:PublishSingleFile=true `
    -p:EnableCompressionInSingleFile=true `
    -p:IncludeNativeLibrariesForSelfExtract=true `
    -p:NovaPayloadZip="$($payloadZip.FullName)" `
    -p:NovaAppVersion="$version" `
    -o "$publishDir"
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed with exit code $LASTEXITCODE" }

$publishedExe = Join-Path $publishDir 'NOVA-Flash-Studio-Setup.exe'
if (-not (Test-Path -LiteralPath $publishedExe -PathType Leaf)) {
    throw "Missing published installer: $publishedExe"
}

$productName = 'N' + [char]0x00D8 + 'VA Flash Studio'
$setupName = "NOVA-Flash-Studio-$version-windows-x64-Setup.exe"
$setupPath = Join-Path $distDir $setupName
Copy-Item -LiteralPath $publishedExe -Destination $setupPath -Force

# --- Smoke tests ---------------------------------------------------------------
$versionResult = Invoke-Native -Executable $setupPath -Arguments '--version'
if ($versionResult.ExitCode -ne 0 -or $versionResult.Output -ne "$productName Setup $version") {
    throw "Unexpected --version output: '$($versionResult.Output)' (exit $($versionResult.ExitCode)) $($versionResult.Error)"
}
Write-Host "  $($versionResult.Output)"

$smokeResult = Invoke-Native -Executable $setupPath -Arguments '--smoke-test'
if ($smokeResult.ExitCode -ne 0 -or $smokeResult.Output -notmatch 'smoke-test: OK') {
    throw "Smoke test failed: $($smokeResult.Output) $($smokeResult.Error)"
}
Write-Host $smokeResult.Output

# --- Checksums ------------------------------------------------------------------
$checksumPath = Join-Path $distDir 'SHA256SUMS.txt'
$lines = @()
if (Test-Path -LiteralPath $checksumPath) {
    # Windows PowerShell 5.1 reads BOM-less files as ANSI by default. The
    # checksum list contains the Unicode product name, so read it explicitly as
    # UTF-8; otherwise the next rewrite double-encodes the name.
    $lines = Get-Content -LiteralPath $checksumPath -Encoding UTF8 |
        Where-Object { $_ -and $_ -notmatch [regex]::Escape($setupName) }
}
$lines += (Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash + "  $setupName"
[System.IO.File]::WriteAllLines($checksumPath, $lines, [System.Text.UTF8Encoding]::new($false))

$sizeMb = [math]::Round((Get-Item -LiteralPath $setupPath).Length / 1MB, 1)
Write-Host ''
Write-Host 'Installer created:' -ForegroundColor Green
Write-Host "  $setupPath ($sizeMb MB)"
Write-Host "  SHA256: $((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash)"
Write-Host '  Checksums appended to dist/windows/SHA256SUMS.txt'
