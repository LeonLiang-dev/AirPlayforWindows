[CmdletBinding()]
param(
    [ValidateRange(15, 600)]
    [int]$RestartDelaySeconds = 60
)

$ErrorActionPreference = 'Stop'
$logDirectory = Join-Path $env:ProgramData 'AirPlay Flow Win\Installer'
$logPath = Join-Path $logDirectory 'disable-flight-signing.log'
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null

try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'This script must run as administrator.'
    }

    $bcdEdit = Join-Path $env:SystemRoot 'System32\bcdedit.exe'
    $currentOutput = (& $bcdEdit /set flightsigning off 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to disable flight-signing for the current boot entry: $currentOutput"
    }

    $bootManagerOutput = (& $bcdEdit /set '{bootmgr}' flightsigning off 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to disable flight-signing for Windows Boot Manager: $bootManagerOutput"
    }

    @(
        "[$(Get-Date -Format o)] Windows flight-signing was disabled."
        "Current boot entry: $currentOutput"
        "Windows Boot Manager: $bootManagerOutput"
        "A restart was scheduled in $RestartDelaySeconds seconds."
    ) | Set-Content -LiteralPath $logPath -Encoding UTF8

    $shutdown = Join-Path $env:SystemRoot 'System32\shutdown.exe'
    & $shutdown /r /t $RestartDelaySeconds /d 'p:0:0' /c 'Windows flight-signing was disabled for PUBG and BattlEye compatibility.'
    if ($LASTEXITCODE -ne 0) {
        throw "Windows restart scheduling failed with exit code $LASTEXITCODE."
    }
} catch {
    "[$(Get-Date -Format o)] ERROR: $($_.Exception.Message)" |
        Set-Content -LiteralPath $logPath -Encoding UTF8
    throw
}
