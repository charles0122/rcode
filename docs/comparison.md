# RCode 与小程序码、异型码对比

资料核对日期：2026-10-23。

## 先统一比较口径

“容量”至少有三种不同含义，不能混为一谈：

1. **原始净载荷容量**：码图直接保护并携带的用户数据字节数。RCode 和标准 QR Code 可以按此口径比较。
2. **接口参数上限**：平台生成接口允许提交的 `scene`、页面路径、URL 或 URI 长度。平台可能先压缩、映射成内部 ID，或把数据保存在服务端，因此它不一定等于码内净载荷。
3. **业务对象范围**：码能打开哪些对象，例如小程序页面、App Clip、Lens 或歌曲。这说明用途，不说明字节容量。

下表优先使用平台或标准维护方公开的数据。`未公开` 表示在所列公开资料中没有可验证的数值，并不表示没有纠错或没有容量限制。

## 总览

| 码制 | 图案直接表示的内容 | 可公开确认的容量 | 纠错信息 | 是否开放、离线 | 主要限制 |
| --- | --- | --- | --- | --- | --- |
| **RCode v2** | 任意二进制内容 | 36/54/72 线、四档组合下为 **13–124 字节**；默认 72-M 为 **108 字节** | Reed–Solomon；L/M/Q/H 目标 10%/15%/25%/35%，可精确纠正 9–61 个未知位置错误码字节 | MIT 开源；生成和解码均可离线 | 目前是项目自有码制；不被微信、抖音或通用 QR 扫码器识别 |
| **微信小程序码** | 微信小程序及页面入口；底层协议私有 | 历史公开设计表的最大二进制容量为 **11–43 字节**。当前 `getUnlimitedQRCode` 的业务 `scene` 参数最多 **32 个可见字符**；另一个 `getQRCode` 接口的 `path` 最多 **128 字节**，两者都不能直接当作底层码容量 | 历史设计资料公开 L/M/Q/H 约 10%/15%/25%/35%；当前 API 不让调用方直接选择底层版本和档位 | 微信服务端生成；官方识别入口是微信 | 依赖微信应用、帐号和服务端；协议不是开放标准 |
| **抖音码** | 字节系小程序/小游戏页面入口 | 当前生成接口接收 `path`，公开页面没有给出码内净载荷或固定字符上限 | 未公开 | 抖音服务端生成；圆形抖音码仅字节系 App 识别 | 同一接口也可生成通用二维码；通用二维码与圆形抖音码的识别范围不同 |
| **Apple App Clip Code** | 一个 HTTPS App Clip invocation URL | Apple 明确说明 URL 会压缩，能编码的确切长度取决于字符和词汇；生成工具负责判断是否过长，没有固定公开字节上限 | 未公开 | Apple 工具生成；iOS 14.3+ Camera/Code Scanner 识别；可选 NFC | URL 字符集和格式受限；需要 App Clip experience 与 Apple 生态 |
| **Snapcode** | Snapchat 功能或内容标识，例如 Lens | 未公开任意二进制容量或固定字符上限 | 未公开 | Snapchat/Lens Studio 生成；Snapchat 或 Camera Kit 识别 | 码关联平台对象；内容下线后对应 Snapcode 可能失效 |
| **Spotify Code** | Spotify URI 对应的歌曲、专辑、艺人、歌单等 | 未公开任意二进制容量；官方明确不能指向任意网页 | 未公开 | Spotify 生成；只能由 Spotify App 内扫码器识别 | 依赖 Spotify URI 和服务；不是通用链接码 |
| **标准 QR Code** | 数字、字母、Kanji 或任意 8-bit 字节 | Version 40：L 档最多 **2,953 二进制字节**，H 档最多 **1,273 二进制字节**；最大图案 177×177 模块 | Reed–Solomon；L/M/Q/H 约恢复总码字的 7%/15%/25%/30% | ISO/IEC 18004 标准；大量离线实现 | 最大容量对应最大的 Version 40 图案，不能与较小圆形码按同一物理尺寸直接比较 |

## RCode 与微信公开设计数据

微信技术团队公开的历史设计资料给出了 36、54、72 线和 L/M/Q/H 四档，并分别列出数字、字符、二进制容量。为保持同一口径，下面只取“二进制最大容量”与 RCode 的任意字节容量比较。

| 线数 | 档位 | RCode 容量（字节） | 微信公开设计的二进制容量（字节） |
| ---: | :---: | ---: | ---: |
| 36 | L | 57 | 25 |
| 36 | M | 47 | 22 |
| 36 | Q | 31 | 17 |
| 36 | H | 13 | 11 |
| 54 | L | 92 | 35 |
| 54 | M | 78 | 32 |
| 54 | Q | 52 | 23 |
| 54 | H | 26 | 17 |
| 72 | L | 124 | 43 |
| 72 | M | 108 | 39 |
| 72 | Q | 74 | 29 |
| 72 | H | 38 | 21 |

这张表不表示 RCode 与微信小程序码兼容，也不表示两者在真实相机环境中具有相同识别率。RCode 的数值来自当前源码和测试；微信数值来自历史公开设计文章中的容量图，平台可以在不公开底层实现的情况下调整服务端策略。

## 纠错口径

RCode 的总码字长度分别为 87、130、172 字节。设总码字长度为 `n`、档位比例为 `p`，可纠正未知位置错误字节数为 `t = ceil(n × p)`，RS 校验长度为 `2t`：

| 线数 | 总码字字节 | L · 10% | M · 15% | Q · 25% | H · 35% |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 36 | 87 | 9 | 14 | 22 | 31 |
| 54 | 130 | 13 | 20 | 33 | 46 |
| 72 | 172 | 18 | 26 | 43 | 61 |

表中数值是解码器在错误位置未知时可纠正的**码字节数**，不是像素数，也不是可任意遮挡的面积。定位点、边界、采样几何被破坏时，即使数据区受损比例不高也可能无法定位。微信历史资料使用相似的 L/M/Q/H 百分比描述，但未公开足够信息让外部实现按当前服务端输出复算精确错误字节数。标准 QR Code 的 L/M/Q/H 又是约 7%/15%/25%/30%，因此三个体系的同名字母不能当作完全相同的参数。

## 设计定位

- 需要开放协议、任意短二进制内容、完全离线生成与识别时，RCode 才是这里的目标。
- 需要最大容量和最广泛的扫码兼容性时，标准 QR Code 更合适。
- 需要直接进入微信、抖音、App Clip、Snapchat 或 Spotify 的平台对象时，应使用对应平台官方生成的码；RCode 不替代平台路由、信任和风控能力。
- RCode 当前的旋转、缩放、平移和轻噪声能力有自动化像素测试，但摄像头透视、反光、弯曲、强遮挡和跨设备实测仍是明确边界，不能仅凭纠错百分比推导扫描成功率。

## 来源

- RCode：[v2 协议、容量与纠错公式](format-v2.md)
- 微信：[小程序码设计篇：菊花绽放](https://developer.cloud.tencent.com/article/1005027)（历史设计、容量与纠错）；[getUnlimitedQRCode](https://developers.weixin.qq.com/miniprogram/dev/OpenApiDoc/qrcode-link/qr-code/getUnlimitedQRCode.html) 与 [getQRCode](https://developers.weixin.qq.com/miniprogram/dev/OpenApiDoc/qrcode-link/qr-code/getQRCode.html)（当前生成接口）
- 抖音：[获取小程序码](https://developer.open-douyin.com/docs/resource/zh-CN/thirdparty/API/smallprogram/auth-app-manage/base-info/get-qrcode)
- Apple：[Creating App Clip Codes](https://developer.apple.com/documentation/appclip/creating-app-clip-codes)、[Encoding a URL in an App Clip Code](https://developer.apple.com/documentation/appclip/encoding-a-url-in-an-app-clip-code)、[App Clip Code 尺寸指南](https://developer.apple.com/design/human-interface-guidelines/app-clips)
- Snap：[Snapcodes](https://developers.snap.com/lens-studio/publishing/distributing/snapcodes)、[Unlocking Lenses](https://developers.snap.com/lens-studio/publishing/distributing/unlocking-lenses)
- Spotify：[Spotify Codes](https://www.spotifycodes.com/)、[Share from Spotify](https://support.spotify.com/us/article/share-from-spotify/)
- QR Code：[DENSO WAVE 概略规格](https://www.qrcode.com/en/about/standards.html/index.html)、[容量与版本](https://www.qrcode.com/en/about/version.html)、[纠错说明](https://www.qrcode.com/en/about/error_correction.html)、[ISO/IEC 18004:2024](https://www.iso.org/standard/83389.html)
