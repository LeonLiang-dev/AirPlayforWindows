!macro NSIS_HOOK_PREINSTALL
  ; Releases before 0.1.1 were installed per-user. Remove that copy before the
  ; elevated per-machine install so Windows does not retain two app entries.
  IfFileExists "$LOCALAPPDATA\AirPlay Flow Win\uninstall.exe" 0 migration_done
  DetailPrint "Removing the previous per-user AirPlay Flow Win installation..."
  nsExec::ExecToStack /TIMEOUT=120000 '"$LOCALAPPDATA\AirPlay Flow Win\uninstall.exe" /S'
  Pop $0
  Pop $1
  DetailPrint "$1"

  ${If} $0 != "0"
    MessageBox MB_ICONSTOP|MB_OK "The previous AirPlay Flow Win installation could not be removed.$\r$\n$\r$\nClose the application and run this installer again.$\r$\n$\r$\n$1"
    SetErrorLevel $0
    Abort
  ${EndIf}

  migration_done:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Only the -IncludeDriver build carries the optional virtual audio package.
  ; The default build streams from a virtual output that Windows already ships
  ; with a signed driver (Steam Streaming Speakers, VB-CABLE, ...), so it never
  ; touches test-signing or Secure Boot.
  IfFileExists "$INSTDIR\driver\Install-Driver.ps1" 0 airplay_driver_not_bundled
  DetailPrint "Installing AirPlay Flow Win virtual audio driver..."
  ; NSIS runs as a 32-bit process. Sysnative starts native 64-bit PowerShell
  ; instead of the redirected SysWOW64 copy, where bcdedit is unavailable.
  nsExec::ExecToStack /TIMEOUT=120000 '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\driver\Install-Driver.ps1" -PackagePath "$INSTDIR\driver"'
  Pop $0
  Pop $1
  DetailPrint "$1"

  ${If} $0 == "3010"
    DetailPrint "The virtual audio driver requires a Windows restart."
    SetRebootFlag true
  ${ElseIf} $0 != "0"
    MessageBox MB_ICONSTOP|MB_OK "The virtual audio driver could not be installed.$\r$\n$\r$\n$1$\r$\n$\r$\nSee the log in C:\ProgramData\AirPlay Flow Win\Installer."
    SetErrorLevel $0
    Abort
  ${EndIf}

  airplay_driver_not_bundled:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$INSTDIR\driver\Uninstall-Driver.ps1" 0 airplay_driver_absent
  DetailPrint "Removing AirPlay Flow Win virtual audio driver..."
  nsExec::ExecToStack /TIMEOUT=120000 '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\driver\Uninstall-Driver.ps1"'
  Pop $0
  Pop $1
  DetailPrint "$1"

  ${If} $0 == "3010"
    SetRebootFlag true
  ${ElseIf} $0 != "0"
    MessageBox MB_ICONEXCLAMATION|MB_OK "The application will be removed, but Windows could not completely remove the virtual audio driver.$\r$\n$\r$\nSee C:\ProgramData\AirPlay Flow Win\Installer for details."
  ${EndIf}

  airplay_driver_absent:
!macroend
