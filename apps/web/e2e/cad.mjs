// Pruebas de punta a punta del espacio Diseñar con el backend CAD real, sin
// Tauri: el arnés (cad-harness.html) manda los comandos al puente HTTP.
//
//   cargo run -p pinocchio-app --example cad_http        # backend en :8766
//   (cd apps/web && npx vite --port 5173)                 # frontend
//   node apps/web/e2e/cad.mjs [filtro]                    # Chromium headless
//
// Capturas en $E2E_OUT (por defecto /tmp/pinocchio-e2e). Cada escenario
// verifica volúmenes contra el valor teórico.

import { existsSync, readFileSync, rmSync, statSync } from "node:fs";
import { launch, sleep } from "./cdp.mjs";

const URL = "http://localhost:5173/e2e/cad-harness.html";
const BRIDGE = "http://127.0.0.1:8766/invoke/";
const call = (cmd, args = {}) => fetch(BRIDGE + cmd, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());
const SPECULUM = process.env.E2E_STL ?? `${process.env.HOME}/Descargas/macho_especulo.stl`;

function near(actual, expected, tol, what) {
  if (!(Math.abs(actual - expected) <= tol)) throw new Error(`${what}: ${actual} (esperado ${expected} ± ${tol})`);
}

async function begin(b) {
  await b.clickText("Diseñar");
  // El diseño vacío se crea solo al entrar
  for (let t = 0; t < 40 && !(await b.eval(`!!window.__cadStore.doc()`)); t++) await sleep(250);
  await sleep(500);
}

const evaluate = () => call("cad_evaluate");

/** Sketch nuevo en la planta: botón "Sketch" y clic en el plano base */
async function sketchOn(b) {
  await b.clickText("Sketch");
  await sleep(400);
  // Un punto de la planta delante-derecha que no tape el sólido ni otro plano
  // (el visor dice qué hay bajo cada punto)
  const s = await b.eval(`window.__cadViewer.planeSize`);
  // Primero el de siempre; si el sólido lo tapa, más cerca del borde del plano
  const tries = [[0.4, -0.4], [0.47, -0.47], [-0.47, -0.47], [0.47, 0.47], [-0.47, 0.47], [0.47, 0], [0, -0.47]];
  for (const [u, v] of tries) {
    const [x, y] = await b.eval(`window.__cadViewer.screenOf([${u * s}, ${v * s}, 0])`);
    const hit = await b.eval(`(() => { const h = window.__cadViewer.pick(${x}, ${y}, { faces: true, planes: true }); return h && (h.kind === "plane" ? h.plane : h.kind); })()`);
    if (hit === "xy") return void (await b.click(x, y, { wait: 1500 }));
  }
  throw new Error("no encontré un punto libre de la planta");
}
const body = async () => (await evaluate()).body;
/** Acepta el diálogo de la operación recién creada (✓) */
async function accept(b, wait = 1500) {
  await b.eval(`document.querySelector('[aria-label="Aceptar"]')?.click()`);
  await sleep(wait);
}
/** Clic en una fila del árbol de operaciones por su nombre */
async function clickRow(b, name) {
  const r = await b.eval(`(() => {
    const e = [...document.querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(name)} && x.offsetParent !== null));
    if (!e) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2];
  })()`);
  if (!r) throw new Error("no encontré la operación " + name);
  await b.click(r[0], r[1], { wait: 1200 });
}
const sketchText = (b, re) => b.eval(`(document.body.innerText.match(${re}) ?? [""])[0]`);
const setInput = (b, labelStart, value) =>
  b.eval(`(() => {
    const i = [...document.querySelectorAll("label")].find((l) => l.textContent.startsWith(${JSON.stringify(labelStart)}))?.querySelector("input");
    if (!i) return false;
    i.value = ${JSON.stringify(String(value))};
    i.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  })()`);

const scenarios = {
  async "caja, sketch dibujado y extrusión"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const r = await evaluate();
    if (r.status.some((s) => s.state !== "ok")) throw new Error(JSON.stringify(r.status));
    if (!r.body.valid) throw new Error("sólido inválido");
  },

  async "redondeo eligiendo aristas y cambio de radio"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(2000);
    await accept(b);
    // Sin aristas elegidas: el diálogo abre con la caja de aristas activa
    await b.clickText("Redondeo");
    await sleep(1500);
    for (const p of [[0, -10, 10], [10, 0, 10]]) {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
      await b.click(x, y, { wait: 1000 });
    }
    // Esc deja de elegir: la vista previa muestra el redondeo
    await b.key("Escape", "Escape", 27);
    await sleep(2000);
    // Dos redondeos a lo largo de 20 mm; comparten un vértice y el empalme de
    // esquina cambia el volumen en ~0,1 mm³ respecto a la suma de los dos
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 20, 0.2, "volumen r=1");
    await setInput(b, "Radio", 3);
    await sleep(2000);
    if ((await body()).faces !== 8) throw new Error("caras tras r=3");
  },

  async "cajas de selección: sumar, quitar y volver a elegir"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const items = () => b.eval(`[...document.querySelector('[aria-label="Aristas"]').querySelectorAll("span")].map((s) => s.textContent)`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Redondeo");
    await sleep(1500);
    const A = await at([0, -10, 10]);
    const B = await at([10, 0, 10]);
    await b.click(...A, { wait: 1200 });
    await b.click(...B, { wait: 1200 });
    if ((await items()).length !== 2) throw new Error(`ítems: ${await items()}`);
    // Clic otra vez en una elegida: se quita
    await b.click(...A, { wait: 1200 });
    if ((await items()).length !== 1) throw new Error(`tras quitar con clic: ${await items()}`);
    // ✗ en la lista
    await b.eval(`document.querySelector('[aria-label="Quitar arista 1"]').click()`);
    await sleep(1200);
    if (!(await b.eval(`document.querySelector('[aria-label="Aristas"]').textContent.includes("Clic en las aristas")`))) throw new Error("la caja no quedó vacía");
    // Con la caja activa se ve el sólido de antes: elegir la arista de adelante
    near((await body()).volume, 8000, 1e-6, "sólido de antes al elegir");
    await b.click(...A, { wait: 1200 });
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    near((await body()).volume, 8000 - (1 - Math.PI / 4) * 20, 0.05, "un redondeo");
    await b.key("Enter", "Enter", 13);
    await sleep(1500);
    const doc = await call("cad_get_document");
    if (doc.features[1]?.kind.edges.length !== 1) throw new Error("no quedó una arista");
    // Editar: la caja se activa con clic y vuelve a mostrar el sólido de antes
    await clickRow(b, "Redondeo 1");
    await sleep(1000);
    await b.eval(`document.querySelector('[aria-label="Aristas"]').click()`);
    await sleep(2000);
    near((await body()).volume, 8000, 1e-6, "editar: sólido de antes");
    await b.click(...B, { wait: 1200 });
    if ((await items()).length !== 2) throw new Error(`editar: ${await items()}`);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    await b.key("Enter", "Enter", 13);
    await sleep(2000);
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 20, 0.2, "dos redondeos");
    if ((await call("cad_get_document")).features[1].kind.edges.length !== 2) throw new Error("no quedaron dos aristas");
  },

  async "referencia perdida: advertencia, en rojo y reemplazo"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const items = () => b.eval(`[...document.querySelector('[aria-label="Aristas"]').querySelectorAll("span")].map((s) => s.textContent)`);
    const fillet = () => 2 * (1 - Math.PI / 4) * 20;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Otra caja más arriba (z 20..40, separada) y un redondeo con una arista de cada una
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    const edge = (z) => ({ point: [0, -10, z], direction: [1, 0, 0] });
    doc.features.push(
      { ...structuredClone(box), id: box.id + 1, name: "Caja arriba", kind: { ...structuredClone(box.kind), origin: [0, 0, 30] } },
      { id: box.id + 2, name: "Redondeo 1", suppressed: false, kind: { type: "fillet", edges: [edge(10), edge(40)], radius: 1 } },
    );
    doc.next_id = box.id + 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    near((await body()).volume, 16000 - fillet(), 0.2, "dos cajas, dos redondeos");
    // Sin la caja de arriba su arista ya no está: advertencia y se redondea la otra
    await b.eval(`(() => {
      const s = [...document.querySelectorAll("span")].find((x) => x.textContent === "Caja arriba"));
      s.parentElement.querySelector('[aria-label="Suprimir"]').click();
    })()`);
    await sleep(2000);
    const st = (await evaluate()).status.find((s) => s.id === box.id + 2);
    if (st.state !== "warning" || st.missing?.[0]?.field !== "edges" || st.missing[0].index !== 1) throw new Error(`estado: ${JSON.stringify(st)}`);
    near((await body()).volume, 8000 - fillet() / 2, 0.05, "queda un redondeo");
    if (!(await b.eval(`!!document.querySelector('[title*="faltan 1 de 2 aristas"]')`))) throw new Error("el árbol no avisa");
    // Al editar, la caja se activa sola y marca la perdida
    await clickRow(b, "Redondeo 1");
    await sleep(2000);
    const before = await items();
    if (!before[1]?.includes("no encontrada") || before[0].includes("no encontrada")) throw new Error(`ítems: ${before}`);
    // Elegir otra arista reemplaza a la perdida
    await b.click(...(await at([10, 0, 10])), { wait: 1500 });
    const after = await items();
    if (after.length !== 2 || after.some((t) => t.includes("no encontrada"))) throw new Error(`tras reemplazar: ${after}`);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    await b.key("Enter", "Enter", 13);
    await sleep(2000);
    const ok = (await evaluate()).status.find((s) => s.id === box.id + 2);
    if (ok.state !== "ok") throw new Error(`tras reemplazar: ${JSON.stringify(ok)}`);
    near((await body()).volume, 8000 - fillet(), 0.2, "dos redondeos en la caja de abajo");
    const edges = (await call("cad_get_document")).features[2].kind.edges;
    near(edges[1].point[0], 10, 1e-6, "la arista nueva quedó en el lugar de la perdida");
  },

  async "árbol: barra de retroceso y reordenar arrastrando"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Caja, agujero (cilindro que resta) y otra caja más arriba
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    const hole = { type: "primitive", shape: { type: "cylinder", radius: 3, height: 40 }, origin: [0, 0, -15], z: [0, 0, 1], x: [1, 0, 0], op: "cut" };
    doc.features.push(
      { id: box.id + 1, name: "Agujero", suppressed: false, kind: hole },
      { ...structuredClone(box), id: box.id + 2, name: "Caja arriba", kind: { ...structuredClone(box.kind), origin: [0, 0, 30] } },
    );
    doc.next_id = box.id + 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const holeVol = Math.PI * 9 * 20;
    near((await body()).volume, 16000 - holeVol, 0.5, "todo");
    // El árbol puede quedar abajo del panel: centrarlo antes de medir
    const rows = () =>
      b.eval(`(() => {
        const all = [...document.querySelectorAll("[data-feature-row]")];
        all[Math.floor(all.length / 2)]?.scrollIntoView({ block: "center" });
        return all.map((r) => { const b = r.getBoundingClientRect(); return [b.x + b.width / 2, b.top, b.bottom]; });
      })()`);
    /** Punto del hueco k del árbol (0 = antes de la primera fila) */
    const gap = async (k) => {
      const r = await rows();
      return k < r.length ? [r[k][0], r[k][1] + 2] : [r[r.length - 1][0], r[r.length - 1][2] + 6];
    };
    const bar = () =>
      b.eval(`(() => {
        const all = [...document.querySelectorAll("[data-feature-row]")];
        all[Math.floor(all.length / 2)]?.scrollIntoView({ block: "center" });
        const r = document.querySelector("[data-rollback-bar]").getBoundingClientRect();
        return [r.x + r.width / 2, r.y + r.height / 2];
      })()`);
    const rollback = async () => (await call("cad_get_document")).rollback ?? null;
    // La barra al final; llevarla debajo de la primera caja
    await b.drag(...(await bar()), ...(await gap(1)));
    await sleep(2000);
    if ((await rollback()) !== 1) throw new Error(`retroceso: ${await rollback()}`);
    near((await body()).volume, 8000, 1e-6, "solo la caja");
    await b.drag(...(await bar()), ...(await gap(2)));
    await sleep(2000);
    near((await body()).volume, 8000 - holeVol, 0.5, "caja con agujero");
    // De vuelta al final: se calcula todo
    await b.drag(...(await bar()), ...(await gap(3)));
    await sleep(2000);
    if ((await rollback()) !== null) throw new Error(`retroceso al final: ${await rollback()}`);
    near((await body()).volume, 16000 - holeVol, 0.5, "todo otra vez");
    // Arrastrar "Caja arriba" al principio: ahora el agujero (z −15..25) también
    // le saca 5 mm de abajo
    const r = await rows();
    await b.drag(r[2][0], (r[2][1] + r[2][2]) / 2, ...(await gap(0)));
    await sleep(2000);
    const names = (await call("cad_get_document")).features.map((f) => f.name);
    if (names.join("|") !== "Caja arriba|Caja 1|Agujero") throw new Error(`orden: ${names}`);
    near((await body()).volume, 16000 - Math.PI * 9 * 25, 0.5, "agujero en las dos cajas");
    // Arrastrar no abre el diálogo
    if (await b.eval(`!!window.__cadStore.draft()`)) throw new Error("se abrió el diálogo al arrastrar");
  },

  async "menú de la cara: editar la operación que la creó"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const [x, y] = await b.eval(`window.__cadViewer.screenOf([3, 3, 10])`);
    await b.click(x, y, { button: "right", buttons: 2, wait: 1500 });
    const label = await b.eval(`[...document.querySelectorAll("button, [role=menuitem]")].map((e) => e.textContent.trim()).find((t) => t.startsWith("Editar «"))`);
    if (label !== "Editar «Caja 1»") throw new Error(`menú: ${label}`);
    // El menú es fijo (sin offsetParent): clic por coordenadas
    const at = await b.eval(`(() => { const e = [...document.querySelectorAll("[role=menuitem]")].find((e) => e.textContent.includes("Caja 1")); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.click(...at);
    await sleep(1500);
    const id = (await call("cad_get_document")).features[0].id;
    if ((await b.eval(`window.__cadStore.draft()?.feature`)) !== id) throw new Error("no se abrió el diálogo de la caja");
  },

  async "medir: cara, vértices y dos caras"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const panel = () => b.eval(`Object.fromEntries([...document.querySelectorAll("[data-measure]")].map((e) => [e.dataset.measure, e.textContent]))`);
    const SHIFT = 8;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Cara de arriba: tipo y área
    await b.click(...(await at([3, 3, 10])), { wait: 1500 });
    let p = await panel();
    if (p[""] !== "Cara plana" || p["Área"] !== "400 mm²") throw new Error(`cara: ${JSON.stringify(p)}`);
    // Más la cara de adelante: se tocan (0) y forman 90°
    await b.click(...(await at([3, -10, -3])), { wait: 1500, modifiers: SHIFT });
    p = await panel();
    if (p["Distancia mínima"] !== "0 mm" || p["Ángulo"] !== "90°") throw new Error(`dos caras: ${JSON.stringify(p)}`);
    // Un vértice (cerca de la esquina): sus coordenadas
    const [cx, cy] = await at([10, -10, 10]);
    await b.click(cx - 3, cy + 2, { wait: 1500 });
    p = await panel();
    if (p[""] !== "Vértice" || p["X"] !== "10 mm" || p["Y"] !== "-10 mm" || p["Z"] !== "10 mm") throw new Error(`vértice: ${JSON.stringify(p)}`);
    // Y el de abajo: 20 mm, todo en Z
    const [dx, dy] = await at([10, -10, -10]);
    await b.click(dx - 3, dy - 2, { wait: 1500, modifiers: SHIFT });
    p = await panel();
    if (p["Distancia mínima"] !== "20 mm" || p["ΔZ"] !== "20 mm" || p["ΔX"] !== "0 mm") throw new Error(`dos vértices: ${JSON.stringify(p)}`);
    await b.shot("medir_dos_vertices");
    // Esc limpia la selección y el panel
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    if (await b.eval(`!!document.querySelector('[aria-label="Medidas"]')`)) throw new Error("el panel quedó");
  },

  async "masa con material y centro de masa"(b) {
    const line = (label) =>
      b.eval(`(() => { const e = [...document.querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(label)}); return e?.nextElementSibling?.textContent.trim(); })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    if ((await line("Masa")) !== undefined) throw new Error("masa sin material");
    if ((await line("Centro de masa")) !== "0,00, 0,00, 0,00 mm") throw new Error(`centro: ${await line("Centro de masa")}`);
    // Acero: 8 cm³ × 7,85 g/cm³
    await b.clickText("Sin material");
    await sleep(500);
    await b.clickText("Acero (7850 kg/m³)");
    await sleep(1500);
    if ((await line("Masa")) !== "62,80 g") throw new Error(`masa: ${await line("Masa")}`);
    // I = m (a² + b²) / 12 = 62,8 g × 800 mm² / 12 = 4,187 kg·mm² en los tres ejes
    if ((await line("Inercia")) !== "4,187 · 4,187 · 4,187 kg·mm²") throw new Error(`inercia: ${await line("Inercia")}`);
    if ((await call("cad_get_document")).material?.density !== 7850) throw new Error("no quedó en el documento");
    // Densidad propia
    await b.clickText("Acero (7850 kg/m³)");
    await sleep(500);
    await b.clickText("Densidad propia");
    await sleep(800);
    await setInput(b, "Densidad", 1000);
    await sleep(1500);
    if ((await line("Masa")) !== "8,00 g") throw new Error(`masa propia: ${await line("Masa")}`);
    // Centro de masa en el visor
    await b.clickText("Ver el centro de masa");
    await sleep(800);
    if (!(await b.eval(`!!window.__cadViewer.centerMark`))) throw new Error("no se dibujó el centro de masa");
    await b.shot("centro_de_masa");
  },

  async "vistas: atajos, cubo, acercar y mirar de frente"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    // Dirección de la cámara en el visor (Y arriba; el frente del CAD, −Y, es +Z)
    const dir = () => b.eval(`window.__cadViewer.camera.position.clone().sub(window.__cadViewer.controls.target).normalize().toArray()`);
    const along = async (want, what) => {
      const d = await dir();
      const dot = d[0] * want[0] + d[1] * want[1] + d[2] * want[2];
      if (!(dot > 0.999)) throw new Error(`${what}: ${d.map((x) => x.toFixed(3))}`);
    };
    const press = (code, shift) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { code: ${JSON.stringify(code)}, key: ${JSON.stringify(code.replace("Key", "").replace("Digit", "").toLowerCase())}, shiftKey: ${shift}, bubbles: true }))`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await press("Digit1", true);
    await sleep(700);
    await along([0, 0, 1], "frente");
    await press("Digit5", true);
    await sleep(700);
    await along([0, 1, 0], "arriba");
    // Isométrica y clic en la parte de arriba del cubo de vistas: vista superior
    await press("Digit7", true);
    await sleep(700);
    const iso = [1, 1, 1].map((x) => x / Math.sqrt(3));
    await along(iso, "isométrica");
    const cube = await b.eval(`(() => { const r = window.__cadViewer.canvas.getBoundingClientRect(); return [r.right - 8 - 65, r.top + 8 + 65]; })()`);
    // La cara de arriba en isométrica: ~0,82 de medio lado (28 px) sobre el centro
    await b.click(cube[0], cube[1] - 23, { wait: 800 });
    await along([0, 1, 0], "cubo: arriba");
    // F con la cara de adelante elegida: el centro de la vista va a esa cara
    await press("Digit7", true);
    await sleep(700);
    await b.click(...(await at([3, -10, -3])), { wait: 1200 });
    await press("KeyF", false);
    await sleep(500);
    const t = await b.eval(`window.__cadViewer.controls.target.toArray()`);
    near(t[2], 10, 1e-3, "centro de la vista en la cara de adelante (z del visor)");
    // Mirarla de frente desde el menú del clic derecho
    const [fx, fy] = await at([3, -10, -3]);
    await b.click(fx, fy, { button: "right", buttons: 2, wait: 1200 });
    const item = await b.eval(`(() => { const e = [...document.querySelectorAll("[role=menuitem]")].find((e) => e.textContent.includes("Mirar de frente")); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.click(...item, { wait: 800 });
    await along([0, 0, 1], "mirar de frente");
    // 3 (como en el resto de la app): derecha; Ctrl+3: izquierda
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "3", code: "Digit3", bubbles: true }))`);
    await sleep(700);
    await along([1, 0, 0], "derecha");
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "3", code: "Digit3", ctrlKey: true, bubbles: true }))`);
    await sleep(700);
    await along([-1, 0, 0], "izquierda");
    // El panel lateral no se tocó
    if (!(await b.eval(`document.body.innerText.includes("Operaciones") || document.body.innerText.includes("OPERACIONES")`))) throw new Error("se plegó el panel");
    await b.shot("vista_normal");
  },

  async "menú del clic derecho: redondear, sketch en la cara y vistas"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const menuItems = () => b.eval(`[...document.querySelectorAll("[role=menuitem]")].map((e) => e.textContent.trim())`);
    const choose = async (text) => {
      const r = await b.eval(`(() => { const e = [...document.querySelectorAll("[role=menuitem]")].find((e) => e.textContent.includes(${JSON.stringify(text)})); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      if (!r) throw new Error(`no está «${text}» en el menú: ${await menuItems()}`);
      await b.click(...r, { wait: 1500 });
    };
    const right = async (p, dx = 0, dy = 0) => {
      const [x, y] = await at(p);
      await b.click(x + dx, y + dy, { button: "right", buttons: 2, wait: 1200 });
    };
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Arista de arriba adelante → Redondear: el diálogo abre con esa arista
    await right([0, -10, 10]);
    await choose("Redondear");
    await sleep(1000);
    await accept(b);
    near((await body()).volume, 8000 - (1 - Math.PI / 4) * 20, 0.05, "redondeo desde el menú");
    // Cara de arriba → Sketch en la cara
    await right([3, 3, 10]);
    await choose("Sketch en la cara");
    await sleep(1500);
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error("no empezó el sketch");
    const plane = (await evaluate()).sketches.at(-1)?.plane;
    near(plane?.origin[2] ?? NaN, 10, 1e-6, "sketch sobre la cara de arriba");
    await b.clickText("Descartar");
    await sleep(1000);
    // En vacío: vistas
    const [ex, ey] = await b.eval(`(() => { const r = window.__cadViewer.canvas.getBoundingClientRect(); return [r.left + 60, r.bottom - 60]; })()`);
    await b.click(ex, ey, { button: "right", buttons: 2, wait: 1000 });
    await choose("Frente");
    await sleep(700);
    const d = await b.eval(`window.__cadViewer.camera.position.clone().sub(window.__cadViewer.controls.target).normalize().toArray()`);
    if (!(d[2] > 0.999)) throw new Error(`frente: ${d}`);
  },

  async "filtros y selección por caja"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const picks = () => b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Rectángulo de pantalla que contiene toda la caja
    const corners = [];
    for (const x of [-10, 10]) for (const y of [-10, 10]) for (const z of [-10, 10]) corners.push(await at([x, y, z]));
    const xs = corners.map((c) => c[0]);
    const ys = corners.map((c) => c[1]);
    const [l, r, t, bt] = [Math.min(...xs) - 25, Math.max(...xs) + 25, Math.min(...ys) - 25, Math.max(...ys) + 25];
    // Aristas, ventana (izquierda → derecha): las 9 que se ven (3 quedan detrás)
    await filter("Aristas");
    await b.drag(l, t, r, bt);
    await sleep(500);
    let k = await picks();
    if (k.length !== 9 || k.some((x) => x !== "edge")) throw new Error(`aristas: ${k}`);
    // Con el filtro, un clic sobre una cara no la elige
    await b.click(...(await at([3, 3, 10])), { wait: 500 });
    if ((await picks()).length) throw new Error("eligió una cara con el filtro de aristas");
    // Caras, cruce (derecha → izquierda) chico en medio de la tapa: solo la tapa
    await filter("Caras");
    const [cx, cy] = await at([0, 0, 10]);
    await b.drag(cx + 8, cy - 6, cx - 8, cy + 6);
    await sleep(500);
    k = await picks();
    if (k.length !== 1 || k[0] !== "face") throw new Error(`cruce: ${k}`);
    // Vértices con caja: los 7 visibles
    await filter("Vértices");
    await b.drag(l, t, r, bt);
    await sleep(500);
    k = await picks();
    if (k.length !== 7) throw new Error(`vértices: ${k.length}`);
    // Mayús suma: más las caras visibles (3)
    await filter("Caras");
    await b.mouse("mouseMoved", l, t, { buttons: 0 });
    await b.mouse("mousePressed", l, t, { modifiers: 8 });
    for (let i = 1; i <= 10; i++) { await b.mouse("mouseMoved", l + ((r - l) * i) / 10, t + ((bt - t) * i) / 10, { buttons: 1, modifiers: 8 }); await sleep(40); }
    await b.mouse("mouseReleased", r, bt, { modifiers: 8 });
    await sleep(500);
    k = await picks();
    if (k.filter((x) => x === "face").length !== 3 || k.length !== 10) throw new Error(`sumar caras: ${k}`);
    await filter("Todo");
  },

  async "vista de corte con tapa"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const pixel = (x, y) =>
      b.eval(`(() => {
        const c = window.__cadViewer.canvas, r = c.getBoundingClientRect();
        const gl = c.getContext("webgl2") || c.getContext("webgl");
        window.__cadViewer.renderNow();
        const px = new Uint8Array(4);
        const dpr = c.width / r.width;
        gl.readPixels(Math.round((${x} - r.left) * dpr), Math.round(c.height - (${y} - r.top) * dpr), 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
        return [...px];
      })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Agujero pasante en Z para que la sección tenga hueco
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.features[0].id + 1, name: "Agujero", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 40 }, origin: [0, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
    doc.next_id = doc.features[1].id + 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const vol = (await body()).volume;
    // De frente y con corte por XZ a la mitad: se ve la tapa (naranja) y el hueco
    await b.eval(`window.__cadViewer.lookFrom([0, -1, 0])`);
    await sleep(800);
    await b.eval(`document.querySelector('[aria-label="Vista de corte"]').click()`);
    await sleep(800);
    // Fuera de la línea de la grilla (z = 0)
    const solid = await pixel(...(await at([7, 0, 4])));
    const hole = await pixel(...(await at([0, 0, 4])));
    const orange = (p) => p[0] > p[1] && p[1] > p[2] && p[0] - p[2] > 50;
    if (!orange(solid)) throw new Error(`sin tapa en el material: ${solid}`);
    if (orange(hole)) throw new Error(`tapa sobre el hueco: ${hole}`);
    await b.shot("corte");
    // El modelo no cambió y al sacar el corte vuelve la cara de adelante
    near((await body()).volume, vol, 1e-9, "volumen");
    await b.eval(`document.querySelector('[aria-label="Vista de corte"]').click()`);
    await sleep(800);
    if (orange(await pixel(...(await at([7, -10, 4]))))) throw new Error("quedó la tapa");
  },

  async "carpetas en el árbol"(b) {
    const row = (name) =>
      b.eval(`(() => { const e = [...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === ${JSON.stringify(name)}); if (!e || !e.offsetParent) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    const folders = async () => (await call("cad_get_document")).folders ?? [];
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    doc.features.push(
      { id: box.id + 1, name: "Agujero", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 3, height: 40 }, origin: [0, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } },
      { ...structuredClone(box), id: box.id + 2, name: "Caja arriba", kind: { ...structuredClone(box.kind), origin: [0, 0, 30] } },
    );
    doc.next_id = box.id + 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const version = await b.eval(`window.__cadStore.result().version`);
    // Clic en "Caja 1", Mayús+clic en "Agujero" y agrupar
    await b.click(...(await row("Caja 1")), { wait: 1200 });
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    await b.click(...(await row("Caja 1")), { wait: 800 });
    await b.click(...(await row("Agujero")), { wait: 800, modifiers: 8 });
    await b.clickText("Agrupar en carpeta");
    await sleep(1200);
    let f = await folders();
    if (f.length !== 1 || f[0].first !== box.id || f[0].last !== box.id + 1) throw new Error(`carpeta: ${JSON.stringify(f)}`);
    // Plegar: sus filas se esconden, la de afuera no
    await b.eval(`document.querySelector("[data-folder]").click()`);
    await sleep(1000);
    if ((await row("Caja 1")) || (await row("Agujero"))) throw new Error("no se plegó");
    if (!(await row("Caja arriba"))) throw new Error("se escondió una de afuera");
    // Renombrar con doble clic
    await b.eval(`document.querySelector("[data-folder] span.truncate").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`);
    await sleep(500);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nombre de la carpeta"]'); i.value = "Base"; i.blur(); })()`);
    await sleep(1000);
    f = await folders();
    if (f[0]?.name !== "Base" || !f[0].collapsed) throw new Error(`renombrar: ${JSON.stringify(f)}`);
    // Nada de esto recalculó
    if ((await b.eval(`window.__cadStore.result().version`)) !== version) throw new Error("las carpetas recalcularon el árbol");
    // Desplegar, borrar la última de la carpeta: queda con la primera
    await b.eval(`document.querySelector("[data-folder]").click()`);
    await sleep(800);
    await b.eval(`(() => { const s = [...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === "Agujero"); s.closest("[data-feature-row]").querySelector('[aria-label="Borrar"]').click(); })()`);
    await sleep(1500);
    f = await folders();
    if (f.length !== 1 || f[0].first !== box.id || f[0].last !== box.id) throw new Error(`tras borrar: ${JSON.stringify(f)}`);
    // Desagrupar
    await b.eval(`document.querySelector('[data-folder] [aria-label="Desagrupar"]').click()`);
    await sleep(1000);
    if ((await folders()).length) throw new Error("no se desagrupó");
    if ((await call("cad_get_document")).features.length !== 2) throw new Error("desagrupar tocó las operaciones");
  },

  async "cambios rápidos: se calcula el último"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await clickRow(b, "Caja 1");
    await sleep(800);
    const before = await b.eval(`window.__cadStore.skippedSends()`);
    // Seis valores sin esperar: los del medio no hace falta calcularlos
    await b.eval(`(() => {
      const i = [...document.querySelectorAll("label")].find((l) => l.textContent.startsWith("Alto (Z)")).querySelector("input");
      for (const v of [21, 22, 23, 24, 25, 30]) { i.value = String(v); i.dispatchEvent(new Event("change", { bubbles: true })); }
    })()`);
    await b.eval(`window.__cadStore.settled()`);
    await sleep(500);
    near((await body()).volume, 20 * 20 * 30, 1e-6, "vista previa con el último valor");
    if (!((await b.eval(`window.__cadStore.skippedSends()`)) > before)) throw new Error("no se salteó ningún envío");
    await b.key("Enter", "Enter", 13);
    await sleep(1500);
    near((await body()).volume, 12000, 1e-6, "aceptado");
  },

  async "piezas: nueva, color, ocultar y exportar"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const parts = async () => (await evaluate()).parts;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Cilindro como pieza nueva, al lado
    await b.clickText("Cilindro");
    await sleep(1500);
    await b.eval(`__cadStore.commit((d) => { const k = d.features.at(-1).kind; k.op = "new"; k.origin = [40, 0, -10]; })`);
    await sleep(1500);
    if ((await b.eval(`__cadStore.tool()?.op`)) !== "new") throw new Error("la herramienta no es de pieza nueva");
    await accept(b);
    let ps = await parts();
    if (ps.length !== 2 || ps[0].name !== "Pieza 1" || ps[1].name !== "Pieza 2") throw new Error(`piezas: ${JSON.stringify(ps.map((p) => p.name))}`);
    near(ps[0].volume, 8000, 1e-6, "caja");
    // Una arista de la caja: el redondeo solo cambia la caja
    const cyl = ps[1].volume;
    await b.click(...(await at([0, -10, 10])), { wait: 800 });
    await b.clickText("Redondeo");
    await sleep(1500);
    await accept(b);
    ps = await parts();
    near(ps[1].volume, cyl, 1e-9, "el cilindro no se tocó");
    if (!(ps[0].volume < 8000)) throw new Error("no se redondeó la caja");
    // Color y nombre de la segunda (no recalculan)
    const version = await b.eval(`__cadStore.result().version`);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Color de Pieza 2"]'); i.value = "#ff0000"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(800);
    await b.eval(`document.querySelector('[data-part="${ps[1].id.feature}:0"] span.truncate').dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`);
    await sleep(400);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nombre de la pieza"]'); i.value = "Eje"; i.blur(); })()`);
    await sleep(1200);
    const props = (await call("cad_get_document")).parts;
    if (props?.[0]?.color !== "#ff0000" || props[0].name !== "Eje") throw new Error(`propiedades: ${JSON.stringify(props)}`);
    if ((await b.eval(`__cadStore.result().version`)) !== version) throw new Error("cambiar color o nombre recalculó");
    if ((await parts())[1].name !== "Eje") throw new Error("no cambió el nombre");
    // Ocultar la caja: no se puede elegir donde estaba
    await b.eval(`document.querySelector('[aria-label="Ocultar Pieza 1"]').click()`);
    await sleep(1000);
    const [x, y] = await at([3, 3, 10]);
    const hit = await b.eval(`window.__cadViewer.pick(${x}, ${y}, { faces: true })?.kind ?? null`);
    if (hit === "face") throw new Error("se eligió una cara de la pieza oculta");
    await b.shot("piezas");
    // Exportar 3MF: dos objetos con nombre; y solo una pieza
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/piezas.3mf`;
    rmSync(path, { force: true });
    await call("cad_export", { path, format: "3mf" });
    const { execSync } = await import("node:child_process");
    const model = execSync(`unzip -p ${path} 3D/3dmodel.model`).toString();
    const objects = (model.match(/<object /g) ?? []).length;
    if (objects !== 2 || !model.includes('name="Eje"')) throw new Error(`3MF: ${objects} objetos`);
    // STEP: cada pieza con su nombre y el color elegido (rojo puro = color predefinido)
    const step = `${out}/piezas.step`;
    rmSync(step, { force: true });
    await call("cad_export", { path: step, format: "step" });
    const text = readFileSync(step, "latin1");
    if (!text.includes("'Eje'") || !text.includes("'Pieza 1'")) throw new Error("STEP sin los nombres de las piezas");
    if (!/DRAUGHTING_PRE_DEFINED_COLOUR\('red'\)|COLOUR_RGB/.test(text)) throw new Error("STEP sin el color");
    const one = `${out}/eje.stl`;
    rmSync(one, { force: true });
    await call("cad_export", { path: one, format: "stl", part: ps[1].id });
    if (!existsSync(one)) throw new Error("no se exportó la pieza sola");
  },

  async "piezas: booleana, separar y borrar"(b) {
    const parts = async () => (await evaluate()).parts;
    /** Tilda una pieza dentro de la lista con ese título */
    const tick = (list, name) =>
      b.eval(`(() => {
        const box = document.querySelector('[aria-label=${JSON.stringify(list)}]');
        const l = [...box.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(name)});
        l.click();
        return !!l;
      })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un cilindro como pieza aparte que se mete en la caja
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    doc.features.push({ id: box.id + 1, name: "Cilindro 1", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 40 }, origin: [10, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = box.id + 2;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    if ((await parts()).length !== 2) throw new Error("no hay dos piezas");
    // Booleana: restar el cilindro de la caja
    await b.clickText("Booleana");
    await sleep(1500);
    await tick("Piezas que quedan", "Pieza 1");
    await sleep(800);
    await tick("Piezas que restan", "Pieza 2");
    await sleep(1500);
    await accept(b);
    let ps = await parts();
    if (ps.length !== 1) throw new Error(`booleana: ${ps.length} piezas`);
    near(ps[0].volume, 8000 - (Math.PI * 16 * 20) / 2, 0.5, "caja menos medio cilindro");
    // Un corte fino por el medio deja dos sólidos sueltos en una pieza: separarlos
    const d2 = await call("cad_get_document");
    d2.features.push({ id: d2.next_id, name: "Ranura", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 2, dy: 30, dz: 30, centered: true, centered_z: true }, origin: [-3, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
    d2.next_id += 1;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    if ((await parts()).length !== 1) throw new Error("la ranura separó piezas sola");
    await b.clickText("Separar");
    await sleep(1500);
    await accept(b);
    ps = await parts();
    if (ps.length !== 2) throw new Error(`separar: ${ps.length} piezas`);
    // Borrar la segunda desde la lista de piezas
    const total = ps[0].volume + ps[1].volume;
    await b.eval(`document.querySelector('[aria-label="Borrar Pieza 2"]').click()`);
    await sleep(1500);
    await accept(b);
    ps = await parts();
    if (ps.length !== 1) throw new Error(`borrar: ${ps.length} piezas`);
    if (!(ps[0].volume < total)) throw new Error("no se borró");
    const kinds = (await call("cad_get_document")).features.map((f) => f.kind.type);
    if (kinds.join(",") !== "primitive,primitive,boolean,primitive,split_parts,delete_parts") throw new Error(`historial: ${kinds}`);
  },

  async "material por pieza y masa total"(b) {
    // Sin raíz: la sección Sólido (la última con ese rótulo; la de la pieza va antes)
    const line = (label, root = "document") =>
      b.eval(`(() => { const e = [...${root}.querySelectorAll("span")].filter((x) => x.textContent === ${JSON.stringify(label)}).at(-1); return e?.nextElementSibling?.textContent.trim(); })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    // Cubo de 10 aparte: 1 cm³
    doc.features.push({ id: box.id + 1, name: "Cubo", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 10, dy: 10, dz: 10, centered: true, centered_z: true }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = box.id + 2;
    doc.material = { name: "PLA", density: 1240 };
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    // 8 cm³ + 1 cm³ de PLA
    if ((await line("Masa")) !== "11,16 g") throw new Error(`masa con PLA: ${await line("Masa")}`);
    // La segunda de acero: 8 × 1,24 + 1 × 7,85
    await b.eval(`[...document.querySelectorAll("[data-part] span.truncate")].find((s) => s.textContent === "Pieza 2").click()`);
    await sleep(800);
    const panel = `document.querySelector('[aria-label="Datos de Pieza 2"]')`;
    const trigger = await b.eval(`(() => { const e = [...${panel}.querySelectorAll("button")].find((x) => x.textContent.includes("El del diseño")); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.click(...trigger, { wait: 600 });
    await b.clickText("Acero (7850 kg/m³)");
    await sleep(1500);
    if ((await line("Masa", panel)) !== "7,85 g (Acero)") throw new Error(`masa de la pieza: ${await line("Masa", panel)}`);
    if ((await line("Masa")) !== "17,77 g") throw new Error(`masa total: ${await line("Masa")}`);
    // Con densidades distintas no hay una inercia del cuerpo
    if ((await line("Inercia")) !== undefined) throw new Error("mostró la inercia con materiales distintos");
    // El centro de masa se corre hacia el cubo de acero: 40 × 7,85 / 17,77
    const c = await line("Centro de masa");
    if (!c?.startsWith("17,67")) throw new Error(`centro: ${c}`);
  },

  async "plano de referencia: sketch encima y extrusión"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Plano");
    await sleep(1500);
    await setInput(b, "Distancia", 15);
    await sleep(1200);
    await accept(b);
    const refs = (await evaluate()).references;
    if (refs.length !== 1 || refs[0].kind !== "plane") throw new Error(`referencias: ${JSON.stringify(refs)}`);
    near(refs[0].plane.origin[2], 15, 1e-9, "altura del plano");
    // Sketch: clic sobre el plano de referencia (está por encima de los base)
    await b.clickText("Sketch");
    await sleep(500);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    await b.click(...(await at([0.15 * S, -0.15 * S, 15])), { wait: 2000 });
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error("no empezó el sketch");
    const doc = await call("cad_get_document");
    const sk = doc.features.find((f) => f.kind.type === "sketch");
    if (sk?.kind.plane.type !== "reference") throw new Error(`plano del sketch: ${JSON.stringify(sk?.kind.plane)}`);
    await b.clickText("Rectángulo");
    await b.click(...(await at([-5, -5, 15])));
    await b.click(...(await at([5, 5, 15])), { wait: 1000 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const r = await body();
    near(r.bbox_min[2], 15, 1e-6, "la pieza empieza en el plano");
    // Ocultar el plano con el ojo del árbol
    await b.eval(`(() => { const s = [...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === "Plano 1"); s.closest("[data-feature-row]").querySelector('[aria-label="Ocultar en el visor"]').click(); })()`);
    await sleep(800);
    const [x, y] = await at([0.4 * S, 0.4 * S, 15]);
    const hit = await b.eval(`window.__cadViewer.pick(${x}, ${y}, { refPlanes: true })?.kind ?? null`);
    if (hit === "refplane") throw new Error("el plano oculto se sigue eligiendo");
  },

  async "extrusión: dos direcciones, desmolde y delgada"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    // Cuadrado de 10 centrado (más rápido que dibujarlo)
    const sk = doc.features[0].kind.sketch;
    const ids = [[-5, -5], [5, -5], [5, 5], [-5, 5]].map(([x, y]) => {
      const id = sk.next_id++;
      sk.points.push({ id, x, y });
      return id;
    });
    for (let k = 0; k < 4; k++) sk.entities.push({ id: sk.next_id++, geometry: { type: "line", start: ids[k], end: ids[(k + 1) % 4] }, construction: false });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    // Dos direcciones: 10 hacia arriba y 4 hacia abajo
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.extent = { type: "two_sides", distance: 10, second: 2 }; })`);
    await sleep(1200);
    await setInput(b, "Hacia atrás", 4);
    await sleep(1500);
    let r = await body();
    near(r.bbox_min[2], -4, 1e-6, "hacia atrás");
    near(r.volume, 1400, 1e-6, "dos direcciones");
    // Desmolde de 10° en las dos direcciones
    await setInput(b, "Desmolde", 10);
    await sleep(1500);
    const t = Math.tan((10 * Math.PI) / 180);
    const frustum = (h) => { const top = 10 - 2 * h * t; return (h / 3) * (100 + top * top + 10 * top); };
    near((await body()).volume, frustum(10) + frustum(4), 1e-3, "con desmolde");
    // Delgada de 1 mm, sin desmolde
    await setInput(b, "Desmolde", 0);
    await sleep(1200);
    await b.clickText("Delgada (solo una pared)");
    await sleep(1500);
    const ring = 100 + 4 * 10 * 0.5 + Math.PI * 0.25 - 81;
    near((await body()).volume, ring * 14, 1e-3, "delgada");
    await accept(b);
    const k = (await call("cad_get_document")).features.at(-1).kind;
    if (k.thin !== 1 || k.extent.second !== 4) throw new Error(`guardado: ${JSON.stringify(k)}`);
  },

  async "barrido y transición"(b) {
    const sketchDoc = (features) => ({ features, next_id: features.length, rollback: null });
    const sketch = (id, plane, offset, build) => {
      const sk = { points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [], next_id: 1, origin: 0 };
      build(sk);
      return { id, name: `Sketch ${id + 1}`, suppressed: false, kind: { type: "sketch", plane: { type: plane }, offset, sketch: sk } };
    };
    const pt = (sk, x, y) => { const id = sk.next_id++; sk.points.push({ id, x, y }); return id; };
    const line = (sk, a, c) => sk.entities.push({ id: sk.next_id++, geometry: { type: "line", start: a, end: c }, construction: false });
    const square = (h) => (sk) => {
      const p = [[-h, -h], [h, -h], [h, h], [-h, h]].map(([x, y]) => pt(sk, x, y));
      for (let k = 0; k < 4; k++) line(sk, p[k], p[(k + 1) % 4]);
    };
    await begin(b);
    // Perfil: cuadrado de 4 en la planta; camino: recta de 20 hacia arriba en el frente
    await call("cad_set_document", {
      document: sketchDoc([
        sketch(0, "xy", 0, square(2)),
        sketch(1, "xz", 0, (sk) => line(sk, pt(sk, 0, 0), pt(sk, 0, 20))),
      ]),
    });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Barrido");
    await sleep(2500);
    await accept(b);
    near((await body()).volume, 16 * 20, 1e-6, "barrido recto");
    const k = (await call("cad_get_document")).features.at(-1).kind;
    if (k.type !== "sweep" || k.sketch !== 0 || k.path.sketch !== 1) throw new Error(`barrido: ${JSON.stringify(k)}`);
    // Transición: dos cuadrados iguales a 0 y 10 = prisma
    await call("cad_set_document", { document: sketchDoc([sketch(0, "xy", 0, square(5)), sketch(1, "xy", 10, square(5))]) });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Transición");
    await sleep(2500);
    await accept(b);
    near((await body()).volume, 1000, 1e-3, "transición");
  },

  async "agujero avellanado y medir su diámetro"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const panel = () => b.eval(`Object.fromEntries([...document.querySelectorAll("[data-measure]")].map((e) => [e.dataset.measure, e.textContent]))`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Sketch en la cara de arriba con un punto en el centro
    await b.click(...(await at([3, 3, 10])), { wait: 800 });
    await b.clickText("Sketch");
    await sleep(2000);
    await b.clickText("Punto");
    await b.click(...(await at([0, 0, 10])), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Agujero");
    await sleep(2000);
    // M6 pasante; avellanado
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.style = { type: "countersink", diameter: 12.4, angle: 90 }; })`);
    await sleep(1500);
    await accept(b);
    const h = (12.4 - 6.6) / 2;
    const cone = (Math.PI * h / 3) * (6.2 * 6.2 + 3.3 * 3.3 + 6.2 * 3.3);
    near((await body()).volume, 8000 - cone - Math.PI * 3.3 * 3.3 * (20 - h), 0.05, "placa avellanada");
    // Medir la pared del agujero (el lado de atrás, que se ve desde la cámara)
    await b.eval(`window.__cadViewer.lookFrom([0.3, -0.3, 1])`);
    await sleep(800);
    await b.click(...(await at([-2.33, 2.33, 0])), { wait: 1500 });
    const p = await panel();
    if (p[""] !== "Cara cilíndrica" || p["Diámetro"] !== "6.6 mm") throw new Error(`medida: ${JSON.stringify(p)}`);
  },

  async "hélice y engrosar"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Hélice");
    await sleep(1500);
    await accept(b);
    const refs = (await evaluate()).references;
    if (refs[0]?.kind !== "curve" || !(refs[0].points.length > 20)) throw new Error(`hélice: ${JSON.stringify(refs[0]?.kind)}`);
    near(Math.max(...refs[0].points.map((p) => p[2])), 15, 1e-3, "alto de la hélice (3 vueltas de 5)");
    // Caja y engrosar su cara de arriba 2 mm
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.click(...(await at([3, 3, 10])), { wait: 800 });
    await b.clickText("Engrosar");
    await sleep(2000);
    await accept(b);
    const r = await body();
    near(r.volume, 8000 + 400 * 2, 1e-6, "caja más 2 mm arriba");
    near(r.bbox_max[2], 12, 1e-6, "llega a 12");
  },

  async "mover cara y escala"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // La cara de arriba, 5 mm hacia afuera
    await b.click(...(await at([3, 3, 10])), { wait: 800 });
    await b.clickText("Mover cara");
    await sleep(2000);
    await accept(b);
    let r = await body();
    near(r.volume, 8000 + 400 * 5, 1e-6, "cara movida");
    near(r.bbox_max[2], 15, 1e-6, "llega a 15");
    // Escala ×2 desde el origen
    await b.clickText("Escala");
    await sleep(2000);
    await accept(b);
    r = await body();
    near(r.volume, 10000 * 8, 1e-3, "escala ×2");
    near(r.bbox_max[2], 30, 1e-5, "alto escalado");
  },

  async "plano 2D con vistas, ocultas y exportar SVG"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Agujero pasante vertical: en el frente y el lateral aparece como líneas ocultas
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id, name: "Agujero", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 3, height: 40 }, origin: [0, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D (vistas, ocultas, cajetín)");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    const info = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      const views = [...svg.querySelectorAll("[data-view]")].map((g) => g.dataset.view);
      const hidden = svg.querySelectorAll('[data-view="front"] [data-kind="hidden"], [data-view="front"] [data-kind="hidden_outline"]').length;
      return { views, hidden, text: svg.textContent };
    })()`);
    if (info.views.join(",") !== "front,top,side,iso") throw new Error(`vistas: ${info.views}`);
    if (info.hidden !== 2) throw new Error(`ocultas del frente: ${info.hidden}`);
    // 20 mm en A4: escala 2:1 (o mayor)
    if (!/[25]:1/.test(info.text)) throw new Error(`escala: ${info.text}`);
    await b.shot("plano_2d");
    // Exportar: el mismo SVG a un archivo
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/plano.svg`;
    rmSync(path, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.clickText("Exportar SVG");
    await sleep(1500);
    if (!existsSync(path) || !readFileSync(path, "utf8").startsWith("<svg")) throw new Error("no se exportó el SVG");
    // Cotas a mano: el diámetro del agujero en la planta y el largo de una arista del frente
    await b.eval(`[...document.querySelectorAll('[aria-label="Herramienta de cotas"] button')].find((x) => x.textContent === "Cota").click()`);
    await sleep(300);
    const at = (sel, pick) =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const lines = [...svg.querySelectorAll(${JSON.stringify(sel)})].map((l) => l.getAttribute("points").split(" ").map((p) => p.split(",").map(Number)));
        const pts = (${pick})(lines);
        const m = svg.getScreenCTM();
        const p = new DOMPoint(pts[0], pts[1]).matrixTransform(m);
        return [p.x, p.y];
      })()`);
    // El círculo más largo de la planta (muchos puntos)
    await b.click(...(await at('[data-view="top"] polyline[data-kind="visible"]', "(ls) => ls.sort((a, b) => b.length - a.length)[0][0]")), { wait: 1000 });
    // Una recta del frente: el medio de su primer tramo
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', "(ls) => { const l = ls.find((x) => x.length === 2); return [(l[0][0] + l[1][0]) / 2, (l[0][1] + l[1][1]) / 2]; }")), { wait: 1000 });
    const texts = await b.eval(`[...document.querySelectorAll("[data-sheet] [data-user-dim] text")].map((t) => t.textContent)`);
    if (!texts.includes("Ø6") || !texts.includes("20")) throw new Error(`cotas a mano: ${texts}`);
    if ((await call("cad_get_document")).drawing?.dims?.length !== 2) throw new Error("no se guardaron las cotas");
    await b.eval(`[...document.querySelectorAll('[aria-label="Herramienta de cotas"] button')].find((x) => x.textContent === "Cota").click()`);
    // Corte A-A: el frente rayado y la línea en la planta
    await b.eval(`[...document.querySelectorAll('[aria-label="Plano 2D"] label')].find((l) => l.textContent.trim() === "Corte A-A").click()`);
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-hatch="front"] polygon')`)); t++) await sleep(500);
    const cut = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      return { tris: svg.querySelectorAll('[data-hatch="front"] polygon').length, label: !!svg.querySelector("[data-section-label]"), line: !!svg.querySelector("[data-section-line]") };
    })()`);
    if (!(cut.tris > 0) || !cut.label || !cut.line) throw new Error(`corte: ${JSON.stringify(cut)}`);
    await b.shot("plano_corte");
    // PDF del mismo tamaño que la hoja (A4: 841,89 × 595,28 pt)
    const pdf = `${out}/plano.pdf`;
    rmSync(pdf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(pdf)}`);
    await b.clickText("Exportar PDF");
    await sleep(3000);
    if (!existsSync(pdf) || !readFileSync(pdf).subarray(0, 5).toString() === "%PDF-") throw new Error("no se exportó el PDF");
    const { execSync } = await import("node:child_process");
    try {
      const info = execSync(`pdfinfo ${pdf}`).toString();
      if (!/Page size:\s+841\.\d+ x 595\.\d+/.test(info)) throw new Error(`tamaño del PDF: ${info}`);
    } catch (e) {
      if (String(e).includes("tamaño")) throw e;
    }
    // DXF de la hoja (capas VISIBLE y OCULTA)
    const dxf = `${out}/plano.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    const text = existsSync(dxf) ? readFileSync(dxf, "utf8") : "";
    if (!text.includes("OCULTA") || !text.trimEnd().endsWith("EOF")) throw new Error("DXF raro");
    await b.clickText("Cerrar");
    await sleep(500);
    if (await b.eval(`!!document.querySelector('[aria-label="Plano 2D"]')`)) throw new Error("no se cerró");
  },

  async "ensamble: instancias, bisagra, grados libres y choques"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un cilindro como pieza aparte
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id, name: "Eje", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 30 }, origin: [60, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Ensamble");
    await sleep(800);
    // Insertar las dos piezas (la primera queda fija)
    await b.clickText("Insertar");
    await sleep(1500);
    await b.eval(`(() => { const parts = window.__cadStore.result().parts; window.__p2 = parts[1].id; })()`);
    await b.eval(`window.__cadStore.editAssembly((a) => { a.instances.push({ id: a.next_id, part: window.__p2, name: "Eje", position: [40, 0, 0], rotation: [0, 0, 0], fixed: false }); a.next_id += 1; })`);
    await sleep(2000);
    if ((await b.eval(`document.querySelectorAll("[data-instance]").length`)) !== 2) throw new Error("no hay dos instancias");
    // Conectores: la tapa de arriba de la caja y el costado del cilindro
    const faces = await call("cad_assembly_faces");
    const conn = async (inst, test) => {
      const [, a, z] = faces.find((f) => f[0] === inst);
      for (let k = a; k < z; k++) {
        const c = await call("cad_assembly_connector", { face: k });
        if (test(c)) return c;
      }
      throw new Error(`sin conector en la instancia ${inst}`);
    };
    const asm = (await call("cad_get_document")).assembly;
    const [i1, i2] = asm.instances.map((i) => i.id);
    const top = await conn(i1, (c) => c.z[2] > 0.99 && Math.abs(c.origin[2] - 10) < 1e-6);
    const side = await conn(i2, (c) => Math.abs(c.z[2]) > 0.99 && Math.abs(c.origin[0] - 60) < 1e-6 && Math.abs(c.origin[2] - 15) < 1e-6);
    await b.eval(`window.__cadStore.editAssembly((a) => { a.mates.push({ id: a.next_id, name: "Bisagra 1", kind: "revolute", a: ${JSON.stringify(top)}, b: ${JSON.stringify(side)}, flip: false }); a.next_id += 1; })`);
    await sleep(2500);
    let sol = (await evaluate()).assembly.solution;
    if (!sol.converged || sol.dof !== 1) throw new Error(`bisagra: ${JSON.stringify(sol)}`);
    const status = await b.eval(`document.querySelector('[aria-label="Estado del ensamble"]').textContent`);
    if (!status.includes("1 grado libre")) throw new Error(`estado: ${status}`);
    // El eje queda clavado en el centro de la tapa: se pisa con la caja
    await b.clickText("Revisar choques");
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-hit]")`)); t++) await sleep(500);
    if (!(await b.eval(`!!document.querySelector("[data-hit]")`))) throw new Error("no detectó el choque");
    await b.shot("ensamble");
    // Lista de materiales: las dos piezas, una de cada una
    const bom = await b.eval(`[...document.querySelectorAll("[data-bom-row]")].map((r) => r.children[0].textContent + "×" + r.children[1].textContent)`);
    if (bom.length !== 2 || !bom.every((r) => r.endsWith("×1"))) throw new Error(`lista: ${bom}`);
    // Con el ángulo impuesto ya no queda nada libre
    await b.eval(`window.__cadStore.editAssembly((a) => { a.mates[0].angle = 45; })`);
    await sleep(2000);
    sol = (await evaluate()).assembly.solution;
    if (sol.dof !== 0) throw new Error(`con ángulo: ${JSON.stringify(sol)}`);
    // Volver al diseño
    await b.clickText("Diseño");
    await sleep(800);
    if (!(await b.eval(`document.body.innerText.includes("Agregar") || document.body.innerText.includes("AGREGAR")`))) throw new Error("no volvió al diseño");
  },

  async "caja de regiones al editar una extrusión"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await sketchOn(b);
    await b.clickText("Círculo");
    for (const y of [-15, 15]) {
      await b.click(...(await at([-15, y, 0])));
      await b.click(...(await at([-10, y, 0])), { wait: 600 });
      await b.key("Escape", "Escape", 27);
    }
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    near((await body()).volume, 2 * Math.PI * 25 * 10, 0.5, "dos cilindros");
    await clickRow(b, "Extrusión 1");
    await b.eval(`document.querySelector('[aria-label="Regiones"]').click()`);
    await sleep(1500);
    await b.click(...(await at([-15, -15, 0])), { wait: 1200 });
    await b.shot("caja_de_regiones");
    const text = await b.eval(`document.querySelector('[aria-label="Regiones"]').textContent`);
    if (!text.includes("Región 1")) throw new Error(`caja: ${text}`);
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    near((await body()).volume, Math.PI * 25 * 10, 0.5, "solo la región elegida");
    await b.key("Enter", "Enter", 13);
    await sleep(1500);
    const ext = (await call("cad_get_document")).features[1].kind;
    if (ext.regions.type !== "points" || ext.regions.points.length !== 1) throw new Error(JSON.stringify(ext.regions));
  },

  async "herramienta translúcida en la vista previa"(b) {
    const tool = () => b.eval(`(() => { const t = __cadStore.tool(); return t && { op: t.op, triangles: t.mesh.indices.length / 3 }; })()`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    if ((await tool())?.op !== "join") throw new Error(`caja: ${JSON.stringify(await tool())}`);
    await accept(b);
    if (await tool()) throw new Error("la herramienta quedó después de aceptar");
    // Círculo sobre la cara de arriba, extruido hacia adentro y restando
    await b.clickText("Sketch");
    await sleep(300);
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([0, 0, 10])`)), { wait: 2500 });
    await b.clickText("Círculo");
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([0, 0, 10])`)));
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([5, 0, 10])`)), { wait: 800 });
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await b.eval(`__cadStore.commit((d) => { const k = d.features.at(-1).kind; k.op = "cut"; k.reverse = true; })`);
    await sleep(2000);
    const t = await tool();
    if (t?.op !== "cut" || !(t.triangles > 0)) throw new Error(`extrusión: ${JSON.stringify(t)}`);
    near((await body()).volume, 8000 - Math.PI * 25 * 10, 0.5, "vista previa del agujero");
    await b.shot("herramienta_resta");
    // Eligiendo regiones no se dibuja la herramienta
    await b.eval(`document.querySelector('[aria-label="Regiones"]').click()`);
    await sleep(1500);
    if (await tool()) throw new Error("herramienta mientras se elige");
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    if ((await tool())?.op !== "cut") throw new Error("no volvió la herramienta");
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    if (await tool()) throw new Error("la herramienta quedó después de cancelar");
  },

  async "diálogo de operación: cancelar y aceptar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const base = await call("cad_get_document");
    const dz = () => call("cad_get_document").then((d) => d.features[0].kind.shape.dz);
    // Tres cambios con vista previa y cancelar: el documento no cambia
    await clickRow(b, "Caja 1");
    for (const h of [30, 40, 50]) {
      await setInput(b, "Alto (Z)", h);
      await sleep(1000);
    }
    near((await body()).volume, 20 * 20 * 50, 1e-6, "vista previa");
    if ((await dz()) !== 20) throw new Error("la vista previa cambió el documento");
    await b.key("Escape", "Escape", 27);
    await sleep(1200);
    near((await body()).volume, 8000, 1e-6, "cancelar");
    if (JSON.stringify(await call("cad_get_document")) !== JSON.stringify(base)) throw new Error("cancelar no volvió al documento");
    // Dos cambios y aceptar con Enter: un solo paso de deshacer
    await clickRow(b, "Caja 1");
    await setInput(b, "Alto (Z)", 25);
    await sleep(1000);
    await setInput(b, "Alto (Z)", 40);
    await sleep(1000);
    await b.eval(`document.activeElement?.blur()`);
    await b.key("Enter", "Enter", 13);
    await sleep(1500);
    if ((await dz()) !== 40) throw new Error("aceptar no guardó");
    near((await body()).volume, 16000, 1e-6, "aceptado");
    await b.clickText("Deshacer");
    await sleep(1500);
    if ((await dz()) !== 20) throw new Error("deshacer no volvió en un paso");
    // Operación nueva cancelada: no queda en el árbol
    await b.clickText("Cilindro");
    await sleep(1500);
    await b.eval(`document.querySelector('[aria-label="Cancelar"]').click()`);
    await sleep(1200);
    if ((await call("cad_get_document")).features.length !== 1) throw new Error("el cilindro cancelado quedó");
    if (await b.eval(`[...document.querySelectorAll("span")].some((s) => s.textContent === "Cilindro 1")`)) throw new Error("el cilindro sigue en el árbol");
  },

  async "caja centrada y exportar el diseño"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    let r = await body();
    near(r.bbox_min[0], -10, 1e-6, "caja centrada en X");
    near(r.bbox_min[1], -10, 1e-6, "caja centrada en Y");
    near(r.bbox_min[2], -10, 1e-6, "centrada en Z: el centro en el origen");
    await accept(b);
    // Exportar del encabezado: con un diseño y sin modelo, exporta el sólido
    await b.clickContains("Exportar");
    await sleep(800);
    if (!(await b.eval(`document.body.innerText.includes("Exportar diseño")`))) throw new Error("no abrió la exportación del diseño");
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    for (const [fmt, label] of [["step", "Exportar STEP"], ["stl", "Exportar STL"]]) {
      await b.clickText(fmt.toUpperCase());
      const path = `${out}/caja.${fmt}`;
      rmSync(path, { force: true });
      await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
      await b.clickText(label);
      await sleep(2000);
      if (!existsSync(path)) throw new Error(`no se escribió ${path}`);
      const head = readFileSync(path).subarray(0, 80).toString("latin1");
      if (fmt === "step" && !head.startsWith("ISO-10303-21")) throw new Error(`STEP raro: ${head}`);
      if (fmt === "stl" && statSync(path).size !== 84 + 12 * 50) throw new Error(`STL de ${statSync(path).size} bytes`);
    }
  },

  async "triángulo cerrado con clics y agujero"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Línea");
    await b.click(400, 450);
    await b.click(700, 450);
    await b.click(550, 250);
    await b.click(400, 450);
    await sleep(800);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Círculo");
    await b.click(550, 400);
    await b.click(580, 400);
    await sleep(1000);
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("2")) throw new Error("regiones");
    await b.clickText("Terminar sketch");
    await sleep(1200);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    if ((await body()).faces !== 6) throw new Error("placa triangular con agujero");
  },

  async "agujero pasante sobre una cara"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(2000);
    await accept(b);
    await b.clickText("Sketch");
    await sleep(300);
    let [x, y] = await b.eval(`window.__cadViewer.screenOf([0, 0, 10])`);
    await b.click(x, y, { wait: 2500 });
    await b.clickText("Círculo");
    [x, y] = await b.eval(`window.__cadViewer.screenOf([0, 0, 10])`);
    const [x2, y2] = await b.eval(`window.__cadViewer.screenOf([5, 0, 10])`);
    await b.click(x, y);
    await b.click(x2, y2);
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const doc = await call("cad_get_document");
    const ext = doc.features.find((f) => f.kind.type === "extrude");
    ext.kind.op = "cut";
    ext.kind.extent = { type: "through_all" };
    await call("cad_set_document", { document: doc });
    near((await body()).volume, 8000 - Math.PI * 25 * 20, 0.5, "volumen con agujero");
  },

  async "anclajes a las aristas del sólido"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(2000);
    await accept(b);
    await b.clickText("Sketch");
    await sleep(300);
    let [x, y] = await b.eval(`window.__cadViewer.screenOf([-3, -3, 10])`);
    await b.click(x, y, { wait: 2500 });
    await b.clickText("Línea");
    const glyph = () => b.eval(`document.querySelector("[data-snap]")?.dataset.snap`);
    // Cerca de la esquina de la cara: vértice del sólido
    [x, y] = await b.eval(`window.__cadViewer.screenOf([10, 10, 10])`);
    await b.click(x - 3, y + 2);
    if ((await glyph()) !== "solid_vertex") throw new Error(`glifo en la esquina: ${await glyph()}`);
    // Cerca del medio de la arista de adelante
    [x, y] = await b.eval(`window.__cadViewer.screenOf([0, -10, 10])`);
    await b.mouse("mouseMoved", x + 3, y - 2, { buttons: 0 });
    await sleep(200);
    if ((await glyph()) !== "solid_midpoint") throw new Error(`glifo en el medio: ${await glyph()}`);
    await b.click(x + 3, y - 2, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const sk = (await call("cad_get_document")).features.find((f) => f.kind.type === "sketch").kind.sketch;
    const [p, q] = sk.points.filter((r) => r.id !== sk.origin);
    near(Math.hypot(p.x - q.x, p.y - q.y), Math.hypot(10, 20), 1e-3, "línea de la esquina al medio");
    if (sk.constraints.filter((c) => c.type === "fixed").length !== 2) throw new Error("los extremos no quedaron fijos");
    if (sk.constraints.some((c) => c.type === "length")) throw new Error("cota de largo entre dos anclajes");
  },

  async "cota editada en el visor"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    // Al terminar el rectángulo se piden sus cotas: ancho (escribir 40 + Enter) y alto (Enter: queda)
    if (!(await b.eval(`document.activeElement?.tagName === "INPUT"`))) throw new Error("no se pidió la cota");
    await b.send("Input.insertText", { text: "40" });
    await b.key("Enter", "Enter", 13);
    await sleep(600);
    await b.key("Enter", "Enter", 13);
    await sleep(1200);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const sk0 = (await call("cad_get_document")).features[0].kind.sketch;
    const pts = sk0.points.filter((p) => p.id !== sk0.origin).map((p) => p.x);
    near(Math.max(...pts) - Math.min(...pts), 40, 1e-6, "ancho acotado");
  },

  async "polígono y ranura"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Polígono");
    await b.click(350, 400);
    await b.click(450, 400);
    await b.clickText("Ranura");
    await b.click(600, 400);
    await b.click(800, 400);
    await b.click(800, 440);
    await sleep(1200);
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("2")) throw new Error("regiones");
  },

  async "redondear esquina del sketch"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    await b.clickText("Elegir");
    await b.click(700, 300, { wait: 500 });
    await setInput(b, "Radio", 10);
    await b.clickText("Redondear esquina");
    await sleep(1200);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const r = await body();
    const [w, h] = [r.bbox_max[0] - r.bbox_min[0], r.bbox_max[1] - r.bbox_min[1]];
    near(r.volume, w * h * 10 - (100 - (Math.PI * 100) / 4) * 10, 0.01, "volumen con esquina redondeada");
  },

  async "recortar y equidistante"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await b.clickText("Línea");
    await b.click(550, 250);
    await b.click(550, 550);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Recortar");
    await b.click(550, 270, { wait: 800 });
    await b.click(550, 530, { wait: 800 });
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("2")) throw new Error("recortar no partió el rectángulo");
    await b.clickText("Descartar");
    await sleep(800);
    // Equidistante sobre un rectángulo nuevo
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await b.clickText("Elegir");
    await b.click(550, 500, { wait: 500 });
    await setInput(b, "Distancia", 5);
    await b.clickText("Equidistante");
    await sleep(1200);
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("2")) throw new Error("equidistante");
  },

  async "parámetros y fórmulas"(b) {
    await begin(b);
    // La caja queda con su diálogo abierto: parámetros y campos van al borrador
    await b.clickText("Caja");
    await sleep(1500);
    // Parámetro nuevo (p1 = 10) renombrado a "ancho" y puesto en 30
    await b.click(...(await b.eval(`(() => { const e = document.querySelector('[aria-label="Agregar parámetro"]'); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`)), { wait: 1200 });
    const setParam = (nth, value) =>
      b.eval(`(() => {
        // La fila del parámetro (nombre = expresión), no el campo de la caja con la fórmula
        const row = [...document.querySelectorAll("input")].filter((i) => (i.value === "p1" || i.value === "ancho") && i.nextElementSibling?.textContent === "=")[0].parentElement;
        const i = row.querySelectorAll("input")[${nth}];
        i.value = ${JSON.stringify(value)};
        i.dispatchEvent(new Event("change", { bubbles: true }));
      })()`);
    await setParam(0, "ancho");
    await sleep(1200);
    await setParam(1, "30");
    await sleep(1200);
    // Ancho de la caja = ancho, alto = ancho / 3 (la caja quedó elegida)
    await setInput(b, "Ancho (X)", "ancho");
    await sleep(1500);
    await setInput(b, "Alto (Z)", "ancho / 3");
    await sleep(1500);
    let r = await body();
    near(r.bbox_max[0] - r.bbox_min[0], 30, 1e-6, "ancho = 30");
    near(r.bbox_max[2] - r.bbox_min[2], 10, 1e-6, "alto = ancho / 3");
    const fx = await b.eval(`[...document.querySelectorAll("span")].filter((s) => s.textContent === "fx").length`);
    if (fx < 2) throw new Error("faltan las marcas fx");
    await setParam(1, "45");
    await sleep(2000);
    r = await body();
    near(r.bbox_max[0] - r.bbox_min[0], 45, 1e-6, "ancho = 45");
    near(r.bbox_max[2] - r.bbox_min[2], 15, 1e-6, "alto sigue la fórmula");
    // Fórmula inválida: queda marcada y no cambia nada
    await setInput(b, "Alto (Z)", "ancho * ");
    await sleep(800);
    const bad = await b.eval(`!!document.querySelector("input.border-error")`);
    if (!bad) throw new Error("la fórmula mala no se marcó");
    // Cota del sketch con fórmula desde el visor
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    // La cota del ancho se pide sola: fórmula + Enter; el alto, Escape (queda lo dibujado)
    await b.send("Input.insertText", { text: "ancho * 2" });
    await b.key("Enter", "Enter", 13);
    await sleep(600);
    await b.key("Escape", "Escape", 27);
    await sleep(1200);
    const label = await b.eval(`[...document.querySelectorAll("button.font-mono")].map((x) => x.textContent).find((t) => t.startsWith("ancho"))`);
    if (!label?.startsWith("ancho * 2 = 90")) throw new Error(`etiqueta: ${label}`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const doc = await call("cad_get_document");
    const key = Object.keys(doc.bindings).find((k) => k.includes("constraints"));
    if (!key || doc.bindings[key] !== "ancho * 2") throw new Error("la cota no quedó vinculada");
  },

  async "arco tangente, extender, recortar círculo y gestor de restricciones"(b) {
    const sketchDoc = async () => {
      await b.clickText("Terminar sketch");
      await sleep(1500);
      return (await call("cad_get_document")).features.at(-1).kind.sketch;
    };
    const status = () => call("cad_evaluate").then((r) => r.sketches.at(-1).report.status);
    await begin(b);
    // Línea y dos arcos tangentes encadenados
    await sketchOn(b);
    await b.clickText("Línea");
    await b.click(400, 400);
    await b.click(600, 400);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Tangente");
    await b.click(600, 400);
    await b.click(650, 350, { wait: 600 });
    await b.click(700, 400, { wait: 600 });
    await sleep(800);
    let sk = await sketchDoc();
    const kinds = sk.entities.map((e) => e.geometry.type).sort().join(",");
    if (kinds !== "arc,arc,line") throw new Error(`entidades: ${kinds}`);
    if (sk.constraints.filter((c) => c.type === "tangent").length !== 2) throw new Error("tangencias");
    if ((await status()) === "over_constrained") throw new Error("arcos tangentes en conflicto");

    // Extender una línea hasta otra vertical
    await sketchOn(b);
    await b.clickText("Línea");
    await b.click(600, 300);
    await b.click(600, 500);
    await b.key("Escape", "Escape", 27);
    await b.click(400, 400);
    await b.click(500, 400);
    await b.key("Escape", "Escape", 27);
    await b.clickContains("Extender");
    await b.click(495, 400, { wait: 800 });
    sk = await sketchDoc();
    const lines = sk.entities.filter((e) => e.geometry.type === "line");
    if (lines.length !== 3) throw new Error(`líneas tras extender: ${lines.length}`);
    const xs = sk.points.map((p) => p.x);
    const vertical = sk.points.filter((p) => Math.abs(p.x - Math.max(...xs)) < 1e-6).length;
    if (vertical < 3) throw new Error("la extensión no llegó a la vertical");

    // Recortar el tramo de un círculo que queda dentro de un rectángulo
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await b.key("Escape", "Escape", 27); // las cotas pedidas quedan como se dibujó
    await b.clickText("Círculo");
    await b.click(700, 400);
    await b.click(750, 400);
    await b.clickText("Recortar");
    await b.click(650, 400, { wait: 800 });
    const regions = await b.eval(`(document.body.innerText.match(/\\d+ regiones cerradas/) ?? [""])[0]`);
    // Gestor: elegir lo que nombra la primera restricción
    await b.eval(`[...document.querySelectorAll("button")].find((x) => x.title === "Elegir lo que restringe")?.click()`);
    await sleep(400);
    const chosen = await b.eval(`(document.body.innerText.match(/ELEGIDO \\((\\d+)\\)/i) ?? [])[1]`);
    sk = await sketchDoc();
    if (sk.entities.some((e) => e.geometry.type === "circle")) throw new Error("el círculo no pasó a arco");
    if (!regions.startsWith("2")) throw new Error(`regiones: ${regions}`);
    if (!(Number(chosen) > 0)) throw new Error(`gestor: elegido ${chosen}`);
  },

  async "selección como Onshape: región, cara y arista"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Dos círculos en la planta, fuera de la caja
    await sketchOn(b);
    await b.clickText("Círculo");
    for (const y of [-15, 15]) {
      await b.click(...(await at([-15, y, 0])));
      await b.click(...(await at([-10, y, 0])), { wait: 600 });
      await b.key("Escape", "Escape", 27);
    }
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Elegir solo la región del primer círculo y extruir
    await b.click(...(await at([-15, -15, 0])), { wait: 500 });
    const summary = await b.eval(`[...document.querySelectorAll("span")].map((x) => x.textContent).find((t) => t?.startsWith("Elegido"))`);
    if (summary !== "Elegido: 1 región") throw new Error(`selección: ${summary}`);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    near((await body()).volume, 8000 + Math.PI * 25 * 10, 0.5, "solo la región elegida");
    // Cara superior de la caja elegida → sketch sobre ella
    await b.click(...(await at([0, 0, 10])), { wait: 600 });
    await b.clickText("Sketch");
    await sleep(1500);
    const planes = (await evaluate()).sketches.map((v) => v.plane.origin[2]);
    if (!planes.some((z) => Math.abs(z - 10) < 1e-6)) throw new Error(`plano del sketch: ${planes}`);
    await b.clickText("Descartar");
    await sleep(800);
    // Arista superior frontal elegida → redondeo directo
    await b.key("Escape", "Escape", 27);
    const faces = (await body()).faces;
    await b.click(...(await at([0, -10, 10])), { wait: 600 });
    await b.clickText("Redondeo");
    await sleep(2000);
    if ((await body()).faces !== faces + 1) throw new Error("el redondeo no tomó la arista elegida");
  },

  async "anclajes: origen, punto medio y cuadrante"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    const [W, H] = [0.4 * S, 0.3 * S];
    // Rectángulo desde el origen (clic 3 px al lado: se pega igual)
    await b.clickText("Rectángulo");
    const [ox, oy] = await at(0, 0);
    await b.click(ox + 3, oy - 2);
    await b.click(...(await at(W, H)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // Línea de medio a medio (abajo y arriba)
    await b.clickText("Línea");
    const [mx, my] = await at(W / 2, 0);
    await b.click(mx + 2, my + 2);
    // El glifo dice a qué se pega
    const glyph = await b.eval(`document.querySelector("[data-snap]")?.dataset.snap`);
    if (glyph !== "midpoint") throw new Error(`glifo: ${glyph}`);
    await b.click(...(await at(W / 2, H)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    // Círculo a la derecha y línea desde su cuadrante superior
    await b.clickText("Círculo");
    const R = 0.05 * S;
    await b.click(...(await at(W + 4 * R, H / 2)));
    await b.click(...(await at(W + 5 * R, H / 2)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Línea");
    await b.click(...(await at(W + 4 * R, H / 2 + R)));
    await b.click(...(await at(W + 7 * R, H / 2 + R)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("3")) throw new Error("regiones");
    await b.clickText("Terminar sketch");
    await sleep(1200);

    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const lines = sk.entities.filter((e) => e.geometry.type === "line");
    if (!lines.some((e) => e.geometry.start === sk.origin || e.geometry.end === sk.origin)) throw new Error("el rectángulo no sale del origen");
    const count = (t) => sk.constraints.filter((c) => c.type === t).length;
    if (count("equal") !== 2) throw new Error(`mitades iguales: ${count("equal")}`);
    if (count("vertical_points") !== 1) throw new Error("cuadrante sin alinear con el centro");
    // Ancho a 2W: la línea del medio sigue en el medio y el rectángulo en el origen
    const width = sk.constraints.findIndex((c) => c.type === "distance");
    if (width < 0) throw new Error("la cota de ancho no pasó a distancia");
    sk.constraints[width].value = 2 * W;
    await call("cad_set_document", { document: doc });
    const solved = (await evaluate()).sketches[0].sketch;
    const pt = new Map(solved.points.map((p) => [p.id, p]));
    const xs = solved.points.filter((p) => p.y > -1e-6 && p.y < H + 1e-6 && p.x < 2 * W + 1e-6).map((p) => p.x);
    near(Math.min(...xs), 0, 1e-6, "esquina en el origen");
    const mid = lines.find((e) => {
      const [a, c] = [pt.get(e.geometry.start), pt.get(e.geometry.end)];
      return Math.abs(a.x - c.x) < 1e-6 && Math.abs(a.y - c.y) > H / 2 && a.x > 1e-3 && a.x < 2 * W - 1e-3;
    });
    if (!mid) throw new Error("no está la línea del medio");
    near(pt.get(mid.geometry.start).x, W, 1e-6, "línea del medio");
  },

  async "anclajes: intersección"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    const line = async (p, q) => {
      await b.clickText("Línea");
      await b.click(...(await at(...p)));
      await b.click(...(await at(...q)), { wait: 800 });
      await b.key("Escape", "Escape", 27);
      await b.key("Escape", "Escape", 27);
    };
    // y = x y una que la cruza fuera de los puntos medios, lejos (> 8 px) de
    // alinearse con otros puntos o de quedar perpendicular
    await line([0.1, 0.1], [0.35, 0.35]);
    await line([0.14, 0.42], [0.4, 0.25]);
    const t = 0.28 / 0.43;
    const X = 0.14 + 0.26 * t;
    await b.clickText("Círculo");
    const [cx, cy] = await at(X, X);
    await b.click(cx + 3, cy - 2);
    if ((await b.eval(`document.querySelector("[data-snap]")?.dataset.snap`)) !== "intersection") throw new Error("sin glifo de intersección");
    await b.click(...(await at(X + 0.04, X)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const sk = (await call("cad_get_document")).features[0].kind.sketch;
    const circle = sk.entities.find((e) => e.geometry.type === "circle");
    const c = sk.points.find((p) => p.id === circle.geometry.center);
    // Las cotas de largo se redondean al dibujar: el solver corre un poco las líneas libres
    near(c.x, X * S, 0.1, "centro x");
    near(c.y, X * S, 0.1, "centro y");
    const ends = sk.entities.filter((e) => e.geometry.type === "line" && [e.geometry.start, e.geometry.end].includes(c.id)).length;
    if (ends !== 4) throw new Error(`las líneas no se partieron en el cruce: ${ends}`);
  },

  async "anclajes: alineado y perpendicular"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    const glyph = () => b.eval(`document.querySelector("[data-snap]")?.dataset.snap`);
    await b.clickText("Rectángulo");
    await b.click(...(await at(0, 0)));
    await b.click(...(await at(0.4, 0.3)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // Polilínea: el primer punto a la altura de la esquina de arriba, el
    // tercero perpendicular al primer tramo
    await b.clickText("Línea");
    const [x1, y1] = await at(0.52, 0.3);
    await b.click(x1, y1 + 3);
    if ((await glyph()) !== "aligned") throw new Error(`glifo alineado: ${await glyph()}`);
    await b.click(...(await at(0.64, 0.18)), { wait: 600 });
    const [x3, y3] = await at(0.7, 0.24);
    await b.click(x3 + 1, y3 + 1);
    if ((await glyph()) !== "perpendicular") throw new Error(`glifo perpendicular: ${await glyph()}`);
    await sleep(600);
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    const count = (t) => doc.features[0].kind.sketch.constraints.filter((c) => c.type === t).length;
    if (count("horizontal_points") !== 1 || count("perpendicular") !== 1) throw new Error(JSON.stringify(doc.features[0].kind.sketch.constraints));
    // Más alto el rectángulo: el punto alineado sube con él y el ángulo recto se mantiene
    const sk = doc.features[0].kind.sketch;
    sk.constraints.filter((c) => c.type === "length").forEach((c) => {
      const g = sk.entities.find((e) => e.id === c.line).geometry;
      const [a, q] = [sk.points.find((p) => p.id === g.start), sk.points.find((p) => p.id === g.end)];
      if (Math.abs(a.x - q.x) < 1e-6) c.value = 0.5 * S; // la cota de alto del rectángulo
    });
    await call("cad_set_document", { document: doc });
    const solved = (await evaluate()).sketches[0].sketch;
    const pt = new Map(solved.points.map((p) => [p.id, p]));
    const aligned = solved.constraints.find((c) => c.type === "horizontal_points");
    near(pt.get(aligned.b).y, 0.5 * S, 1e-6, "punto alineado");
    const perp = solved.constraints.find((c) => c.type === "perpendicular");
    const dir = (id) => {
      const g = solved.entities.find((e) => e.id === id).geometry;
      const [a, q] = [pt.get(g.start), pt.get(g.end)];
      const l = Math.hypot(q.x - a.x, q.y - a.y);
      return [(q.x - a.x) / l, (q.y - a.y) / l];
    };
    const [u, v] = [dir(perp.a), dir(perp.b)];
    near(u[0] * v[0] + u[1] * v[1], 0, 1e-6, "ángulo recto");
  },

  async "círculos tangentes desde el panel"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    const circle = async (c, r) => {
      await b.clickText("Círculo");
      await b.click(...(await at(...c)));
      await b.click(...(await at(c[0] + r, c[1])), { wait: 800 });
      await b.key("Escape", "Escape", 27);
    };
    await circle([0.2, 0.2], 0.08);
    await circle([0.5, 0.25], 0.05);
    // Elegir los dos (Mayús suma) y "Tangentes" en el panel
    await b.clickText("Elegir");
    await b.click(...(await at(0.2, 0.28)), { wait: 300 });
    await b.click(...(await at(0.5, 0.3)), { wait: 300, modifiers: 8 });
    await b.clickText("Tangentes");
    await sleep(1200);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const solved = (await evaluate()).sketches[0].sketch;
    const circles = solved.entities.filter((e) => e.geometry.type === "circle").map((e) => e.geometry);
    const c = circles.map((g) => solved.points.find((p) => p.id === g.center));
    near(Math.hypot(c[1].x - c[0].x, c[1].y - c[0].y), circles[0].radius + circles[1].radius, 1e-6, "tangentes por fuera");
    near(circles[0].radius + circles[1].radius, 0.13 * S, 0.11, "los radios acotados no cambian");
  },

  async "colores por entidad y cota de referencia"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    // Rectángulo desde el origen: con sus dos cotas queda definido
    await b.clickText("Rectángulo");
    await b.click(...(await at(0, 0)));
    await b.click(...(await at(0.4, 0.3)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // Círculo suelto: el diámetro tiene cota, el centro no
    await b.clickText("Círculo");
    await b.click(...(await at(0.6, 0.15)));
    await b.click(...(await at(0.66, 0.15)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    // Una cota de más: elegir la línea de abajo y pedir "Largo"
    await b.clickText("Elegir");
    await b.click(...(await at(0.2, 0)), { wait: 300 });
    await b.clickText("Largo");
    await sleep(1200);
    if (!(await b.eval(`document.body.innerText.includes("Esta cota sobre-define el sketch")`))) throw new Error("no ofreció dejarla de referencia");
    await b.clickText("Dejarla de referencia");
    await sleep(1200);
    if (await b.eval(`document.body.innerText.includes("Esta cota sobre-define el sketch")`)) throw new Error("sigue sobre-definido");
    const label = await b.eval(`[...document.querySelectorAll("button")].map((x) => x.textContent).find((t) => /^\\(\\d/.test(t ?? ""))`);
    if (!label) throw new Error("sin etiqueta entre paréntesis");
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const r = await call("cad_solve_sketch", { sketch: sk, drag: null });
    if (r.report.status !== "under_constrained") throw new Error(`estado: ${r.report.status}`);
    const circle = sk.entities.find((e) => e.geometry.type === "circle").id;
    // Solo el círculo (centro libre) queda azul
    if (JSON.stringify(r.report.free_entities) !== JSON.stringify([circle])) throw new Error(`libres: ${JSON.stringify(r.report.free_entities)}`);
    const ref = sk.constraints.find((c) => c.reference);
    if (!ref) throw new Error("la cota de referencia no se guardó");
    near(ref.value, 0.4 * S, 0.11, "la referencia mide el ancho");
  },

  async "rectángulo por el centro, arco por 3 puntos, punto y Q"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    // Rectángulo centrado en el origen
    await b.clickText("Rect. centro");
    await b.click(...(await at(0, 0)));
    await b.click(...(await at(0.2, 0.15)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // Arco por 3 puntos: de A a B pasando por arriba (va horario: se guarda de B a A)
    await b.clickText("Arco 3 p.");
    await b.click(...(await at(0.45, 0.05)));
    await b.click(...(await at(0.65, 0.05)));
    await b.click(...(await at(0.55, 0.12)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Punto");
    await b.click(...(await at(0.3, -0.3)), { wait: 600 });
    // Q: el lado de arriba pasa a construcción
    await b.clickText("Elegir");
    await b.click(...(await at(0.1, 0.15)), { wait: 300 });
    await b.key("q", "KeyQ", 81);
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const arc = sk.entities.find((e) => e.geometry.type === "arc");
    if (!arc) throw new Error("sin arco");
    const pt = (id) => sk.points.find((p) => p.id === id);
    const [c, a0] = [pt(arc.geometry.center), pt(arc.geometry.start)];
    near(Math.hypot(a0.x - c.x, a0.y - c.y), 0.10643 * S, 0.11, "radio del arco por 3 puntos");
    if (a0.x < c.x) throw new Error("el arco por arriba de izquierda a derecha debía guardarse al revés");
    if (!sk.entities.some((e) => e.geometry.type === "point")) throw new Error("sin punto suelto");
    if (sk.entities.filter((e) => e.construction).length !== 3) throw new Error("Q no pasó la línea a construcción");
    // Más ancho: sigue centrado en el origen
    const width = sk.constraints.find((k) => k.type === "length");
    width.value = 0.6 * S;
    await call("cad_set_document", { document: doc });
    const solved = (await evaluate()).sketches[0].sketch;
    const xs = solved.entities
      .filter((e) => e.geometry.type === "line" && !e.construction)
      .flatMap((e) => [e.geometry.start, e.geometry.end])
      .map((id) => solved.points.find((p) => p.id === id).x);
    near(Math.max(...xs), 0.3 * S, 1e-6, "lado derecho");
    near(Math.min(...xs), -0.3 * S, 1e-6, "lado izquierdo");
  },

  async "simetría y patrones en el sketch"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    // Eje vertical desde el origen y un círculo a la derecha
    await b.clickText("Línea");
    await b.click(...(await at(0, 0)));
    await b.click(...(await at(0, 0.3)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Círculo");
    await b.click(...(await at(0.15, 0.1)));
    await b.click(...(await at(0.19, 0.1)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // Simetría: primero el eje, después el círculo
    await b.clickText("Elegir");
    await b.click(...(await at(0, 0.2)), { wait: 300 });
    await b.click(...(await at(0.15, 0.14)), { wait: 300, modifiers: 8 });
    await b.clickText("Simetría (eje: primera línea elegida)");
    await sleep(1000);
    // Patrón lineal del círculo de la derecha: 3 en total, de a 20 mm en x
    await b.click(...(await at(0.15, 0.14)), { wait: 300 });
    await setInput(b, "Cantidad", 3);
    await b.clickText("Repetir en patrón");
    await sleep(1000);
    // Patrón circular del reflejado alrededor del origen: 4 en total
    await b.click(...(await at(-0.15, 0.14)), { wait: 300 });
    await setInput(b, "Cantidad", 4);
    await b.clickText("Lineal");
    await b.clickText("Circular");
    await b.clickText("Repetir en patrón");
    await sleep(1200);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const centers = () => sk.entities.filter((e) => e.geometry.type === "circle").map((e) => sk.points.find((p) => p.id === e.geometry.center));
    if (centers().length !== 7) throw new Error(`círculos: ${centers().length}`);
    const has = (list, x, y, what) => {
      if (!list.some((c) => Math.abs(c.x - x) < 0.05 && Math.abs(c.y - y) < 0.05)) throw new Error(`${what}: ${JSON.stringify(list.map((c) => [c.x, c.y]))}`);
    };
    const [x0, y0] = [0.15 * S, 0.1 * S];
    has(centers(), -x0, y0, "reflejado");
    has(centers(), x0 + 20, y0, "copia lineal 1");
    has(centers(), x0 + 40, y0, "copia lineal 2");
    has(centers(), -y0, -x0, "copia circular a 90°");
    has(centers(), x0, -y0, "copia circular a 180°");
    // El paso del patrón lineal es una cota: a 25 mm las copias la siguen
    const step = sk.constraints.find((k) => k.type === "horizontal_distance");
    step.value = 25;
    await call("cad_set_document", { document: doc });
    const solved = (await evaluate()).sketches[0].sketch;
    const after = solved.entities.filter((e) => e.geometry.type === "circle").map((e) => solved.points.find((p) => p.id === e.geometry.center));
    // El original no tiene cotas de posición (el solver también lo corre): se mide relativo
    const row = after.filter((c) => c.x > 0 && Math.abs(c.y - after[0].y) < 1e-6).map((c) => c.x).sort((a, b) => a - b);
    if (row.length !== 3) throw new Error(`fila del patrón: ${row}`);
    near(row[1] - row[0], 25, 1e-6, "paso 1");
    near(row[2] - row[0], 50, 1e-6, "paso 2");
  },

  async "elipse extruida"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    // Centro en el origen y eje mayor sobre x (se alinea con el origen): queda definida
    await b.clickText("Elipse");
    await b.click(...(await at(0, 0)));
    await b.click(...(await at(0.2, 0.004)));
    await b.click(...(await at(0.05, 0.08)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await sleep(600);
    if (!(await sketchText(b, "/Totalmente definido/"))) throw new Error("la elipse no quedó definida");
    await b.clickText("Terminar sketch");
    await sleep(1200);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const doc = await call("cad_get_document");
    const [ra, rb] = doc.features[0].kind.sketch.constraints.filter((c) => c.type === "distance").map((c) => c.value);
    const h = doc.features[1].kind.extent.distance;
    near((await body()).volume, Math.PI * ra * rb * h, 1e-6 * ra * rb * h, "volumen de la elipse");
  },

  async "spline con manijas"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    // Spline abierta de tres puntos (Esc la termina) cerrada con una línea
    await b.clickText("Spline");
    for (const p of [[0.05, 0.05], [0.15, 0.15], [0.3, 0.05]]) await b.click(...(await at(...p)));
    await b.key("Escape", "Escape", 27);
    await sleep(600);
    await b.clickText("Línea");
    await b.click(...(await at(0.3, 0.05)));
    await b.click(...(await at(0.05, 0.05)), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    if (!(await sketchText(b, "/\\d+ regiones cerradas/")).startsWith("1")) throw new Error("la spline y la línea no cierran");
    // Elegir la spline (sobre la curva, entre el primer y el segundo punto) y ponerle manijas
    await b.clickText("Elegir");
    await b.click(...(await at(0.0969, 0.1125)), { wait: 300 });
    await b.clickText("Manijas en los extremos sí/no");
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1200);
    await b.clickText("Extrusión");
    await sleep(2000);
    await accept(b);
    const v0 = (await body()).volume;
    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const g = sk.entities.find((e) => e.geometry.type === "spline").geometry;
    if (g.start_handle === undefined || g.end_handle === undefined) throw new Error("sin manijas");
    // Manijas hacia arriba en los dos extremos (salida hacia arriba, llegada hacia abajo): más área
    const pt = (id) => sk.points.find((p) => p.id === id);
    const [a, z] = [pt(g.points[0]), pt(g.points[g.points.length - 1])];
    Object.assign(pt(g.start_handle), { x: a.x, y: a.y + 5 });
    Object.assign(pt(g.end_handle), { x: z.x, y: z.y - 5 });
    sk.constraints.push({ type: "fixed", point: g.start_handle, x: a.x, y: a.y + 5 }, { type: "fixed", point: g.end_handle, x: z.x, y: z.y - 5 });
    await call("cad_set_document", { document: doc });
    const r = await body();
    if (!r.valid) throw new Error("sólido inválido");
    if (!(r.volume > v0 * 1.05)) throw new Error(`volumen ${v0} → ${r.volume}`);
  },

  async "texto extruido"(b) {
    await begin(b);
    await sketchOn(b);
    const S = await b.eval(`window.__cadViewer.planeSize`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x * S}, ${y * S}, 0])`);
    await b.clickText("Texto");
    await b.eval(`(() => {
      const i = document.querySelector('input[aria-label="Texto"]');
      i.value = "Hola";
      i.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await b.click(...(await at(0.07, 0.13)), { wait: 2000 });
    // H (1) + o (anillo y su agujero) + l (1) + a (cuerpo y agujero)
    const regions = parseInt(await sketchText(b, "/\\d+ regiones cerradas/"));
    if (!(regions >= 5)) throw new Error(`regiones: ${regions}`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.clickText("Extrusión");
    await sleep(3000);
    await accept(b);
    const r = await body();
    if (!r.valid) throw new Error("sólido inválido");
    const w = r.bbox_max[0] - r.bbox_min[0];
    // "Hola" a 10 mm en Liberation Sans mide unos 19 mm de ancho
    if (!(w > 15 && w < 25)) throw new Error(`ancho ${w}`);
    if (!(r.volume > 0)) throw new Error("sin volumen");
  },

  async "escaneo: comparar el diseño con colores de desviación"(b) {
    await b.eval(`window.__nextPath = [${JSON.stringify(SPECULUM)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(4000);
    await begin(b);
    await b.clickContains("Cilindro / agujero");
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([6.3, -6.3, 38])`)), { wait: 3000 });
    await b.clickContains("Agregar el cilindro");
    await sleep(3000);
    await b.eval(`[...document.querySelectorAll('[aria-label="Comparar con el escaneo"] button')].find((x) => x.textContent.trim() === "Comparar").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Dentro de tolerancia")`)); t++) await sleep(500);
    const text = await b.eval(`document.querySelector('[aria-label="Comparar con el escaneo"]').innerText`);
    const within = parseFloat((text.match(/Dentro de tolerancia\s+([\d,]+)/) ?? [])[1]?.replace(",", "."));
    // El cilindro calza con su zona del escaneo: una parte apreciable queda dentro de ±0,2 mm
    if (!(within > 1)) throw new Error(`dentro de tolerancia: ${text}`);
    if (!(await b.eval(`!!window.__cadViewer.scan.geometry.getAttribute("color")`))) throw new Error("el escaneo no se coloreó");
    await b.shot("desviacion");
    await b.clickText("Quitar los colores");
    await sleep(500);
    if (await b.eval(`!!window.__cadViewer.scan.geometry.getAttribute("color")`)) throw new Error("quedaron los colores");
  },

  async "escaneo: cilindro elegido con un clic"(b) {
    await b.eval(`window.__nextPath = [${JSON.stringify(SPECULUM)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(4000);
    await begin(b);
    await b.clickContains("Cilindro / agujero");
    // Pared del cilindro superior del espéculo (r 9, z 30..45), del lado de la cámara
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([6.3, -6.3, 38])`)), { wait: 3000 });
    await b.clickContains("Agregar el cilindro");
    await sleep(3000);
    const r = await body();
    near(r.bbox_max[0] - r.bbox_min[0], 18, 0.05, "diámetro del cilindro escaneado");
  },

  async "outliner: planos, operaciones y piezas"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Plano 1", kind: { type: "plane", def: { type: "offset", base: { type: "xy" }, distance: 15 } } });
      d.features.push({ id: 51, name: "Cilindro aparte", kind: { type: "primitive", shape: { type: "cylinder", radius: 5, height: 10 }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 52;
    })`);
    await sleep(2500);
    // Filas del Outliner: sangría y texto
    const rows = () => b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    const row = (label) => `[...document.querySelector("[data-outliner]").querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(label)})`;
    const expect = ["PLANOS", "Grid", "Planta (XY)", "Frente (XZ)", "Lateral (YZ)", "Plano 1", "OPERACIONES", "OBJETOS", "Pieza 1", "Pieza 2"];
    const got = await rows();
    if (JSON.stringify(got) !== JSON.stringify(expect)) throw new Error(`árbol: ${JSON.stringify(got)}`);
    // Las operaciones son el árbol de Diseñar, dentro del grupo Operaciones
    const tree = await b.eval(`document.querySelector("[data-outliner] [data-feature-tree]")?.innerText ?? ""`);
    if (!["Caja 1", "Plano 1", "Cilindro aparte"].every((n) => tree.includes(n))) throw new Error(`operaciones: ${tree}`);
    // Elegir la pieza la abre en la lista de piezas
    await b.eval(`${row("Pieza 2")}.click()`);
    await sleep(500);
    if ((await b.eval(`window.__cadUi.openPart()`)) !== "51:0") throw new Error("no se abrió la pieza");
    // Ojo: el último botón de la fila
    const eye = (label) => b.eval(`(() => { const bs = ${row(label)}.parentElement.querySelectorAll("button"); bs[bs.length - 1].click(); })()`);
    await eye("Pieza 2");
    await sleep(1500);
    if (!(await b.eval(`window.__cadStore.doc().parts?.[0]?.hidden`))) throw new Error("no se ocultó la pieza");
    // La grilla también se apaga en el visor de Diseñar
    await eye("Grid");
    await sleep(300);
    if (await b.eval(`window.__cadViewer.grid.visible`)) throw new Error("la grilla sigue visible");
    await eye("Grid");
    await eye("Frente (XZ)");
    await sleep(300);
    if (JSON.stringify(await b.eval(`window.__cadUi.hiddenPlanes()`)) !== '["xz"]') throw new Error("no se ocultó el plano");
    // Elegir una operación abre su diálogo
    await b.eval(`${row("Plano 1")}.click()`);
    await sleep(1200);
    if ((await b.eval(`window.__cadStore.draft()?.feature`)) !== 50) throw new Error("no se abrió el plano");
    await accept(b, 1000);
    // Borrar la pieza (dos clics en la papelera)
    for (let k = 0; k < 2; k++) {
      await b.eval(`[...${row("Pieza 2")}.parentElement.querySelectorAll("button")].find((x) => x.title.includes("orrar")).click()`);
      await sleep(300);
    }
    await sleep(2500);
    const parts = await b.eval(`window.__cadStore.result().parts.length`);
    if (parts !== 1) throw new Error(`quedaron ${parts} piezas`);
    await b.shot("outliner");
  },

  async "objetos: las piezas pasan solas a Fabricar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 51, name: "Cilindro aparte", kind: { type: "primitive", shape: { type: "cylinder", radius: 5, height: 10 }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 52;
    })`);
    await sleep(2500);
    const rows = () => b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    const row = (label) => `[...document.querySelector("[data-outliner]").querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(label)})`;
    // Sin botones: a Fabricar y la primera pieza ya tiene malla
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1500);
    let got = await rows();
    if (!got.includes("Malla generada") || got.indexOf("Malla generada") !== got.indexOf("Pieza 1") + 1) throw new Error(`sin malla bajo la pieza 1: ${JSON.stringify(got)}`);
    // Otra pieza: clic en el Outliner y pasa a ser el objeto activo
    await b.eval(`${row("Pieza 2")}.click()`);
    for (let t = 0; t < 40 && (await rows()).indexOf("Malla generada") !== (await rows()).indexOf("Pieza 2") + 1; t++) await sleep(250);
    got = await rows();
    if (got.indexOf("Malla generada") !== got.indexOf("Pieza 2") + 1) throw new Error(`la pieza 2 no quedó activa: ${JSON.stringify(got)}`);
    const objs = await b.eval(`JSON.stringify(window.__objects?.().map((o) => [o.name, !!o.source.hash, !!o.ui]))`);
    if (objs !== JSON.stringify([["Pieza 1", true, true], ["Pieza 2", true, false]])) throw new Error(`objetos: ${objs}`);
    // La pieza 1 se sigue viendo, en gris
    for (let t = 0; t < 20 && (await b.eval(`window.__viewer().ghostCount`)) !== 1; t++) await sleep(250);
    if ((await b.eval(`window.__viewer().ghostCount`)) !== 1) throw new Error("la pieza 1 no se ve en gris");
    await b.shot("objetos-fantasma");
    // Se cambia el diseño (cilindro más ancho) y al volver a Fabricar la malla se regenera sola
    const before = await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features.find((f) => f.id === 51).kind.shape.radius = 8; })`);
    await sleep(2000);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && (await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`)) === before; t++) await sleep(250);
    const after = await b.eval(`window.__objects().find((o) => o.name === "Pieza 2").source.hash`);
    if (after === before) throw new Error("la malla no se regeneró");
    await b.shot("objetos");
  },

  async "objetos: lo hecho sobre la malla se rehace si cambia el diseño"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__objects?.()[0]?.source.hash`)); t++) await sleep(250);
    await sleep(1000);
    const dims = async () => (await call("analyze_print3d")).dimensions;
    const d0 = await dims();
    if (!(await call("analyze_print3d")).is_closed) throw new Error("la malla de la pieza no quedó cerrada");
    // Escalar ×2 desde Fabricar
    await b.clickText("Analizar para impresión");
    await sleep(1000);
    await b.eval(`(() => { const i = [...document.querySelectorAll("label")].find((l) => l.textContent.trim().startsWith("×"))?.querySelector("input"); i.value = "2"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
    await b.clickText("Escalar");
    await sleep(2000);
    const d1 = await dims();
    near(d1[0], 2 * d0[0], 1e-3 * d0[0], "ancho escalado");
    // El diseño cambia: la caja el doble de ancha
    const hash = await b.eval(`window.__objects()[0].source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features[0].kind.shape.dx = 40; })`);
    await sleep(2000);
    await b.clickText("Fabricar");
    for (let t = 0; t < 60 && (await b.eval(`window.__objects()[0].source.hash`)) === hash; t++) await sleep(250);
    await sleep(2000);
    // La malla nueva sale ya escalada: el ancho es 2 × 2 y lo demás 2 ×
    const d2 = await dims();
    near(d2[0], 4 * d0[0], 1e-3 * d0[0], "ancho rehecho");
    near(d2[1], 2 * d0[1], 1e-3 * d0[1], "alto rehecho");
    // El panel muestra el análisis de la malla nueva
    const shown = await b.eval(`document.body.innerText.match(/Dimensiones\\s*([\\d.]+)/)?.[1]`);
    near(parseFloat(shown), d2[0], 0.1, "ancho en el panel");
    const status = await b.eval(`document.body.innerText.match(/El diseño cambió[^\\n]*/)?.[0] ?? ""`);
    if (!status.includes("1 modificación")) throw new Error(`aviso: ${status}`);
    // En el Outliner, la pieza con su malla y lo hecho sobre ella
    const got = await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    const at = got.indexOf("Pieza 1");
    if (got[at + 1] !== "Malla generada" || got[at + 2] !== "Escalar para fabricar") throw new Error(`árbol del objeto: ${JSON.stringify(got.slice(at))}`);
    await b.shot("objetos-rehecho");
  },
};

const filter = process.argv[2];
let failed = 0;
for (const [name, run] of Object.entries(scenarios)) {
  if (filter && !name.includes(filter)) continue;
  await call("cad_close");
  const b = await launch(URL);
  // Las herramientas de los menús de la barra: si no está a la vista, se abre su menú
  const clickText = b.clickText.bind(b);
  b.clickText = async (text, nth = 0) => {
    try {
      return await clickText(text, nth);
    } catch (e) {
      const menu = await b.eval(`[...document.querySelectorAll("[data-toolbar-menu]")].find((m) => JSON.parse(m.dataset.items).includes(${JSON.stringify(text)}))?.dataset.toolbarMenu`);
      if (menu) {
        await clickText(menu);
        return clickText(text, nth);
      }
      // Lo del panel de Diseñar está repartido en pestañas: probar en cada una
      for (const tab of ["Diseño", "Pieza", "Inspección", "Desde el escaneo"]) {
        if (!(await b.eval(`(() => { const t = document.querySelector('nav button[aria-label=${JSON.stringify(tab)}]'); t?.click(); return !!t; })()`))) continue;
        await sleep(400);
        try {
          return await clickText(text, nth);
        } catch {}
      }
      throw e;
    }
  };
  try {
    // Con la máquina cargada la app tarda en montar: esperar el encabezado
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Diseñar")`)); t++) await sleep(500);
    await run(b);
    const errors = b.logs.filter((l) => l.startsWith("EXCEPTION"));
    if (errors.length) throw new Error(errors.join("\n"));
    console.log(`✓ ${name}`);
  } catch (e) {
    failed++;
    console.log(`✗ ${name}: ${e.message}`);
    await b.shot(name.replace(/\W+/g, "_"));
  } finally {
    await b.close();
  }
}
process.exit(failed ? 1 : 0);
