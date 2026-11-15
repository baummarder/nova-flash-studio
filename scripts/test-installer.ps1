#Requires -Version 5.1
<#
.SYNOPSIS
    M23.5 headless installer tests for all three editions.

    For each edition (user / edu / enterprise):
      1. --smoke-test on the built Setup exe
      2. Headless per-user install into a temp dir (no shortcuts,
         no file associations) via --install
      3. Verify payload files, install-manifest.json and the
         per-edition Uninstall registry entry
      4. Run the installed uninstaller with --uninstall --quiet
      5. Verify install dir and registry entry are gone and that a
         sentinel file in the edition config dir (user data) survives

    No UAC, no shortcuts, no associations are touched. Safe to run
    repeatedly; uses unique temp dirs.
#>
[CmdletBinding()]
param(
    [string]$DistRoot = ''
)

if ([string]::IsNullOrEmpty($DistRoot)) {
    $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
    $DistRoot = Join-Path (Split-Path -Parent $scriptRoot) 'dist\windows'
}

$ErrorActionPreference = 'Stop'
$script:failures = 0

function Pass([string]$name) { Write-Output "  PASS  $name" }
function Fail([string]$name, [string]$why) {
    Write-Output "  FAIL  $name — $why"
    $script:failures++
}
function Check([bool]$cond, [string]$name, [string]$why = '') {
    if ($cond) { Pass $name } else { Fail $name $why }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$editionsPath = Join-Path $repoRoot 'packaging\editions.json'
$editionList = (Get-Content -LiteralPath $editionsPath -Raw -Encoding UTF8 | ConvertFrom-Json).editions

foreach ($ed in $editionList) {
    Write-Output ""
    Write-Output "#################### Edition: $($ed.id) ####################"
    $setup = Join-Path $DistRoot "$($ed.id)\$($ed.setupExe)"
    if (-not (Test-Path -LiteralPath $setup)) {
        Fail $ed.id "setup exe missing: $setup"
        continue
    }

    # 1. smoke test -------------------------------------------------------
    $smoke = & $setup --smoke-test 2>&1 | Out-String
    Check ($LASTEXITCODE -eq 0 -and $smoke -match 'smoke-test: OK') `
        "$($ed.id): smoke-test" ($smoke.Trim())

    # 2. headless per-user install ----------------------------------------
    $installDir = Join-Path $env:TEMP ("nova-m23-test-" + $ed.id + "-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    $installOut = & $setup --install $installDir --per-user --no-desktop --no-start-menu --no-assoc --quiet 2>&1 | Out-String
    Check ($LASTEXITCODE -eq 0) "$($ed.id): headless install" $installOut.Trim()

    $appExe = Join-Path $installDir $ed.appExe
    $manifest = Join-Path $installDir 'install-manifest.json'
    Check (Test-Path -LiteralPath $appExe) "$($ed.id): app exe installed"
    Check (Test-Path -LiteralPath $manifest) "$($ed.id): install manifest written"
    Check (Test-Path -LiteralPath (Join-Path $installDir 'docs')) `
        "$($ed.id): docs folder shipped"

    # Uninstall registry entry (HKCU for per-user).
    $regPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$($ed.uninstallKey)"
    Check (Test-Path -LiteralPath $regPath) "$($ed.id): uninstall registry entry"

    # 3. user-data sentinel ----------------------------------------------
    # The edition config dir must never be touched by install/uninstall.
    $configDir = Join-Path $env:APPDATA $ed.configDir
    $sentinel = Join-Path $configDir 'm23-sentinel.txt'
    $createdConfig = $false
    try {
        if (-not (Test-Path -LiteralPath $configDir)) {
            New-Item -ItemType Directory -Path $configDir -Force | Out-Null
            $createdConfig = $true
        }
        Set-Content -LiteralPath $sentinel -Value 'user-data-must-survive' -NoNewline

        # 4. uninstall -----------------------------------------------------
        $uninstaller = Join-Path $installDir $ed.uninstallExe
        Check (Test-Path -LiteralPath $uninstaller) "$($ed.id): uninstaller copied"
        if (Test-Path -LiteralPath $uninstaller) {
            $unOut = & $uninstaller --uninstall --quiet 2>&1 | Out-String
            Check ($LASTEXITCODE -eq 0) "$($ed.id): headless uninstall" $unOut.Trim()
        }

        Start-Sleep -Milliseconds 300
        Check (-not (Test-Path -LiteralPath $appExe)) `
            "$($ed.id): app exe removed"
        Check (-not (Test-Path -LiteralPath $manifest)) `
            "$($ed.id): manifest removed"
        Check (-not (Test-Path -LiteralPath $regPath)) `
            "$($ed.id): uninstall registry entry removed"
        Check (Test-Path -LiteralPath $sentinel) `
            "$($ed.id): user data preserved"
        if (Test-Path -LiteralPath $sentinel) {
            $content = Get-Content -LiteralPath $sentinel -Raw
            Check ($content -eq 'user-data-must-survive') `
                "$($ed.id): user data content intact"
        }
    }
    finally {
        Remove-Item -LiteralPath $sentinel -Force -ErrorAction SilentlyContinue
        if ($createdConfig) {
            # Only remove the config dir if we created it and it is empty.
            $left = Get-ChildItem -LiteralPath $configDir -Force -ErrorAction SilentlyContinue
            if (-not $left) { Remove-Item -LiteralPath $configDir -Force -ErrorAction SilentlyContinue }
        }
        if (Test-Path -LiteralPath $installDir) {
            Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

Write-Output ""
if ($script:failures -gt 0) {
    Write-Output "Installer tests FAILED ($($script:failures) failures)."
    exit 1
}
Write-Output "All installer tests passed."
exit 0
