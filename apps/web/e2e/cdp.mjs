// Driver CDP mínimo para Chromium headless (Node 22: WebSocket nativo, sin
// puppeteer). Lo usan las pruebas de punta a punta de e2e/.
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";

// Capturas y perfil de Chromium
const OUT = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
export async function launch(url, { width = 1400, height = 900 } = {}) {
  mkdirSync(OUT, { recursive: true });
  const chrome = spawn("chromium", [
    "--headless=new", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--remote-debugging-port=9333",
    `--window-size=${width},${height}`, `--user-data-dir=${OUT}/chrome`, "--no-first-run", "about:blank",
  ], { stdio: "ignore" });
  let targets;
  for (let i = 0; i < 50; i++) {
    try { targets = await (await fetch("http://127.0.0.1:9333/json")).json(); if (targets.length) break; } catch {}
    await sleep(200);
  }
  const page = targets.find((t) => t.type === "page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pending = new Map();
  const logs = [];
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
    if (msg.method === "Runtime.consoleAPICalled") logs.push(msg.params.type + ": " + msg.params.args.map((a) => a.value ?? a.description).join(" "));
    if (msg.method === "Runtime.exceptionThrown") logs.push("EXCEPTION: " + (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text));
  };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send("Runtime.enable");
  await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url });
  await sleep(3000);
  // Vite puede recargar la página al reoptimizar dependencias: esperar a que
  // una marca sobreviva un par de segundos
  for (let i = 0; i < 20; i++) {
    const mark = await send("Runtime.evaluate", { expression: "window.__stable = (window.__stable ?? 0) + 1", returnByValue: true });
    await sleep(1500);
    const again = await send("Runtime.evaluate", { expression: "window.__stable", returnByValue: true });
    if (again.result?.result?.value === mark.result?.result?.value) break;
  }
  const api = {
    send, logs,
    async eval(expr) {
      const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? JSON.stringify(r.result.exceptionDetails));
      return r.result?.result?.value;
    },
    async mouse(type, x, y, opts = {}) {
      await send("Input.dispatchMouseEvent", { type, x, y, button: opts.button ?? "left", buttons: opts.buttons ?? (type === "mouseReleased" ? 0 : 1), clickCount: 1, modifiers: opts.modifiers ?? 0 });
    },
    async click(x, y, opts = {}) {
      await api.mouse("mouseMoved", x, y, { buttons: 0 });
      await api.mouse("mousePressed", x, y, opts);
      await api.mouse("mouseReleased", x, y, opts);
      await sleep(opts.wait ?? 300);
    },
    async drag(x0, y0, x1, y1, steps = 10) {
      await api.mouse("mouseMoved", x0, y0, { buttons: 0 });
      await api.mouse("mousePressed", x0, y0);
      for (let i = 1; i <= steps; i++) { await api.mouse("mouseMoved", x0 + ((x1 - x0) * i) / steps, y0 + ((y1 - y0) * i) / steps, { buttons: 1 }); await sleep(60); }
      await api.mouse("mouseReleased", x1, y1);
      await sleep(400);
    },
    async key(key, code = key, keyCode = 0) {
      await send("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode: keyCode });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: keyCode });
      await sleep(200);
    },
    /** Clic en el elemento cuyo texto visible coincide (button/a/[role]) */
    async clickText(text, nth = 0) {
      const r = await api.eval(`(() => {
        const els = [...document.querySelectorAll("button, [role=tab], [role=option], label, a")].filter((e) => e.textContent.trim() === ${JSON.stringify(text)} && e.offsetParent !== null);
        const e = els[${nth}]; if (!e) return null; e.scrollIntoView({ block: "center" }); const b = e.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2];
      })()`);
      if (!r) throw new Error("no encontré: " + text);
      await api.click(r[0], r[1]);
    },
    async clickContains(text) {
      const r = await api.eval(`(() => {
        const els = [...document.querySelectorAll("button")].filter((e) => e.textContent.includes(${JSON.stringify(text)}) && e.offsetParent !== null);
        const e = els[0]; if (!e) return null; e.scrollIntoView({ block: "center" }); const b = e.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2];
      })()`);
      if (!r) throw new Error("no encontré: " + text);
      await api.click(r[0], r[1]);
    },
    async shot(name) {
      const r = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.result.data, "base64"));
    },
    close() { ws.close(); chrome.kill(); },
  };
  return api;
}
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
