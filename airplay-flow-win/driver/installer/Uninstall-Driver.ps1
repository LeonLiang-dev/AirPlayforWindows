[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$hardwareId = 'Root\AirPlayFlowVad'
$logDirectory = Join-Path $env:ProgramData 'AirPlay Flow Win\Installer'
$logPath = Join-Path $logDirectory 'driver-uninstall.log'
$restartRequired = $false

New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
Set-Content -LiteralPath $logPath -Value "AirPlay Flow Win driver removal`r`n" -Encoding UTF8

function Write-UninstallLog {
    param([string]$Message)
    $line = "[$(Get-Date -Format o)] $Message"
    Add-Content -LiteralPath $logPath -Value $line -Encoding UTF8
    Write-Output $line
}

function Get-NativeSystemTool {
    param([Parameter(Mandatory)][string]$Name)

    $systemDirectory = Join-Path $env:SystemRoot 'System32'
    if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
        $systemDirectory = Join-Path $env:SystemRoot 'Sysnative'
    }
    $path = Join-Path $systemDirectory $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required Windows system tool was not found: $path"
    }
    $path
}

function Invoke-PnpUtil {
    param([string[]]$Arguments)
    $output = (& $pnpUtilPath @Arguments 2>&1 | Out-String).Trim()
    $exitCode = $LASTEXITCODE
    Write-UninstallLog "pnputil $($Arguments -join ' ') => $exitCode`r`n$output"
    if ($exitCode -eq 3010) {
        $script:restartRequired = $true
    } elseif ($exitCode -ne 0) {
        throw "PnPUtil failed with exit code $exitCode`: $output"
    }
}

try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'The AirPlay Flow Win uninstaller must run as administrator.'
    }
    $pnpUtilPath = Get-NativeSystemTool -Name 'pnputil.exe'

    $matching = @()
    foreach ($device in @(Get-PnpDevice -Class Media -ErrorAction SilentlyContinue)) {
        try {
            $property = Get-PnpDeviceProperty `
                -InstanceId $device.InstanceId `
                -KeyName 'DEVPKEY_Device_HardwareIds' `
                -ErrorAction Stop
            $ids = @($property.Data)
            if (($ids -contains $hardwareId) -or (($ids -join '') -eq $hardwareId)) {
                $driver = Get-CimInstance Win32_PnPSignedDriver | Where-Object DeviceID -eq $device.InstanceId
                $matching += [pscustomobject]@{
                    InstanceId = $device.InstanceId
                    InfName = $driver.InfName
                }
            }
        } catch {
            continue
        }
    }

    foreach ($device in $matching) {
        Invoke-PnpUtil @('/remove-device', $device.InstanceId)
    }
    foreach ($infName in @($matching.InfName | Where-Object { $_ } | Select-Object -Unique)) {
        Invoke-PnpUtil @('/delete-driver', $infName, '/uninstall')
    }

    Write-UninstallLog "Driver removal completed. RestartRequired=$restartRequired"
    if ($restartRequired) {
        exit 3010
    }
    exit 0
} catch {
    Write-UninstallLog "ERROR: $($_.Exception.Message)"
    $_ | Format-List * -Force | Out-File -LiteralPath $logPath -Append -Encoding UTF8
    exit 1
}
