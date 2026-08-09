[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$toolDirectory = Join-Path $PSScriptRoot '.tools'
$nugetPath = Join-Path $toolDirectory 'nuget.exe'
$packageDirectory = Join-Path $PSScriptRoot 'packages'

if (-not (Test-Path -LiteralPath $nugetPath)) {
    New-Item -ItemType Directory -Path $toolDirectory -Force | Out-Null
    Invoke-WebRequest `
        -Uri 'https://dist.nuget.org/win-x86-commandline/latest/nuget.exe' `
        -OutFile $nugetPath
}

& $nugetPath restore (Join-Path $PSScriptRoot 'packages.config') `
    -PackagesDirectory $packageDirectory `
    -NonInteractive

if ($LASTEXITCODE -ne 0) {
    throw "NuGet restore failed with exit code $LASTEXITCODE."
}
