# RadialCode

Rust 实现的开放圆形异形码，支持编码、图片解码，以及本地网页工作台。采用放射线条、三个圆形牛眼和独立中央 Logo 留白。自定义码制不兼容微信或标准 QR 扫码器。

## 启动

需要 Rust 1.85 或更新版本（edition 2024）。无需 Node、数据库或外部服务。

```sh
cargo run --release -- serve
```

打开 http://127.0.0.1:8787 。编码页输入文本并调整圆点/放射线、深色前景色、尺寸及 Logo；下载 PNG/SVG，或点击“验证当前码”。解码页支持选择、拖放和粘贴 PNG/JPEG。耐受实验对真实像素进行旋转、缩放和加噪，再调用同一解码器。

```sh
cargo run --release -- encode --text '你好，RadialCode 👋' --output code.png
cargo run --release -- encode --text 'https://example.com' --output code.svg --style dots
cargo run --release -- decode code.png
cargo run --release -- serve --port 8788
```

编码支持 `--spokes 36|54|72`、`--ecc L|M|Q|H`（默认 72 线 M 档），还支持 `--size 768`、`--foreground '#173B32'`、`--logo logo.png`。解码二进制内容时命令行输出 Base64。

## 库

```rust
use radialcode::{encode_with_profile, Profile, render_png, decode_image, RenderOptions};
fn main() -> radialcode::Result<()> {
    let symbol = encode_with_profile("你好".as_bytes(), Profile::default())?;
    let png = render_png(&symbol, &RenderOptions::default())?;
    assert_eq!(decode_image(&png)?.payload, "你好".as_bytes());
    Ok(())
}
```

## 边界

- 新版容量随线数和纠错档位变化，按 UTF-8 字节计数。旧库接口 `encode` 保留 96 线 v1（128 字节），新项目使用 `encode_with_profile`。解码自动识别新版 12 种组合和旧版。
- 输出 384–2048 像素，默认 768；白底、实色前景，白底对比度至少 7:1。
- 输入 PNG/JPEG，最大 8 MiB、1600 万像素。网页 Logo 限制 2 MiB，居中裁剪至独立圆形留白。
- 支持完整截图、旋转、等比缩放、平移和轻微噪声。建议码画布至少 384 像素，保留外围留白和全部定位点。
- 不承诺摄像头透视、反光、纸面弯曲、强遮挡或多码识别。
- L/M/Q/H 目标分别为 10%/15%/25%/35% 码字节纠错，使用 `ceil(码字节数×目标比例)` 决定可纠正字节数。旧版为 32 个错误字节。这不是可遮挡面积百分比。CRC 检查完整性，不提供签名、加密或真实性验证。
- 服务仅监听回环地址，同步处理单用户请求；图片和内容不持久保存、不发送第三方。

## 新版容量（字节）

| 线数 | L · 10% | M · 15% | Q · 25% | H · 35% |
| --- | ---: | ---: | ---: | ---: |
| 36 | 57 | 47 | 31 | 13 |
| 54 | 92 | 78 | 52 | 26 |
| 72 | 124 | 108 | 74 | 38 |

容量基于本项目几何、帧和 RS 参数计算，与微信私有码制的字符集和容量不同。

## HTTP

`POST /api/encode`：JSON `{"text":"你好","spokes":72,"ecc":"M","size":768,"foreground":"#173B32","style":"lines"}`，可选 `logo_base64`。返回 `png_base64`、`svg`、`byte_length` 和 `profile`（线数、纠错档位、容量与纠错字节数）。`GET /api/profiles` 提供 12 档权威容量数据。

`POST /api/decode`：请求体直接为 PNG/JPEG 字节。返回 `text`（非 UTF-8 为 null）、`payload_base64`、`byte_length`、`corrected_bytes`、原图中的 `landmarks`、`width`、`height`、`profile`（旧版为 null）。错误使用非 2xx 状态和 `{"error":"..."}`。

## 验证

```sh
cargo fmt --check
cargo test --all-targets
cargo build --release
node --check web/app.js # 可选；运行工作台不需要 Node
```

测试覆盖协议固定向量、内容边界、1/16/32 字节纠错、CRC/版本/补零拒绝、两种渲染样式、Logo、PNG/JPEG、单独噪声与模糊、200 组固定种子旋转缩放、无效输入及尺寸限制。浏览器验收记录见 [验证记录](docs/validation.md)。

协议、几何及掩码定义见 [v2 协议](docs/format-v2.md) 和 [兼容 v1 协议](docs/format-v1.md)。
