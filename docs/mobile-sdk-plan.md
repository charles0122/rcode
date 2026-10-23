# RCode 移动端 SDK 开发计划

## 目标与边界

RCode v3 的协议、定位、纠错、渲染和解码只保留一份 Rust 实现。四个相同的黑白黑
定位点配合四点单应变换，是当前唯一格式。Android、iOS 和
Flutter 不重写算法，只负责构建产物、相机权限与生命周期、帧采集、坐标映射和业务
UI。

已完成：旧协议移除、原始 `decode_luma8` 灰度帧入口、四定位点单应解码。真实设备
样本验收仍在里程碑 2 范围内；本计划不把离线图片测试视为相机鲁棒性验收。

第一版移动扫码范围限定为：后置相机、单个完整 RCode、8-bit 灰度帧、离线解码。
多码、视频文件、自动变焦、连续跟踪、弯曲纸面和强遮挡不在第一版范围内。

## 分层

```text
Android / iOS / Flutter 业务 API 与 UI
                 |
CameraX / AVFoundation 相机适配
                 |
          稳定的 C ABI 边界
                 |
 Rust 协议、检测、纠错、编解码核心
```

Flutter 插件复用 Android 和 iOS 的原生相机实现，只把控制命令和扫码结果传给
Dart，不把每一帧图像通过平台通道发送到 Dart。

## 里程碑 1：原始灰度帧入口（已完成）

### 实现

- 增加 `decode_luma8(pixels, width, height, row_stride)` 公共 Rust API。
- 将现有图片解码后的 `GrayImage` 处理提取为共享内部路径；PNG/JPEG 与灰度帧使用
  同一套定位和纠错逻辑。
- 支持 `row_stride > width` 的相机平面，每行只复制有效的 `width` 字节。
- 第一版允许一次灰度帧打包复制，不引入零拷贝抽象。
- 校验最小尺寸、1600 万像素上限、stride 和缓冲区长度，所有算术使用 checked
  运算，非法输入返回 `Error::InvalidImage`。

### 验收

- 紧凑灰度缓冲区和带 padding 的缓冲区解出相同 payload。
- 空尺寸、过小尺寸、窄 stride、截断缓冲区和超大尺寸不会 panic。
- 原有 PNG/JPEG 行为和测试不变。
- `cargo fmt --check`、`cargo test --all-targets`、`cargo build --release` 通过。

## 里程碑 2：真实相机鲁棒性（进行中）

### 数据集

建立带清单的设备样本集，区分以下变量：

- 载体：打印纸、磨砂纸、另一台手机屏幕。
- 距离与占比：码占画面短边 20% 到 80%。
- 角度：正视以及水平/垂直方向 15、25、35 度。
- 光照：室内弱光、普通室内、窗边强光，并包含局部反光。
- 运动：静止、轻微手抖、一次快速经过。
- 负样本：普通 QR、同心圆、随机纹理、无代码场景。

提交小型、可公开的回归集；较大的设备原始集用独立制品保存，并用 manifest 记录
设备、分辨率、方向和预期结果。所有失败样本必须可复现，不以人工挑选成功帧代替
连续扫码验证。

### 算法顺序

1. 先测量现有定位器在灰度帧上的失败类型。
2. 根据失败样本验证四定位点单应校正，并按实测需要再增加局部阈值或预缩放。
3. 保留单码检测；多码候选跟踪后置。
4. 对 720p 灰度帧建立基准，记录中位数和 P95 延迟。

### 初始发布门槛

- 支持范围样本解码成功率不低于 95%。
- 负样本不得返回成功 payload。
- 基准设备 720p 单帧 P95 解码时间不高于 100 ms。
- 连续运行 5 分钟无帧队列累积或持续内存增长。

这些数值是第一版工程门槛；完成首轮设备测量后可以调整，但必须记录调整原因。

## 里程碑 3A：稳定 C ABI 与核心构建产物（已完成）

### C ABI

`ffi/` 下的 `rcode-ffi` 独立 crate 依赖现有 Rust 库，不把当前仓库立即拆成多个业务
crate。ABI 版本为 1，只暴露 C 兼容整数、指针、长度和不透明句柄：

- `rcode_abi_version()` 与 `rcode_core_version()`：返回 ABI 和核心 crate 版本。
- `rcode_decode_luma8(...)`：同步解码一帧。
- 结果读取函数：payload、landmarks、profile、corrected bytes。
- `rcode_result_free(...)`：由创建结果的一侧释放内存。
- 稳定错误码；诊断文本只作为补充，不用于业务分支。

Rust panic 不得跨越 FFI 边界；输入指针、长度和整数转换必须先校验。不得暴露 Rust
`Vec`、`String`、enum 或未标记布局的 struct。解码器不保存全局可变状态，以便不同
相机线程安全调用。

### 产物

- Android：已生成 `arm64-v8a` 和 `x86_64` shared libraries。
- Apple：已生成 iOS device 与 arm64 simulator static libraries 和 XCFramework。
- 头文件、ABI 测试、C11 语法、宿主导出符号检查、跨目标构建均已验证。
- AAR、Swift Package 和平台封装已在后续里程碑完成；本 milestone 自身不包含 UI 或相机管理。

## 里程碑 4：Android 与 iOS 扫码 SDK（实现完成，真机验收待完成）

### Android

- 使用 CameraX `Preview` 与 `ImageAnalysis`。
- 请求 `YUV_420_888`，直接读取 Y plane，将 `rowStride` 传给 Rust。
- 使用只保留最新帧的背压策略；上一帧未完成时不排队。
- `ImageProxy` 必须在所有路径关闭。
- Kotlin 层负责权限、前后台切换、手电筒和预览坐标映射。

### iOS

- 使用 `AVCaptureSession`、`AVCaptureVideoDataOutput` 和串行分析队列。
- 读取 bi-planar PixelBuffer 的 Y plane，将 `bytesPerRow` 作为 stride。
- 丢弃迟到帧；锁定的 PixelBuffer 必须在所有路径解锁。
- Swift 层负责权限、前后台切换、手电筒和预览坐标映射。

### 公共业务语义

两端保持相同的概念：`ScanResult`、`Landmark`、`Profile`、`ScanError`、
`start/stop`、闪光灯控制和一次结果去重。平台 API 可以符合各自语言习惯，但 payload
和错误码语义必须一致。

验收必须包含真实 Android 与 iPhone；构建成功或相机预览出现不能代替扫码成功。

当前已完成 CameraX/AVFoundation 后置相机实现、生命周期、权限状态、手电筒、单次结果
门控、预览坐标映射，以及 PNG/JPEG 单图解码。Android JVM 测试、Debug/Release AAR、
lint 和双 ABI 打包均通过；Apple 6 个模拟器测试通过，其中包含当前 v3 图片的真实
解码。由于本轮没有可用 Android 真机与 iPhone，真实相机矩阵仍是发布阻塞项。

## 里程碑 5：Flutter 插件（实现完成，真机验收与发布打包待完成）

- 使用标准 Flutter plugin，而不是在 Dart 中重写解码。
- Android/iOS 侧直接复用里程碑 4 的 SDK。
- 第一版用原生预览承载相机，Flutter 绘制扫描框和业务 UI。
- 方法调用只传启动、停止、闪光灯等控制；事件流只传成功结果和状态。
- 不通过 MethodChannel/EventChannel 传连续相机帧。
- 提供相册图片解码和相机扫码两个独立 API，业务可按需引入扫码 UI。

验收包含 Android/iOS Flutter 示例应用的权限拒绝、前后台切换、旋转、重复结果抑制
和真实扫码。

当前插件已复用两端原生 SDK，连续相机帧不经过平台通道；仅显式调用单图解码时传一
张图片。Dart 分析和 4 个状态/通道测试通过，Android 示例 APK 与 iOS arm64 模拟器
App 均已构建。仓库内相对路径只用于联调，发布前仍需替换为同版本的 AAR 与 Apple
二进制制品，并完成两端真机验收。

## 发布与维护

- Rust 核心、AAR、XCFramework 和 Flutter plugin 使用同一发布版本，并在包装层记录
  核心版本。
- 当前 v3 协议由固定向量验证；跨端只核对同一向量结果，不保留旧协议 fixture。
- 每次提交运行 Rust 格式化和全量测试；SDK 变更再运行 Android/iOS 构建与绑定
  smoke test。
- 发布候选必须完成真实设备矩阵，不把模拟器或桌面图片解码当作相机验收。
- 先完成一个端到端平台，再复制已验证的边界到第二个平台和 Flutter，避免三端同时
  调试核心 API。

## 明确后置的事项

- 多码同时识别与追踪。
- 自动变焦和基于 landmarks 的持续跟踪。
- Web/WASM 相机版。
- 云端扫码服务。
- 码内容签名、加密或业务真实性验证。
- 为减少一次灰度打包复制而进行的复杂零拷贝重构。
