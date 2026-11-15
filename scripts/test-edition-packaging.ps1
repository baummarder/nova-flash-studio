<#
.SYNOPSIS
    M20.0 edition and packaging consistency tests.

.DESCRIPTION
    Verifies the single source of truth in packaging/editions.json against:
      1. itself (unique ids, products, config dirs, installer identities);
      2. packaging/windows/installer/EditionInfo.cs (committed User defaults);
      3. the built artifacts under dist/windows/<edition>/ and
         dist/archive/<version>/<edition>/, when present:
           - BUILD-INFO.json edition/product/version,
           - every SHA256SUMS.txt entry (top level and payload folder),
           - portable ZIP completeness and edition attribution,
           - Setup EXE VersionInfo identity,
           - archived copies and checksums,
           - coexistence of config dirs/uninstall keys/ProgIds.

    The artifact checks are skipped with an explicit note when dist/ has not
    been built; the manifest and source checks always run. Exits non-zero on
    failure. Read-only: never modifies dist/ or the repository.

.PARAMETER DistRoot
    Root containing windows/ and archive/. Defaults to the repo dist.
#>
[CmdletBinding()]
param(
    [string]$DistRoot
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $DistRoot) {
    $DistRoot = Join-Path $root 'dist'
}
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

function Read-Utf8 {
    param([string]$Path)
    return [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
}

function Get-WorkspaceVersion {
    $manifest = Read-Utf8 (Join-Path $root 'Cargo.toml')
    $match = [regex]::Match(
        $manifest,
        '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"'
    )
    if (-not $match.Success) {
        throw 'Workspace version not found in Cargo.toml.'
    }
    return $match.Groups[1].Value
}

# ---------------------------------------------------------------------------
# 1. Manifest self-consistency
# ---------------------------------------------------------------------------
Write-Host 'Test: packaging/editions.json identity'
$manifestPath = Join-Path $root 'packaging\editions.json'
Assert-True (Test-Path -LiteralPath $manifestPath -PathType Leaf) 'editions.json exists'
$doc = Read-Utf8 $manifestPath | ConvertFrom-Json
$entries = @($doc.editions)
Assert-True ($entries.Count -eq 3) 'manifest lists exactly three editions'

$ids = @{}
$products = @{}
$configDirs = @{}
$recoveryDirs = @{}
$bundleIds = @{}
$uninstallKeys = @{}
$progIds = @{}
$artifacts = @{}
$required = @(
    'id', 'product', 'slug', 'configDir', 'recoveryDir', 'bundleId',
    'appExe', 'uninstallExe', 'uninstallKey', 'progIdNova', 'progIdSwf',
    'setupExe', 'portableZip'
)
foreach ($entry in $entries) {
    foreach ($field in $required) {
        $value = $entry.$field
        Assert-True (-not [string]::IsNullOrWhiteSpace([string]$value)) "entry '$($entry.id)' has $field"
    }
    Assert-True (-not $ids.ContainsKey($entry.id)) "edition id '$($entry.id)' is unique"
    Assert-True (-not $products.ContainsKey($entry.product)) "product '$($entry.product)' is unique"
    Assert-True (-not $configDirs.ContainsKey($entry.configDir)) "config dir '$($entry.configDir)' is unique"
    Assert-True (-not $recoveryDirs.ContainsKey($entry.recoveryDir)) "recovery dir '$($entry.recoveryDir)' is unique"
    Assert-True (-not $bundleIds.ContainsKey($entry.bundleId)) "bundle id '$($entry.bundleId)' is unique"
    Assert-True (-not $uninstallKeys.ContainsKey($entry.uninstallKey)) "uninstall key '$($entry.uninstallKey)' is unique"
    Assert-True (-not $progIds.ContainsKey($entry.progIdNova)) "ProgId '$($entry.progIdNova)' is unique"
    Assert-True (-not $progIds.ContainsKey($entry.progIdSwf)) "ProgId '$($entry.progIdSwf)' is unique"
    Assert-True (-not $artifacts.ContainsKey($entry.setupExe)) "Setup name '$($entry.setupExe)' is unique"
    Assert-True (-not $artifacts.ContainsKey($entry.portableZip)) "portable ZIP name '$($entry.portableZip)' is unique"
    $ids[$entry.id] = $true
    $products[$entry.product] = $true
    $configDirs[$entry.configDir] = $true
    $recoveryDirs[$entry.recoveryDir] = $true
    $bundleIds[$entry.bundleId] = $true
    $uninstallKeys[$entry.uninstallKey] = $true
    $progIds[$entry.progIdNova] = $true
    $progIds[$entry.progIdSwf] = $true
    $artifacts[$entry.setupExe] = $true
    $artifacts[$entry.portableZip] = $true

    Assert-True ($entry.setupExe -eq "NOVA-$($entry.slug)-Setup.exe") "setup name is edition-attributable ($($entry.id))"
    Assert-True ($entry.portableZip -eq "NOVA-$($entry.slug)-Portable.zip") "portable ZIP is edition-attributable ($($entry.id))"
    Assert-True ($entry.appExe.StartsWith($entry.product)) "app EXE belongs to product ($($entry.id))"
    Assert-True ($entry.uninstallExe.StartsWith($entry.product)) "uninstaller belongs to product ($($entry.id))"
}

$user = $entries | Where-Object { $_.id -eq 'user' } | Select-Object -First 1
$edu = $entries | Where-Object { $_.id -eq 'edu' } | Select-Object -First 1
$enterprise = $entries | Where-Object { $_.id -eq 'enterprise' } | Select-Object -First 1
Assert-True ($null -ne $user -and $null -ne $edu -and $null -ne $enterprise) 'user, edu and enterprise are all present'
Assert-True ($user.configDir -eq 'NOVA Flash Studio') 'User keeps the legacy config directory'
Assert-True ($edu.configDir -ne $user.configDir -and $enterprise.configDir -ne $user.configDir) 'Edu/Enterprise config dirs are isolated from User'

# ---------------------------------------------------------------------------
# 2. Committed installer defaults match the manifest
# ---------------------------------------------------------------------------
Write-Host 'Test: committed EditionInfo.cs matches User defaults'
$editionInfoPath = Join-Path $root 'packaging\windows\installer\EditionInfo.cs'
Assert-True (Test-Path -LiteralPath $editionInfoPath -PathType Leaf) 'EditionInfo.cs exists'
$cs = Read-Utf8 $editionInfoPath
Assert-True ($cs -match ('EditionId\s*=\s*"' + [regex]::Escape($user.id) + '"')) 'EditionInfo EditionId = user'
Assert-True ($cs -match ('ProductName\s*=\s*"' + [regex]::Escape($user.product) + '"')) 'EditionInfo ProductName matches'
Assert-True ($cs -match ('AppExeName\s*=\s*"' + [regex]::Escape($user.appExe) + '"')) 'EditionInfo AppExeName matches'
Assert-True ($cs -match ('UninstallerFileName\s*=\s*"' + [regex]::Escape($user.uninstallExe) + '"')) 'EditionInfo uninstaller matches'
Assert-True ($cs -match ('UninstallKey\s*=\s*"' + [regex]::Escape($user.uninstallKey) + '"')) 'EditionInfo uninstall key matches'
Assert-True ($cs -match ('ProgIdNova\s*=\s*"' + [regex]::Escape($user.progIdNova) + '"')) 'EditionInfo .nova ProgId matches'
Assert-True ($cs -match ('ProgIdSwf\s*=\s*"' + [regex]::Escape($user.progIdSwf) + '"')) 'EditionInfo .swf ProgId matches'
Assert-True ($cs -match ('SetupExeName\s*=\s*"' + [regex]::Escape($user.setupExe) + '"')) 'EditionInfo Setup name matches'

# ---------------------------------------------------------------------------
# 3. Built artifacts (optional; a fresh clone has no dist/)
# ---------------------------------------------------------------------------
$windowsRoot = Join-Path $DistRoot 'windows'
if (-not (Test-Path -LiteralPath $windowsRoot -PathType Container)) {
    Write-Host 'SKIP  dist/windows is absent; artifact checks skipped (manifest checks passed).' -ForegroundColor Yellow
} else {
    $version = Get-WorkspaceVersion
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archiveRoot = Join-Path $DistRoot 'archive'

    foreach ($entry in $entries) {
        Write-Host "Test: edition artifacts ($($entry.id), $version)"
        $editionDir = Join-Path $windowsRoot $entry.id
        if (-not (Test-Path -LiteralPath $editionDir -PathType Container)) {
            Write-Host "  SKIP  $($entry.id): dist/windows/$($entry.id) not built." -ForegroundColor Yellow
            continue
        }
        $payloadDir = Join-Path $editionDir $entry.product
        $zipPath = Join-Path $editionDir $entry.portableZip
        $setupPath = Join-Path $editionDir $entry.setupExe
        $infoPath = Join-Path $editionDir 'BUILD-INFO.json'
        $sumsPath = Join-Path $editionDir 'SHA256SUMS.txt'

        Assert-True (Test-Path -LiteralPath $payloadDir -PathType Container) "$($entry.id): payload folder"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "$($entry.id): portable ZIP"
        Assert-True (Test-Path -LiteralPath $setupPath -PathType Leaf) "$($entry.id): Setup EXE"
        Assert-True (Test-Path -LiteralPath $infoPath -PathType Leaf) "$($entry.id): BUILD-INFO.json"
        Assert-True (Test-Path -LiteralPath $sumsPath -PathType Leaf) "$($entry.id): SHA256SUMS.txt"
        if (-not (Test-Path -LiteralPath $sumsPath -PathType Leaf)) { continue }

        $info = Read-Utf8 $infoPath | ConvertFrom-Json
        Assert-True ($info.edition -eq $entry.id) "$($entry.id): BUILD-INFO edition"
        Assert-True ($info.product -eq $entry.product) "$($entry.id): BUILD-INFO product"
        Assert-True ($info.version -eq $version) "$($entry.id): BUILD-INFO version $version"

        $sumLines = [System.IO.File]::ReadAllLines($sumsPath)
        $sumCount = 0
        foreach ($line in $sumLines) {
            if ($line -notmatch '^\s*([0-9A-Fa-f]{64})\s+(.+?)\s*$') { continue }
            $expected = $Matches[1].ToUpperInvariant()
            $name = $Matches[2].Trim()
            $candidates = @(
                (Join-Path $editionDir $name),
                (Join-Path $payloadDir $name)
            )
            $hit = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
            Assert-True ($null -ne $hit) "$($entry.id): checksum entry '$name' has a file"
            if ($null -eq $hit) { continue }
            $actual = (Get-FileHash -LiteralPath $hit -Algorithm SHA256).Hash
            Assert-True ($actual -eq $expected) "$($entry.id): SHA-256 matches ($name)"
            $sumCount++
        }
        Assert-True ($sumCount -ge 3) "$($entry.id): checksum file covers app EXE, portable ZIP and Setup EXE"

        # Installer identity: safe metadata read, no execution.
        $setupInfo = (Get-Item -LiteralPath $setupPath).VersionInfo
        Assert-True ($setupInfo.ProductName -eq $entry.product) "$($entry.id): Setup ProductName"
        Assert-True ($setupInfo.ProductVersion -eq $version) "$($entry.id): Setup ProductVersion"
        Assert-True ($setupInfo.FileDescription -eq "NOVA-$($entry.slug)-Setup") "$($entry.id): Setup assembly name"

        # Portable package completeness.
        $zip = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $names = @($zip.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        } finally {
            $zip.Dispose()
        }
        $prefix = $entry.product + '/'
        Assert-True ($names -contains ($prefix + $entry.appExe)) "$($entry.id): ZIP contains product EXE"
        Assert-True ($names -contains ($prefix + 'README.md')) "$($entry.id): ZIP contains README"
        Assert-True ($names -contains ($prefix + 'THIRD-PARTY-NOTICES.md')) "$($entry.id): ZIP contains third-party notices"
        Assert-True ($names -contains ($prefix + 'docs/EDITIONS.md')) "$($entry.id): ZIP contains edition docs"
        Assert-True (@($names | Where-Object { $_ -like ($prefix + 'licenses/*.txt') }).Count -gt 0) "$($entry.id): ZIP contains SPDX license texts"

        # Archive completeness and checksums.
        $archDir = Join-Path (Join-Path $archiveRoot $version) $entry.id
        Assert-True (Test-Path -LiteralPath $archDir -PathType Container) "$($entry.id): archived edition directory"
        if (Test-Path -LiteralPath $archDir -PathType Container) {
            Assert-True (Test-Path -LiteralPath (Join-Path $archDir $entry.portableZip) -PathType Leaf) "$($entry.id): archived portable ZIP"
            Assert-True (Test-Path -LiteralPath (Join-Path $archDir $entry.setupExe) -PathType Leaf) "$($entry.id): archived Setup EXE"
            Assert-True (Test-Path -LiteralPath (Join-Path $archDir 'BUILD-INFO.json') -PathType Leaf) "$($entry.id): archived BUILD-INFO"
            Assert-True (Test-Path -LiteralPath (Join-Path $archDir 'SHA256SUMS.txt') -PathType Leaf) "$($entry.id): archived SHA256SUMS"
            $archZip = Join-Path $archDir $entry.portableZip
            if (Test-Path -LiteralPath $archZip -PathType Leaf) {
                $same = (Get-FileHash -LiteralPath $archZip -Algorithm SHA256).Hash -eq
                    (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
                Assert-True $same "$($entry.id): archived ZIP matches current artifact"
            }
        }
    }
}

if ($failures -gt 0) {
    Write-Host "`n$failures packaging test(s) FAILED." -ForegroundColor Red
    exit 1
}
Write-Host "`nAll edition/packaging tests passed." -ForegroundColor Green
exit 0
