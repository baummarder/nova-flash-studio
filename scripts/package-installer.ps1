<#
.SYNOPSIS
    Build one edition's NØVA Windows installer (setup wizard + uninstaller).

.DESCRIPTION
    Produces dist/windows/<edition>/NOVA-<Edition>-Setup.exe, a self-contained
    single-file executable that embeds the portable ZIP produced by
    scripts/package-windows.ps1 for the same edition. The setup exe doubles
    as the uninstaller: it is copied into the install directory during setup
    and removes the installation when invoked with --uninstall.

    EditionInfo.cs is regenerated for the requested edition before dotnet
    publish and restored to the User defaults afterwards, so product name,
    executable name, uninstall key and file-association ProgIds always match
    the payload. Editions install side by side: distinct product names,
    install directories, uninstall keys and ProgIds.

    Requires the .NET 8 SDK (dotnet) on PATH. No WiX/Inno/NSIS dependency.

.PARAMETER Edition
    Product edition: user, edu or enterprise.

.PARAMETER SkipTests
    Forwarded to package-windows.ps1 when the payload is rebuilt.

.PARAMETER UseExistingPayload
    Reuse the newest portable ZIP for this edition instead of rebuilding it.

.PARAMETER NoInstaller
    Only (re)build the portable ZIP payload; do not compile the installer.
#>
[CmdletBinding()]
param(
    [ValidateSet('user', 'edu', 'enterprise')]
    [string]$Edition = 'user',
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
$distRoot = Join-Path $root 'dist'
$distDir = Join-Path $distRoot 'windows'
$editionDir = Join-Path $distDir $Edition

# Edition table — keep in sync with package-windows.ps1,
# crates/nova-core/src/edition.rs and EditionInfo.cs.
$editionTable = @{
    user = @{
        Product      = 'N' + [char]0x00D8 + 'VA Flash Studio'
        Slug         = 'User'
        PortableZip  = 'NOVA-User-Portable.zip'
        SetupExe     = 'NOVA-User-Setup.exe'
        AppExe       = 'N' + [char]0x00D8 + 'VA Flash Studio.exe'
        UninstallExe = 'N' + [char]0x00D8 + 'VA Flash Studio Uninstall.exe'
        UninstallKey = 'NOVAFlashStudio'
        ProgIdNova   = 'NOVAFlashStudio.Project'
        ProgIdSwf    = 'NOVAFlashStudio.SWF'
    }
    edu = @{
        Product      = 'N' + [char]0x00D8 + 'VA Flash Studio Edu'
        Slug         = 'Edu'
        PortableZip  = 'NOVA-Edu-Portable.zip'
        SetupExe     = 'NOVA-Edu-Setup.exe'
        AppExe       = 'N' + [char]0x00D8 + 'VA Flash Studio Edu.exe'
        UninstallExe = 'N' + [char]0x00D8 + 'VA Flash Studio Edu Uninstall.exe'
        UninstallKey = 'NOVAFlashStudioEdu'
        ProgIdNova   = 'NOVAFlashStudioEdu.Project'
        ProgIdSwf    = 'NOVAFlashStudioEdu.SWF'
    }
    enterprise = @{
        Product      = 'N' + [char]0x00D8 + 'VA Flash Studio Enterprise'
        Slug         = 'Enterprise'
        PortableZip  = 'NOVA-Enterprise-Portable.zip'
        SetupExe     = 'NOVA-Enterprise-Setup.exe'
        AppExe       = 'N' + [char]0x00D8 + 'VA Flash Studio Enterprise.exe'
        UninstallExe = 'N' + [char]0x00D8 + 'VA Flash Studio Enterprise Uninstall.exe'
        UninstallKey = 'NOVAFlashStudioEnterprise'
        ProgIdNova   = 'NOVAFlashStudioEnterprise.Project'
        ProgIdSwf    = 'NOVAFlashStudioEnterprise.SWF'
    }
}
$ed = $editionTable[$Edition]
$productName = $ed.Product

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
    Write-Host "== Building portable payload (edition: $Edition) ==" -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot 'package-windows.ps1') -Edition $Edition -SkipTests:$SkipTests
    if ($LASTEXITCODE -ne 0) { throw "package-windows.ps1 failed with exit code $LASTEXITCODE" }
}

$payloadZip = Join-Path $editionDir $ed.PortableZip
if (-not (Test-Path -LiteralPath $payloadZip -PathType Leaf)) {
    # Fall back to any portable zip in the edition directory (older naming).
    $payloadZip = Get-ChildItem -LiteralPath $editionDir -Filter '*-Portable*.zip' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $payloadZip -or -not (Test-Path -LiteralPath $payloadZip -PathType Leaf)) {
    throw "No portable payload ZIP found in $editionDir. Run package-windows.ps1 -Edition $Edition first."
}
Write-Host "Payload: $payloadZip" -ForegroundColor Cyan

if ($NoInstaller) {
    Write-Host 'Payload built; installer skipped (-NoInstaller).' -ForegroundColor Green
    return
}

# --- EditionInfo.cs ------------------------------------------------------------
$editionInfoPath = Join-Path $root 'packaging\windows\installer\EditionInfo.cs'
$userEditionInfo = @'
namespace NovaFlashStudio.Setup;

/// <summary>
/// Edition identity for the installer (M19). This file is regenerated by
/// scripts/package-installer.ps1 for each edition before dotnet publish and
/// restored to the User defaults afterwards. Do not edit by hand for
/// edition builds — change the script's edition table instead.
/// </summary>
internal static class EditionInfo
{
    public const string EditionId = "user";
    public const string ProductName = "NØVA Flash Studio";
    public const string AppExeName = "NØVA Flash Studio.exe";
    public const string UninstallerFileName = "NØVA Flash Studio Uninstall.exe";
    public const string UninstallKey = "NOVAFlashStudio";
    public const string ProgIdNova = "NOVAFlashStudio.Project";
    public const string ProgIdSwf = "NOVAFlashStudio.SWF";
    /// <summary>Distributed setup executable name (artifact file name).</summary>
    public const string SetupExeName = "NOVA-User-Setup.exe";
}
'@

$editionInfoCs = @"
namespace NovaFlashStudio.Setup;

/// <summary>
/// Edition identity for the installer (M19). GENERATED by
/// scripts/package-installer.ps1 — do not edit.
/// </summary>
internal static class EditionInfo
{
    public const string EditionId = "$Edition";
    public const string ProductName = "$productName";
    public const string AppExeName = "$($ed.AppExe)";
    public const string UninstallerFileName = "$($ed.UninstallExe)";
    public const string UninstallKey = "$($ed.UninstallKey)";
    public const string ProgIdNova = "$($ed.ProgIdNova)";
    public const string ProgIdSwf = "$($ed.ProgIdSwf)";
    /// <summary>Distributed setup executable name (artifact file name).</summary>
    public const string SetupExeName = "$($ed.SetupExe)";
}
"@

[System.IO.File]::WriteAllText($editionInfoPath, $editionInfoCs, [System.Text.UTF8Encoding]::new($false))

# --- dotnet publish ------------------------------------------------------------
$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($null -eq $dotnet) {
    [System.IO.File]::WriteAllText($editionInfoPath, $userEditionInfo, [System.Text.UTF8Encoding]::new($false))
    throw 'dotnet SDK not found on PATH. Install .NET 8 SDK to build the installer.'
}

$publishDir = Join-Path $editionDir 'installer-publish'
if (Test-Path -LiteralPath $publishDir) {
    Remove-Item -LiteralPath $publishDir -Recurse -Force
}
New-Item -ItemType Directory -Path $publishDir -Force | Out-Null

$csproj = Join-Path $root 'packaging\windows\installer\NovaFlashStudio.Setup.csproj'
Write-Host "== dotnet publish (single-file, self-contained, edition: $Edition) ==" -ForegroundColor Cyan
try {
    & dotnet publish $csproj `
        -c Release `
        -r win-x64 `
        --self-contained true `
        -p:PublishSingleFile=true `
        -p:EnableCompressionInSingleFile=true `
        -p:IncludeNativeLibrariesForSelfExtract=true `
        -p:NovaPayloadZip="$payloadZip" `
        -p:NovaAppVersion="$version" `
        -p:NovaProductName="$productName" `
        -p:NovaSetupAssemblyName="NOVA-$($ed.Slug)-Setup" `
        -o "$publishDir"
    if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed with exit code $LASTEXITCODE" }
} finally {
    # Always restore the committed User defaults so the working tree stays clean.
    [System.IO.File]::WriteAllText($editionInfoPath, $userEditionInfo, [System.Text.UTF8Encoding]::new($false))
}

$publishedExe = Join-Path $publishDir "NOVA-$($ed.Slug)-Setup.exe"
if (-not (Test-Path -LiteralPath $publishedExe -PathType Leaf)) {
    throw "Missing published installer: $publishedExe"
}

$setupName = $ed.SetupExe
$setupPath = Join-Path $editionDir $setupName
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
$checksumPath = Join-Path $editionDir 'SHA256SUMS.txt'
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
Write-Host "Installer created (edition: $Edition):" -ForegroundColor Green
Write-Host "  $setupPath ($sizeMb MB)"
Write-Host "  SHA256: $((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash)"
Write-Host "  Checksums appended to $checksumPath"
