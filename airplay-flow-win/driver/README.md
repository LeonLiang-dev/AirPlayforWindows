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

## Installer integration

The default `Build-Installer.ps1` build does **not** ship this driver. The app streams from whatever virtual render endpoint Windows already has — a signed third-party device such as Steam Streaming Speakers or VB-CABLE — so no test-signing, Secure Boot change, or driver signature is involved. See "Streaming without this driver" below.

To bundle the driver, build the installer with `-IncludeDriver`:

```powershell
.\Build-Installer.ps1 -IncludeDriver
.\Build-Installer.ps1 -IncludeDriver -DriverPackagePath "C:\path\to\signed-driver"
```

That build embeds the staged driver package and the files under `driver/installer` into the Tauri NSIS installer. The elevated installer uses the Windows inbox `PnPUtil` plus a small SetupAPI helper to create or update `Root\AirPlayFlowVad`; no redistributable DevCon or DevGen binary is required. Application uninstall removes the device node and its OEM INF package.

The package still needs an appropriate catalog and Windows-compatible driver signature. The repository's current release package is test-signed for development and requires Secure Boot to be disabled. Production distribution requires a Microsoft Attestation or WHQL signed package. Supply that package to `Build-Installer.ps1 -IncludeDriver -DriverPackagePath <directory>` to build the same one-step installer for Secure Boot systems.

## Streaming without this driver

WASAPI loopback can capture any render endpoint, including a virtual one that other vendors already signed. The app treats these endpoint names as virtual outputs:

- `AirPlay Flow Win` (this driver)
- `Steam Streaming Speakers` (installed with Steam)
- `VB-Audio` / `CABLE Input` (VB-CABLE)
- `VoiceMeeter`
- `Virtual Audio Cable`
- endpoints whose localized name contains `虚拟音频设备` or `虚拟声卡`

Everything works with the endpoint selected as the Windows default output: the app streams it and nothing is played on the physical speakers. The trade-off is that the user needs one of those devices installed, and the endpoint cannot carry this project's branding.
