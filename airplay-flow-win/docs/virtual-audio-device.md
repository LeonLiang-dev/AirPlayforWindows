# Windows 虚拟播放设备

## 用户体验

安装驱动后，Windows 11 的声音输出列表会出现 **AirPlay Flow Win**。将它设为默认输出后：

```text
Windows 应用
  → AirPlay Flow Win 虚拟输出（WaveRT）
  → WASAPI loopback
  → 44.1 kHz / 16-bit / 双声道转换
  → ALAC / RTP
  → AirPlay 音箱
```

物理耳机或本机音响不在这条播放路径中，因此不会与 AirPlay 音箱同时出声。切回物理输出设备后，本机恢复播放；应用会跟随 Windows 默认输出并自动重启采集。

AirPlay 音箱自身的网络缓冲仍会带来固有延迟。虚拟设备解决的是“双重播放”和 Windows 输出切换问题，不会把 AirPlay 变成零延迟链路。

## 驱动边界

- 基于 Microsoft SysVAD，只注册一个渲染端点。
- 不注册麦克风、HDMI、SPDIF、蓝牙、USB、关键字检测器或 APO 接口。
- 驱动不执行网络请求和 ALAC 编码。
- 用户态继续使用现有 WASAPI loopback，避免新增自定义 IOCTL 或共享内存协议。
- 桌面应用通过设备名称识别虚拟端点，并在设置页显示当前是否处于“仅 AirPlay”路径。

## 构建和发布

`driver/sysvad/Build-Driver.ps1` 使用固定版本的官方 WDK NuGet 包构建 x64 驱动。构建产物默认不签名，也不会自动安装。

开发机测试安装需要单独处理测试证书、测试签名和 Secure Boot；正式发布需要符合微软要求的内核驱动签名与目录文件。所有会修改系统启动或证书信任的步骤都不属于普通构建流程。
