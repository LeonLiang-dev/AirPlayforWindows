[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release'
)

$ErrorActionPreference = 'Stop'

function Find-MSBuild {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $installationPath = & $vswhere -latest -products '*' `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -property installationPath
        if ($installationPath) {
            $amd64 = Join-Path $installationPath 'MSBuild\Current\Bin\amd64\MSBuild.exe'
            if (Test-Path -LiteralPath $amd64) {
                return $amd64
            }
            $fallback = Join-Path $installationPath 'MSBuild\Current\Bin\MSBuild.exe'
            if (Test-Path -LiteralPath $fallback) {
                return $fallback
            }
        }
    }

    $fromPath = Get-Command 'MSBuild.exe' -ErrorAction SilentlyContinue
    if ($fromPath) {
        return $fromPath.Source
    }

    throw 'MSBuild with the Visual C++ toolchain was not found.'
}

$globalWdk = Join-Path $env:USERPROFILE '.nuget\packages\microsoft.windows.wdk.x64\10.0.26100.6584\build\native\Microsoft.Windows.WDK.x64.props'
$localWdk = Join-Path $PSScriptRoot 'packages\Microsoft.Windows.WDK.x64.10.0.26100.6584\build\native\Microsoft.Windows.WDK.x64.props'
if (-not (Test-Path -LiteralPath $globalWdk) -and -not (Test-Path -LiteralPath $localWdk)) {
    & (Join-Path $PSScriptRoot 'Restore-Wdk.ps1')
}

$wdkPackageRoot = if (Test-Path -LiteralPath $localWdk) {
    Split-Path (Split-Path (Split-Path $localWdk -Parent) -Parent) -Parent
} else {
    Split-Path (Split-Path (Split-Path $globalWdk -Parent) -Parent) -Parent
}

$msbuild = Find-MSBuild
$commonProject = Join-Path $PSScriptRoot 'EndpointsCommon\EndpointsCommon.vcxproj'
$driverProject = Join-Path $PSScriptRoot 'AirPlayFlowVad\AirPlayFlowVad.vcxproj'
$commonArguments = @(
    '/m',
    '/t:Rebuild',
    "/p:Configuration=$Configuration",
    '/p:Platform=x64',
    '/p:EnableTestSign=false',
    '/p:SignOutput=false',
    '/p:SkipPackageVerification=false',
    '/p:ApiValidator_Enable=true',
    '/v:minimal'
)

& $msbuild $commonProject @commonArguments
if ($LASTEXITCODE -ne 0) {
    throw "EndpointsCommon build failed with exit code $LASTEXITCODE."
}

& $msbuild $driverProject @commonArguments
if ($LASTEXITCODE -ne 0) {
    throw "AirPlayFlowVad build failed with exit code $LASTEXITCODE."
}

$driverOutput = Join-Path $PSScriptRoot "AirPlayFlowVad\x64\$Configuration"
$packageOutput = Join-Path (Split-Path $PSScriptRoot -Parent) "build\x64\$Configuration"
New-Item -ItemType Directory -Path $packageOutput -Force | Out-Null

foreach ($fileName in @('AirPlayFlowVad.sys', 'AirPlayFlowVad.inf', 'AirPlayFlowVad.pdb')) {
    $source = Join-Path $driverOutput $fileName
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Expected build output is missing: $source"
    }
    Copy-Item -LiteralPath $source -Destination (Join-Path $packageOutput $fileName) -Force
}

$inf2CatDirectory = Join-Path $wdkPackageRoot 'c\bin\10.0.26100.0\x86'
$catalogArguments = @(
    '/t:Inf2Cat',
    "/p:Configuration=$Configuration",
    '/p:Platform=x64',
    "/p:Inf2CatSource=$packageOutput",
    '/p:EnableInf2cat=true',
    "/p:Inf2CatToolPath=$inf2CatDirectory\",
    '/p:Inf2CatToolExe=Inf2Cat.exe',
    '/p:Inf2CatWindowsVersionList=10_X64',
    '/p:Inf2CatUseLocalTime=true',
    '/v:minimal'
)
& $msbuild $driverProject @catalogArguments
if ($LASTEXITCODE -ne 0) {
    throw "Driver catalog generation failed with exit code $LASTEXITCODE."
}

$catalogPath = Join-Path $packageOutput 'AirPlayFlowVad.cat'
if (-not (Test-Path -LiteralPath $catalogPath)) {
    throw "Expected catalog output is missing: $catalogPath"
}

Write-Output "Validated unsigned driver package staged at: $packageOutput"
Write-Output 'No driver was installed and no Windows boot or certificate settings were changed.'
