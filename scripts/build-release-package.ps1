<#
.SYNOPSIS
    Assemble the M29 release package: release/<version>/windows/ (M29.9).

.DESCRIPTION
    Copies the three edition payloads from dist/windows/<edition>/ into
    release/<version>/windows/<edition>/, then generates:

      - release-manifest.json  (machine-readable, per-artifact metadata)
      - RELEASE.md             (human-readable summary)

    Signing status is read from each edition's SIGNING-STATUS.json
    (written by package-windows.ps1 / package-installer.ps1). Hashes are
    recomputed for the FINAL copied files -- never trusted from staging.

    This script does not build anything; run package-*.ps1 first.

.PARAMETER Version
    Release version (default: workspace version from Cargo.toml).

.PARAMETER RequireSigned
    Fail when any edition reports a non-SIGNED executable.
#>
[CmdletBinding()]
param(
    [string]$Version = '',
    [switch]$RequireSigned
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib\Signing.ps1')

if ($Version -eq '') {
    $manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
    $m = [regex]::Match($manifest, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
    if (-not $m.Success) { throw 'Workspace version not found in Cargo.toml.' }
    $Version = $m.Groups[1].Value
}

$distDir = Join-Path $root 'dist\windows'
$releaseDir = Join-Path $root "release\$Version\windows"
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

$editionsDoc = [System.IO.File]::ReadAllText(
    (Join-Path $root 'packaging\editions.json'), [System.Text.Encoding]::UTF8) | ConvertFrom-Json

$manifestArtifacts = @()
$lines = @()

foreach ($ed in $editionsDoc.editions) {
    $src = Join-Path $distDir $ed.id
    if (-not (Test-Path -LiteralPath $src)) {
        throw "Missing edition payload: $src -- run package-installer.ps1 -Edition $($ed.id) first."
    }
    $dst = Join-Path $releaseDir $ed.id
    if (Test-Path -LiteralPath $dst) { Remove-Item -LiteralPath $dst -Recurse -Force }
    Copy-Item -LiteralPath $src -Destination $dst -Recurse

    # Collect distributable artifacts: portable zip, setup exe and the
    # payload app exe inside the product bundle. installer-publish/ is
    # an intermediate dotnet output and is excluded.
    $editionFiles = @(
        Get-ChildItem -LiteralPath $dst -Recurse -File |
            Where-Object {
                $_.Extension -in '.exe', '.zip' -and
                $_.FullName -notmatch 'installer-publish'
            }
    )
    $statusDoc = $null
    $statusPath = Join-Path $dst 'SIGNING-STATUS.json'
    if (Test-Path -LiteralPath $statusPath -PathType Leaf) {
        $statusDoc = [System.IO.File]::ReadAllText($statusPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    }
    $signByName = @{}
    if ($statusDoc) {
        foreach ($f in @($statusDoc.files)) { $signByName[$f.file] = $f }
    }

    foreach ($file in $editionFiles) {
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        $sign = $signByName[$file.Name]
        $status = if ($sign) { $sign.status } else { 'UNSIGNED' }
        if ($RequireSigned -and $file.Extension -eq '.exe' -and $status -ne 'SIGNED') {
            throw "RequireSigned: $($ed.id)/$($file.Name) is $status"
        }
        $rel = $file.FullName.Substring($dst.Length).TrimStart('\', '/')
        $manifestArtifacts += [ordered]@{
            path           = "$($ed.id)/$rel"
            edition        = $ed.id
            product        = $ed.product
            version        = $Version
            platform       = 'windows'
            architecture   = 'x64'
            sizeBytes      = $file.Length
            sha256         = $hash
            signingStatus  = $status
            publisher      = if ($statusDoc -and $statusDoc.publisher) { $statusDoc.publisher } else { '' }
            certThumbprint = if ($sign) { $sign.thumbprint } else { $null }
            signatureValid = if ($sign) { [bool]$sign.verified } else { $false }
            timestamped    = if ($sign) { [bool]$sign.timestamped } else { $false }
        }
        $lines += "$hash  $($ed.id)/$rel"
    }
    Write-Host "Staged $($ed.id): $($editionFiles.Count) artifact(s)"
}

# Top-level checksum file covering every artifact.
$sumPath = Join-Path $releaseDir 'SHA256SUMS.txt'
[System.IO.File]::WriteAllLines($sumPath, $lines, [System.Text.UTF8Encoding]::new($false))

$releaseManifest = [ordered]@{
    schemaVersion = 1
    product       = 'NOVA Flash Studio'
    version       = $Version
    platform      = 'windows'
    architecture  = 'x64'
    generatedUtc  = (Get-Date).ToUniversalTime().ToString('o')
    gitCommit     = (& git -C $root rev-parse HEAD 2>$null)
    artifacts     = $manifestArtifacts
}
[System.IO.File]::WriteAllText(
    (Join-Path $releaseDir 'release-manifest.json'),
    ($releaseManifest | ConvertTo-Json -Depth 8),
    [System.Text.UTF8Encoding]::new($false)
)

# Human-readable summary.
$md = @"
# NOeVA Flash Studio $Version -- Windows Release

Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm') UTC
Git: $($releaseManifest.gitCommit)

| Edition | Artifact | Size | SHA-256 (short) | Signing |
|---|---|---|---|---|
"@
foreach ($a in $manifestArtifacts) {
    $md += "| $($a.edition) | ``$($a.path)`` | $([math]::Round($a.sizeBytes/1MB,1)) MB | ``$($a.sha256.Substring(0,16))...`` | $($a.signingStatus) |`n"
}
$md += @"

## Notes

- Signing status ``SIGNED`` means Authenticode + RFC-3161 timestamp
  verified locally. ``SIGNING_NOT_CONFIGURED``/``UNSIGNED`` means the
  build was produced without a certificate -- it is **not** a trusted
  publisher signature.
- EULA: ``legal/LICENSE-EULA*`` (v1.0) ships inside every installer;
  placeholders ``[RECHTEINHABER]`` etc. must be filled before public
  release (see docs/EULA.md).
- Verify with: ``powershell scripts/verify-release.ps1 -ReleaseDir release/$Version/windows``
"@
[System.IO.File]::WriteAllText(
    (Join-Path $releaseDir 'RELEASE.md'), $md, [System.Text.UTF8Encoding]::new($false))

# Bundle docs + legal pointers into the release dir.
foreach ($doc in @('CHANGELOG.md', 'docs\EULA.md', 'docs\CODE_SIGNING.md')) {
    $src = Join-Path $root $doc
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination (Join-Path $releaseDir (Split-Path -Leaf $doc))
    }
}

Write-Host ''
Write-Host "Release package: $releaseDir" -ForegroundColor Green
Write-Host "  manifest: release-manifest.json"
Write-Host "  checksums: SHA256SUMS.txt"
Write-Host "  summary:   RELEASE.md"
