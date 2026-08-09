@echo off
setlocal

fltmc.exe >nul 2>&1
if errorlevel 1 (
    powershell.exe -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

title AirPlay Flow Win - Install Virtual Audio Driver
powershell.exe -NoProfile -File "%~dp0sysvad\Install-TestDriver.ps1"
if errorlevel 1 (
    echo.
    echo Driver installation failed. See:
    echo %~dp0build\install-test-driver-error.txt
    echo.
    pause
    exit /b 1
)

echo.
echo AirPlay Flow Win virtual audio driver installation completed.
echo Return to Codex so the endpoint can be verified.
echo.
pause
