<#
.SYNOPSIS
    Archive existing Windows release artifacts before they are replaced.

.DESCRIPTION
    Scans dist/windows for release artifacts (versioned ZIPs, Setup.exe files,
    SHA256SUMS.txt, the staged payload folder and installer-publish output),
    verifies SHA-256 checksums where a SHA256SUMS.txt is available, and copies
    everything into dist/archive/<version>/ without overwriting existing
    archives silently.

    The script NEVER deletes anything. It is invoked by package-windows.ps1
    before dist/windows is cleaned, and can be run standalone for verification.

    Archive layout per version:
      dist/archive/<version>/
        NOVA-Flash-Studio-<version>-windows-x64.zip
        NOVA-Flash-Studio-<version>-windows-x64-Setup.exe
        SHA256SUMS.txt
        BUILD-INFO.json            (when present in dist/windows)
        NÃ˜VA Flash Studio/         (staged payload, when present)
        installer-publish/         (when present)

.PARAMETER DistRoot
    Root containing dist/windows and dist/archive. Defaults to the repo dist.

.PARAMETER Force
    Allow overwriting archive files whose hashes differ from the source.
    Without this switch a hash mismatch is a hard error.

.PARAMETER WhatIfArchive
    Report what would be archived without copying anything.

.OUTPUTS
    Nothing on success. Writes progress to the host. Throws on integrity
    failures.
#>
[CmdletBinding()]
param(
    [string]$DistRoot,
    [switch]$Force,
    [switch]$WhatIfArchive
)

$ErrorActionPreference = 'Stop'

if (-not $DistRoot) {
    $DistRoot = Join-Path (Split-Path -Parent $PSScriptRoot) 'dist'
}
$windowsDir = Join-Path $DistRoot 'windows'
$archiveRoot = Join-Path $DistRoot 'archive'
# Windows PowerShell 5.1 reads BOM-less scripts as ANSI; build the product
# folder name from a code point like package-windows.ps1 does.
$productDirName = 'N' + [char]0x00D8 + 'VA Flash Studio'

function Get-ArtifactVersion {
    param([string]$Name)
    # NOVA-Flash-Studio-<version>-windows-x64.zip / -Setup.exe
    if ($Name -match '^NOVA-Flash-Studio-(.+)-windows-x64(-(Setup))?\.(zip|exe)$') {
        return $Matches[1]
    }
    return $null
}

function Test-Sha256File {
    param([string]$Path, [string]$ExpectedHash)
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    return ($actual -ieq $ExpectedHash)
}

# ---------------------------------------------------------------------------
# Collect artifacts
# ---------------------------------------------------------------------------
if (-not (Test-Path -LiteralPath $windowsDir -PathType Container)) {
    Write-Host "archive-releases: no $windowsDir; nothing to archive."
    return
}

$artifacts = Get-ChildItem -LiteralPath $windowsDir -File |
    Where-Object { $null -ne (Get-ArtifactVersion $_.Name) }

# Edition payloads (M19): dist/windows/<edition>/ directories produced by
# package-windows.ps1 -Edition. Their artifact names carry no version, so
# the version is read from the sibling BUILD-INFO.json.
$editionJobs = @()
Get-ChildItem -LiteralPath $windowsDir -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -in @('user', 'edu', 'enterprise') } |
    ForEach-Object {
        $buildInfoPath = Join-Path $_.FullName 'BUILD-INFO.json'
        if (-not (Test-Path -LiteralPath $buildInfoPath -PathType Leaf)) { return }
        try {
            $info = Get-Content -Raw -LiteralPath $buildInfoPath | ConvertFrom-Json
            if ($info.version) {
                $editionJobs += [pscustomobject]@{
                    EditionId = $_.Name
                    Version   = [string]$info.version
                    Dir       = $_.FullName
                }
            }
        } catch {
            Write-Warning "archive-releases: cannot read $buildInfoPath; skipping edition archive."
        }
    }

if ($artifacts.Count -eq 0 -and $editionJobs.Count -eq 0) {
    Write-Host 'archive-releases: no versioned release artifacts found; nothing to archive.'
    return
}

$versions = $artifacts |
    ForEach-Object { Get-ArtifactVersion $_.Name } |
    Sort-Object -Unique

Write-Host "archive-releases: found artifacts for version(s): $($versions -join ', ')"

# ---------------------------------------------------------------------------
# Verify SHA256SUMS.txt entries before archiving
# ---------------------------------------------------------------------------
$sumsPath = Join-Path $windowsDir 'SHA256SUMS.txt'
$verified = @{}
if (Test-Path -LiteralPath $sumsPath -PathType Leaf) {
    foreach ($line in [System.IO.File]::ReadAllLines($sumsPath)) {
        if ($line -match '^\s*([0-9A-Fa-f]{64})\s+(.+?)\s*$') {
            $hash = $Matches[1].ToUpperInvariant()
            $name = $Matches[2].Trim()
            $verified[$name] = $hash
        }
    }
    foreach ($entry in $verified.GetEnumerator()) {
        # Entries may carry the product folder prefix ("NÃ˜VA Flash Studio.exe")
        # or a bare file name inside dist/windows.
        $productExeDir = Join-Path $windowsDir $productDirName
        $candidates = @(
            (Join-Path $windowsDir $entry.Key),
            (Join-Path $productExeDir $entry.Key)
        )
        $hit = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
        if (-not $hit) {
            Write-Warning "archive-releases: SHA256SUMS entry '$($entry.Key)' has no matching file; skipped."
            continue
        }
        if (-not (Test-Sha256File -Path $hit -ExpectedHash $entry.Value)) {
            Write-Warning "archive-releases: SHA-256 MISMATCH for '$($entry.Key)' ($hit). The artifact may be corrupt; it will still be archived for preservation."
        } else {
            Write-Host "archive-releases: checksum OK  $($entry.Key)"
        }
    }
} else {
    Write-Warning 'archive-releases: no SHA256SUMS.txt in dist/windows; archiving without checksum verification.'
}

# ---------------------------------------------------------------------------
# Copy into dist/archive/<version>/
# ---------------------------------------------------------------------------
foreach ($version in $versions) {
    $dest = Join-Path $archiveRoot $version
    if ($WhatIfArchive) {
        Write-Host "archive-releases: [WhatIf] would archive version $version -> $dest"
        continue
    }
    New-Item -ItemType Directory -Path $dest -Force | Out-Null

    # Files belonging to this version.
    $versionFiles = $artifacts | Where-Object { (Get-ArtifactVersion $_.Name) -eq $version }
    foreach ($file in $versionFiles) {
        $target = Join-Path $dest $file.Name
        if (Test-Path -LiteralPath $target -PathType Leaf) {
            $same = Test-Sha256File -Path $file.FullName -ExpectedHash `
                ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash)
            if ($same) {
                Write-Host "archive-releases: already archived, identical: $($file.Name)"
                continue
            }
            if (-not $Force) {
                throw "archive-releases: $target already exists with DIFFERENT content. Refusing to overwrite; re-run with -Force if this is intentional."
            }
            Write-Warning "archive-releases: overwriting differing archive file $target (-Force)."
        }
        Copy-Item -LiteralPath $file.FullName -Destination $target
        # Post-copy verification.
        $srcHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        $dstHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        if ($srcHash -ne $dstHash) {
            throw "archive-releases: copy verification FAILED for $($file.Name)"
        }
        Write-Host "archive-releases: archived $($file.Name) -> $dest"
    }

    # Shared metadata files.
    foreach ($meta in @('SHA256SUMS.txt', 'BUILD-INFO.json', 'RELEASE-NOTES.md')) {
        $src = Join-Path $windowsDir $meta
        if (Test-Path -LiteralPath $src -PathType Leaf) {
            $target = Join-Path $dest $meta
            if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
                Copy-Item -LiteralPath $src -Destination $target
                Write-Host "archive-releases: archived $meta"
            }
        }
    }

    # Staged payload folder + installer publish output.
    foreach ($dirName in @($productDirName, 'installer-publish')) {
        $srcDir = Join-Path $windowsDir $dirName
        if (Test-Path -LiteralPath $srcDir -PathType Container) {
            $destDir = Join-Path $dest $dirName
            if (Test-Path -LiteralPath $destDir -PathType Container) {
                Write-Host "archive-releases: $dirName already present in archive; verifying key file hashes."
                # Verify the payload EXE hash when SHA256SUMS lists it.
                $exeName = $dirName + '.exe'
                $exeSrc = Join-Path $srcDir $exeName
                $exeDst = Join-Path $destDir $exeName
                if ((Test-Path -LiteralPath $exeSrc) -and (Test-Path -LiteralPath $exeDst)) {
                    $h1 = (Get-FileHash -LiteralPath $exeSrc -Algorithm SHA256).Hash
                    $h2 = (Get-FileHash -LiteralPath $exeDst -Algorithm SHA256).Hash
                    if ($h1 -ne $h2 -and -not $Force) {
                        throw "archive-releases: archived $dirName differs from current payload; use -Force to overwrite."
                    }
                }
                continue
            }
            Copy-Item -LiteralPath $srcDir -Destination $destDir -Recurse
            Write-Host "archive-releases: archived $dirName/"
        }
    }
}

# ---------------------------------------------------------------------------
# Archive edition payloads (M19): dist/windows/<edition>/ -> archive/<v>/<edition>/
# ---------------------------------------------------------------------------
foreach ($job in $editionJobs) {
    $dest = Join-Path (Join-Path $archiveRoot $job.Version) $job.EditionId
    if ($WhatIfArchive) {
        Write-Host "archive-releases: [WhatIf] would archive edition $($job.EditionId) $($job.Version) -> $dest"
        continue
    }
    New-Item -ItemType Directory -Path $dest -Force | Out-Null

    Get-ChildItem -LiteralPath $job.Dir -Force | Where-Object {
        $_.Name -ne 'installer-publish' -and $_.Name -notlike '*.tmp'
    } | ForEach-Object {
        $target = Join-Path $dest $_.Name
        if (Test-Path -LiteralPath $target) {
            if ($_.PSIsContainer) {
                Write-Host "archive-releases: $($job.EditionId)/$($_.Name) already present in archive; verifying."
                # The staged payload folder contains the branded app EXE.
                $exeName = $_.Name + '.exe'
                $exeSrc = Join-Path $_.FullName $exeName
                $exeDst = Join-Path $target $exeName
                if ((Test-Path -LiteralPath $exeSrc -PathType Leaf) -and
                    (Test-Path -LiteralPath $exeDst -PathType Leaf)) {
                    $h1 = (Get-FileHash -LiteralPath $exeSrc -Algorithm SHA256).Hash
                    $h2 = (Get-FileHash -LiteralPath $exeDst -Algorithm SHA256).Hash
                    if ($h1 -ne $h2 -and -not $Force) {
                        throw "archive-releases: archived $($job.EditionId)/$exeName differs; use -Force to overwrite."
                    }
                }
                # Verify any SHA256SUMS entries that live inside the payload.
                $sums = Join-Path $job.Dir 'SHA256SUMS.txt'
                if (Test-Path -LiteralPath $sums) {
                    foreach ($line in [System.IO.File]::ReadAllLines($sums)) {
                        if ($line -match '^\s*([0-9A-Fa-f]{64})\s+(.+?)\s*$') {
                            $name = $Matches[2].Trim()
                            $srcFile = Join-Path $job.Dir $name
                            if (-not (Test-Path -LiteralPath $srcFile -PathType Leaf)) { continue }
                            $dstFile = Join-Path $target $name
                            if (-not (Test-Path -LiteralPath $dstFile -PathType Leaf)) { continue }
                            $h1 = (Get-FileHash -LiteralPath $srcFile -Algorithm SHA256).Hash
                            $h2 = (Get-FileHash -LiteralPath $dstFile -Algorithm SHA256).Hash
                            if ($h1 -ne $h2 -and -not $Force) {
                                throw "archive-releases: archived $($job.EditionId)/$name differs; use -Force to overwrite."
                            }
                        }
                    }
                }
                return
            }
            $same = Test-Sha256File -Path $_.FullName -ExpectedHash `
                ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash)
            if ($same) {
                Write-Host "archive-releases: already archived, identical: $($job.EditionId)/$($_.Name)"
                return
            }
            if (-not $Force) {
                throw "archive-releases: $target already exists with DIFFERENT content. Refusing to overwrite; re-run with -Force if intentional."
            }
            Write-Warning "archive-releases: overwriting differing archive file $target (-Force)."
        }
        if ($_.PSIsContainer) {
            Copy-Item -LiteralPath $_.FullName -Destination $target -Recurse
        } else {
            Copy-Item -LiteralPath $_.FullName -Destination $target
        }
        Write-Host "archive-releases: archived $($job.EditionId)/$($_.Name) -> $dest"
    }
}

Write-Host 'archive-releases: done.'
