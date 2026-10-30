import init, { profiles_json, encode_plan_json, decode_rgba8_json } from "./pkg/rcode_wasm.js";

const ready = init();
ready.catch(() => self.postMessage({ id: null, error: "WASM 核心加载失败，请刷新页面重试。" }));

self.onmessage = async ({ data: { id, path, body } }) => {
  try {
    await ready;
    let result;
    if (path === "/api/profiles") {
      result = JSON.parse(profiles_json());
    } else if (path === "/api/encode") {
      const options = JSON.parse(body);
      const payload = new TextEncoder().encode(options.text);
      result = {
        plan: JSON.parse(encode_plan_json(payload, options.spokes, "LMQH".indexOf(options.ecc), ["lines", "dots", "rings"].indexOf(options.style))),
        byte_length: payload.length,
      };
    } else if (path === "/api/decode") {
      if (!/^image\/(png|jpeg)$/.test(body.type)) throw new Error("只支持 PNG 或 JPEG 图片。");
      if (body.size > 8 * 1024 * 1024) throw new Error("图片超过 8 MiB 限制。");
      const image = await createImageBitmap(body);
      try {
        const { width, height } = image;
        if (width * height > 16_000_000) throw new Error("图片像素超过 1600 万限制。");
        const context = new OffscreenCanvas(width, height).getContext("2d");
        context.fillStyle = "#FFFFFF";
        context.fillRect(0, 0, width, height);
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, width, height).data;
        const decoded = JSON.parse(decode_rgba8_json(pixels, width, height, width * 4));
        const payload = Uint8Array.from(decoded.payload);
        let text = null;
        try { text = new TextDecoder("utf-8", { fatal: true }).decode(payload); } catch { /* Binary payloads use Base64. */ }
        result = {
          ...decoded,
          profile: JSON.parse(profiles_json()).find((profile) => profile.spokes === decoded.spokes && profile.ecc === "LMQH"[decoded.ecc]),
          text,
          payload_base64: btoa(String.fromCharCode(...payload)),
          byte_length: payload.length,
          landmarks: decoded.landmarks.map(([x, y]) => ({ x, y })),
        };
      } finally {
        image.close();
      }
    } else {
      throw new Error("未知的浏览器核心操作。");
    }
    self.postMessage({ id, result });
  } catch (error) {
    self.postMessage({ id, error: String(error?.message || error) });
  }
};
