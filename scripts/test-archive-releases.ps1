<#
.SYNOPSIS
    Automated tests for scripts/archive-releases.ps1.

.DESCRIPTION
    Builds a sandbox dist/ tree with fake release artifacts and verifies:
      1. Versioned artifacts are archived into dist/archive/<version>/.
      2. SHA256SUMS.txt and payload dirs are preserved.
      3. A second run is idempotent (identical content is skipped).
      4. Differing archive content is a hard error without -Force.
      5. -Force permits the overwrite.
      6. -WhatIfArchive copies nothing.
      7. Checksum mismatches are reported (warning, not silent pass).

    Exits non-zero on the first failure. Safe: works entirely inside a temp dir.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$archiveScript = Join-Path $PSScriptRoot 'archive-releases.ps1'
$failures = 0

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if ($Condition) {
        Write-Host "  PASS  $Message" -ForegroundColor Green
    } else {
        Write-Host "  FAIL  $Message" -ForegroundColor Red
        $script:failures++
    }
}

function New-FakeDist {
    param([string]$Root, [string]$Version = '9.9.9-test')
    $win = Join-Path $Root 'windows'
    New-Item -ItemType Directory -Path $win -Force | Out-Null
    $payload = Join-Path $win ('N' + [char]0x00D8 + 'VA Flash Studio')
    New-Item -ItemType Directory -Path $payload -Force | Out-Null
    $exe = Join-Path $payload ('N' + [char]0x00D8 + 'VA Flash Studio.exe')
    [System.IO.File]::WriteAllBytes($exe, [byte[]](1..60))
    $zip = Join-Path $win "NOVA-Flash-Studio-$Version-windows-x64.zip"
    [System.IO.File]::WriteAllBytes($zip, [byte[]](101..160))
    $setup = Join-Path $win "NOVA-Flash-Studio-$Version-windows-x64-Setup.exe"
    [System.IO.File]::WriteAllBytes($setup, [byte[]](161..220))
    # Real checksums so verification has something to check.
    $lines = @()
    $lines += ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash + '  N' + [char]0x00D8 + 'VA Flash Studio.exe')
    $lines += ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash + "  NOVA-Flash-Studio-$Version-windows-x64.zip")
    [System.IO.File]::WriteAllLines((Join-Path $win 'SHA256SUMS.txt'), $lines)
    return $win
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("nova-archive-test-" + [guid]::NewGuid().ToString('N'))
try {
    # --- Test 1+2: basic archive + idempotency -------------------------------
    Write-Host 'Test: basic archive and idempotent re-run'
    $dist = Join-Path $tmp 'case1'
    New-FakeDist -Root $dist | Out-Null
    $err = $null; try { & $archiveScript -DistRoot $dist } catch { $err = $_ }; Assert-True ($null -eq $err) 'archive script exits cleanly'
    $arch = Join-Path $dist 'archive\9.9.9-test'
    Assert-True (Test-Path (Join-Path $arch 'NOVA-Flash-Studio-9.9.9-test-windows-x64.zip')) 'zip archived'
    Assert-True (Test-Path (Join-Path $arch 'NOVA-Flash-Studio-9.9.9-test-windows-x64-Setup.exe')) 'setup archived'
    Assert-True (Test-Path (Join-Path $arch 'SHA256SUMS.txt')) 'SHA256SUMS archived'
    Assert-True (Test-Path (Join-Path $arch ('N' + [char]0x00D8 + 'VA Flash Studio' + '\N' + [char]0x00D8 + 'VA Flash Studio.exe'))) 'payload archived'
    # Source files must still exist (archive never deletes).
    Assert-True (Test-Path (Join-Path $dist 'windows\NOVA-Flash-Studio-9.9.9-test-windows-x64.zip')) 'source zip preserved'
    # Idempotent second run.
    $err = $null; try { & $archiveScript -DistRoot $dist } catch { $err = $_ }; Assert-True ($null -eq $err) 'second run exits cleanly'

    # --- Test 3: differing archive content is a hard error -------------------
    Write-Host 'Test: differing archive content refused'
    $dist2 = Join-Path $tmp 'case2'
    New-FakeDist -Root $dist2 | Out-Null
    & $archiveScript -DistRoot $dist2
    # Tamper with the source zip, then re-run: archive has old content.
    $zipPath = Join-Path $dist2 'windows\NOVA-Flash-Studio-9.9.9-test-windows-x64.zip'
    [System.IO.File]::WriteAllBytes($zipPath, [byte[]](1..40))
    $threw = $false
    try { & $archiveScript -DistRoot $dist2 } catch { $threw = $true }
    Assert-True $threw 'differing archive content throws without -Force'
    # With -Force it succeeds.
    $threw2 = $false
    try { & $archiveScript -DistRoot $dist2 -Force } catch { $threw2 = $true }
    Assert-True (-not $threw2) '-Force allows the overwrite'

    # --- Test 4: WhatIf copies nothing ---------------------------------------
    Write-Host 'Test: WhatIfArchive is side-effect free'
    $dist3 = Join-Path $tmp 'case3'
    New-FakeDist -Root $dist3 | Out-Null
    $err = $null; try { & $archiveScript -DistRoot $dist3 -WhatIfArchive } catch { $err = $_ }; Assert-True ($null -eq $err) 'WhatIf exits cleanly'
    Assert-True (-not (Test-Path (Join-Path $dist3 'archive'))) 'WhatIf creates no archive dir'

    # --- Test 5: checksum mismatch is reported --------------------------------
    Write-Host 'Test: checksum mismatch surfaces a warning'
    $dist4 = Join-Path $tmp 'case4'
    $win4 = New-FakeDist -Root $dist4
    # Corrupt the setup AFTER checksums were written? Checksums only cover exe+zip;
    # corrupt the zip so its recorded hash no longer matches.
    $zip4 = Join-Path $win4 'NOVA-Flash-Studio-9.9.9-test-windows-x64.zip'
    [System.IO.File]::WriteAllBytes($zip4, [byte[]](1..20))
    $warnings = @()
    $prev = $WarningPreference
    try {
        & $archiveScript -DistRoot $dist4 -WarningVariable +archiveWarnings
    } finally { $WarningPreference = $prev }
    $warnText = ($archiveWarnings | Out-String)
    Assert-True ($warnText -match 'MISMATCH') 'checksum mismatch produces a warning'
    Assert-True (Test-Path (Join-Path $dist4 'archive\9.9.9-test')) 'corrupt artifact still archived (preservation)'

    # --- Test 6: empty windows dir is a no-op ---------------------------------
    Write-Host 'Test: no artifacts is a clean no-op'
    $dist5 = Join-Path $tmp 'case5'
    New-Item -ItemType Directory -Path (Join-Path $dist5 'windows') -Force | Out-Null
    $err = $null; try { & $archiveScript -DistRoot $dist5 } catch { $err = $_ }; Assert-True ($null -eq $err) 'empty windows dir exits cleanly'

    # --- Test 7: missing windows dir is a no-op --------------------------------
    $dist6 = Join-Path $tmp 'case6'
    New-Item -ItemType Directory -Path $dist6 -Force | Out-Null
    $err = $null; try { & $archiveScript -DistRoot $dist6 } catch { $err = $_ }; Assert-True ($null -eq $err) 'missing windows dir exits cleanly'
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures -gt 0) {
    Write-Host "`n$failures test(s) FAILED." -ForegroundColor Red
    exit 1
}
Write-Host "`nAll archive-release tests passed." -ForegroundColor Green
exit 0
