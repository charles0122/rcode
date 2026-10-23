# RCode

Rust 实现的开放圆形异形码，支持编码、图片解码，以及本地网页工作台。采用放射线条、四个相同的黑白黑定位点和独立中央 Logo 留白。自定义码制不兼容微信或标准 QR 扫码器。

## 启动

需要 Rust 1.85 或更新版本（edition 2024）。无需 Node、数据库或外部服务。

```sh
cargo run --release -- serve
```

打开 http://127.0.0.1:8787 。编码页输入文本并调整圆点/放射线、深色前景色、尺寸及 Logo；下载 PNG/SVG，或点击“验证当前码”。解码页支持选择、拖放和粘贴 PNG/JPEG。耐受实验对真实像素进行旋转、缩放和加噪，再调用同一解码器。

```sh
cargo run --release -- encode --text '你好，RCode 👋' --output code.png
cargo run --release -- encode --text 'https://example.com' --output code.svg --style dots
cargo run --release -- decode code.png
cargo run --release -- serve --port 8788
```

编码支持 `--spokes 36|54|72`、`--ecc L|M|Q|H`（默认 72 线 M 档），还支持 `--size 768`、`--foreground '#173B32'`、`--logo logo.png`。解码二进制内容时命令行输出 Base64。

## 库

```rust
use rcode::{encode, Profile, render_png, decode_image, RenderOptions};
fn main() -> rcode::Result<()> {
    let symbol = encode("你好".as_bytes(), Profile::default())?;
    let png = render_png(&symbol, &RenderOptions::default())?;
    assert_eq!(decode_image(&png)?.payload, "你好".as_bytes());
    Ok(())
}
```

## 边界

- 容量随线数和纠错档位变化，按 UTF-8 字节计数。`encode` 必须传入 `Profile`；解码自动识别 12 种线数与纠错档位组合。
- 输出 384–2048 像素，默认 768；白底、实色前景，白底对比度至少 7:1。
- 输入 PNG/JPEG，最大 8 MiB、1600 万像素。网页 Logo 限制 2 MiB，居中裁剪至独立圆形留白。
- 库 API `decode_luma8` 面向摄像头帧，接收 8 位亮度数据、宽高和行步长，并将活动像素打包复制一次；摄像头权限和生命周期由调用方管理。
- 支持完整截图、旋转、等比缩放、平移、轻微噪声和合成图像上的适度透视变形；真实相机透视尚未验收。建议码画布至少 384 像素，保留外围留白和全部四个定位点。
- 不承诺强透视、反光、纸面弯曲、强遮挡或多码识别。
- L/M/Q/H 目标分别为 10%/15%/25%/35% 码字节纠错，使用 `ceil(码字节数×目标比例)` 决定可纠正字节数。这不是可遮挡面积百分比。CRC 检查完整性，不提供签名、加密或真实性验证。
- 服务仅监听回环地址，同步处理单用户请求；图片和内容不持久保存、不发送第三方。

## 新版容量（字节）

| 线数 | L · 10% | M · 15% | Q · 25% | H · 35% |
| --- | ---: | ---: | ---: | ---: |
| 36 | 57 | 47 | 31 | 13 |
| 54 | 92 | 78 | 52 | 26 |
| 72 | 124 | 108 | 74 | 38 |

容量基于本项目几何、帧和 RS 参数计算，与微信私有码制的字符集和容量不同。

## 与其他码制的区别

RCode 是可离线生成、离线解码的通用二进制码；微信小程序码、抖音码、App Clip Code、Snapcode 和 Spotify Code 主要是各自平台的入口标识。平台接口允许传入的 `scene`、页面路径或 URI，不等于图案中的原始净载荷容量。

| 码制 | 可公开确认的容量 | 纠错 | 生成与识别 |
| --- | --- | --- | --- |
| **RCode v3** | 任意二进制 **13–124 字节**；默认 72 线 M 档为 **108 字节** | RS；L/M/Q/H 目标为 **10%/15%/25%/35%**，按档位精确纠正 9–61 个错误码字节 | 开源；本地生成和解码，不依赖平台 |
| **微信小程序码** | 公开设计资料中的二进制上限为 **11–43 字节**；当前不限量接口的业务参数 `scene` 最多 32 个可见字符 | 公开设计资料为 L/M/Q/H **10%/15%/25%/35%**；底层协议私有 | 微信服务端生成，微信扫码 |
| **抖音码** | 接口接收小程序 `path`，官方未公开码内净载荷上限 | 未公开 | 抖音服务端生成；圆形抖音码仅字节系 App 识别 |
| **Apple App Clip Code** | 编码 HTTPS invocation URL；使用专用压缩，长度随字符和词汇变化，没有固定公开上限 | 未公开 | Apple 工具生成；iOS 14.3+ 相机/扫码器识别，可结合 NFC |
| **Snapcode** | 表示 Snapchat 功能或内容标识；未公开任意数据容量 | 未公开 | Snapchat/Lens Studio 生成，Snapchat 或 Camera Kit 识别 |
| **Spotify Code** | 表示 Spotify URI，不能指向任意网页；未公开任意数据容量 | 未公开 | Spotify 生成并由 Spotify App 内扫描 |
| **标准 QR Code** | Version 40 最大 **2,953 字节（L）/1,273 字节（H）** | RS；L/M/Q/H 约 **7%/15%/25%/30%** | ISO/IEC 18004 标准，通用生成与识别 |

微信公开设计资料与 RCode 使用相同的 36/54/72 线和纠错档位名称，但协议、字符集、帧开销和解码器均不兼容：

| 线数 | RCode 二进制容量 L/M/Q/H | 微信公开设计的二进制容量 L/M/Q/H |
| --- | ---: | ---: |
| 36 | 57 / 47 / 31 / 13 | 25 / 22 / 17 / 11 |
| 54 | 92 / 78 / 52 / 26 | 35 / 32 / 23 / 17 |
| 72 | 124 / 108 / 74 / 38 | 43 / 39 / 29 / 21 |

百分比描述的是码字纠错预算，不代表任意位置可遮挡的面积。微信容量数字来自其历史公开设计资料，不应视作当前服务端实现承诺；其他平台未公开的字段也不作推测。完整口径、逐档纠错字节数和官方来源见 [码制对比](docs/comparison.md)。

## HTTP

`POST /api/encode`：JSON `{"text":"你好","spokes":72,"ecc":"M","size":768,"foreground":"#173B32","style":"lines"}`，可选 `logo_base64`。返回 `png_base64`、`svg`、`byte_length` 和 `profile`（线数、纠错档位、容量与纠错字节数）。`GET /api/profiles` 提供 12 档权威容量数据。

`POST /api/decode`：请求体直接为 PNG/JPEG 字节。返回 `text`（非 UTF-8 为 null）、`payload_base64`、`byte_length`、`corrected_bytes`、原图中的 `landmarks`、`width`、`height`、`profile`。错误使用非 2xx 状态和 `{"error":"..."}`。

## 验证

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace --release
node --check web/app.js # 可选；运行工作台不需要 Node
```

测试覆盖协议固定向量、内容边界、1/16/32 字节纠错、CRC/版本/补零拒绝、两种渲染样式、Logo、PNG/JPEG、单独噪声与模糊、200 组固定种子旋转缩放、无效输入及尺寸限制。浏览器验收记录见 [验证记录](docs/validation.md)。

协议、几何及掩码定义见 [当前协议](docs/format.md)。

## 移动端 SDK

同一 Rust 核心通过稳定 C ABI 供 [Android SDK](mobile/android/README.md)、
[Apple Swift Package](mobile/apple/README.md) 和 [Flutter 插件](mobile/flutter/README.md)
复用；三端不重复实现协议与解码算法。先运行 `scripts/build-mobile-core.sh` 生成 Android
双 ABI 动态库和 Apple XCFramework。开发里程碑、已验证范围及真机发布门槛见
[移动端 SDK 计划](docs/mobile-sdk-plan.md)。
