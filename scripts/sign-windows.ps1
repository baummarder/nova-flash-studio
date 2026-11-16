<#
.SYNOPSIS
    Sign (or classify) Windows release artifacts for one edition (M29).

.DESCRIPTION
    Central entry point of the NOeVA signing pipeline. It:

      1. resolves the signing configuration (packaging/signing.json or
         NOVA_SIGN_* environment variables -- see scripts/lib/Signing.ps1),
      2. locates signtool.exe and validates the certificate
         (identity, validity window, Code Signing EKU, chain),
      3. signs every Authenticode-signable file (.exe/.dll) with
         SHA-256 + RFC-3161 timestamp,
      4. verifies each signature and aborts on SIGNATURE_INVALID /
         CERTIFICATE_EXPIRED / SIGNING_FAILED when -RequireSigned,
      5. writes SIGNING-STATUS.json next to the artifacts.

    Without configuration the run reports SIGNING_NOT_CONFIGURED and
    exits 0 (unsigned development build). With -RequireSigned any
    non-SIGNED outcome is a hard error.

.PARAMETER Path
    File or directory to process.

.PARAMETER Edition
    Edition id for the status file (user|edu|enterprise|all).

.PARAMETER Version
    Product version recorded in SIGNING-STATUS.json.

.PARAMETER RequireSigned
    Fail unless every artifact verifies as SIGNED.

.PARAMETER WhatIfSign
    Validate config + certificate only; do not modify files.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [ValidateSet('user', 'edu', 'enterprise', 'all')]
    [string]$Edition = 'all',
    [string]$Version = '',
    [switch]$RequireSigned,
    [switch]$WhatIfSign
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib\Signing.ps1')

if (-not (Test-Path -LiteralPath $Path)) {
    throw "Signing target not found: $Path"
}

$config = Get-NovaSigningConfig -RepoRoot $root
$signTool = Find-NovaSignTool

$certInfo = @{
    Status        = 'SIGNING_NOT_CONFIGURED'
    Certificate   = $null
    Reason        = ''
    Details       = @{}
    pfxImported   = $false
    pfxThumbprint = $null
}

try {
    if ($null -eq $config) {
        Write-Host 'SIGNING_NOT_CONFIGURED: no certificate configured (packaging/signing.json or NOVA_SIGN_* env).' -ForegroundColor Yellow
    } else {
        if ($null -eq $signTool) {
            $certInfo.Status = 'SIGNING_FAILED'
            $certInfo.Reason = 'signtool.exe not found (Windows SDK required).'
        } else {
            Write-Host "Signing tool: $signTool" -ForegroundColor Cyan
            $certInfo = Test-NovaSigningCertificate -Config $config -SignTool $signTool
            Write-Host "Certificate: $($certInfo.Status) -- $($certInfo.Reason)" -ForegroundColor Cyan
            if ($certInfo.Certificate) {
                Write-Host "  Subject:    $($certInfo.Certificate.Subject)"
                Write-Host "  Thumbprint: $($certInfo.Certificate.Thumbprint)"
                Write-Host "  Expires:    $($certInfo.Certificate.NotAfter.ToString('yyyy-MM-dd'))"
                if ($certInfo.Details.selfSigned) {
                    Write-Warning 'Self-signed certificate -- technical test only, NOT a trusted publisher signature.'
                }
            }
        }
    }

    $files = @(Get-NovaSignableFiles -Path $Path)
    Write-Host "Signable files: $($files.Count)" -ForegroundColor Cyan

    $records = @()
    if ($certInfo.Status -eq 'READY' -and -not $WhatIfSign) {
        $records = @(Invoke-NovaArtifactSigning `
            -Files ($files | ForEach-Object { $_.FullName }) `
            -Config $config `
            -CertInfo $certInfo `
            -SignTool $signTool `
            -Description 'NOVA Flash Studio')
        foreach ($record in $records) {
            $color = if ($record.status -eq 'SIGNED') { 'Green' } else { 'Red' }
            Write-Host ("  {0,-24} {1,-22} verified={2} timestamped={3}" -f `
                $record.file, $record.status, $record.verified, $record.timestamped) -ForegroundColor $color
        }
    } elseif ($files.Count -gt 0) {
        foreach ($file in $files) {
            $records += [pscustomobject][ordered]@{
                file        = $file.Name
                path        = $file.FullName
                status      = if ($certInfo.Status -eq 'READY' -and $WhatIfSign) { 'UNSIGNED' } else { $certInfo.Status }
                thumbprint  = $null
                timestamped = $false
                verified    = $false
                detail      = $certInfo.Reason
            }
        }
    }

    # Persist status next to the artifacts (edition dir for dist paths).
    $statusDir = if (Test-Path -LiteralPath $Path -PathType Container) { $Path } else { Split-Path -Parent $Path }
    if ($records.Count -gt 0 -or $certInfo.Status -ne 'SIGNING_NOT_CONFIGURED') {
        $statusPath = Write-NovaSigningStatus `
            -Directory $statusDir `
            -Edition $Edition `
            -Version $Version `
            -CertInfo $certInfo `
            -Records $records
        Write-Host "Status written: $statusPath"
    }

    $bad = @($records | Where-Object { $_.status -in @('SIGNATURE_INVALID', 'CERTIFICATE_EXPIRED', 'SIGNING_FAILED') })
    $unsigned = @($records | Where-Object { $_.status -ne 'SIGNED' })
    if ($RequireSigned -and ($bad.Count -gt 0 -or $unsigned.Count -gt 0)) {
        throw "Signing requirement not met: $($unsigned.Count) artifact(s) not SIGNED, $($bad.Count) invalid/failed."
    }
    if ($certInfo.Status -in @('CERTIFICATE_EXPIRED', 'SIGNING_FAILED') -and $RequireSigned) {
        throw "Certificate problem: $($certInfo.Status) -- $($certInfo.Reason)"
    }
    Write-Host "Done. Overall: $(if ($records.Count -gt 0 -and $unsigned.Count -eq 0) { 'SIGNED' } else { $certInfo.Status })" -ForegroundColor Green
} finally {
    Remove-NovaTemporaryCertificate -CertInfo $certInfo
}
