# RCode WebAssembly core

`rcode-wasm` exposes the same Rust encoder, RenderPlan builder and decoder to browsers:

- `encode_plan_json`: returns canvas-ready circle, round-line and round-arc commands.
- `decode_luma8_json`: accepts a packed or padded grayscale frame.
- `decode_rgba8_json`: accepts Canvas `ImageData` directly and converts it inside WASM.

Run camera decoding in a Web Worker and keep only the latest frame. The browser owns
`getUserMedia`, permissions, preview and overlay mapping; the Rust core owns detection,
error correction and payload validation.
