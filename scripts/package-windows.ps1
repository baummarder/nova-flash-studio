<#
.SYNOPSIS
    Build and package one edition of the NØVA Flash Studio Windows release.

.DESCRIPTION
    Produces a self-contained edition payload under dist/windows/<edition>/:
    the product folder with the branded executable, docs and legal files,
    a portable ZIP named NOVA-<Edition>-Portable.zip, SHA256SUMS.txt and
    BUILD-INFO.json. The executable embeds the ICO and version resource
    through crates/nova-app/build.rs; NOVA_EDITION is set for the Rust
    build so branding, configuration paths and module registration all
    match the edition.

    Editions: user (default), edu, enterprise. Each edition installs and
    coexists independently; settings live in separate per-edition
    configuration directories.

.PARAMETER Edition
    Product edition: user, edu or enterprise.

.PARAMETER SkipTests
    Skip the workspace test suite. Intended for CI jobs that already ran it.

.PARAMETER NoZip
    Do not create the portable ZIP archive.
#>
[CmdletBinding()]
param(
    [ValidateSet('user', 'edu', 'enterprise')]
    [string]$Edition = 'user',
    [switch]$SkipTests,
    [switch]$NoZip
)

$ErrorActionPreference = 'Stop'

# Edition table — single source of truth: packaging/editions.json.
# Keep in sync with crates/nova-core/src/edition.rs and
# packaging/windows/installer/EditionInfo.cs (checked by
# scripts/test-edition-packaging.ps1 and nova-core/tests/editions.rs).
$editionsPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'packaging\editions.json'
$editionsDoc = [System.IO.File]::ReadAllText($editionsPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$editionInfo = $editionsDoc.editions | Where-Object { $_.id -eq $Edition } | Select-Object -First 1
if ($null -eq $editionInfo) {
    throw "Edition '$Edition' is missing from packaging/editions.json."
}
$productName = $editionInfo.product
$editionId = $editionInfo.id

function Invoke-ReleaseCli {
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

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
    $versionMatch = [regex]::Match(
        $manifest,
        '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"'
    )
    if (-not $versionMatch.Success) {
        throw 'Workspace version not found in Cargo.toml.'
    }
    $version = $versionMatch.Groups[1].Value

    if (-not $SkipTests) {
        Write-Host "== cargo test --workspace (edition: $editionId) ==" -ForegroundColor Cyan
        & cargo test --workspace
        if ($LASTEXITCODE -ne 0) {
            throw "cargo test failed with exit code $LASTEXITCODE"
        }
    }

    Write-Host "== cargo build --release -p nova-app (NOVA_EDITION=$editionId) ==" -ForegroundColor Cyan
    $env:NOVA_EDITION = $editionId
    try {
        & cargo build --release -p nova-app
    } finally {
        Remove-Item Env:NOVA_EDITION -ErrorAction SilentlyContinue
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $sourceExe = Join-Path $root 'target\release\nova-flash-studio.exe'
    if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
        throw "Missing release executable: $sourceExe"
    }

    # Stage into a sibling directory so a failed build never destroys the
    # previous release of THIS edition. Edition payloads live in
    # dist/windows/<edition>/; other editions are untouched.
    $distDir = Join-Path $root 'dist'
    $stageRoot = Join-Path $distDir "windows.staging-$editionId"
    $releaseRoot = Join-Path (Join-Path $distDir 'windows') $editionId
    if (Test-Path -LiteralPath $stageRoot) {
        Remove-Item -LiteralPath $stageRoot -Recurse -Force
    }
    $bundle = Join-Path $stageRoot $productName
    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $bundle 'docs') -Force | Out-Null

    $targetExe = Join-Path $bundle ($productName + '.exe')
    Copy-Item -LiteralPath $sourceExe -Destination $targetExe
    Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $bundle
    Copy-Item -Path (Join-Path $root 'docs\*.md') -Destination (Join-Path $bundle 'docs')

    # Legal files: project license texts, generated third-party notices, the
    # EULA draft and the SPDX license texts referenced by the notices.
    $legalDir = Join-Path $root 'legal'
    if (Test-Path -LiteralPath (Join-Path $legalDir 'THIRD-PARTY-NOTICES.md')) {
        Copy-Item -Path (Join-Path $legalDir '*') -Destination $bundle
    } else {
        Write-Warning 'legal/THIRD-PARTY-NOTICES.md missing; run scripts/gen-third-party-notices.py'
    }
    $licensesDir = Join-Path $bundle 'licenses'
    New-Item -ItemType Directory -Path $licensesDir -Force | Out-Null
    Copy-Item -Path (Join-Path $root 'LICENSES\*.txt') -Destination $licensesDir

    $info = (Get-Item -LiteralPath $targetExe).VersionInfo
    if ($info.ProductName -ne $productName) {
        throw "Embedded ProductName is '$($info.ProductName)', expected '$productName'. Is rc.exe available and NOVA_EDITION set?"
    }
    if ([string]::IsNullOrWhiteSpace($info.FileVersion)) {
        throw 'Embedded FileVersion is empty.'
    }

    Add-Type -AssemblyName System.Drawing
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($targetExe)
    if ($null -eq $icon) {
        throw 'No executable icon could be extracted.'
    }
    $iconSize = "$($icon.Width)x$($icon.Height)"
    $icon.Dispose()

    $versionResult = Invoke-ReleaseCli -Executable $targetExe -Arguments '--version'
    if ($versionResult.ExitCode -ne 0 -or $versionResult.Output -ne "$productName $version") {
        throw "Unexpected --version output: '$($versionResult.Output)' (exit $($versionResult.ExitCode)) $($versionResult.Error)"
    }
    $versionOutput = $versionResult.Output

    $startupResult = Invoke-ReleaseCli -Executable $targetExe -Arguments '--startup-check'
    if ($startupResult.ExitCode -ne 0 -or $startupResult.Output -notmatch 'window-icon:') {
        throw "Startup check failed: $($startupResult.Output) $($startupResult.Error)"
    }

    $checksums = @()
    $checksums += (Get-FileHash -LiteralPath $targetExe -Algorithm SHA256).Hash + "  $productName.exe"
    $zipPath = $null
    if (-not $NoZip) {
        $zipPath = Join-Path $stageRoot $editionInfo.portableZip
        if (Test-Path -LiteralPath $zipPath) {
            Remove-Item -LiteralPath $zipPath -Force
        }
        Compress-Archive -LiteralPath $bundle -DestinationPath $zipPath -CompressionLevel Optimal
        $checksums += (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash + "  $(Split-Path -Leaf $zipPath)"
    }
    $checksumPath = Join-Path $stageRoot 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllLines($checksumPath, $checksums, [System.Text.UTF8Encoding]::new($false))

    # Record build metadata for reproducibility/traceability.
    $buildInfo = [ordered]@{
        product       = $productName
        edition       = $editionId
        version       = $version
        gitCommit     = (& git rev-parse HEAD 2>$null)
        gitDirty      = [bool](& git status --porcelain 2>$null)
        builtAtUtc    = (Get-Date).ToUniversalTime().ToString('o')
        rustc         = ((& rustc --version 2>$null) -join ' ')
        target        = 'x86_64-pc-windows-msvc'
        configuration = 'release'
        zipSha256     = if ($zipPath) { (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash } else { $null }
        exeSha256     = $checksums[0].Split(' ')[0]
    }
    [System.IO.File]::WriteAllText(
        (Join-Path $stageRoot 'BUILD-INFO.json'),
        ($buildInfo | ConvertTo-Json),
        [System.Text.UTF8Encoding]::new($false)
    )

    # Archive the PREVIOUS release artifacts of this edition before swapping.
    # archive-releases.ps1 never deletes; it verifies checksums and copies
    # into dist/archive/<version>/<edition>/.
    & (Join-Path $PSScriptRoot 'archive-releases.ps1') -DistRoot $distDir
    if ($LASTEXITCODE -ne 0) {
        throw "archive-releases.ps1 failed with exit code $LASTEXITCODE; previous release left untouched in $releaseRoot"
    }

    # Swap staged payload into place. If this fails, the old release remains.
    if (Test-Path -LiteralPath $releaseRoot) {
        Remove-Item -LiteralPath $releaseRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $releaseRoot) -Force | Out-Null
    Move-Item -LiteralPath $stageRoot -Destination $releaseRoot

    Write-Host ''
    Write-Host "Windows package created (edition: $editionId):" -ForegroundColor Green
    Write-Host "  $targetExe"
    if ($zipPath) { Write-Host "  $zipPath" }
    Write-Host "  Version: $($info.FileVersion) / $($info.ProductVersion)"
    Write-Host "  Product: $($info.ProductName)"
    Write-Host "  Company: $($info.CompanyName)"
    Write-Host "  Icon resource: $iconSize"
    Write-Host "  Smoke test: $versionOutput"
} finally {
    Pop-Location
}
