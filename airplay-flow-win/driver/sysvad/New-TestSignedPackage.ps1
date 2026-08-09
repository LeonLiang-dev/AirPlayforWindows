[CmdletBinding()]
param(
    [string]$PackagePath
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($PackagePath)) {
    $PackagePath = Join-Path (Split-Path $PSScriptRoot -Parent) 'build\x64\Release'
}

function Find-FirstExistingPath {
    param([string[]]$Candidates)

    foreach ($candidate in $Candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "None of the required tool paths exists:`n$($Candidates -join "`n")"
}

$resolvedPackagePath = (Resolve-Path -LiteralPath $PackagePath).Path
$systemPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.sys'
$infPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.inf'
$catalogPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.cat'

foreach ($requiredFile in @($systemPath, $infPath)) {
    if (-not (Test-Path -LiteralPath $requiredFile)) {
        throw "Required driver package file is missing: $requiredFile"
    }
}

$signtool = Find-FirstExistingPath @(
    (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'),
    (Join-Path $env:USERPROFILE '.nuget\packages\microsoft.windows.sdk.cpp\10.0.26100.1\c\bin\10.0.26100.0\x64\signtool.exe')
)
$inf2Cat = Find-FirstExistingPath @(
    (Join-Path $env:USERPROFILE '.nuget\packages\microsoft.windows.wdk.x64\10.0.26100.6584\c\bin\10.0.26100.0\x86\Inf2Cat.exe')
)

$subject = 'CN=AirPlay Flow Win Test Driver'
$certificate = Get-ChildItem -Path Cert:\CurrentUser\My |
    Where-Object {
        $_.Subject -eq $subject -and
        $_.HasPrivateKey -and
        $_.NotAfter -gt (Get-Date).AddDays(30)
    } |
    Sort-Object NotBefore -Descending |
    Select-Object -First 1

if (-not $certificate) {
    $certificate = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $subject `
        -FriendlyName 'AirPlay Flow Win test driver signing certificate' `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -HashAlgorithm SHA256 `
        -KeyAlgorithm RSA `
        -KeyLength 2048 `
        -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddYears(2)
}

$publicCertificatePath = Join-Path $resolvedPackagePath 'AirPlayFlowVad-Test.cer'
Export-Certificate -Cert $certificate -FilePath $publicCertificatePath -Force | Out-Null

# HVCI requires the kernel binary itself to carry a signature. Sign the SYS first,
# regenerate the catalog so it contains the signed binary's hash, then sign the CAT.
& $signtool sign /v /fd SHA256 /s My /sha1 $certificate.Thumbprint $systemPath
if ($LASTEXITCODE -ne 0) {
    throw "Signing AirPlayFlowVad.sys failed with exit code $LASTEXITCODE."
}

& $inf2Cat "/driver:$resolvedPackagePath" /os:10_X64 /uselocaltime
if ($LASTEXITCODE -ne 0) {
    throw "Inf2Cat failed with exit code $LASTEXITCODE."
}

if (-not (Test-Path -LiteralPath $catalogPath)) {
    throw "Inf2Cat did not produce the expected catalog: $catalogPath"
}

& $signtool sign /v /fd SHA256 /s My /sha1 $certificate.Thumbprint $catalogPath
if ($LASTEXITCODE -ne 0) {
    throw "Signing AirPlayFlowVad.cat failed with exit code $LASTEXITCODE."
}

$systemSignature = Get-AuthenticodeSignature -LiteralPath $systemPath
$catalogSignature = Get-AuthenticodeSignature -LiteralPath $catalogPath

[pscustomobject]@{
    PackagePath = $resolvedPackagePath
    CertificateSubject = $certificate.Subject
    CertificateThumbprint = $certificate.Thumbprint
    CertificateNotAfter = $certificate.NotAfter
    PublicCertificatePath = $publicCertificatePath
    SystemSignatureStatus = [string]$systemSignature.Status
    CatalogSignatureStatus = [string]$catalogSignature.Status
    Note = 'The certificate is not trusted yet. Administrator installation imports the public certificate into LocalMachine Root and TrustedPublisher.'
}
