// Run after build-web.sh with Playwright and Chromium installed.
const assert = require("node:assert/strict");
const http = require("node:http");
const { readFile } = require("node:fs/promises");
const path = require("node:path");
const { chromium } = require("playwright");

(async () => {
  const root = path.resolve(__dirname, "../dist/web");
  const server = http.createServer(async (request, response) => {
    const url = new URL(request.url, "http://localhost");
    const file = path.resolve(root, url.pathname.slice("/rcode/".length) || "index.html");
    if (!url.pathname.startsWith("/rcode/") || !file.startsWith(`${root}/`)) {
      response.writeHead(404).end();
      return;
    }
    try {
      const body = await readFile(file);
      const type = { ".wasm": "application/wasm", ".js": "text/javascript", ".css": "text/css", ".html": "text/html" }[path.extname(file)];
      response.writeHead(200, { "Content-Type": type || "application/octet-stream" }).end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  let browser;
  try {
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    page.setDefaultTimeout(15_000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      if (request.url().includes("/api/")) errors.push(`Unexpected API request: ${request.url()}`);
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/rcode/`);
    await page.waitForFunction(() => document.querySelector("#profile-info").textContent.includes("最多 108 字节"));
    console.log("WASM loaded under /rcode/.");
    const profiles = await page.evaluate(() => window.rcodeRequest("/api/profiles"));
    assert.equal(profiles.length, 12);
    assert.deepEqual(profiles.map((p) => p.capacity), [57, 47, 31, 13, 92, 78, 52, 26, 124, 108, 74, 38]);

    const text = "你好，RCode 👋";
    await page.locator("#encode-text").fill(text);
    for (const style of ["lines", "dots", "rings"]) {
      await page.locator("#tab-encode").click();
      await page.locator(`input[name=style][value=${style}]`).check({ force: true });
      await page.locator("#encode-run").click();
      await page.waitForFunction(() => document.querySelector("#encode-status").classList.contains("is-success"));
      for (const format of ["png", "svg"]) {
        const blob = await page.locator(`#download-${format}`).evaluate(async (anchor) => {
          const response = await fetch(anchor.href);
          return { type: response.headers.get("content-type"), size: (await response.blob()).size };
        });
        assert(blob.size > 100);
        assert.match(blob.type, format === "png" ? /image\/png/ : /image\/svg\+xml/);
      }
      await page.locator("#verify-current").click();
      await page.waitForFunction(() => document.querySelector("#decode-status").matches(".is-success, .is-error"));
      assert(await page.locator("#decode-status").evaluate((node) => node.classList.contains("is-success")), await page.locator("#decode-status").textContent());
      assert.equal(await page.locator("#decode-content").textContent(), text);
      assert.equal(await page.locator("#landmark-overlay circle").count(), 4);
      console.log(`${style}: export and UTF-8 decode passed.`);
    }
    await page.locator("#tab-lab").click();
    await page.locator("#lab-run").click();
    await page.waitForFunction(() => document.querySelector("#lab-result-pill").textContent === "识别成功");
    assert.equal(await page.locator("#lab-content").textContent(), text);

    const jpeg = await page.evaluate(async () => {
      const image = await createImageBitmap(await (await fetch(document.querySelector("#download-png").href)).blob());
      const canvas = new OffscreenCanvas(image.width, image.height);
      canvas.getContext("2d").drawImage(image, 0, 0);
      image.close();
      return window.rcodeRequest("/api/decode", { body: await canvas.convertToBlob({ type: "image/jpeg", quality: 0.95 }) });
    });
    assert.equal(jpeg.text, text);
    const invalid = await page.evaluate(async () => {
      try {
        await window.rcodeRequest("/api/decode", { body: new Blob(["broken"], { type: "image/png" }) });
        return false;
      } catch { return true; }
    });
    assert(invalid);
    assert.equal((await page.evaluate(() => window.rcodeRequest("/api/profiles"))).length, 12);

    const aborted = await page.evaluate(async () => {
      const controller = new AbortController();
      const request = window.rcodeRequest("/api/profiles", { signal: controller.signal });
      controller.abort();
      try { await request; return false; } catch (error) { return error.name === "AbortError"; }
    });
    assert(aborted);
    assert.deepEqual(errors, []);
    console.log("Static Web smoke test passed: repository subpath, 12 profiles, 3 styles, PNG/SVG, UTF-8 decode, landmarks, lab, JPEG, invalid image recovery, cancellation; no backend requests.");
  } finally {
    if (browser) await browser.close();
    await new Promise((resolve) => server.close(resolve));
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
