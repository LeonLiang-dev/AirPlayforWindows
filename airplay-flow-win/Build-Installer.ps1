[CmdletBinding()]
param(
    [string]$DriverPackagePath,
    [switch]$IncludeDriver
)

# Default build: no kernel driver. The app streams from a virtual output that
# Windows already ships with a signed driver, so the installer never enables
# test-signing and never requires Secure Boot to be disabled.
#
# -IncludeDriver: bundle and install the driver package under release\driver
# (or the -DriverPackagePath directory). Only use this for development builds
# with the test-signed package, or for a Microsoft attestation-signed package.

$ErrorActionPreference = 'Stop'
$driverResources = Join-Path $PSScriptRoot 'release\driver'
$tauriArguments = @('build', '--bundles', 'nsis')
$includedDriverPackage = $null

if ($IncludeDriver) {
    if ([string]::IsNullOrWhiteSpace($DriverPackagePath)) {
        $DriverPackagePath = $driverResources
    }
    $resolvedDriverPackage = (Resolve-Path -LiteralPath $DriverPackagePath).Path
    $requiredDriverFiles = @('AirPlayFlowVad.inf', 'AirPlayFlowVad.sys', 'airplayflowvad.cat')
    foreach ($name in $requiredDriverFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $resolvedDriverPackage $name))) {
            throw "Driver package file is missing: $name"
        }
    }

    if ($resolvedDriverPackage -ne (Resolve-Path -LiteralPath $driverResources).Path) {
        foreach ($name in @($requiredDriverFiles + 'AirPlayFlowVad-Test.cer')) {
            $source = Join-Path $resolvedDriverPackage $name
            $destination = Join-Path $driverResources $name
            if (Test-Path -LiteralPath $source) {
                Copy-Item -LiteralPath $source -Destination $destination -Force
            } else {
                Remove-Item -LiteralPath $destination -Force -ErrorAction SilentlyContinue
            }
        }
    }

    $includedDriverPackage = $resolvedDriverPackage
    $tauriArguments += @(
        '--config',
        (Join-Path $PSScriptRoot 'src-tauri\tauri.driver.conf.json')
    )
}

if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'node_modules'))) {
    & npm.cmd ci
    if ($LASTEXITCODE -ne 0) {
        throw "npm ci failed with exit code $LASTEXITCODE"
    }
}

$tauri = Join-Path $PSScriptRoot 'node_modules\.bin\tauri.cmd'
& $tauri @tauriArguments
if ($LASTEXITCODE -ne 0) {
    throw "Tauri NSIS build failed with exit code $LASTEXITCODE"
}

$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'src-tauri\tauri.conf.json') |
    ConvertFrom-Json
$installerName = "AirPlay Flow Win_$($tauriConfig.version)_x64-setup.exe"
$builtInstaller = Join-Path $PSScriptRoot "src-tauri\target\release\bundle\nsis\$installerName"
$releaseInstaller = Join-Path $PSScriptRoot "release\$installerName"
Copy-Item -LiteralPath $builtInstaller -Destination $releaseInstaller -Force

$result = [ordered]@{
    Installer = $releaseInstaller
    Sha256 = (Get-FileHash -LiteralPath $releaseInstaller -Algorithm SHA256).Hash
    IncludesDriver = [bool]$IncludeDriver
    DriverPackage = $includedDriverPackage
    TestSigned = [bool]($IncludeDriver -and (Test-Path -LiteralPath (Join-Path $driverResources 'AirPlayFlowVad-Test.cer')))
}
$result
