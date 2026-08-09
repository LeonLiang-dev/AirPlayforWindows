@echo off
setlocal

fltmc.exe >nul 2>&1
if errorlevel 1 (
    powershell.exe -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

title AirPlay Flow Win - Enable Test Driver Mode
powershell.exe -NoProfile -File "%~dp0sysvad\Enable-TestDriverMode.ps1"
if errorlevel 1 (
    echo.
    echo Failed to enable test driver mode. See:
    echo %~dp0build\enable-test-driver-mode-error.txt
    echo.
    pause
    exit /b 1
)

echo.
echo Test driver mode was configured successfully.
echo Return to Codex so the result can be verified before restarting.
echo.
pause
