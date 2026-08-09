# AirPlay Flow Win

Windows 11 桌面端 AirPlay 音频发送器，使用 Tauri 2、Rust、React 和 TypeScript 构建。

## 当前进度

目前已经具备：

- `_raop._tcp.local.` / `_airplay._tcp.local.` mDNS 设备发现、移除与重新扫描
- AirPlay 与 RAOP 记录合并、设备能力和连接状态展示
- 基础 RTSP 会话流程：`OPTIONS -> ANNOUNCE -> SETUP -> RECORD`
- WASAPI loopback 系统音频采集，自动跟随 Windows 默认输出设备
- 单一 `AirPlay Flow Win` 虚拟播放端点驱动源码、WDK 构建与 INF/API 验证
- 虚拟端点识别与“双重播放”状态提示
- 44.1 kHz 双声道 PCM 转换、ALAC 编码与 RTP 分发
- 多设备连接、播放、暂停、停止和音量控制 UI
- 前后端状态事件同步

仍未完成：

- 虚拟播放设备的正式签名、安装器集成与升级/卸载流程
- RSA/AES 加密、FairPlay、PIN 配对和凭据持久化
- RTP 重传、NTP timing、延迟校准和严格的多房间同步
- 在 HomePod、Apple TV 和第三方音箱上的硬件兼容性验证

因此当前版本适合继续开发和调试，还不是可日常使用的 AirPlay 2 产品。

## 本地开发

需要 Node.js、Rust stable 和 Windows WebView2。

```powershell
npm install
npm run tauri dev
```

质量检查：

```powershell
npm run build
npm run lint
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## 目录

- `src/`：React 界面、状态与 Tauri 事件订阅
- `src-tauri/src/airplay/`：mDNS、RTSP、SDP、RTP 与会话状态机
- `src-tauri/src/audio/`：采集、ALAC 编码和音频 pipeline
- `src-tauri/src/commands/`：前端可调用的 Tauri 命令
- `src-tauri/src/sync/`：多房间同步（当前为后续阶段骨架）
- `driver/`：基于 Microsoft SysVAD 的单端点虚拟音频驱动与独立构建脚本

驱动构建方式和签名边界见 [`driver/README.md`](driver/README.md)。

## 协议实现说明

当前 SDP 与 RTP 路径按未加密 RAOP MVP 保持一致。不要在仅添加 `rsaaeskey` 字段后就宣称支持加密：AES key 必须经过 AirPlay RSA 公钥封装，音频 payload 也必须使用对应的 AES 模式加密。配对和 AirPlay 2 原生流需要单独实现。
