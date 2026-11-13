<#
.SYNOPSIS
    Dispatch release packaging for one or more targets.

.DESCRIPTION
    Windows packaging runs natively in PowerShell. macOS and Linux packaging
    require their respective host tools (bash, Xcode command line tools,
    appimagetool). The script fails loudly instead of pretending a foreign
    platform was built.
#>
[CmdletBinding()]
param(
    [ValidateSet('windows', 'macos', 'linux', 'all')]
    [string]$Target = 'windows',
    [switch]$SkipTests,
    [switch]$SkipInstaller
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $windowsArgs = @()
    if ($SkipTests) { $windowsArgs += '-SkipTests' }

    function Invoke-WindowsInstaller {
        if ($SkipInstaller) { return }
        & (Join-Path $PSScriptRoot 'package-installer.ps1') -UseExistingPayload
        if ($LASTEXITCODE -ne 0) { throw "package-installer.ps1 failed with exit code $LASTEXITCODE" }
    }

    switch ($Target) {
        'windows' {
            & (Join-Path $PSScriptRoot 'package-windows.ps1') @windowsArgs
            Invoke-WindowsInstaller
        }
        'macos' {
            if (-not (Get-Command bash -ErrorAction SilentlyContinue)) {
                throw 'bash is required for macOS packaging. Run scripts/package-macos.sh on macOS.'
            }
            & bash (Join-Path $PSScriptRoot 'package-macos.sh')
            if ($LASTEXITCODE -ne 0) { throw "macOS packaging failed with exit code $LASTEXITCODE" }
        }
        'linux' {
            if (-not (Get-Command bash -ErrorAction SilentlyContinue)) {
                throw 'bash is required for Linux packaging. Run scripts/package-linux.sh on Linux.'
            }
            & bash (Join-Path $PSScriptRoot 'package-linux.sh')
            if ($LASTEXITCODE -ne 0) { throw "Linux packaging failed with exit code $LASTEXITCODE" }
        }
        'all' {
            & (Join-Path $PSScriptRoot 'package-windows.ps1') @windowsArgs
            Invoke-WindowsInstaller
            Write-Warning 'macOS and Linux artifacts require their native hosts; run the corresponding scripts there.'
        }
    }
} finally {
    Pop-Location
}
