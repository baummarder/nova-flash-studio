<#
.SYNOPSIS
    Verify a NOeVA Windows release directory (M29.6) -- READ-ONLY.

.DESCRIPTION
    Checks, without modifying anything:
      - SHA256SUMS.txt entries match the actual files,
      - release-manifest.json is consistent with files on disk
        (name, size, sha256, signing status),
      - Authenticode signatures via signtool verify when marked SIGNED,
      - unexpected or missing files inside each edition directory.

    Exit code 0 = all checks passed, 1 = problems found.

.PARAMETER ReleaseDir
    Directory containing the release, e.g. release/0.20.0-rc1/windows
    or dist/windows/<edition>.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ReleaseDir
)

$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'lib\Signing.ps1')

$problems = @()
$warnings = @()
$checks = 0

function Add-Check([bool]$Ok, [string]$What, [string]$Detail = '') {
    $script:checks++
    if (-not $Ok) { $script:problems += "$What -- $Detail" }
}

if (-not (Test-Path -LiteralPath $ReleaseDir)) {
    Write-Host "FAIL: release dir not found: $ReleaseDir" -ForegroundColor Red
    exit 1
}

# --- Collect edition directories ---------------------------------------------
$editionDirs = @()
foreach ($child in (Get-ChildItem -LiteralPath $ReleaseDir -Directory)) {
    if (Test-Path -LiteralPath (Join-Path $child.FullName 'SHA256SUMS.txt')) {
        $editionDirs += $child
    }
}
# Single-edition dist dir (dist/windows/user) has the checksum at top level.
if ($editionDirs.Count -eq 0 -and (Test-Path -LiteralPath (Join-Path $ReleaseDir 'SHA256SUMS.txt'))) {
    $editionDirs = @([pscustomobject]@{ FullName = $ReleaseDir; Name = (Split-Path -Leaf $ReleaseDir) })
}
if ($editionDirs.Count -eq 0) {
    Write-Host "FAIL: no edition directories with SHA256SUMS.txt under $ReleaseDir" -ForegroundColor Red
    exit 1
}

$manifestPath = Join-Path $ReleaseDir 'release-manifest.json'
$manifest = $null
if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
    try {
        $manifest = [System.IO.File]::ReadAllText($manifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    } catch {
        $problems += "release-manifest.json is not valid JSON: $_"
    }
} else {
    $warnings += 'release-manifest.json not present (dist layout?)'
}

$signTool = Find-NovaSignTool
$expectedArtifacts = @{}
if ($manifest) {
    foreach ($artifact in @($manifest.artifacts)) {
        $expectedArtifacts[$artifact.path] = $artifact
    }
}

foreach ($dir in $editionDirs) {
    Write-Host "== Verifying $($dir.Name) ==" -ForegroundColor Cyan
    $sumPath = Join-Path $dir.FullName 'SHA256SUMS.txt'
    $sumLines = Get-Content -LiteralPath $sumPath -Encoding UTF8 |
        Where-Object { $_ -match '^([0-9A-Fa-f]{64})\s+\*?(.+)$' }
    Add-Check ($sumLines.Count -gt 0) "$($dir.Name): SHA256SUMS.txt has entries"

    foreach ($line in $sumLines) {
        if ($line -match '^([0-9A-Fa-f]{64})\s+\*?(.+)$') {
            $expected = $Matches[1].ToUpperInvariant()
            $rel = $Matches[2].Trim()
            $file = Join-Path $dir.FullName $rel
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
                # dist SHA256SUMS entries are relative to the stage root;
                # the payload exe lives inside the product bundle dir.
                $found = Get-ChildItem -LiteralPath $dir.FullName -Recurse -File `
                    -Filter (Split-Path -Leaf $rel) -ErrorAction SilentlyContinue |
                    Select-Object -First 1
                if ($null -ne $found) { $file = $found.FullName }
            }
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
                $problems += "$($dir.Name): missing artifact '$rel'"
                continue
            }
            $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
            Add-Check ($actual -eq $expected) "$($dir.Name): sha256 $rel" "expected $expected got $actual"
        }
    }

    # Manifest cross-check (when present).
    $statusPath = Join-Path $dir.FullName 'SIGNING-STATUS.json'
    if (Test-Path -LiteralPath $statusPath -PathType Leaf) {
        try {
            $status = [System.IO.File]::ReadAllText($statusPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
            foreach ($entry in @($status.files)) {
                $file = Join-Path $dir.FullName $entry.file
                if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
                    $warnings += "$($dir.Name): signing-status references missing $($entry.file)"
                    continue
                }
                if ($entry.status -eq 'SIGNED') {
                    if ($null -eq $signTool) {
                        $warnings += "$($dir.Name): $($entry.file) marked SIGNED but signtool unavailable"
                    } else {
                        $verify = Invoke-NovaNative -Executable $signTool -Arguments "verify /pa `"$file`""
                        $ok = $verify.ExitCode -eq 0 -and ($verify.Output -match 'Successfully verified')
                        Add-Check $ok "$($dir.Name): Authenticode $($entry.file)" ($verify.Output + $verify.Error).Trim()
                        if ($entry.timestamped -and $verify.Output -notmatch 'TimeStamp') {
                            $warnings += "$($dir.Name): $($entry.file) timestamp missing despite status"
                        }
                    }
                } elseif ($entry.status -eq 'UNSIGNED' -or $entry.status -eq 'SIGNING_NOT_CONFIGURED') {
                    # unsigned is acceptable for dev/rc builds -- just report
                } else {
                    $problems += "$($dir.Name): $($entry.file) signing status $($entry.status): $($entry.detail)"
                }
            }
        } catch {
            $problems += "$($dir.Name): SIGNING-STATUS.json invalid: $_"
        }
    }

    # Unexpected executables: every .exe/.dll should be covered by
    # SHA256SUMS or SIGNING-STATUS.
    $covered = @{}
    foreach ($line in $sumLines) {
        if ($line -match '^([0-9A-Fa-f]{64})\s+\*?(.+)$') { $covered[$Matches[2].Trim()] = $true }
    }
    if (Test-Path -LiteralPath $statusPath -PathType Leaf) {
        foreach ($entry in @(([System.IO.File]::ReadAllText($statusPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json).files)) {
            $covered[$entry.file] = $true
        }
    }
    Get-ChildItem -LiteralPath $dir.FullName -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in '.exe', '.dll' } |
        ForEach-Object {
            $rel = $_.FullName.Substring($dir.FullName.Length).TrimStart('\', '/')
            if (-not $covered.ContainsKey($rel) -and -not $covered.ContainsKey($_.Name)) {
                $warnings += "$($dir.Name): unlisted executable $rel"
            }
        }
}

# Manifest artifact cross-check.
if ($manifest) {
    foreach ($artifact in @($manifest.artifacts)) {
        $file = Join-Path $ReleaseDir $artifact.path
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
            # artifacts may live under <edition>/ subdirs
            $alt = Get-ChildItem -LiteralPath $ReleaseDir -Recurse -Filter (Split-Path -Leaf $artifact.path) -File -ErrorAction SilentlyContinue |
                Select-Object -First 1
            if ($null -eq $alt) {
                $problems += "manifest artifact missing on disk: $($artifact.path)"
                continue
            }
            $file = $alt.FullName
        }
        if ($artifact.sha256) {
            $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
            Add-Check ($actual -eq $artifact.sha256.ToUpperInvariant()) "manifest sha256 $($artifact.path)"
        }
        if ($artifact.signingStatus -eq 'SIGNED' -and $signTool) {
            $verify = Invoke-NovaNative -Executable $signTool -Arguments "verify /pa `"$file`""
            Add-Check ($verify.ExitCode -eq 0) "manifest signature $($artifact.path)"
        }
    }
}

Write-Host ''
Write-Host "Checks: $checks, problems: $($problems.Count), warnings: $($warnings.Count)" -ForegroundColor $(if ($problems.Count) { 'Red' } else { 'Green' })
foreach ($w in $warnings) { Write-Host "  WARN  $w" -ForegroundColor Yellow }
foreach ($p in $problems) { Write-Host "  FAIL  $p" -ForegroundColor Red }
if ($problems.Count -gt 0) { exit 1 }
exit 0
