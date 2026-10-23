(() => {
  "use strict";

  let MAX_BYTES = 108;
  let profiles = [];
  const MAX_UPLOAD_BYTES = 8 * 1024 * 1024;
  const MAX_PIXELS = 16_000_000;
  const PREVIEW_DELAY = 300;

  const $ = (id) => document.getElementById(id);
  const refs = {
    tabs: [...document.querySelectorAll(".tab")],
    panels: [...document.querySelectorAll(".tab-panel")],
    encodeForm: $("encode-form"),
    encodeText: $("encode-text"),
    spokes: $("code-spokes"),
    ecc: $("code-ecc"),
    profileInfo: $("profile-info"),
    textCount: $("text-count"),
    textError: $("text-error"),
    foregroundColor: $("foreground-color"),
    foregroundHex: $("foreground-hex"),
    colorError: $("color-error"),
    encodeSize: $("encode-size"),
    sizeOutput: $("size-output"),
    logoDrop: $("logo-drop"),
    logoInput: $("logo-input"),
    logoEmpty: $("logo-empty"),
    logoPreview: $("logo-preview"),
    logoImage: $("logo-image"),
    logoName: $("logo-name"),
    logoSize: $("logo-size"),
    logoRemove: $("logo-remove"),
    logoError: $("logo-error"),
    encodeRun: $("encode-run"),
    encodeClear: $("encode-clear"),
    encodeEmpty: $("encode-empty"),
    encodePngImage: $("encode-png-image"),
    encodeSvgImage: $("encode-svg-image"),
    encodeLoading: $("encode-loading"),
    encodeStatus: $("encode-status"),
    outputBytes: $("output-bytes"),
    outputSize: $("output-size"),
    downloadPng: $("download-png"),
    downloadSvg: $("download-svg"),
    verifyCurrent: $("verify-current"),
    previewTabs: [...document.querySelectorAll("[data-preview-kind]")],
    decodeDrop: $("decode-drop"),
    decodeInput: $("decode-input"),
    decodeFileInfo: $("decode-file-info"),
    decodeFileName: $("decode-file-name"),
    decodeClear: $("decode-clear"),
    decodeInputError: $("decode-input-error"),
    decodeRun: $("decode-run"),
    decodeEmpty: $("decode-empty"),
    decodeImage: $("decode-image"),
    decodeOverlay: $("landmark-overlay"),
    landmarkToggle: $("landmark-toggle"),
    decodeLoading: $("decode-loading"),
    decodeStatus: $("decode-status"),
    decodeStatePill: $("decode-state-pill"),
    decodeResult: $("decode-result"),
    decodeContentKind: $("decode-content-kind"),
    decodeContent: $("decode-content"),
    decodeCopy: $("decode-copy"),
    decodeBytes: $("decode-bytes"),
    decodeCorrected: $("decode-corrected"),
    decodeDimensions: $("decode-dimensions"),
    labUseEncode: $("lab-use-encode"),
    labUseDecode: $("lab-use-decode"),
    labSourceName: $("lab-source-name"),
    labRotation: $("lab-rotation"),
    rotationOutput: $("rotation-output"),
    labScale: $("lab-scale"),
    scaleOutput: $("scale-output"),
    labNoise: $("lab-noise"),
    noiseOutput: $("noise-output"),
    labRun: $("lab-run"),
    labStatus: $("lab-status"),
    labEmpty: $("lab-empty"),
    labCanvas: $("lab-canvas"),
    labImage: $("lab-image"),
    labLoading: $("lab-loading"),
    labResultPill: $("lab-result-pill"),
    labResult: $("lab-result"),
    labContentKind: $("lab-content-kind"),
    labContent: $("lab-content"),
    labCopy: $("lab-copy"),
    labBytes: $("lab-bytes"),
    labCorrected: $("lab-corrected"),
    labDimensions: $("lab-dimensions"),
  };

  const state = {
    logoBase64: null,
    logoSequence: 0,
    logoUrl: null,
    previewKind: "png",
    generated: null,
    encodeSequence: 0,
    encodeController: null,
    decode: { blob: null, url: null, name: "", sequence: 0, fileSequence: 0, controller: null, result: null, displayContent: "" },
    lab: { source: null, sourceKind: "encode", sequence: 0, controller: null, result: null, displayContent: "", url: null },
    activePanel: "encode",
  };

  class ApiError extends Error {
    constructor(message, status = 0) {
      super(message);
      this.name = "ApiError";
      this.status = status;
    }
  }

  function byteLength(value) {
    return new TextEncoder().encode(value).length;
  }

  function formatByteCount(value) {
    return `${value} / ${MAX_BYTES} 字节`;
  }

  function setStatus(node, message = "", kind = "") {
    node.textContent = message;
    node.classList.toggle("is-error", kind === "error");
    node.classList.toggle("is-success", kind === "success");
  }

  function setBusy(button, busy) {
    button.classList.toggle("is-busy", busy);
    button.disabled = busy;
    button.setAttribute("aria-busy", String(busy));
  }

  function setLoading(node, visible) {
    node.hidden = !visible;
  }

  function replaceObjectUrl(oldUrl, blob) {
    if (oldUrl) URL.revokeObjectURL(oldUrl);
    return blob ? URL.createObjectURL(blob) : null;
  }

  function base64ToBlob(value, type) {
    const raw = String(value || "").replace(/^data:[^,]+,/, "");
    const binary = atob(raw);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return new Blob([bytes], { type });
  }

  function imageDataUrl(base64, type = "image/png") {
    return `data:${type};base64,${base64}`;
  }

  function loadImage(source) {
    return new Promise((resolve, reject) => {
      const image = new Image();
      let url = null;
      image.onload = () => {
        if (url) URL.revokeObjectURL(url);
        resolve(image);
      };
      image.onerror = () => {
        if (url) URL.revokeObjectURL(url);
        reject(new ApiError("图片无法读取，请选择有效的 PNG 或 JPEG。"));
      };
      if (source instanceof Blob) {
        url = URL.createObjectURL(source);
        image.src = url;
      } else {
        image.src = source;
      }
    });
  }

  async function inspectImage(blob) {
    if (!(blob instanceof Blob) || !/^image\/(png|jpeg)$/.test(blob.type || "")) {
      throw new ApiError("只支持 PNG 或 JPEG 图片。");
    }
    if (blob.size > MAX_UPLOAD_BYTES) throw new ApiError("图片超过 8 MiB 限制。");
    const image = await loadImage(blob);
    if (!image.naturalWidth || !image.naturalHeight) throw new ApiError("无法读取图片尺寸。");
    if (image.naturalWidth * image.naturalHeight > MAX_PIXELS) throw new ApiError("图片像素超过 1600 万限制。");
    return { width: image.naturalWidth, height: image.naturalHeight };
  }

  async function requestJson(path, options) {
    let response;
    try {
      response = await fetch(path, options);
    } catch (error) {
      if (error && error.name === "AbortError") throw error;
      throw new ApiError("无法连接本机服务，请确认 RadialCode 已启动。");
    }
    let body = null;
    try {
      body = await response.json();
    } catch (error) {
      if (error && error.name === "AbortError") throw error;
      body = null;
    }
    if (!response.ok) throw new ApiError(body && body.error ? body.error : `请求失败（${response.status}）`, response.status);
    if (!body || typeof body !== "object") throw new ApiError("服务返回了无法识别的结果。");
    return body;
  }

  function normalizedHex(value) {
    const candidate = String(value || "").trim();
    return /^#[0-9a-f]{6}$/i.test(candidate) ? candidate.toUpperCase() : null;
  }

  function relativeLuminance(hex) {
    const rgb = hex.slice(1).match(/.{2}/g).map((part) => parseInt(part, 16) / 255);
    const linear = rgb.map((channel) => (channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4));
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
  }

  function validateText(show = true) {
    const bytes = byteLength(refs.encodeText.value);
    const valid = bytes <= MAX_BYTES;
    refs.textCount.textContent = formatByteCount(bytes);
    refs.textCount.classList.toggle("is-over", !valid);
    refs.encodeText.setAttribute("aria-invalid", String(!valid));
    if (show) refs.textError.textContent = valid ? "" : `内容过长 ${bytes - MAX_BYTES} 字节，请删减后再生成。`;
    return valid;
  }

  function validateColor(show = true) {
    const value = normalizedHex(refs.foregroundHex.value);
    let message = "";
    if (!value) message = "请输入 6 位十六进制颜色，例如 #173B32。";
    else if (relativeLuminance(value) > 0.1) message = "前景色需与白底保持至少 7:1 对比度（相对亮度 ≤ 0.10）。";
    refs.foregroundHex.setAttribute("aria-invalid", String(Boolean(message)));
    if (show) refs.colorError.textContent = message;
    return !message ? value : null;
  }

  function updateSize() {
    refs.sizeOutput.textContent = `${refs.encodeSize.value} px`;
  }

  function clearEncodeErrors() {
    refs.textError.textContent = "";
    refs.colorError.textContent = "";
    refs.logoError.textContent = "";
  }

  function encodeOptions() {
    clearEncodeErrors();
    const textOk = validateText(true);
    const foreground = validateColor(true);
    if (!textOk || !foreground || !profiles.length) return null;
    const selectedStyle = document.querySelector('input[name="style"]:checked');
    return {
      text: refs.encodeText.value,
      spokes: Number(refs.spokes.value),
      ecc: refs.ecc.value,
      size: Number(refs.encodeSize.value),
      foreground,
      style: selectedStyle ? selectedStyle.value : "lines",
      ...(state.logoBase64 ? { logo_base64: state.logoBase64 } : {}),
    };
  }

  function setAnchorEnabled(anchor, enabled, href) {
    anchor.classList.toggle("is-disabled", !enabled);
    anchor.setAttribute("aria-disabled", String(!enabled));
    if (enabled) anchor.href = href;
    else anchor.removeAttribute("href");
  }

  function setPreviewKind(kind) {
    state.previewKind = kind;
    refs.previewTabs.forEach((button) => button.classList.toggle("is-active", button.dataset.previewKind === kind));
    const ready = Boolean(state.generated);
    refs.encodePngImage.hidden = !ready || kind !== "png";
    refs.encodeSvgImage.hidden = !ready || kind !== "svg";
  }

  function clearGenerated() {
    if (!state.generated) return;
    state.generated.pngUrl = replaceObjectUrl(state.generated.pngUrl, null);
    state.generated.svgUrl = replaceObjectUrl(state.generated.svgUrl, null);
    state.generated = null;
    refs.encodePngImage.removeAttribute("src");
    refs.encodeSvgImage.removeAttribute("src");
    refs.encodePngImage.hidden = true;
    refs.encodeSvgImage.hidden = true;
    refs.encodeEmpty.hidden = false;
    refs.outputBytes.textContent = "—";
    refs.outputSize.textContent = "—";
    setAnchorEnabled(refs.downloadPng, false);
    setAnchorEnabled(refs.downloadSvg, false);
    refs.verifyCurrent.disabled = true;
    updateLabSources();
  }

  function renderGenerated(payload, options) {
    const pngBlob = base64ToBlob(payload.png_base64, "image/png");
    const svgText = typeof payload.svg === "string" ? payload.svg : "";
    if (!pngBlob.size || !svgText) throw new ApiError("编码服务没有返回完整图像。");
    const pngUrl = URL.createObjectURL(pngBlob);
    const svgUrl = URL.createObjectURL(new Blob([svgText], { type: "image/svg+xml" }));
    clearGenerated();
    state.generated = { pngBlob, pngUrl, svgText, svgUrl, byteLength: Number(payload.byte_length) || byteLength(options.text), size: options.size };
    refs.encodePngImage.src = pngUrl;
    refs.encodeSvgImage.src = svgUrl;
    refs.encodePngImage.alt = `生成的 ${options.size}px RadialCode PNG`;
    refs.encodeSvgImage.alt = `生成的 ${options.size}px RadialCode SVG`;
    refs.encodeEmpty.hidden = true;
    refs.outputBytes.textContent = String(state.generated.byteLength);
    refs.outputSize.textContent = `${options.size}px`;
    setAnchorEnabled(refs.downloadPng, true, pngUrl);
    setAnchorEnabled(refs.downloadSvg, true, svgUrl);
    refs.verifyCurrent.disabled = false;
    setPreviewKind(state.previewKind);
    updateLabSources();
  }

  async function runEncode(event) {
    if (event) event.preventDefault();
    invalidateEncoding();
    const options = encodeOptions();
    if (!options) {
      setStatus(refs.encodeStatus, "请先修正输入内容。", "error");
      return;
    }
    if (state.encodeController) state.encodeController.abort();
    const controller = new AbortController();
    state.encodeController = controller;
    const sequence = ++state.encodeSequence;
    setBusy(refs.encodeRun, true);
    setLoading(refs.encodeLoading, true);
    refs.encodeEmpty.hidden = true;
    setStatus(refs.encodeStatus, "正在生成…");
    try {
      const payload = await requestJson("/api/encode", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(options),
        signal: controller.signal,
      });
      if (sequence !== state.encodeSequence) return;
      renderGenerated(payload, options);
      setStatus(refs.encodeStatus, "已生成，可下载或直接验证。", "success");
    } catch (error) {
      if (error.name === "AbortError" || sequence !== state.encodeSequence) return;
      setStatus(refs.encodeStatus, error.message || "生成失败。", "error");
      refs.encodeEmpty.hidden = false;
    } finally {
      if (sequence === state.encodeSequence) {
        setBusy(refs.encodeRun, false);
        setLoading(refs.encodeLoading, false);
      }
    }
  }

  function debounce(callback, delay) {
    let timer = null;
    const wrapped = (...args) => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { timer = null; callback(...args); }, delay);
    };
    wrapped.cancel = () => { if (timer) clearTimeout(timer); timer = null; };
    return wrapped;
  }

  const delayedEncode = debounce(() => runEncode(), PREVIEW_DELAY);
  function invalidateEncoding() {
    state.encodeSequence += 1;
    state.encodeController?.abort();
    setBusy(refs.encodeRun, false);
    setLoading(refs.encodeLoading, false);
    clearGenerated();
  }
  function scheduleEncode() {
    invalidateEncoding();
    delayedEncode();
  }
  scheduleEncode.cancel = delayedEncode.cancel;

  function handleTextInput() {
    validateText(true);
    scheduleEncode();
  }

  function handleColorInput(value) {
    const hex = normalizedHex(value);
    if (hex) {
      refs.foregroundHex.value = hex;
      refs.foregroundColor.value = hex;
    }
    validateColor(true);
    scheduleEncode();
  }

  function readLogo(file) {
    const logoSequence = ++state.logoSequence;
    refs.logoError.textContent = "";
    if (!file) return;
    if (!/^image\/(png|jpeg)$/.test(file.type || "")) {
      refs.logoError.textContent = "Logo 只支持 PNG 或 JPEG。";
      return;
    }
    if (file.size > 2 * 1024 * 1024) {
      refs.logoError.textContent = "Logo 不能超过 2 MiB。";
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (logoSequence !== state.logoSequence) return;
      const dataUrl = String(reader.result || "");
      const comma = dataUrl.indexOf(",");
      if (comma < 0) {
        refs.logoError.textContent = "Logo 无法读取。";
        return;
      }
      state.logoBase64 = dataUrl.slice(comma + 1);
      state.logoUrl = replaceObjectUrl(state.logoUrl, file);
      refs.logoImage.src = state.logoUrl;
      refs.logoName.textContent = file.name;
      refs.logoSize.textContent = `${Math.round(file.size / 1024)} KiB`;
      refs.logoEmpty.hidden = true;
      refs.logoPreview.hidden = false;
      scheduleEncode();
    };
    reader.onerror = () => { refs.logoError.textContent = "Logo 无法读取。"; };
    reader.readAsDataURL(file);
  }

  function clearLogo() {
    state.logoSequence += 1;
    state.logoBase64 = null;
    state.logoUrl = replaceObjectUrl(state.logoUrl, null);
    refs.logoImage.removeAttribute("src");
    refs.logoName.textContent = "";
    refs.logoSize.textContent = "";
    refs.logoInput.value = "";
    refs.logoEmpty.hidden = false;
    refs.logoPreview.hidden = true;
    refs.logoError.textContent = "";
    scheduleEncode();
  }

  function clearEncodeForm() {
    scheduleEncode.cancel();
    invalidateEncoding();
    refs.encodeText.value = "";
    refs.foregroundHex.value = "#173B32";
    refs.foregroundColor.value = "#173b32";
    refs.encodeSize.value = "768";
    refs.spokes.value = "72";
    refs.ecc.value = "M";
    updateProfile();
    document.querySelector('input[name="style"][value="lines"]').checked = true;
    clearLogo();
    scheduleEncode.cancel();
    validateText(false);
    validateColor(false);
    updateSize();
    clearGenerated();
    setLoading(refs.encodeLoading, false);
    setStatus(refs.encodeStatus, "");
  }

  function setDecodeStatePill(text, kind = "") {
    refs.decodeStatePill.textContent = text;
    refs.decodeStatePill.classList.toggle("is-success", kind === "success");
    refs.decodeStatePill.classList.toggle("is-error", kind === "error");
  }

  function renderLandmarks(result) {
    const landmarks = Array.isArray(result.landmarks) ? result.landmarks.filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y)) : [];
    refs.decodeOverlay.replaceChildren();
    if (!landmarks.length || !result.width || !result.height) {
      refs.decodeOverlay.toggleAttribute("hidden", true);
      return;
    }
    refs.decodeOverlay.setAttribute("viewBox", `0 0 ${result.width} ${result.height}`);
    const radius = Math.max(4, Math.min(result.width, result.height) * 0.016);
    landmarks.forEach((point) => {
      const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      circle.setAttribute("cx", String(point.x));
      circle.setAttribute("cy", String(point.y));
      circle.setAttribute("r", String(radius));
      refs.decodeOverlay.appendChild(circle);
    });
    if (landmarks.length >= 2) {
      for (let i = 0; i < landmarks.length; i += 1) {
        const from = landmarks[i];
        const to = landmarks[(i + 1) % landmarks.length];
        const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
        line.setAttribute("x1", String(from.x));
        line.setAttribute("y1", String(from.y));
        line.setAttribute("x2", String(to.x));
        line.setAttribute("y2", String(to.y));
        refs.decodeOverlay.appendChild(line);
      }
    }
    refs.decodeOverlay.toggleAttribute("hidden", !refs.landmarkToggle.checked);
  }

  function getDecodedContent(result) {
    const isText = typeof result.text === "string";
    return { isText, content: isText ? result.text : String(result.payload_base64 || "") };
  }

  function renderDecodedResult(result, target = "decode") {
    const content = getDecodedContent(result);
    if (target === "decode") {
      refs.decodeResult.hidden = false;
      const p = result.profile;
      $("decode-profile").textContent = p ? `${p.spokes} 线 · ${p.ecc} 档 · 可纠正 ${p.correctable_bytes} 个码字节` : "旧版 v1 · 96 线";
      refs.decodeContentKind.textContent = content.isText ? "UTF-8 文本" : "二进制 · Base64";
      refs.decodeContent.textContent = content.content;
      refs.decodeBytes.textContent = String(result.byte_length ?? "—");
      refs.decodeCorrected.textContent = String(result.corrected_bytes ?? 0);
      refs.decodeDimensions.textContent = `${result.width || "—"} × ${result.height || "—"}`;
    } else {
      refs.labResult.hidden = false;
      refs.labContentKind.textContent = content.isText ? "UTF-8 文本" : "二进制 · Base64";
      refs.labContent.textContent = content.content;
      refs.labBytes.textContent = String(result.byte_length ?? "—");
      refs.labCorrected.textContent = String(result.corrected_bytes ?? 0);
      refs.labDimensions.textContent = `${result.width || "—"} × ${result.height || "—"}`;
    }
    return content;
  }

  async function decodeBlob(blob, signal) {
    return requestJson("/api/decode", { method: "POST", headers: { "Content-Type": blob.type || "image/png" }, body: blob, signal });
  }

  async function setDecodeFile(blob, name, autoDecode = true) {
    clearDecode();
    const fileSequence = ++state.decode.fileSequence;
    try {
      const dimensions = await inspectImage(blob);
      if (fileSequence !== state.decode.fileSequence) return;
      state.decode.blob = blob;
      state.decode.name = name || "image.png";
      state.decode.url = replaceObjectUrl(state.decode.url, blob);
      refs.decodeImage.src = state.decode.url;
      refs.decodeImage.hidden = false;
      refs.decodeEmpty.hidden = true;
      refs.decodeFileName.textContent = `${state.decode.name} · ${dimensions.width}×${dimensions.height}`;
      refs.decodeFileInfo.hidden = false;
      refs.decodeRun.disabled = false;
      refs.decodeInputError.textContent = "";
      refs.decodeResult.hidden = true;
      refs.decodeOverlay.toggleAttribute("hidden", true);
      setDecodeStatePill("待解码");
      setStatus(refs.decodeStatus, "");
      updateLabSources();
      if (autoDecode) await runDecode();
    } catch (error) {
      if (fileSequence !== state.decode.fileSequence) return;
      refs.decodeInputError.textContent = error.message || "无法使用这张图片。";
      setStatus(refs.decodeStatus, "");
      refs.decodeRun.disabled = !state.decode.blob;
    }
  }

  async function runDecode() {
    if (!state.decode.blob) return;
    if (state.decode.controller) state.decode.controller.abort();
    const controller = new AbortController();
    state.decode.controller = controller;
    const sequence = ++state.decode.sequence;
    setBusy(refs.decodeRun, true);
    setLoading(refs.decodeLoading, true);
    state.decode.result = null;
    refs.decodeOverlay.toggleAttribute("hidden", true);
    refs.decodeResult.hidden = true;
    setDecodeStatePill("识别中");
    setStatus(refs.decodeStatus, "正在读取图像…");
    try {
      const result = await decodeBlob(state.decode.blob, controller.signal);
      if (sequence !== state.decode.sequence) return;
      state.decode.result = result;
      const content = renderDecodedResult(result, "decode");
      state.decode.displayContent = content.content;
      renderLandmarks(result);
      refs.decodeCopy.disabled = false;
      setDecodeStatePill("识别成功", "success");
      setStatus(refs.decodeStatus, result.corrected_bytes ? `已识别，纠正 ${result.corrected_bytes} 个字节。` : "已识别，未发现需要纠正的字节。", "success");
    } catch (error) {
      if (error.name === "AbortError" || sequence !== state.decode.sequence) return;
      refs.decodeResult.hidden = true;
      refs.decodeOverlay.toggleAttribute("hidden", true);
      setDecodeStatePill("识别失败", "error");
      setStatus(refs.decodeStatus, error.message || "无法识别这张图片。", "error");
    } finally {
      if (sequence === state.decode.sequence) {
        setBusy(refs.decodeRun, false);
        setLoading(refs.decodeLoading, false);
      }
    }
  }

  function clearDecode() {
    if (state.decode.controller) state.decode.controller.abort();
    state.decode.fileSequence += 1;
    state.decode.sequence += 1;
    setBusy(refs.decodeRun, false);
    setLoading(refs.decodeLoading, false);
    state.decode.blob = null;
    state.decode.url = replaceObjectUrl(state.decode.url, null);
    state.decode.result = null;
    state.decode.displayContent = "";
    refs.decodeInput.value = "";
    refs.decodeImage.removeAttribute("src");
    refs.decodeImage.hidden = true;
    refs.decodeEmpty.hidden = false;
    refs.decodeFileInfo.hidden = true;
    refs.decodeResult.hidden = true;
    refs.decodeOverlay.toggleAttribute("hidden", true);
    refs.decodeRun.disabled = true;
    refs.decodeCopy.disabled = true;
    refs.decodeInputError.textContent = "";
    setDecodeStatePill("等待图片");
    setStatus(refs.decodeStatus, "");
    updateLabSources();
  }

  async function receiveDecodeFile(file) {
    if (!file) return;
    await setDecodeFile(file, file.name, true);
  }

  function activeTab(name) {
    return state.activePanel === name;
  }

  function switchPanel(name, updateHash = true) {
    if (!["encode", "decode", "lab"].includes(name)) name = "encode";
    state.activePanel = name;
    refs.tabs.forEach((tab) => {
      const selected = tab.dataset.panel === name;
      tab.classList.toggle("is-active", selected);
      tab.setAttribute("aria-selected", String(selected));
      tab.tabIndex = selected ? 0 : -1;
    });
    refs.panels.forEach((panel) => { panel.hidden = panel.id !== `panel-${name}`; panel.classList.toggle("is-active", panel.id === `panel-${name}`); });
    if (updateHash) history.replaceState(null, "", `#${name}`);
    if (name === "lab") updateLabSources();
  }

  function updateLabSources() {
    const previousSource = state.lab.source;
    const hasEncode = Boolean(state.generated && state.generated.pngBlob);
    const hasDecode = Boolean(state.decode.blob);
    refs.labUseEncode.disabled = !hasEncode;
    refs.labUseDecode.disabled = !hasDecode;
    if (state.lab.sourceKind === "encode" && hasEncode) {
      state.lab.source = state.generated.pngBlob;
      refs.labSourceName.textContent = "当前编码图 · PNG";
    } else if (state.lab.sourceKind === "decode" && hasDecode) {
      state.lab.source = state.decode.blob;
      refs.labSourceName.textContent = state.decode.name;
    } else if (hasEncode) {
      state.lab.sourceKind = "encode";
      state.lab.source = state.generated.pngBlob;
      refs.labSourceName.textContent = "当前编码图 · PNG";
    } else if (hasDecode) {
      state.lab.sourceKind = "decode";
      state.lab.source = state.decode.blob;
      refs.labSourceName.textContent = state.decode.name;
    } else {
      state.lab.source = null;
      refs.labSourceName.textContent = "先在编码页生成图片，或在解码页选择图片";
    }
    refs.labUseEncode.classList.toggle("is-active", state.lab.sourceKind === "encode" && hasEncode);
    refs.labUseDecode.classList.toggle("is-active", state.lab.sourceKind === "decode" && hasDecode);
    if (previousSource !== state.lab.source) {
      state.lab.sequence += 1;
      state.lab.controller?.abort();
      state.lab.result = null;
      state.lab.displayContent = "";
      refs.labResult.hidden = true;
      refs.labImage.hidden = true;
      refs.labEmpty.hidden = false;
      setBusy(refs.labRun, false);
      setLoading(refs.labLoading, false);
      refs.labResultPill.textContent = "尚未运行";
      refs.labResultPill.classList.remove("is-success", "is-error");
      setStatus(refs.labStatus, "");
    }
    refs.labRun.disabled = !state.lab.source;
  }

  function seededRandom(seed) {
    let value = seed >>> 0;
    return () => {
      value = (1664525 * value + 1013904223) >>> 0;
      return value / 4294967296;
    };
  }

  async function transformImage(blob, rotation, scale, noise) {
    const image = await loadImage(blob);
    const sourceWidth = image.naturalWidth;
    const sourceHeight = image.naturalHeight;
    const scaledWidth = sourceWidth * scale;
    const scaledHeight = sourceHeight * scale;
    const radians = (rotation * Math.PI) / 180;
    const cosine = Math.abs(Math.cos(radians));
    const sine = Math.abs(Math.sin(radians));
    const padding = Math.max(24, Math.ceil(Math.max(scaledWidth, scaledHeight) * 0.06));
    const rawWidth = Math.ceil(scaledWidth * cosine + scaledHeight * sine) + padding * 2;
    const rawHeight = Math.ceil(scaledWidth * sine + scaledHeight * cosine) + padding * 2;
    // Keep the whole transformed image inside the decoder's 16MP request bound.
    const fit = rawWidth * rawHeight > MAX_PIXELS ? Math.sqrt(MAX_PIXELS / (rawWidth * rawHeight)) : 1;
    const width = Math.max(1, Math.floor(rawWidth * fit));
    const height = Math.max(1, Math.floor(rawHeight * fit));
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d", { willReadFrequently: noise > 0 });
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, width, height);
    context.save();
    context.translate(width / 2, height / 2);
    context.scale(fit, fit);
    context.rotate(radians);
    context.drawImage(image, -scaledWidth / 2, -scaledHeight / 2, scaledWidth, scaledHeight);
    context.restore();
    if (noise > 0) {
      const pixels = context.getImageData(0, 0, width, height);
      const random = seededRandom(0x53c0de ^ Math.round(rotation * 97) ^ Math.round(scale * 1000) ^ Math.round(noise * 100));
      for (let index = 0; index < pixels.data.length; index += 4) {
        const gray = pixels.data[index] * 0.2126 + pixels.data[index + 1] * 0.7152 + pixels.data[index + 2] * 0.0722;
        const delta = (random() * 2 - 1) * noise;
        const channel = Math.max(0, Math.min(255, Math.round(gray + delta)));
        pixels.data[index] = channel;
        pixels.data[index + 1] = channel;
        pixels.data[index + 2] = channel;
      }
      context.putImageData(pixels, 0, 0);
    }
    const output = await new Promise((resolve, reject) => canvas.toBlob((result) => result ? resolve(result) : reject(new ApiError("无法生成实验图片。")), "image/png"));
    return { canvas, blob: output, width, height, fit };
  }

  async function runLab() {
    if (!state.lab.source) return;
    if (state.lab.controller) state.lab.controller.abort();
    const controller = new AbortController();
    state.lab.controller = controller;
    const sequence = ++state.lab.sequence;
    const rotation = Number(refs.labRotation.value);
    const scale = Number(refs.labScale.value);
    const noise = Number(refs.labNoise.value);
    setBusy(refs.labRun, true);
    refs.labEmpty.hidden = true;
    setLoading(refs.labLoading, true);
    setStatus(refs.labStatus, "正在变换像素并解码…");
    refs.labResult.hidden = true;
    refs.labResultPill.textContent = "实验中";
    refs.labResultPill.classList.remove("is-success", "is-error");
    try {
      const transformed = await transformImage(state.lab.source, rotation, scale, noise);
      if (sequence !== state.lab.sequence) return;
      state.lab.url = replaceObjectUrl(state.lab.url, transformed.blob);
      refs.labImage.src = state.lab.url;
      refs.labImage.hidden = false;
      const result = await decodeBlob(transformed.blob, controller.signal);
      if (sequence !== state.lab.sequence) return;
      state.lab.result = result;
      const content = renderDecodedResult(result, "lab");
      state.lab.displayContent = content.content;
      refs.labCopy.disabled = false;
      refs.labResultPill.textContent = "识别成功";
      refs.labResultPill.classList.add("is-success");
      const fitNote = transformed.fit < 1 ? " 已按像素上限缩小画布但保留完整图像。" : "";
      setStatus(refs.labStatus, `${result.corrected_bytes ? `已识别并纠正 ${result.corrected_bytes} 个字节。` : "已识别。"}${fitNote}`, "success");
    } catch (error) {
      if (error.name === "AbortError" || sequence !== state.lab.sequence) return;
      refs.labResult.hidden = true;
      refs.labResultPill.textContent = "识别失败";
      refs.labResultPill.classList.add("is-error");
      setStatus(refs.labStatus, error.message || "实验图片无法识别。", "error");
    } finally {
      if (sequence === state.lab.sequence) {
        setBusy(refs.labRun, false);
        setLoading(refs.labLoading, false);
      }
    }
  }

  async function copyText(value, statusNode) {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      setStatus(statusNode, "已复制到剪贴板。", "success");
    } catch {
      setStatus(statusNode, "浏览器未允许访问剪贴板，请手动选择内容复制。", "error");
    }
  }

  function updateProfile() {
    const p = profiles.find((p) => p.spokes === Number(refs.spokes.value) && p.ecc === refs.ecc.value);
    if (!p) return;
    MAX_BYTES = p.capacity;
    refs.profileInfo.textContent = `${p.spokes} 线 ${p.ecc} 档 · 最多 ${p.capacity} 字节 · 可纠正 ${p.correctable_bytes} 个码字节（目标 ${p.correction_percent}%）。此比例不是图片可遮挡面积。`;
    validateText(true);
  }
  [refs.spokes, refs.ecc].forEach((control) => control.addEventListener("change", () => { updateProfile(); scheduleEncode(); }));
  requestJson("/api/profiles").then((result) => {
    profiles = result;
    updateProfile();
    if (refs.encodeText.value) scheduleEncode();
  }).catch((error) => setStatus(refs.encodeStatus, error.message, "error"));

  refs.encodeText.addEventListener("input", handleTextInput);
  refs.foregroundColor.addEventListener("input", (event) => handleColorInput(event.target.value));
  refs.foregroundHex.addEventListener("input", (event) => {
    const hex = normalizedHex(event.target.value);
    if (hex) refs.foregroundColor.value = hex;
    validateColor(true);
    scheduleEncode();
  });
  refs.foregroundHex.addEventListener("blur", () => {
    const hex = normalizedHex(refs.foregroundHex.value);
    if (hex) refs.foregroundHex.value = hex;
  });
  refs.encodeSize.addEventListener("input", () => { updateSize(); scheduleEncode(); });
  document.querySelectorAll('input[name="style"]').forEach((input) => input.addEventListener("change", () => scheduleEncode()));
  refs.encodeForm.addEventListener("submit", (event) => { scheduleEncode.cancel(); runEncode(event); });
  refs.encodeClear.addEventListener("click", clearEncodeForm);
  refs.previewTabs.forEach((button) => button.addEventListener("click", () => setPreviewKind(button.dataset.previewKind)));
  refs.logoDrop.addEventListener("click", (event) => { if (event.target !== refs.logoRemove) refs.logoInput.click(); });
  refs.logoDrop.setAttribute("role", "button");
  refs.logoDrop.tabIndex = 0;
  refs.logoDrop.setAttribute("aria-label", "选择中心 Logo");
  refs.logoDrop.addEventListener("keydown", (event) => {
    if (event.target === refs.logoDrop && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault(); refs.logoInput.click();
    }
  });
  refs.logoInput.addEventListener("change", () => readLogo(refs.logoInput.files[0]));
  refs.logoRemove.addEventListener("click", (event) => { event.stopPropagation(); clearLogo(); });
  ["dragenter", "dragover"].forEach((type) => refs.logoDrop.addEventListener(type, (event) => { event.preventDefault(); refs.logoDrop.classList.add("is-over"); }));
  ["dragleave", "drop"].forEach((type) => refs.logoDrop.addEventListener(type, (event) => { event.preventDefault(); refs.logoDrop.classList.remove("is-over"); }));
  refs.logoDrop.addEventListener("drop", (event) => readLogo(event.dataTransfer.files[0]));

  refs.decodeDrop.addEventListener("click", () => refs.decodeInput.click());
  refs.decodeDrop.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); refs.decodeInput.click(); } });
  refs.decodeInput.addEventListener("change", () => receiveDecodeFile(refs.decodeInput.files[0]));
  refs.decodeClear.addEventListener("click", clearDecode);
  refs.decodeRun.addEventListener("click", runDecode);
  refs.landmarkToggle.addEventListener("change", () => { refs.decodeOverlay.toggleAttribute("hidden", !refs.landmarkToggle.checked || !state.decode.result); });
  ["dragenter", "dragover"].forEach((type) => refs.decodeDrop.addEventListener(type, (event) => { event.preventDefault(); refs.decodeDrop.classList.add("is-over"); }));
  ["dragleave", "drop"].forEach((type) => refs.decodeDrop.addEventListener(type, (event) => { event.preventDefault(); refs.decodeDrop.classList.remove("is-over"); }));
  refs.decodeDrop.addEventListener("drop", (event) => receiveDecodeFile(event.dataTransfer.files[0]));
  document.addEventListener("paste", (event) => {
    if (!activeTab("decode")) return;
    const imageItem = [...(event.clipboardData?.items || [])].find((item) => item.type.startsWith("image/"));
    if (imageItem) { event.preventDefault(); receiveDecodeFile(imageItem.getAsFile()); }
  });

  refs.verifyCurrent.addEventListener("click", async () => {
    if (!state.generated?.pngBlob) return;
    switchPanel("decode");
    await setDecodeFile(state.generated.pngBlob, "当前编码图.png", true);
  });
  refs.decodeCopy.addEventListener("click", () => copyText(state.decode.displayContent, refs.decodeStatus));

  refs.labUseEncode.addEventListener("click", () => { state.lab.sourceKind = "encode"; updateLabSources(); });
  refs.labUseDecode.addEventListener("click", () => { state.lab.sourceKind = "decode"; updateLabSources(); });
  refs.labRotation.addEventListener("input", () => { refs.rotationOutput.textContent = `${refs.labRotation.value}°`; });
  refs.labScale.addEventListener("input", () => { refs.scaleOutput.textContent = `${Math.round(Number(refs.labScale.value) * 100)}%`; });
  refs.labNoise.addEventListener("input", () => { refs.noiseOutput.textContent = `${Number(refs.labNoise.value).toFixed(1).replace(/\.0$/, "")} / 8`; });
  refs.labRun.addEventListener("click", runLab);
  refs.labCopy.addEventListener("click", () => copyText(state.lab.displayContent, refs.labStatus));

  refs.tabs.forEach((tab, index) => tab.addEventListener("click", () => switchPanel(tab.dataset.panel)) || tab.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    refs.tabs[(index + direction + refs.tabs.length) % refs.tabs.length].focus();
  }));

  validateText(false);
  validateColor(false);
  updateSize();
  refs.decodeCopy.disabled = true;
  refs.labCopy.disabled = true;
  const initialPanel = location.hash.slice(1);
  switchPanel(["encode", "decode", "lab"].includes(initialPanel) ? initialPanel : "encode", false);
})();
