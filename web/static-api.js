// Loaded only by the static build; the local Rust workbench keeps its HTTP API.
(() => {
  const worker = new Worker(new URL("./wasm-worker.js", document.currentScript.src), { type: "module" });
  const pending = new Map();
  let sequence = 0;
  let failure;

  function fail(message) {
    failure = new Error(message);
    for (const request of pending.values()) request.reject(failure);
    pending.clear();
    worker.terminate();
  }

  worker.onerror = () => fail("WASM 核心加载失败，请刷新页面重试。");
  worker.onmessage = ({ data }) => {
    if (data.id === null) return fail(data.error);
    const request = pending.get(data.id);
    if (!request) return;
    pending.delete(data.id);
    if (data.error) request.reject(new Error(data.error));
    else request.resolve(data.result);
  };

  window.rcodeRequest = async (path, { body, signal } = {}) => {
    if (failure) throw failure;
    signal?.throwIfAborted();
    const id = ++sequence;
    let abort;
    try {
      return await new Promise((resolve, reject) => {
        abort = () => {
          pending.delete(id);
          reject(new DOMException("Request aborted", "AbortError"));
        };
        signal?.addEventListener("abort", abort, { once: true });
        pending.set(id, { resolve, reject });
        worker.postMessage({ id, path, body });
      });
    } finally {
      pending.delete(id);
      signal?.removeEventListener("abort", abort);
    }
  };
})();
