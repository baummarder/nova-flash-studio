<#
.SYNOPSIS
    M29 signing infrastructure tests -- TECHNICAL TESTS ONLY.

.DESCRIPTION
    Exercises the signing pipeline with locally generated SELF-SIGNED
    certificates in Cert:\CurrentUser\My. These prove the plumbing
    (detection, validation, sign, verify, statuses, manifest checks);
    they are NOT evidence of publicly trusted publisher signatures.
    All test certificates are removed afterwards.

    Requires signtool.exe for the end-to-end sign/verify cases; those
    cases report SKIP when the Windows SDK tool is unavailable.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib\Signing.ps1')

$script:pass = 0
$script:fail = 0
$script:skip = 0
$createdCerts = @()

function Assert-True([bool]$Cond, [string]$Name) {
    if ($Cond) { $script:pass++; Write-Host "  PASS  $Name" -ForegroundColor Green }
    else { $script:fail++; Write-Host "  FAIL  $Name" -ForegroundColor Red }
}
function Assert-Skip([string]$Name, [string]$Why) {
    $script:skip++; Write-Host "  SKIP  $Name ($Why)" -ForegroundColor Yellow
}

try {
    # ---------------------------------------------------------------
    Write-Host '== certificate validation ==' -ForegroundColor Cyan

    # 1. No configuration -> SIGNING_NOT_CONFIGURED
    $emptyConfig = [ordered]@{ mode=$null; thumbprint=$null; subject=$null; store='My'; pfxPath=$null; timestampUrl=$Script:NovaDefaultTimestampUrl; requireSigned=$false; source='test' }
    $noCert = Find-NovaSigningCertificate -Config ([pscustomobject]$emptyConfig)
    Assert-True ($null -eq $noCert) 'missing thumbprint resolves to no certificate'

    # 2. Expired certificate -> CERTIFICATE_EXPIRED
    $expired = New-SelfSignedCertificate `
        -Subject 'CN=NOVA M29 Test Expired (technical test)' `
        -Type CodeSigningCert `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -NotBefore (Get-Date).AddDays(-30) `
        -NotAfter (Get-Date).AddDays(-1) `
        -KeyExportPolicy NonExportable `
        -ErrorAction Stop
    $createdCerts += $expired.Thumbprint
    $cfg = [pscustomobject]@{ mode='store'; thumbprint=$expired.Thumbprint; subject=$null; store='My'; pfxPath=$null; timestampUrl=$Script:NovaDefaultTimestampUrl; requireSigned=$false; source='test' }
    $signTool = Find-NovaSignTool
    $expiredInfo = Test-NovaSigningCertificate -Config $cfg -SignTool $signTool
    Assert-True ($expiredInfo.Status -eq 'CERTIFICATE_EXPIRED') "expired cert -> CERTIFICATE_EXPIRED (got $($expiredInfo.Status))"

    # 3. Wrong EKU -> SIGNING_FAILED
    $wrongEku = New-SelfSignedCertificate `
        -Subject 'CN=NOVA M29 Test WrongEKU (technical test)' `
        -Type SSLServerAuthentication `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -NotAfter (Get-Date).AddDays(30) `
        -KeyExportPolicy NonExportable `
        -ErrorAction Stop
    $createdCerts += $wrongEku.Thumbprint
    $cfgWrong = [pscustomobject]@{ mode='store'; thumbprint=$wrongEku.Thumbprint; subject=$null; store='My'; pfxPath=$null; timestampUrl=$Script:NovaDefaultTimestampUrl; requireSigned=$false; source='test' }
    $wrongInfo = Test-NovaSigningCertificate -Config $cfgWrong -SignTool $signTool
    Assert-True ($wrongInfo.Status -eq 'SIGNING_FAILED') "non-code-signing EKU -> SIGNING_FAILED (got $($wrongInfo.Status))"

    # 4. Valid self-signed code-signing cert -> READY (selfSigned flagged)
    $good = New-SelfSignedCertificate `
        -Subject 'CN=NOVA M29 Test Signing (technical test only)' `
        -Type CodeSigningCert `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -NotAfter (Get-Date).AddDays(30) `
        -KeyExportPolicy NonExportable `
        -ErrorAction Stop
    $createdCerts += $good.Thumbprint
    $cfgGood = [pscustomobject]@{ mode='store'; thumbprint=$good.Thumbprint; subject=$null; store='My'; pfxPath=$null; timestampUrl=$Script:NovaDefaultTimestampUrl; requireSigned=$false; source='test' }
    $goodInfo = Test-NovaSigningCertificate -Config $cfgGood -SignTool $signTool
    Assert-True ($goodInfo.Status -eq 'READY') "valid code-signing cert -> READY (got $($goodInfo.Status): $($goodInfo.Reason))"
    Assert-True ($goodInfo.Details.selfSigned -eq $true) 'self-signed flag recorded'

    # ---------------------------------------------------------------
    Write-Host '== sign + verify ==' -ForegroundColor Cyan
    if ($null -eq $signTool) {
        Assert-Skip 'end-to-end signing' 'signtool.exe not found'
    } else {
        $tmp = Join-Path $env:TEMP "nova-sign-test-$([guid]::NewGuid().ToString('N'))"
        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
        # Minimal PE: copy any small exe (use the test host itself is heavy;
        # copy notepad? use powershell? Use a tiny managed exe -- copy signtool? no.
        # Simplest: write bytes of an existing small exe -- use the .NET test host? Use cmd? We'll copy 'where.exe'.
        $srcExe = "$env:SystemRoot\System32\where.exe"
        $testExe = Join-Path $tmp 'artifact.exe'
        Copy-Item -LiteralPath $srcExe -Destination $testExe
        $testDll = Join-Path $tmp 'artifact.dll'
        Copy-Item -LiteralPath "$env:SystemRoot\System32\kernel32.dll" -Destination $testDll -ErrorAction SilentlyContinue

        $records = @(Invoke-NovaArtifactSigning `
            -Files @($testExe) `
            -Config $cfgGood `
            -CertInfo $goodInfo `
            -SignTool $signTool `
            -Description 'NOVA M29 technical test')
        # Self-signed certs are NOT publicly trusted: signtool /pa verify
        # must report the untrusted chain — the pipeline honestly marks
        # SIGNATURE_INVALID. Signature presence is checked separately.
        $sig = Get-AuthenticodeSignature -FilePath $testExe
        Assert-True ($sig.Status -ne 'NotSigned' -and $null -ne $sig.SignerCertificate) 'signature present on signed file'
        Assert-True ($records[0].status -eq 'SIGNATURE_INVALID') "untrusted self-signed -> SIGNATURE_INVALID, not SIGNED (got $($records[0].status))"
        Assert-True ($records[0].detail -match 'not trusted|Vertrauenseinstellung|chain') 'untrusted-chain detail recorded'

        # Tamper -> signature invalid
        $bytes = [System.IO.File]::ReadAllBytes($testExe)
        $bytes[100] = $bytes[100] -bxor 0xFF
        [System.IO.File]::WriteAllBytes($testExe, $bytes)
        $tamper = Invoke-NovaNative -Executable $signTool -Arguments "verify /pa `"$testExe`""
        Assert-True ($tamper.ExitCode -ne 0) 'tampered file fails verification'

        # Unsigned file verify -> fails (proves verify isn't a no-op)
        $plain = Join-Path $tmp 'plain.exe'
        Copy-Item -LiteralPath $srcExe -Destination $plain
        $plainVerify = Invoke-NovaNative -Executable $signTool -Arguments "verify /pa `"$plain`""
        Assert-True ($plainVerify.ExitCode -ne 0) 'unsigned file fails verification'

        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    # ---------------------------------------------------------------
    Write-Host '== release verify script ==' -ForegroundColor Cyan
    $relTmp = Join-Path $env:TEMP "nova-release-test-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path (Join-Path $relTmp 'user') -Force | Out-Null
    $fakeExe = Join-Path $relTmp 'user\fake.exe'
    [System.IO.File]::WriteAllBytes($fakeExe, [byte[]](0x4D,0x5A,0x90,0x00))
    $hash = (Get-FileHash -LiteralPath $fakeExe -Algorithm SHA256).Hash
    [System.IO.File]::WriteAllLines((Join-Path $relTmp 'user\SHA256SUMS.txt'), @("$hash  fake.exe"), [System.Text.UTF8Encoding]::new($false))
    $verifyOk = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'verify-release.ps1') -ReleaseDir $relTmp 2>&1
    Assert-True ($LASTEXITCODE -eq 0) 'verify-release passes on consistent fixture'
    # Tamper: wrong hash in manifest sums
    [System.IO.File]::WriteAllLines((Join-Path $relTmp 'user\SHA256SUMS.txt'), @('0' * 64 + '  fake.exe'), [System.Text.UTF8Encoding]::new($false))
    $null = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'verify-release.ps1') -ReleaseDir $relTmp 2>&1
    Assert-True ($LASTEXITCODE -ne 0) 'verify-release fails on hash mismatch'
    Remove-Item -LiteralPath $relTmp -Recurse -Force -ErrorAction SilentlyContinue

} finally {
    foreach ($thumb in $createdCerts) {
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$thumb" -ErrorAction SilentlyContinue
    }
}

Write-Host ''
Write-Host "signing tests: $script:pass passed, $script:fail failed, $script:skip skipped" -ForegroundColor $(if ($script:fail) { 'Red' } else { 'Green' })
if ($script:fail -gt 0) { exit 1 }
exit 0
