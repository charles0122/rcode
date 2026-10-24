# RenderPlan 跨端绘制协议

## 边界

Rust 是数据帧、纠错、掩码、槽位、样式排版和解码的唯一权威实现。业务端不接收
bit matrix，也不计算相邻数据格如何连接；它只把 `RenderPlan` 中的基础图元翻译到
当前平台画布。Rust 的 PNG/SVG renderer 保留为 CLI、参考输出和回归测试工具，不是
移动或 Web SDK 的必需依赖。

## 坐标与图层

- viewport 固定为 128×128，原点在左上角，正 y 轴向下。
- 当前 RenderPlan `version` 为 `1`；适配器遇到未知版本必须拒绝绘制。
- 圆弧角度使用弧度，从正 x 轴开始，正 sweep 在屏幕坐标中顺时针。
- operations 按数组顺序绘制，线段和圆弧必须使用圆形端点。
- `background` 与 `foreground` 是颜色角色，实际颜色由业务端传入。
- logo region 当前为中心 `(64, 64)`、半径 `20` 的圆；Logo 由业务端最后裁剪绘制。
- 业务端必须等比缩放并使用抗锯齿，不能非等比拉伸码图。

第一版操作只有：

| kind | values |
| --- | --- |
| circle | `cx, cy, radius` |
| line | `x1, y1, x2, y2, width` |
| arc | `cx, cy, radius, startAngle, sweepAngle, width` |

新增样式若能由这些图元表达，只修改 Rust 排版器；只有新增图元种类才需要同步更新
Canvas/CoreGraphics/CustomPainter 适配器并提升计划版本。

## 平台映射

- Android：`RCodeEncoder` + `RCodeRenderer`，CameraX Y plane 继续进入 `decode_luma8`。
- Apple：`RCodeEncoder` + `RCodeRenderer`，AVFoundation Y plane 继续进入同一 C ABI。
- Flutter：方法通道只传一次 RenderPlan；`RCodeView` 用 CustomPainter 绘制。连续相机帧
  不经过 Dart。
- Web：HTTP 工作台返回 RenderPlan 并由 Canvas 绘制；`rcode-wasm` 提供相同编码计划、
  `decode_luma8_json` 和适合 Canvas ImageData 的 `decode_rgba8_json`。生产相机应在
  Web Worker 中解码并只保留最新帧。

跨端不要求抗锯齿像素逐点相同。验收以固定输入产生相同操作语义、关键间距合格以及
各端渲染结果能够由同一 Rust 解码器回读为准。
