<#
.SYNOPSIS
    Build all three NØVA editions (user, edu, enterprise) for Windows.

.DESCRIPTION
    Runs package-windows.ps1 and package-installer.ps1 once per edition,
    producing six artifacts under dist/windows/<edition>/:

      NOVA-<Edition>-Portable.zip
      NOVA-<Edition>-Setup.exe
      SHA256SUMS.txt / BUILD-INFO.json

    Editions are independent products from one shared Rust codebase; each
    gets its own branding, configuration directory and installer identity.
    See docs/EDITIONS.md.

.PARAMETER SkipTests
    Forwarded to package-windows.ps1 (tests run once per edition build;
    pass this when the workspace suite already ran).

.PARAMETER Editions
    Subset of editions to build. Default: all three.
#>
[CmdletBinding()]
param(
    [ValidateSet('user', 'edu', 'enterprise')]
    [string[]]$Editions = @('user', 'edu', 'enterprise'),
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

foreach ($edition in $Editions) {
    Write-Host ''
    Write-Host "#################### Edition: $edition ####################" -ForegroundColor Magenta
    & (Join-Path $PSScriptRoot 'package-windows.ps1') -Edition $edition -SkipTests:$SkipTests
    if ($LASTEXITCODE -ne 0) { throw "package-windows.ps1 -Edition $edition failed ($LASTEXITCODE)" }
    & (Join-Path $PSScriptRoot 'package-installer.ps1') -Edition $edition -UseExistingPayload
    if ($LASTEXITCODE -ne 0) { throw "package-installer.ps1 -Edition $edition failed ($LASTEXITCODE)" }
}

Write-Host ''
Write-Host 'All requested editions packaged:' -ForegroundColor Green
foreach ($edition in $Editions) {
    $dir = Join-Path $root "dist\windows\$edition"
    Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^(NOVA-.*-(Portable\.zip|Setup\.exe)|SHA256SUMS\.txt|BUILD-INFO\.json)$' } |
        ForEach-Object { Write-Host "  $edition\$($_.Name)" }
}
