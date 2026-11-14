<#
.SYNOPSIS
    Build and package the Windows release of NØVA Flash Studio.

.DESCRIPTION
    Produces dist/windows/NØVA Flash Studio/NØVA Flash Studio.exe and a
    versioned ZIP. The executable embeds the ICO and version resource through
    crates/nova-app/build.rs. This script refuses to publish an executable
    without the expected version metadata.

.PARAMETER SkipTests
    Skip the workspace test suite. Intended for CI jobs that already ran it.

.PARAMETER NoZip
    Do not create the optional ZIP archive.
#>
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$NoZip
)

$ErrorActionPreference = 'Stop'

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
        Write-Host '== cargo test --workspace ==' -ForegroundColor Cyan
        & cargo test --workspace
        if ($LASTEXITCODE -ne 0) {
            throw "cargo test failed with exit code $LASTEXITCODE"
        }
    }

    Write-Host '== cargo build --release -p nova-app ==' -ForegroundColor Cyan
    & cargo build --release -p nova-app
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $sourceExe = Join-Path $root 'target\release\nova-flash-studio.exe'
    if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
        throw "Missing release executable: $sourceExe"
    }

    # Windows PowerShell 5.1 reads BOM-less scripts as ANSI. Build the product
    # name from a code point so the folder and EXE keep the exact "NØVA" name
    # regardless of the console or script encoding.
    $productName = 'N' + [char]0x00D8 + 'VA Flash Studio'
    # Stage into a sibling directory so a failed build never destroys the
    # previous release. The old dist/windows is archived (never deleted
    # silently) and only swapped out after the new payload is complete.
    $distDir = Join-Path $root 'dist'
    $stageRoot = Join-Path $distDir 'windows.staging'
    $releaseRoot = Join-Path $distDir 'windows'
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
        throw "Embedded ProductName is '$($info.ProductName)', expected '$productName'. Is rc.exe available?"
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
        $zipPath = Join-Path $stageRoot "NOVA-Flash-Studio-$version-windows-x64.zip"
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

    # Archive the PREVIOUS release artifacts before swapping directories.
    # archive-releases.ps1 never deletes; it verifies checksums, copies into
    # dist/archive/<version>/ and refuses silent overwrites.
    & (Join-Path $PSScriptRoot 'archive-releases.ps1') -DistRoot $distDir
    if ($LASTEXITCODE -ne 0) {
        throw "archive-releases.ps1 failed with exit code $LASTEXITCODE; previous release left untouched in $releaseRoot"
    }

    # Swap staged payload into place. If this fails, the old release remains.
    if (Test-Path -LiteralPath $releaseRoot) {
        Remove-Item -LiteralPath $releaseRoot -Recurse -Force
    }
    Move-Item -LiteralPath $stageRoot -Destination $releaseRoot

    Write-Host ''
    Write-Host 'Windows package created:' -ForegroundColor Green
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
