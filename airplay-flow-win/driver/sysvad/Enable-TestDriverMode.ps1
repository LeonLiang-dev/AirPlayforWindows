[CmdletBinding()]
param(
    [string]$PackagePath,
    [string]$ReportPath,
    [string]$ErrorReportPath
)

$ErrorActionPreference = 'Stop'

$driverRoot = Split-Path $PSScriptRoot -Parent
if ([string]::IsNullOrWhiteSpace($PackagePath)) {
    $PackagePath = Join-Path $driverRoot 'build\x64\Release'
}
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Join-Path $driverRoot 'build\enable-test-driver-mode.json'
}
if ([string]::IsNullOrWhiteSpace($ErrorReportPath)) {
    $ErrorReportPath = Join-Path $driverRoot 'build\enable-test-driver-mode-error.txt'
}

$errorReportDirectory = Split-Path -Parent $ErrorReportPath
if ($errorReportDirectory) {
    New-Item -ItemType Directory -Path $errorReportDirectory -Force | Out-Null
}
Remove-Item -LiteralPath $ErrorReportPath -Force -ErrorAction SilentlyContinue
trap {
    $_ | Format-List * -Force | Out-File -LiteralPath $ErrorReportPath -Encoding utf8 -Force
    exit 1
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'This script must run from an elevated PowerShell process.'
}

if (Confirm-SecureBootUEFI) {
    throw 'Secure Boot is enabled. Disable it in UEFI before enabling Windows test-signing.'
}

$bitLocker = Get-BitLockerVolume -MountPoint $env:SystemDrive
if (
    [string]$bitLocker.VolumeStatus -ne 'FullyDecrypted' -or
    [string]$bitLocker.ProtectionStatus -ne 'Off'
) {
    throw "The system drive is not fully decrypted with BitLocker protection off. Current state: $($bitLocker.VolumeStatus), $($bitLocker.ProtectionStatus)."
}

$resolvedPackagePath = (Resolve-Path -LiteralPath $PackagePath).Path
$certificatePath = Join-Path $resolvedPackagePath 'AirPlayFlowVad-Test.cer'
if (-not (Test-Path -LiteralPath $certificatePath)) {
    throw "The driver test certificate is missing: $certificatePath"
}

$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($certificatePath)
if ($certificate.Subject -ne 'CN=AirPlay Flow Win Test Driver') {
    throw "Unexpected certificate subject: $($certificate.Subject)"
}

$rootImport = Import-Certificate -FilePath $certificatePath -CertStoreLocation 'Cert:\LocalMachine\Root'
$publisherImport = Import-Certificate -FilePath $certificatePath -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher'

$bcdOutput = (& "$env:SystemRoot\System32\bcdedit.exe" /set testsigning on 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "BCDEdit failed to enable test-signing: $bcdOutput"
}

$report = [ordered]@{
    Timestamp = (Get-Date).ToString('o')
    SecureBootEnabled = $false
    BitLockerVolumeStatus = [string]$bitLocker.VolumeStatus
    BitLockerProtectionStatus = [string]$bitLocker.ProtectionStatus
    CertificateSubject = $certificate.Subject
    CertificateThumbprint = $certificate.Thumbprint
    RootStoreThumbprint = $rootImport.Thumbprint
    TrustedPublisherThumbprint = $publisherImport.Thumbprint
    TestSigningConfigured = $true
    BcdEdit = $bcdOutput
    RestartRequired = $true
}

$reportDirectory = Split-Path -Parent $ReportPath
if ($reportDirectory) {
    New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
}
$report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
$report
