# AirPlay Flow Win

AirPlay Flow Win 是一款面向 Windows 11 的开源桌面音频发送器。它可以发现局域网中的 AirPlay/RAOP 接收器，将 Windows 系统声音采集、编码为 ALAC，并通过 RTP 推送到兼容音箱。

项目使用 **Tauri 2 + Rust + React + TypeScript** 构建，并提供一个基于 Microsoft SysVAD 的虚拟播放设备。安装完成后，用户只需在 Windows 声音面板中选择“扬声器 (AirPlay Flow Win Virtual Audio)”，即可像切换普通声卡一样决定是否把系统声音发送到 AirPlay 音箱。

> [!WARNING]
> 项目仍处于开发阶段，当前版本为 `v0.1.4`，实现的是以未加密 RAOP 为主的音频发送链路，并非完整的 AirPlay 2 协议栈。仓库内安装包附带的是**测试签名驱动**，仅适合开发与兼容性测试，不建议直接安装到日常使用或运行内核反作弊游戏的电脑。

## 功能特性

- 通过 mDNS 自动发现 `_raop._tcp.local.` 和 `_airplay._tcp.local.` 设备
- 合并同一接收器的 AirPlay/RAOP 服务记录并展示设备能力
- 实现基础 RTSP 会话：`OPTIONS → ANNOUNCE → SETUP → RECORD`
- 支持部分接收器需要的 `/auth-setup` 握手
- 使用 WASAPI loopback 采集 Windows 系统播放内容
- 将音频统一转换为 44.1 kHz、16-bit、双声道 PCM
- 实时 ALAC 编码并通过 RTP/UDP 发送到音箱
- 支持同时连接多个接收器并分发同一音频流
- 提供连接、断开、播放、暂停、停止和接收器音量控制
- 跟随 Windows 默认输出设备切换
- Windows 选择虚拟声卡时发送 AirPlay；切回耳机或实体音箱时自动暂停发送
- 将虚拟端点的 Windows 主音量和静音状态应用到发送的 PCM 音频
- 提供应用、虚拟声卡和驱动卸载逻辑一体化的 NSIS 安装包

## 工作原理

```mermaid
flowchart LR
    App[Windows 应用和系统声音] --> VAD[AirPlay Flow Win<br/>虚拟播放设备]
    VAD -->|WASAPI loopback| Capture[Rust 音频采集]
    Capture --> Convert[44.1 kHz / 16-bit / Stereo]
    Convert --> ALAC[ALAC 编码]
    ALAC --> RTP[RTP/UDP 分发]
    RTP --> Speaker[AirPlay / RAOP 音箱]
```

虚拟声卡只负责向 Windows 注册一个 WaveRT 播放端点，网络、协议和编码工作全部位于 Rust 用户态进程中。这样既避免了自定义内核通信协议，也能防止实体耳机与 AirPlay 音箱同时播放同一份声音。

## 快速开始

### 运行要求

- Windows 11 x64
- 电脑和接收器位于同一局域网
- 网络允许 mDNS 和应用的局域网通信
- 当前测试版驱动需要管理员权限、关闭 Secure Boot，并启用 Windows 测试签名模式
- 接收器需要兼容当前实现的未加密 RAOP/ALAC 音频链路

当前安装包位于：

```text
release/AirPlay Flow Win_0.1.4_x64-setup.exe
```

### 安装

1. 运行安装包并同意管理员权限请求。
2. 安装器会同时安装桌面应用和 `AirPlay Flow Win Virtual Audio` 虚拟声卡。
3. 如果安装器首次启用了 Windows 测试签名模式，请在安装完成后重启电脑。
4. 打开 Windows 声音设置，确认输出设备列表中出现“扬声器 (AirPlay Flow Win Virtual Audio)”。

安装器会自动迁移早期 `v0.1.0` 的按用户安装版本，不需要手工运行 DevCon、PnPUtil 或驱动 PowerShell 脚本。

### 使用

1. 启动 AirPlay Flow Win，等待设备扫描完成。
2. 在设备列表中连接目标音箱。
3. 在 Windows 快速设置或“设置 → 系统 → 声音”中，将输出设备切换为“扬声器 (AirPlay Flow Win Virtual Audio)”。
4. 正常播放音乐、视频或其他系统声音。
5. 如需恢复本机播放，在 Windows 中改选耳机、显示器或其他实体输出设备；应用会检测切换并暂停 AirPlay 音频采集。

卸载 AirPlay Flow Win 时，安装器会尝试同步删除虚拟设备节点和对应的 OEM 驱动包。详细日志保存在：

```text
C:\ProgramData\AirPlay Flow Win\Installer
```

## 测试签名与 Secure Boot

当前仓库内的 `release/driver` 是开发测试签名包，因此存在以下限制：

- 启用 Windows 测试签名模式后，Secure Boot 必须保持关闭。
- BattlEye 等内核级反作弊通常拒绝在测试签名模式下运行。
- 仅关闭 `TESTSIGNING` 或 `FLIGHTSIGNING` 并不能把测试驱动变为正式驱动；关闭后，Windows 也可能拒绝加载该虚拟声卡。
- 若要在 Secure Boot 开启的普通用户电脑上实现“一次安装即可使用”，必须将驱动提交微软完成 Attestation 或 WHQL 签名，并用正式签名包重新构建安装器。

因此，不要把当前测试安装包当作正式发行版。正式签名是发布前的必要步骤，而不是可以通过安装脚本绕过的限制。

## 协议实现状态

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| mDNS 设备发现 | 已实现 | 扫描 AirPlay 与 RAOP 服务并合并记录 |
| RTSP 基础会话 | 已实现 | OPTIONS、ANNOUNCE、SETUP、RECORD、TEARDOWN |
| ALAC/RTP 音频 | 已实现 | 44.1 kHz 双声道实时编码与发送 |
| `/auth-setup` | 部分实现 | 用于部分第三方接收器的连接兼容 |
| 音量控制 | 已实现 | 接收器音量与 Windows 虚拟端点增益分别生效 |
| Timing/同步包 | 基础实现 | 已有 timing responder 和周期同步，仍需硬件校准 |
| RTP 丢包重传 | 未完成 | 网络抖动时可能出现卡顿或爆音 |
| RSA/AES 与 FairPlay | 未完成 | 不支持要求完整加密链路的设备 |
| PIN 配对与凭据持久化 | 未完成 | 受保护接收器可能拒绝连接 |
| AirPlay 2 原生协议 | 未完成 | 当前不能视为完整 AirPlay 2 实现 |
| 严格多房间同步 | 未完成 | 多设备可发送，但不能保证采样级同步 |

部分第三方设备会广播与传统 RAOP 不完全一致的能力组合。项目包含相应的兼容处理，但尚未形成完整硬件兼容清单；HomePod、Apple TV 和不同品牌音箱仍需要逐台验证。

## 技术架构

### 桌面界面

- React 19 + TypeScript
- Zustand 状态管理
- Tailwind CSS 4
- Tauri 事件负责前后端状态同步

### Rust 后端

- `mdns-sd`：局域网设备发现
- Tokio：异步 RTSP、RTP 和任务调度
- Windows API：WASAPI loopback、默认端点监听、音量与静音读取
- `alac-encoder`：PCM 到 ALAC 的实时编码

### 虚拟音频驱动

- 基于 Microsoft Windows Driver Samples 的 SysVAD 示例裁剪
- 只注册一个名为 `AirPlay Flow Win` 的播放端点
- 不注册麦克风、蓝牙、USB、HDMI、SPDIF、关键字检测器或 APO
- 硬件 ID：`Root\AirPlayFlowVad`

### 安装器

- Tauri NSIS，按计算机安装
- 通过 Windows 自带 PnPUtil 与 SetupAPI 创建或更新根枚举设备
- 安装应用时部署驱动，卸载应用时清理设备节点和驱动包

## 本地开发

### 环境要求

- Node.js 与 npm
- Rust stable，最低 Rust 版本 `1.77.2`
- Microsoft Edge WebView2 Runtime
- Windows 11 SDK
- 构建驱动时还需要 Visual Studio C++ Build Tools `v143`
- 驱动依赖首次还原时需要网络连接

安装依赖并启动开发模式：

```powershell
npm install
npm run tauri dev
```

只启动前端：

```powershell
npm run dev
```

### 质量检查

```powershell
npm run lint
npm run build

Set-Location .\src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

真实接收器集成测试默认被忽略。可使用 IP 地址或 `IP:端口` 指定测试设备：

```powershell
$env:AIRPLAY_TEST_RECEIVER = "192.168.1.100:7000"
Set-Location .\src-tauri
cargo test connects_to_real_receiver_from_environment -- --ignored --nocapture
cargo test streams_windows_loopback_to_real_receiver -- --ignored --nocapture
```

运行音频流测试前，需要先在 Windows 中选择 AirPlay Flow Win 虚拟输出设备，并确保测试接收器允许当前协议链路连接。

## 构建虚拟声卡

```powershell
Set-Location .\driver\sysvad
.\Build-Driver.ps1 -Configuration Release
```

脚本会还原固定版本的 WDK/SDK NuGet 包，执行 INF 和 Universal API 验证，生成目录文件，并把**未签名**驱动暂存到：

```text
driver/build/x64/Release
```

该脚本不会安装驱动、信任证书、修改 Secure Boot、启用测试签名模式或重启电脑。驱动实现及签名边界详见 [`driver/README.md`](driver/README.md)。

## 构建安装包

使用仓库内已经暂存的驱动包：

```powershell
.\Build-Installer.ps1
```

使用外部的微软正式签名驱动包：

```powershell
.\Build-Installer.ps1 -DriverPackagePath "C:\path\to\signed-driver"
```

外部目录至少需要包含匹配的 `.inf`、`.sys` 和 `.cat` 文件。脚本会先验证驱动包，再构建 Tauri/NSIS 安装器，并将最终产物复制到 `release/`。

## 项目结构

```text
airplay-flow-win/
├─ src/                         React 界面、状态管理与 Tauri 事件
├─ src-tauri/
│  ├─ src/airplay/              mDNS、RTSP、SDP、RTP、加密辅助与会话状态机
│  ├─ src/audio/                WASAPI 采集、格式转换、ALAC 编码与音频管线
│  ├─ src/commands/             前端可调用的 Tauri 命令
│  ├─ src/sync/                 同步逻辑
│  └─ windows/                  NSIS 驱动安装/卸载钩子
├─ driver/
│  ├─ sysvad/                   虚拟音频驱动源码与构建脚本
│  └─ installer/               PnPUtil、SetupAPI 驱动部署脚本
├─ release/                     当前安装包和随包驱动
└─ Build-Installer.ps1          一体化安装包构建入口
```

## 常见问题

### 能否直接从 Windows 声音菜单切换到 AirPlay？

可以。安装虚拟声卡后，在 Windows 输出设备中选择 AirPlay Flow Win 即可开始向已连接音箱发送声音；切到其他输出设备会暂停发送。AirPlay 音箱本身不会作为原生硬件端点出现，Windows 中显示的是本项目提供的虚拟端点。

### 为什么连接成功却没有声音？

先确认 Windows 默认输出是 AirPlay Flow Win 虚拟设备，而不是耳机或显示器；然后确认应用中的接收器状态为 Streaming。若仍无声音，检查防火墙、局域网隔离、接收器是否要求加密或 PIN 配对，以及接收器是否支持当前 RAOP/ALAC 实现。

### 为什么会有明显延迟？

AirPlay/RAOP 接收器会主动缓存音频，项目也需要进行编码和网络调度，因此无法达到有线耳机级别的低延迟。当前延迟与时钟校准仍在完善，不适合实时游戏、语音监听或乐器返听。

### 为什么测试签名模式下游戏无法启动？

这是部分内核反作弊的安全策略，不是应用界面故障。需要关闭测试签名并重启后再运行游戏；但此时测试签名虚拟声卡可能无法加载。长期解决方案是使用微软正式签名驱动。

## 路线图

- 获取微软 Attestation/WHQL 驱动签名并支持 Secure Boot
- 完成 RSA/AES、FairPlay 与 PIN 配对
- 实现 RTP 重传、延迟测量和更完整的时钟同步
- 改善多设备同步、断线重连和弱网稳定性
- 建立 HomePod、Apple TV 与第三方音箱兼容性测试矩阵
- 完善诊断日志、自动更新和正式发行流程

## 来源与许可

Rust 包元数据当前声明为 MIT。虚拟音频驱动衍生自 Microsoft `Windows-driver-samples` 的 SysVAD 示例，固定基线提交为 `26a27df80772dbcfd69e6449b671d5c29eb5aedc`，其上游许可文件保留在 [`driver/LICENSE-MICROSOFT-SAMPLES.txt`](driver/LICENSE-MICROSOFT-SAMPLES.txt)。正式对外分发前，请同时保留并核对所有第三方依赖与驱动源码的许可要求。
