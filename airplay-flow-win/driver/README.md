# AirPlay Flow Win virtual audio driver

This directory contains a single Windows render endpoint named **AirPlay Flow Win**. When it is selected as the Windows default output, the desktop app captures that endpoint through WASAPI loopback and sends the PCM stream through the existing AirPlay pipeline. The physical headphones or speakers are no longer part of that route, so they do not play a second copy.

## Build

Requirements:

- Windows 11 x64
- Visual Studio C++ build tools (`v143`)
- PowerShell 5.1 or newer
- Internet access on the first restore

Run:

```powershell
cd .\driver\sysvad
.\Build-Driver.ps1 -Configuration Release
```

The script restores pinned Microsoft WDK/SDK NuGet packages when needed, runs WDK INF and Universal API validation, generates a catalog, and stages an **unsigned** package under `driver\build\x64\Release`. It does not install the driver, trust a certificate, enable Windows test-signing, change Secure Boot, or reboot the computer.

## Design boundary

- One render endpoint: `AirPlay Flow Win`
- No capture/microphone endpoint
- No Bluetooth, USB, HDMI, SPDIF, keyword detector, or APO interface is registered
- The kernel driver only supplies the WaveRT render endpoint; networking and ALAC encoding stay in the Rust process
- WASAPI loopback remains the user-mode transport, avoiding a custom kernel/user shared-memory protocol

## Source provenance

The implementation is derived from Microsoft's SysVAD sample in `microsoft/Windows-driver-samples`, commit `26a27df80772dbcfd69e6449b671d5c29eb5aedc`. The upstream sample license is preserved in `LICENSE-MICROSOFT-SAMPLES.txt`.

Before installation, the package still needs an appropriate catalog and Windows-compatible driver signature. Development test installation may require test-signing and can conflict with Secure Boot; production distribution requires Microsoft driver signing. Those system-level steps are intentionally separate from the build script.
