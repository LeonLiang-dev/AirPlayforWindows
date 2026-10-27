[CmdletBinding()]
param(
    [string]$PackagePath
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($PackagePath)) {
    $PackagePath = $PSScriptRoot
}
$hardwareId = 'Root\AirPlayFlowVad'
$logDirectory = Join-Path $env:ProgramData 'AirPlay Flow Win\Installer'
$logPath = Join-Path $logDirectory 'driver-install.log'
$reportPath = Join-Path $logDirectory 'driver-install.json'
$restartRequired = $false

New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
Set-Content -LiteralPath $logPath -Value "AirPlay Flow Win driver installation`r`n" -Encoding UTF8

function Write-InstallLog {
    param([string]$Message)
    $line = "[$(Get-Date -Format o)] $Message"
    Add-Content -LiteralPath $logPath -Value $line -Encoding UTF8
    Write-Output $line
}

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'The AirPlay Flow Win installer must run as administrator.'
    }
}

function Get-NativeSystemTool {
    param([Parameter(Mandatory)][string]$Name)

    $systemDirectory = Join-Path $env:SystemRoot 'System32'
    if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
        # A 32-bit NSIS/PowerShell process is redirected from System32 to
        # SysWOW64. Sysnative is the supported alias for the native tools.
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
    Write-InstallLog "pnputil $($Arguments -join ' ') => $exitCode`r`n$output"
    if ($exitCode -eq 3010) {
        $script:restartRequired = $true
    } elseif ($exitCode -eq 259 -and $Arguments.Count -gt 0 -and $Arguments[0] -eq '/add-driver') {
        # PnPUtil can return ERROR_NO_MORE_ITEMS when the package is already
        # present and every matching device already has the newest driver.
        # The device/status checks below remain the source of truth.
        Write-InstallLog 'PnPUtil reported that the existing driver package and device are already current.'
    } elseif ($exitCode -ne 0) {
        throw "PnPUtil failed with exit code $exitCode`: $output"
    }
}

function Get-AirPlayFlowDevices {
    $devices = @(Get-PnpDevice -Class Media -ErrorAction SilentlyContinue)
    foreach ($device in $devices) {
        try {
            $property = Get-PnpDeviceProperty `
                -InstanceId $device.InstanceId `
                -KeyName 'DEVPKEY_Device_HardwareIds' `
                -ErrorAction Stop
            if (@($property.Data) -contains $hardwareId) {
                $device
            }
        } catch {
            continue
        }
    }
}

function Get-MalformedAirPlayFlowDevices {
    $devices = @(Get-PnpDevice -Class Media -PresentOnly:$false -ErrorAction SilentlyContinue)
    foreach ($device in $devices) {
        try {
            $property = Get-PnpDeviceProperty `
                -InstanceId $device.InstanceId `
                -KeyName 'DEVPKEY_Device_HardwareIds' `
                -ErrorAction Stop
            $ids = @($property.Data)
            # Installer 0.1.3 called the ANSI SetupAPI entry point with an
            # UTF-16 buffer, producing one REG_MULTI_SZ element per character.
            if (-not ($ids -contains $hardwareId) -and ($ids -join '') -eq $hardwareId) {
                $device
            }
        } catch {
            continue
        }
    }
}

try {
    Assert-Administrator
    $pnpUtilPath = Get-NativeSystemTool -Name 'pnputil.exe'
    $bcdEditPath = Get-NativeSystemTool -Name 'bcdedit.exe'
    $resolvedPackage = (Resolve-Path -LiteralPath $PackagePath).Path
    $infPath = Join-Path $resolvedPackage 'AirPlayFlowVad.inf'
    $sysPath = Join-Path $resolvedPackage 'AirPlayFlowVad.sys'
    $catPath = Join-Path $resolvedPackage 'airplayflowvad.cat'
    $deviceSetupPath = Join-Path $resolvedPackage 'DeviceSetup.cs'
    $testCertificatePath = Join-Path $resolvedPackage 'AirPlayFlowVad-Test.cer'

    foreach ($requiredFile in @($infPath, $sysPath, $catPath, $deviceSetupPath)) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            throw "The installer resource is missing: $requiredFile"
        }
    }

    $packageMode = 'MicrosoftSigned'
    if (Test-Path -LiteralPath $testCertificatePath) {
        $packageMode = 'TestSigned'
        $secureBoot = try { Confirm-SecureBootUEFI } catch { $false }
        if ($secureBoot) {
            throw 'This build contains a test-signed driver. Disable Secure Boot or replace the package with the Microsoft-signed driver.'
        }

        $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
            $testCertificatePath
        )
        if ($certificate.Subject -ne 'CN=AirPlay Flow Win Test Driver') {
            throw "Unexpected test certificate: $($certificate.Subject)"
        }
        if (-not (Get-ChildItem Cert:\LocalMachine\Root | Where-Object Thumbprint -eq $certificate.Thumbprint)) {
            Import-Certificate `
                -FilePath $testCertificatePath `
                -CertStoreLocation 'Cert:\LocalMachine\Root' | Out-Null
        }
        if (-not (Get-ChildItem Cert:\LocalMachine\TrustedPublisher | Where-Object Thumbprint -eq $certificate.Thumbprint)) {
            Import-Certificate `
                -FilePath $testCertificatePath `
                -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher' | Out-Null
        }

        $bcd = (& $bcdEditPath /enum '{current}' 2>&1 | Out-String)
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to inspect Windows test-signing state: $bcd"
        }
        if ($bcd -notmatch '(?im)^\s*testsigning\s+(Yes|On|1)\s*$') {
            $bcdResult = (& $bcdEditPath /set testsigning on 2>&1 | Out-String).Trim()
            if ($LASTEXITCODE -ne 0) {
                throw "Unable to enable Windows test-signing: $bcdResult"
            }
            $restartRequired = $true
            Write-InstallLog 'Windows test-signing was enabled; a restart is required.'
        }
    }

    foreach ($signedPath in @($sysPath, $catPath)) {
        $signature = Get-AuthenticodeSignature -LiteralPath $signedPath
        if ([string]$signature.Status -ne 'Valid') {
            throw "Driver signature verification failed for $signedPath`: $($signature.Status)"
        }
    }

    Write-InstallLog "Installing $packageMode driver from $resolvedPackage"
    Invoke-PnpUtil @('/add-driver', $infPath, '/install')

    $devices = @(Get-AirPlayFlowDevices)
    if ($devices.Count -eq 0) {
        foreach ($malformedDevice in @(Get-MalformedAirPlayFlowDevices)) {
            Write-InstallLog "Removing malformed legacy device $($malformedDevice.InstanceId)"
            Invoke-PnpUtil @('/remove-device', $malformedDevice.InstanceId)
        }

        Write-InstallLog "Creating root-enumerated device $hardwareId"
        Add-Type -Path $deviceSetupPath
        [AirPlayFlowWin.Installer.DeviceSetup]::CreateRootDevice($infPath, $hardwareId)
        Invoke-PnpUtil @('/add-driver', $infPath, '/install')
        Start-Sleep -Seconds 2
        $devices = @(Get-AirPlayFlowDevices)
    }

    if ($devices.Count -eq 0) {
        throw "Windows did not create the $hardwareId device."
    }

    $deviceReport = @($devices | Select-Object Status, Class, FriendlyName, InstanceId)
    if (-not $restartRequired -and @($devices | Where-Object Status -eq 'OK').Count -eq 0) {
        throw 'The virtual audio device was created but did not start successfully.'
    }

    $report = [ordered]@{
        Timestamp = (Get-Date).ToString('o')
        PackageMode = $packageMode
        PackagePath = $resolvedPackage
        HardwareId = $hardwareId
        Devices = $deviceReport
        RestartRequired = $restartRequired
        LogPath = $logPath
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    Write-InstallLog "Driver installation completed. RestartRequired=$restartRequired"
    if ($restartRequired) {
        exit 3010
    }
    exit 0
} catch {
    Write-InstallLog "ERROR: $($_.Exception.Message)"
    $_ | Format-List * -Force | Out-File -LiteralPath $logPath -Append -Encoding UTF8
    exit 1
}
