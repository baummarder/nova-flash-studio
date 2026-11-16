<#
.SYNOPSIS
    Shared code-signing helpers for the NOeVA Windows release pipeline (M29).

.DESCRIPTION
    Dot-source this file from scripts/sign-windows.ps1, package-windows.ps1,
    package-installer.ps1 and the release/verify scripts. It provides:

      - Find-NovaSignTool          locate signtool.exe (Windows SDK)
      - Get-NovaSigningConfig      resolve certificate configuration
      - Test-NovaSigningCertificate validate identity/validity/EKU/chain
      - Get-NovaSignableFiles      classify Authenticode-signable files
      - Invoke-NovaArtifactSigning sign + verify + timestamp, per-file status
      - Write-NovaSigningStatus    persist SIGNING-STATUS.json

    Signing statuses (M29.4):
      UNSIGNED, SIGNING_NOT_CONFIGURED, SIGNED, SIGNATURE_INVALID,
      CERTIFICATE_EXPIRED, SIGNING_FAILED

    Security rules:
      - No private keys or passwords in the repository, logs or command
        lines. PFX passwords are read from the NOVA_SIGN_PFX_PASSWORD
        environment variable only and are never echoed.
      - A missing certificate yields SIGNING_NOT_CONFIGURED (build may
        continue unsigned). An invalid/expired certificate is an error.
#>

$Script:NovaSigningStatuses = @(
    'UNSIGNED',
    'SIGNING_NOT_CONFIGURED',
    'SIGNED',
    'SIGNATURE_INVALID',
    'CERTIFICATE_EXPIRED',
    'SIGNING_FAILED'
)

$Script:NovaCodeSigningOid = '1.3.6.1.5.5.7.3.3'
$Script:NovaDefaultTimestampUrl = 'http://timestamp.digicert.com'

function Find-NovaSignTool {
    <#
    .SYNOPSIS Locate signtool.exe (Windows SDK). Returns $null when absent.
    #>
    $candidates = @()
    $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $kitsRoot = 'C:\Program Files (x86)\Windows Kits\10\bin'
    if (Test-Path -LiteralPath $kitsRoot) {
        $candidates += Get-ChildItem -LiteralPath $kitsRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' }
    }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    return $null
}

function Get-NovaSigningConfig {
    <#
    .SYNOPSIS
        Resolve the signing configuration. Precedence:
          1. Environment variables (CI secrets):
               NOVA_SIGN_CERT_THUMBPRINT  - cert in Windows store
               NOVA_SIGN_CERT_STORE       - My|Root|CA (default My)
               NOVA_SIGN_CERT_SUBJECT     - subject substring fallback
               NOVA_SIGN_PFX_PATH         - PFX file (password via env only)
               NOVA_SIGN_TIMESTAMP_URL    - RFC-3161 TSA (default digicert)
               NOVA_SIGN_REQUIRE          - '1' => unsigned is an error
          2. packaging/signing.json (same keys, camelCase).
        Returns $null when nothing is configured.
    #>
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    $config = [ordered]@{
        mode           = $null      # 'store' | 'pfx' | $null
        thumbprint     = $null
        subject        = $null
        store          = 'My'
        pfxPath        = $null
        timestampUrl   = $Script:NovaDefaultTimestampUrl
        requireSigned  = $false
        source         = $null
    }

    $jsonPath = Join-Path $RepoRoot 'packaging\signing.json'
    if (Test-Path -LiteralPath $jsonPath -PathType Leaf) {
        try {
            $doc = [System.IO.File]::ReadAllText($jsonPath, [System.Text.Encoding]::UTF8) |
                ConvertFrom-Json
            foreach ($prop in $doc.PSObject.Properties) {
                $key = $prop.Name -replace '_', ''
                switch -Regex ($key) {
                    'thumbprint'   { $config.thumbprint    = [string]$prop.Value }
                    'subject'      { $config.subject       = [string]$prop.Value }
                    'store'        { $config.store         = [string]$prop.Value }
                    'pfxpath'      { $config.pfxPath       = [string]$prop.Value }
                    'timestampurl' { $config.timestampUrl  = [string]$prop.Value }
                    'requiresigned'{ $config.requireSigned = [bool]$prop.Value }
                }
            }
            $config.source = "signing.json"
        } catch {
            throw "packaging/signing.json is invalid JSON: $_"
        }
    }

    # Environment overrides (never logged).
    if ($env:NOVA_SIGN_CERT_THUMBPRINT) { $config.thumbprint = $env:NOVA_SIGN_CERT_THUMBPRINT }
    if ($env:NOVA_SIGN_CERT_SUBJECT)    { $config.subject    = $env:NOVA_SIGN_CERT_SUBJECT }
    if ($env:NOVA_SIGN_CERT_STORE)      { $config.store      = $env:NOVA_SIGN_CERT_STORE }
    if ($env:NOVA_SIGN_PFX_PATH)        { $config.pfxPath    = $env:NOVA_SIGN_PFX_PATH }
    if ($env:NOVA_SIGN_TIMESTAMP_URL)   { $config.timestampUrl = $env:NOVA_SIGN_TIMESTAMP_URL }
    if ($env:NOVA_SIGN_REQUIRE -eq '1') { $config.requireSigned = $true }
    if ($env:NOVA_SIGN_SOURCE)          { $config.source = $env:NOVA_SIGN_SOURCE }

    if ($config.pfxPath) {
        $config.mode = 'pfx'
    } elseif ($config.thumbprint -or $config.subject) {
        $config.mode = 'store'
    } else {
        return $null
    }
    return $config
}

function Find-NovaSigningCertificate {
    <#
    .SYNOPSIS
        Locate the configured certificate. Store mode searches
        Cert:\<store>\My by thumbprint then subject substring.
        Returns the X509Certificate2 or $null.
    #>
    param([Parameter(Mandatory = $true)]$Config)
    if ($Config.mode -ne 'store') { return $null }
    $storePath = "Cert:\CurrentUser\$($Config.store)"
    if (-not (Test-Path -LiteralPath $storePath)) { return $null }
    $certs = @(Get-ChildItem -LiteralPath $storePath -ErrorAction SilentlyContinue)
    if ($Config.thumbprint) {
        $wanted = $Config.thumbprint.Replace(' ', '').ToUpperInvariant()
        $match = $certs | Where-Object {
            $_.Thumbprint.Replace(' ', '').ToUpperInvariant() -eq $wanted
        } | Select-Object -First 1
        if ($match) { return $match }
        return $null
    }
    if ($Config.subject) {
        return $certs | Where-Object {
            $_.Subject -like "*$($Config.subject)*"
        } | Sort-Object NotAfter -Descending | Select-Object -First 1
    }
    return $null
}

function Test-NovaSigningCertificate {
    <#
    .SYNOPSIS
        Validate a certificate for Authenticode use.
        Returns @{ Status; Certificate; Reason; Details }.
        Status is one of the M29.4 signing statuses or 'READY'.
    #>
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]$SignTool
    )

    $result = @{
        Status         = 'SIGNING_NOT_CONFIGURED'
        Certificate    = $null
        Reason         = ''
        Details        = [ordered]@{}
        pfxImported    = $false
        pfxThumbprint  = $null
    }

    # PFX mode: import into CurrentUser\My temporarily (password from env
    # only, never on a command line), sign via thumbprint, remove after.
    $importedPfx = $null
    if ($Config.mode -eq 'pfx') {
        if (-not (Test-Path -LiteralPath $Config.pfxPath -PathType Leaf)) {
            $result.Status = 'SIGNING_FAILED'
            $result.Reason = "PFX file not found: $($Config.pfxPath)"
            return $result
        }
        $passwordEnv = $env:NOVA_SIGN_PFX_PASSWORD
        if ([string]::IsNullOrEmpty($passwordEnv)) {
            $result.Status = 'SIGNING_FAILED'
            $result.Reason = 'NOVA_SIGN_PFX_PASSWORD environment variable is not set.'
            return $result
        }
        try {
            $secure = ConvertTo-SecureString $passwordEnv -AsPlainText -Force
            $importedPfx = Import-PfxCertificate `
                -FilePath $Config.pfxPath `
                -CertStoreLocation 'Cert:\CurrentUser\My' `
                -Password $secure `
                -Exportable:$false `
                -ErrorAction Stop
            # Keep the temporary import until signing completes -- callers
            # must invoke Remove-NovaTemporaryCertificate afterwards.
            $result.pfxImported = $true
            $result.pfxThumbprint = $importedPfx.Thumbprint
            $configCopy = $Config.Clone()
            $configCopy.mode = 'store'
            $configCopy.store = 'My'
            $configCopy.thumbprint = $importedPfx.Thumbprint
            $configCopy.subject = $null
            $Config = $configCopy
        } catch {
            $result.Status = 'SIGNING_FAILED'
            $result.Reason = "PFX import failed: $($_.Exception.Message)"
            return $result
        }
    }

    try {
        $cert = Find-NovaSigningCertificate -Config $Config
        if ($null -eq $cert) {
            $result.Reason = if ($Config.mode -eq 'pfx') {
                'PFX certificate could not be resolved after import.'
            } else {
                'No certificate matched thumbprint/subject in the Windows store.'
            }
            return $result
        }
        $result.Certificate = $cert
        $result.Details.subject   = $cert.Subject
        $result.Details.issuer    = $cert.Issuer
        $result.Details.thumbprint = $cert.Thumbprint
        $result.Details.notBefore = $cert.NotBefore.ToString('o')
        $result.Details.notAfter  = $cert.NotAfter.ToString('o')

        $now = Get-Date
        if ($now -lt $cert.NotBefore -or $now -gt $cert.NotAfter) {
            $result.Status = 'CERTIFICATE_EXPIRED'
            $result.Reason = "Certificate validity $($cert.NotBefore.ToString('yyyy-MM-dd'))..$($cert.NotAfter.ToString('yyyy-MM-dd')) does not cover today."
            return $result
        }

        # PS5.1 exposes EnhancedKeyUsageList items as
        # EnhancedKeyUsageRepresentation; ObjectId.ToString() yields the OID.
        $ekuOids = @($cert.EnhancedKeyUsageList | ForEach-Object { "$($_.ObjectId)" })
        $result.Details.eku = $ekuOids -join ','
        if ($ekuOids -notcontains $Script:NovaCodeSigningOid) {
            $result.Status = 'SIGNING_FAILED'
            $result.Reason = 'Certificate lacks the Code Signing EKU (1.3.6.1.5.5.7.3.3).'
            return $result
        }

        # Chain check: must at least build a chain; self-signed test certs
        # are allowed locally but flagged (never claimed as trusted).
        $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
        $chain.ChainPolicy.RevocationMode = 'NoCheck'
        $built = $chain.Build($cert)
        $result.Details.chainBuilt = $built
        $result.Details.chainStatus = @($chain.ChainStatus | ForEach-Object { $_.Status }) -join ','
        $selfSigned = ($cert.Subject -eq $cert.Issuer)
        $result.Details.selfSigned = $selfSigned
        if (-not $built -and -not $selfSigned) {
            $result.Status = 'SIGNING_FAILED'
            $result.Reason = 'Certificate chain could not be built.'
            return $result
        }

        $result.Status = 'READY'
        return $result
    } catch {
        $result.Status = 'SIGNING_FAILED'
        $result.Reason = "Certificate validation error: $($_.Exception.Message)"
        return $result
    }
}

function Remove-NovaTemporaryCertificate {
    <#
    .SYNOPSIS Remove a PFX-imported temporary certificate from the store.
    #>
    param([Parameter(Mandatory = $true)]$CertInfo)
    if ($CertInfo.pfxImported -and $CertInfo.pfxThumbprint) {
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($CertInfo.pfxThumbprint)" `
            -ErrorAction SilentlyContinue
        $CertInfo.pfxImported = $false
    }
}

function Get-NovaSignableFiles {
    <#
    .SYNOPSIS Return Authenticode-signable files (.exe/.dll) under Path.
    ZIPs and other containers are deliberately excluded -- contained
    executables are signed before archiving.
    #>
    param([Parameter(Mandatory = $true)][string]$Path)
    $items = if (Test-Path -LiteralPath $Path -PathType Container) {
        Get-ChildItem -LiteralPath $Path -Recurse -File -ErrorAction SilentlyContinue
    } else {
        Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
    }
    return @($items | Where-Object {
        $_.Extension -in '.exe', '.dll' -and $_.Length -gt 0
    })
}

function Invoke-NovaNative {
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
    $process = [System.Diagnostics.Process]::Start($startInfo)
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    [pscustomobject]@{ ExitCode = $process.ExitCode; Output = $stdout; Error = $stderr }
}

function Invoke-NovaArtifactSigning {
    <#
    .SYNOPSIS
        Sign and verify every signable file. Returns per-file records:
        @{ File; Status; Thumbprint; Timestamped; Verified; Detail }.
        Never throws for per-file failures; the caller decides.
    #>
    param(
        [Parameter(Mandatory = $true)][string[]]$Files,
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]$CertInfo,
        [Parameter(Mandatory = $true)][string]$SignTool,
        [string]$Description = 'NOVA Flash Studio'
    )

    $records = @()
    $thumb = $CertInfo.Certificate.Thumbprint
    foreach ($file in $Files) {
        $name = Split-Path -Leaf $file
        $record = [ordered]@{
            file        = $name
            path        = $file
            status      = 'UNSIGNED'
            thumbprint  = $thumb
            timestamped = $false
            verified    = $false
            detail      = ''
        }
        $signArgs = @(
            'sign', '/fd', 'SHA256',
            '/tr', $Config.timestampUrl, '/td', 'SHA256',
            '/sha1', $thumb,
            '/d', "`"$Description`"",
            "`"$file`""
        ) -join ' '
        $signResult = Invoke-NovaNative -Executable $SignTool -Arguments $signArgs
        if ($signResult.ExitCode -ne 0) {
            $record.status = 'SIGNING_FAILED'
            $record.detail = ($signResult.Output + $signResult.Error).Trim()
            $records += [pscustomobject]$record
            continue
        }
        $verifyArgs = "verify /pa `"$file`""
        $verifyResult = Invoke-NovaNative -Executable $SignTool -Arguments $verifyArgs
        $verified = $verifyResult.ExitCode -eq 0 -and
            ($verifyResult.Output -match 'Successfully verified')
        $timestamped = $verifyResult.Output -match 'TimeStamp'
        if ($verified) {
            $record.status = 'SIGNED'
            $record.verified = $true
            $record.timestamped = $timestamped
        } else {
            $record.status = 'SIGNATURE_INVALID'
            $record.detail = ($verifyResult.Output + $verifyResult.Error).Trim()
        }
        $records += [pscustomobject]$record
    }
    return $records
}

function Write-NovaSigningStatus {
    <#
    .SYNOPSIS Write SIGNING-STATUS.json next to the artifacts.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$Edition,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)]$CertInfo,
        [Parameter(Mandatory = $true)]$Records,
        [string]$Publisher = ''
    )
    $doc = [ordered]@{
        schemaVersion = 1
        edition       = $Edition
        version       = $Version
        generatedUtc  = (Get-Date).ToUniversalTime().ToString('o')
        publisher     = $Publisher
        certificate   = if ($CertInfo.Certificate) {
            [ordered]@{
                subject    = $CertInfo.Certificate.Subject
                issuer     = $CertInfo.Certificate.Issuer
                thumbprint = $CertInfo.Certificate.Thumbprint
                notAfter   = $CertInfo.Certificate.NotAfter.ToString('o')
                selfSigned = $CertInfo.Details.selfSigned
            }
        } else { $null }
        files         = @($Records)
        overall       = if (@($Records | Where-Object { $_.status -ne 'SIGNED' }).Count -eq 0 -and
                              @($Records).Count -gt 0) { 'SIGNED' } else { 'UNSIGNED' }
    }
    $path = Join-Path $Directory 'SIGNING-STATUS.json'
    [System.IO.File]::WriteAllText(
        $path,
        ($doc | ConvertTo-Json -Depth 6),
        [System.Text.UTF8Encoding]::new($false)
    )
    return $path
}
