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
    $ReportPath = Join-Path $driverRoot 'build\install-test-driver.json'
}
if ([string]::IsNullOrWhiteSpace($ErrorReportPath)) {
    $ErrorReportPath = Join-Path $driverRoot 'build\install-test-driver-error.txt'
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
    throw 'Secure Boot is enabled. The local test-signed driver cannot be loaded.'
}

$bcdText = (& "$env:SystemRoot\System32\bcdedit.exe" /enum 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "BCDEdit could not read the active boot configuration: $bcdText"
}
if ($bcdText -notmatch '(?im)^\s*testsigning\s+(Yes|On|1)\s*$') {
    throw 'Windows test-signing is not enabled in the active boot configuration. Restart after running Enable-TestDriverMode.cmd.'
}

$resolvedPackagePath = (Resolve-Path -LiteralPath $PackagePath).Path
$infPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.inf'
$systemPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.sys'
$catalogPath = Join-Path $resolvedPackagePath 'AirPlayFlowVad.cat'
$expectedThumbprint = '8F33FA6504DB3F516C2C320A72BE0F084E5EACC3'

foreach ($requiredFile in @($infPath, $systemPath, $catalogPath)) {
    if (-not (Test-Path -LiteralPath $requiredFile)) {
        throw "Required driver package file is missing: $requiredFile"
    }
}

foreach ($signedFile in @($systemPath, $catalogPath)) {
    $signature = Get-AuthenticodeSignature -LiteralPath $signedFile
    if (
        [string]$signature.Status -ne 'Valid' -or
        $signature.SignerCertificate.Thumbprint -ne $expectedThumbprint
    ) {
        throw "Driver signature validation failed for $signedFile. Status: $($signature.Status)."
    }
}

$rootCertificate = Get-ChildItem Cert:\LocalMachine\Root |
    Where-Object Thumbprint -eq $expectedThumbprint |
    Select-Object -First 1
$publisherCertificate = Get-ChildItem Cert:\LocalMachine\TrustedPublisher |
    Where-Object Thumbprint -eq $expectedThumbprint |
    Select-Object -First 1
if (-not $rootCertificate -or -not $publisherCertificate) {
    throw 'The AirPlay Flow Win test certificate is not trusted by LocalMachine Root and TrustedPublisher.'
}

$devcon = Join-Path $env:USERPROFILE '.nuget\packages\microsoft.windows.wdk.x64\10.0.26100.6584\c\tools\10.0.26100.0\x64\devcon.exe'
if (-not (Test-Path -LiteralPath $devcon)) {
    throw "The x64 DevCon tool is missing: $devcon"
}

$existingOutput = (& $devcon find 'Root\AirPlayFlowVad' 2>&1 | Out-String).Trim()
$deviceExists = $LASTEXITCODE -eq 0 -and $existingOutput -match '(?im)^ROOT\\'
$installAction = if ($deviceExists) { 'update' } else { 'install' }
$installOutput = if ($deviceExists) {
    (& $devcon update $infPath 'Root\AirPlayFlowVad' 2>&1 | Out-String).Trim()
} else {
    (& $devcon install $infPath 'Root\AirPlayFlowVad' 2>&1 | Out-String).Trim()
}
$installExitCode = $LASTEXITCODE
if ($installExitCode -notin @(0, 1)) {
    throw "DevCon failed with exit code $installExitCode`: $installOutput"
}

Start-Sleep -Seconds 3
$deviceOutput = (& $devcon status 'Root\AirPlayFlowVad' 2>&1 | Out-String).Trim()
$audioEndpoints = @(
    Get-PnpDevice -Class AudioEndpoint -PresentOnly -ErrorAction SilentlyContinue |
        Where-Object FriendlyName -like '*AirPlay Flow Win*' |
        Select-Object Status, Class, FriendlyName, InstanceId
)

$report = [ordered]@{
    Timestamp = (Get-Date).ToString('o')
    PackagePath = $resolvedPackagePath
    HardwareId = 'Root\AirPlayFlowVad'
    CertificateThumbprint = $expectedThumbprint
    DevConAction = $installAction
    DevConExitCode = $installExitCode
    DevConInstall = $installOutput
    DevConStatus = $deviceOutput
    AudioEndpoints = $audioEndpoints
    RestartRequired = ($installExitCode -eq 1)
}

$reportDirectory = Split-Path -Parent $ReportPath
if ($reportDirectory) {
    New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
}
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
$report
