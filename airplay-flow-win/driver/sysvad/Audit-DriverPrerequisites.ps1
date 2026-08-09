param(
    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "This audit must run from an elevated PowerShell process."
}

try {
    $secureBoot = Confirm-SecureBootUEFI
    $secureBootError = $null
}
catch {
    $secureBoot = $null
    $secureBootError = $_.Exception.Message
}

$deviceGuard = Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard -ErrorAction SilentlyContinue
$bitLocker = Get-BitLockerVolume -MountPoint $env:SystemDrive -ErrorAction SilentlyContinue
$bcdText = (& "$env:SystemRoot\System32\bcdedit.exe" /enum 2>&1 | Out-String).Trim()
$manageBdeText = (& "$env:SystemRoot\System32\manage-bde.exe" -status $env:SystemDrive 2>&1 | Out-String).Trim()

$result = [ordered]@{
    Timestamp = (Get-Date).ToString("o")
    IsAdministrator = $true
    SecureBootEnabled = $secureBoot
    SecureBootError = $secureBootError
    DeviceGuard = if ($deviceGuard) {
        [ordered]@{
            VirtualizationBasedSecurityStatus = $deviceGuard.VirtualizationBasedSecurityStatus
            SecurityServicesConfigured = @($deviceGuard.SecurityServicesConfigured)
            SecurityServicesRunning = @($deviceGuard.SecurityServicesRunning)
            CodeIntegrityPolicyEnforcementStatus = $deviceGuard.CodeIntegrityPolicyEnforcementStatus
            UsermodeCodeIntegrityPolicyEnforcementStatus = $deviceGuard.UsermodeCodeIntegrityPolicyEnforcementStatus
        }
    } else {
        $null
    }
    BitLocker = if ($bitLocker) {
        [ordered]@{
            MountPoint = $bitLocker.MountPoint
            VolumeStatus = [string]$bitLocker.VolumeStatus
            ProtectionStatus = [string]$bitLocker.ProtectionStatus
            LockStatus = [string]$bitLocker.LockStatus
            EncryptionMethod = [string]$bitLocker.EncryptionMethod
            EncryptionPercentage = $bitLocker.EncryptionPercentage
            AutoUnlockEnabled = $bitLocker.AutoUnlockEnabled
        }
    } else {
        $null
    }
    BcdEdit = $bcdText
    ManageBde = $manageBdeText
}

$outputDirectory = Split-Path -Parent $OutputPath
if ($outputDirectory) {
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
}

$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
