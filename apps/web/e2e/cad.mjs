// Pruebas de punta a punta del espacio Diseñar con el backend CAD real, sin
// Tauri: el arnés (cad-harness.html) manda los comandos al puente HTTP.
//
//   cargo run -p pinocchio-app --example cad_http        # backend en :8766 (E2E_BRIDGE_PORT: otro)
//   (cd apps/web && npx vite --port 5173)                 # frontend
//   node apps/web/e2e/cad.mjs [filtro]                    # Chromium headless
//
// Capturas en $E2E_OUT (por defecto /tmp/pinocchio-e2e). Cada escenario
// verifica volúmenes contra el valor teórico.

import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { launch, sleep } from "./cdp.mjs";

const PORT = process.env.E2E_BRIDGE_PORT ?? "8766";
const URL = `http://localhost:5173/e2e/cad-harness.html?bridge=${PORT}`;
const BRIDGE = `http://127.0.0.1:${PORT}/invoke/`;
const call = (cmd, args = {}) => fetch(BRIDGE + cmd, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());
const SPECULUM = process.env.E2E_STL ?? `${process.env.HOME}/Descargas/macho_especulo.stl`;

function near(actual, expected, tol, what) {
  if (!(Math.abs(actual - expected) <= tol)) throw new Error(`${what}: ${actual} (esperado ${expected} ± ${tol})`);
}

/** STL de texto con dos cubos de 20 mm, el segundo corrido 10 mm en cada eje */
function twoCubesStl() {
  const quads = [
    [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]],
    [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
    [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
    [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
    [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]],
  ];
  let text = "solid dos\n";
  for (const offset of [0, 10]) {
    for (const q of quads) {
      const v = q.map((p) => p.map((c) => c * 20 + offset));
      for (const t of [[v[0], v[1], v[2]], [v[0], v[2], v[3]]]) {
        text += " facet normal 0 0 0\n  outer loop\n" + t.map((p) => `   vertex ${p.join(" ")}\n`).join("") + "  endloop\n endfacet\n";
      }
    }
  }
  return text + "endsolid dos\n";
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
/** Abre una pestaña del panel de Diseñar (Diseño, Pieza, Inspección, Desde el escaneo) */
async function tab(b, name) {
  if (!(await b.eval(`(() => { const t = document.querySelector('nav button[aria-label=${JSON.stringify(name)}]'); t?.click(); return !!t; })()`)))
    throw new Error("no encontré la pestaña " + name);
  await sleep(400);
}
/** Clic en una fila del árbol de operaciones por su nombre */
async function clickRow(b, name) {
  const r = await b.eval(`(() => {
    const e = [...document.querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(name)} && x.offsetParent !== null);
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
      const s = [...document.querySelectorAll("span")].find((x) => x.textContent === "Caja arriba");
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
    await tab(b, "Inspección");
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
    await tab(b, "Pieza");
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
    // Desde el área Exportar: elegir la pieza y exportarla como STL
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Eje");
    await sleep(400);
    await b.clickText("STL");
    const ui = `${out}/eje-ui.stl`;
    rmSync(ui, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(ui)}`);
    await b.clickText("Exportar STL");
    await sleep(2000);
    if (!existsSync(ui) || statSync(ui).size !== statSync(one).size) throw new Error("Exportar no sacó la pieza sola");
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
    await tab(b, "Pieza");
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
    await tab(b, "Inspección");
    if ((await line("Masa")) !== "11,16 g") throw new Error(`masa con PLA: ${await line("Masa")}`);
    // La segunda de acero: 8 × 1,24 + 1 × 7,85
    await tab(b, "Pieza");
    await b.eval(`[...document.querySelectorAll("[data-part] span.truncate")].find((s) => s.textContent === "Pieza 2").click()`);
    await sleep(800);
    const panel = `document.querySelector('[aria-label="Datos de Pieza 2"]')`;
    const trigger = await b.eval(`(() => { const e = [...${panel}.querySelectorAll("button")].find((x) => x.textContent.includes("El del diseño")); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.click(...trigger, { wait: 600 });
    await b.clickText("Acero (7850 kg/m³)");
    await sleep(1500);
    if ((await line("Masa", panel)) !== "7,85 g (Acero)") throw new Error(`masa de la pieza: ${await line("Masa", panel)}`);
    await tab(b, "Inspección");
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

  async "agujero: la pieza más cercana y cambiarla"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Otra caja suelta debajo, en la línea del agujero
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    doc.features.unshift({ ...structuredClone(box), id: doc.next_id, name: "Caja abajo", kind: { ...structuredClone(box.kind), op: "new", origin: [0, 0, -40] } });
    doc.next_id++;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    near((await body()).volume, 16000, 1e-6, "dos cajas");
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
    // Pasante, pero solo en la de arriba (la más cercana), y el panel la muestra marcada
    const hole = Math.PI * 3.3 * 3.3 * 20;
    near((await body()).volume, 16000 - hole, 0.05, "solo la de arriba");
    const boxes = () =>
      b.eval(`[...document.querySelectorAll('[aria-label="En las piezas"] input[type=checkbox]')].map((i) => [i.closest("label, div")?.textContent ?? "", i.checked])`);
    const list = await boxes();
    if (list.length !== 2 || list.filter(([, c]) => c).length !== 1) throw new Error(`piezas: ${JSON.stringify(list)}`);
    // Marcar también la de abajo: atraviesa las dos
    await b.eval(`(() => { const i = [...document.querySelectorAll('[aria-label="En las piezas"] input[type=checkbox]')].find((i) => !i.checked); i.click(); })()`);
    await sleep(2000);
    near((await body()).volume, 16000 - 2 * hole, 0.05, "las dos");
    if ((await boxes()).some(([, c]) => !c)) throw new Error("la de abajo no quedó marcada");
    await accept(b);
  },

  async "flecha: invertir y arrastrar la extrusión; el agujero avisa si no corta"(b) {
    await begin(b);
    // Sketch en la planta: cuadrado de 40 con un círculo de radio 8; bloque con las dos regiones
    const pts = [[-20, -20], [20, -20], [20, 20], [-20, 20], [0, 0]].map(([x, y], i) => ({ id: i + 1, x, y }));
    const line = (id, a, c) => ({ id, geometry: { type: "line", start: a, end: c } });
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, ...pts],
      entities: [line(6, 1, 2), line(7, 2, 3), line(8, 3, 4), line(9, 4, 1), { id: 10, geometry: { type: "circle", center: 5, radius: 8 } }],
      constraints: [],
      next_id: 11,
      origin: 0,
    };
    const doc = await call("cad_get_document");
    doc.features = [
      { id: 1, name: "Sketch 1", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } },
      { id: 2, name: "Bloque", suppressed: false, kind: { type: "extrude", sketch: 1, regions: { type: "points", points: [[15, 15], [0, 0]] }, extent: { type: "blind", distance: 10 }, reverse: false, op: "join" } },
    ];
    doc.next_id = 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    near((await body()).volume, 16000, 1e-6, "bloque");
    const handle = (id) => b.eval(`window.__cadStore.stateOf(${id})?.handle ?? null`);
    const tip = (h) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(h.origin.map((o, i) => o + h.dir[i] * h.length))})`);
    const kind = (id) => b.eval(`window.__cadStore.doc().features.find((f) => f.id === ${id}).kind`);
    // Abrir el bloque: su flecha sube 10 desde el centro
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(2000);
    let h = await handle(2);
    if (!h || h.dir[2] !== 1 || h.length !== 10 || h.reversed) throw new Error(`flecha del bloque: ${JSON.stringify(h)}`);
    // Clic en la punta: hacia abajo
    const [x, y] = await tip(h);
    if (!(await b.eval(`window.__cadViewer.handleHit(${x}, ${y})`))) throw new Error("la punta no se puede pinchar");
    await b.click(x, y, { wait: 2000 });
    if (!(await kind(2)).reverse) throw new Error("no se dio vuelta");
    let r = await body();
    near(r.volume, 16000, 1e-6, "invertido");
    near(r.bbox_max[2], 0, 1e-6, "invertido: hacia abajo");
    // Arrastrar la punta hasta 25 mm hacia abajo
    h = await handle(2);
    const [x0, y0] = await tip(h);
    const [x1, y1] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(h.origin.map((o, i) => o + h.dir[i] * 25))})`);
    await b.drag(x0, y0, x1, y1, 15);
    await sleep(2500);
    const k = await kind(2);
    near(k.extent.distance, 25, 0.5, "distancia arrastrada");
    if (!k.reverse) throw new Error("arrastrar cambió el sentido");
    near((await body()).volume, 1600 * k.extent.distance, 1e-3, "bloque más alto");
    await accept(b);
    // Agujero con el centro del círculo: va solo hacia el bloque (abajo, contra la normal)
    await b.eval(`window.__cadStore.select(undefined)`);
    await b.clickText("Agujero");
    await sleep(2500);
    const hid = await b.eval(`window.__cadStore.draft().feature`);
    if ((await kind(hid)).sketch !== 1) throw new Error("el agujero no tomó el sketch del bloque");
    const area = 1600 * k.extent.distance;
    near((await body()).volume, area - Math.PI * 3.3 * 3.3 * k.extent.distance, 0.05, "agujero en el bloque");
    // Clic en su flecha: hacia afuera del material; no corta y lo dice
    h = await handle(hid);
    await b.click(...(await tip(h)), { wait: 2500 });
    if ((await kind(hid)).reverse !== !h.reversed) throw new Error("el agujero no se dio vuelta");
    near((await body()).volume, area, 1e-6, "agujero hacia afuera");
    const st = await b.eval(`window.__cadStore.stateOf(${hid})`);
    if (st.state !== "warning" || !st.message.includes("no toca ninguna pieza")) throw new Error(`estado: ${JSON.stringify(st)}`);
    if (!(await b.eval(`document.body.innerText.includes("no toca ninguna pieza")`))) throw new Error("el aviso no se ve");
    // "Automático" lo devuelve al material
    await b.clickText("Automático");
    await sleep(2500);
    near((await body()).volume, area - Math.PI * 3.3 * 3.3 * k.extent.distance, 0.05, "de vuelta al material");
    await accept(b);
  },

  async "cámara: girar por encima del polo y desplazar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const cam = () => b.eval(`(() => { const v = window.__cadViewer; return { up: v.camera.up.toArray(), pos: v.camera.position.toArray(), target: v.controls.target.toArray() }; })()`);
    const rect = await b.eval(`(() => { const r = window.__cadViewer.canvas.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()`);
    const [cx, cy] = [rect[0] + rect[2] / 2, rect[1] + rect[3] * 0.2];
    const drag = async (button, buttons, dy, modifiers = 0) => {
      await b.mouse("mouseMoved", cx, cy, { buttons: 0 });
      await b.mouse("mousePressed", cx, cy, { button, buttons, modifiers });
      for (let i = 1; i <= 20; i++) {
        await b.mouse("mouseMoved", cx, cy + (dy * i) / 20, { button, buttons, modifiers });
        await sleep(20);
      }
      await b.mouse("mouseReleased", cx, cy + dy, { button, buttons: 0, modifiers });
      await sleep(300);
    };
    // Media altura del visor = media vuelta hacia abajo: pasa por encima y queda de cabeza
    const before = await cam();
    await drag("middle", 4, rect[3] * 0.55);
    const after = await cam();
    if (!(after.up[1] < 0)) throw new Error(`no pasó el polo: ${JSON.stringify(after)}`);
    const dist = (c) => Math.hypot(...c.pos.map((p, i) => p - c.target[i]));
    near(dist(after), dist(before), 1e-6 * dist(before), "misma distancia");
    // El cubo de vistas lo endereza
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 1])`);
    await sleep(800);
    if (!((await cam()).up[1] > 0.99)) throw new Error("no se enderezó");
    // Botón derecho desplaza: el centro se mueve con la cámara
    const t0 = (await cam()).target;
    await drag("right", 2, 80);
    const t1 = (await cam()).target;
    if (!(Math.hypot(...t1.map((v, i) => v - t0[i])) > 1e-3)) throw new Error("no desplazó");
    // Modo notebook: la rueda gira (misma distancia, otra posición) en vez de acercar
    const c0 = await cam();
    await b.eval(`(() => { const v = window.__cadViewer; v.trackpad = true; v.canvas.dispatchEvent(new WheelEvent("wheel", { deltaX: 40, deltaY: 0, bubbles: true, cancelable: true })); })()`);
    await sleep(300);
    const c1 = await cam();
    near(dist(c1), dist(c0), 1e-6 * dist(c0), "la rueda no acerca en modo notebook");
    if (!(Math.hypot(...c1.pos.map((v, i) => v - c0.pos[i])) > 1e-3)) throw new Error("la rueda no giró");
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
    await b.clickText("Plano 2D");
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

  async "plano 2D: radio, ángulo, detalle y corte movido"(b) {
    await begin(b);
    // Medio cono (r 10 → 5, alto 10) cortado en y < 0: arcos en la planta, trapecio en el frente
    const doc = await call("cad_get_document");
    const prim = (shape, origin, op) => ({ id: doc.next_id++, name: shape.type, suppressed: false, kind: { type: "primitive", shape, origin, z: [0, 0, 1], x: [1, 0, 0], op } });
    doc.features.push(prim({ type: "cone", r1: 10, r2: 5, height: 10 }, [0, 0, 0], "join"));
    doc.features.push(prim({ type: "box", dx: 30, dy: 15, dz: 30, centered: true }, [0, -7.5, -10], "cut"));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await sleep(500);
    const toolBtn = (name) => b.eval(`[...document.querySelectorAll('[aria-label="Herramienta de cotas"] button')].find((x) => x.textContent === ${JSON.stringify(name)}).click()`);
    // Punto de la hoja (coordenadas del SVG) → pantalla
    const at = (sel, pick) =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const lines = [...svg.querySelectorAll(${JSON.stringify(sel)})].map((l) => l.getAttribute("points").split(" ").map((p) => p.split(",").map(Number)));
        const pts = (${pick})(lines);
        const p = new DOMPoint(pts[0], pts[1]).matrixTransform(svg.getScreenCTM());
        return [p.x, p.y];
      })()`);
    // Radio: el arco más largo de la planta, en su medio
    await toolBtn("Cota");
    await sleep(300);
    await b.click(...(await at('[data-view="top"] polyline[data-kind="visible"]', "(ls) => { const l = ls.sort((a, b) => b.length - a.length)[0]; return l[Math.floor(l.length / 2)]; }")), { wait: 1000 });
    // Ángulo: la base del frente y un costado inclinado
    await toolBtn("Ángulo");
    await sleep(300);
    const seg = (kind) =>
      `(ls) => { const s = ls.filter((l) => l.length === 2).map((l) => ({ l, dx: l[1][0] - l[0][0], dy: l[1][1] - l[0][1] })); const k = ${JSON.stringify(kind)} === "h" ? s.filter((x) => Math.abs(x.dy) < 1e-6).sort((a, b) => Math.abs(b.dx) - Math.abs(a.dx))[0] : s.find((x) => Math.abs(x.dx) > 1e-6 && Math.abs(x.dy) > 1e-6); const l = k.l; return [l[0][0] * 0.7 + l[1][0] * 0.3, l[0][1] * 0.7 + l[1][1] * 0.3]; }`;
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("h"))), { wait: 600 });
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("s"))), { wait: 1000 });
    const texts = await b.eval(`[...document.querySelectorAll("[data-sheet] [data-user-dim] text, [data-sheet] [data-radius] text, [data-sheet] [data-angle] text")].map((t) => t.textContent)`);
    if (!texts.includes("R10")) throw new Error(`radio: ${texts}`);
    // atan(10 / 5) = 63,43° (o su suplemento si se eligió el otro lado)
    if (!texts.some((t) => t === "63,43°" || t === "116,57°")) throw new Error(`ángulo: ${texts}`);
    // Detalle en el frente
    await toolBtn("Detalle");
    await sleep(300);
    await b.click(...(await at('[data-view="front"] polyline[data-kind="visible"]', seg("s"))), { wait: 1000 });
    const det = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      return { view: svg.querySelectorAll('[data-view="detalle-B"] polyline').length, mark: !!svg.querySelector('[data-detail-mark="B"]'), label: svg.querySelector('[data-detail="B"] text')?.textContent, chip: !!document.querySelector('[data-detail-chip="B"]') };
    })()`);
    if (!(det.view > 0) || !det.mark || !det.chip || !/^Detalle B \(\d+:1\)$/.test(det.label ?? "")) throw new Error(`detalle: ${JSON.stringify(det)}`);
    if ((await call("cad_get_document")).drawing?.details?.length !== 1) throw new Error("no se guardó el detalle");
    await toolBtn("Detalle");
    // Corte A-A por el medio (Y = 5) y después en Y = 2: el rayado es el ancho del cono ahí
    const hatchW = () =>
      b.eval(`(() => {
        const ps = [...document.querySelectorAll('[data-sheet] [data-hatch="front"] polygon')].flatMap((p) => p.getAttribute("points").split(" ").map((q) => Number(q.split(",")[0])));
        return ps.length ? Math.max(...ps) - Math.min(...ps) : 0;
      })()`);
    await b.eval(`[...document.querySelectorAll('[aria-label="Plano 2D"] label')].find((l) => l.textContent.trim() === "Corte A-A").click()`);
    for (let t = 0; t < 30 && !(await hatchW()); t++) await sleep(500);
    await sleep(800);
    const mid = await hatchW();
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Posición del corte"]'); i.value = "2"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    for (let t = 0; t < 30 && Math.abs((await hatchW()) - mid) < 1e-6; t++) await sleep(500);
    await sleep(800);
    // 2·√(100 − 4) contra 2·√(100 − 25)
    near((await hatchW()) / mid, Math.sqrt(96 / 75), 0.01, "ancho del corte en Y = 2 contra el medio");
    await b.shot("plano_2d_detalle");
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
    if (await b.eval(`window.__cadUi.assemblyMode()`)) throw new Error("no volvió al diseño");
    // Exportar el ensamble: cada instancia en su lugar, con su nombre
    const names = (await call("cad_get_document")).assembly.instances.map((i) => i.name);
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Ensamble (2 instancias)");
    await sleep(400);
    for (const [fmt, label] of [["step", "Exportar STEP"], ["3mf", "Exportar 3MF"]]) {
      await b.clickText(fmt.toUpperCase());
      const path = `${out}/ensamble.${fmt}`;
      rmSync(path, { force: true });
      await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
      await b.clickText(label);
      await sleep(2500);
      if (!existsSync(path)) throw new Error(`no se exportó ${path}`);
      const { execSync } = await import("node:child_process");
      const text = fmt === "step" ? readFileSync(path, "latin1") : execSync(`unzip -p ${path} 3D/3dmodel.model`).toString();
      if (!names.every((n) => text.includes(n))) throw new Error(`${fmt} sin los nombres ${names}`);
      if (fmt === "3mf" && (text.match(/<object /g) ?? []).length !== 2) throw new Error("3MF sin un objeto por instancia");
    }
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

  async "nervio desde una línea hasta el sólido"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Sketch vertical por el medio de la caja con una línea a z = 20 (la caja llega a z = 10)
    const doc = await call("cad_get_document");
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: -6, y: 20 }, { id: 2, x: 6, y: 20 }], entities: [{ id: 3, geometry: { type: "line", start: 1, end: 2 } }], constraints: [], next_id: 4, origin: 0 };
    doc.features.push({ id: doc.next_id, name: "Línea del nervio", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Nervio");
    await sleep(2000);
    await accept(b);
    // Pared de 12 × 10 × 2 encima de la caja de 20
    near((await body()).volume, 8000 + 240, 1e-6, "caja con nervio");
    await b.shot("nervio");
  },

  async "reemplazar cara hasta un plano"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // La cara de arriba (z = 10) hasta la planta (z = 0): queda la mitad
    await b.click(...(await at([0, 0, 10])), { wait: 800 });
    await b.clickText("Reemplazar cara");
    await sleep(2000);
    await accept(b);
    near((await body()).volume, 4000, 1e-6, "hasta la planta");
    await b.shot("reemplazar_cara");
  },

  async "revolución con el eje elegido en el visor"(b) {
    await begin(b);
    // En el frente: rectángulo x 10..15, z 0..20 y una línea vertical en x = −5 (el eje)
    const doc = await call("cad_get_document");
    const P = [[10, 0], [15, 0], [15, 20], [10, 20], [-5, 0], [-5, 20]];
    const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))];
    const line = (id, a, b) => ({ id, geometry: { type: "line", start: a, end: b } });
    const sketch = { points, entities: [line(7, 1, 2), line(8, 2, 3), line(9, 3, 4), line(10, 4, 1), line(11, 5, 6)], constraints: [], next_id: 12, origin: 0 };
    doc.features.push({ id: doc.next_id, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Revolución");
    await sleep(2000);
    // Alrededor de Z (por defecto) es un anillo de radios 10 y 15
    near((await body()).volume, Math.PI * (15 ** 2 - 10 ** 2) * 20, 1e-3, "alrededor de Z");
    await b.clickText("Elegir el eje en el visor");
    await sleep(500);
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([-5, 0, 10])`)), { wait: 2000 });
    const axis = (await b.eval(`JSON.stringify(window.__cadStore.doc().features.at(-1).kind.axis)`));
    if (!axis.includes('"sketch_line"') || !axis.includes('"line":11')) throw new Error(`eje: ${axis}`);
    // Alrededor de la línea: radios 15 y 20
    near((await body()).volume, Math.PI * (20 ** 2 - 15 ** 2) * 20, 1e-3, "alrededor de la línea");
    await accept(b);
    await b.shot("revolucion_eje_con_clic");
  },

  async "caja de selección dentro del sketch"(b) {
    await begin(b);
    await sketchOn(b);
    // Un rectángulo a la izquierda y un círculo a la derecha
    await b.clickText("Rectángulo");
    await b.click(380, 320);
    await b.click(520, 460);
    await b.clickText("Círculo");
    await b.click(680, 390);
    await b.click(730, 390);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Elegir");
    const kinds = () => b.eval(`(() => {
      const s = window.__cadUi.session().sketch;
      return window.__cadUi.selection().map((id) => s.entities.find((e) => e.id === id)?.geometry.type ?? "punto").filter((k) => k !== "punto").sort().join(",");
    })()`);
    // De izquierda a derecha, solo lo que queda entero adentro: el rectángulo
    await b.drag(350, 290, 600, 490);
    if ((await kinds()) !== "line,line,line,line") throw new Error(`ventana: ${await kinds()}`);
    // De derecha a izquierda, lo que toca: el círculo (sin tenerlo entero adentro)
    await b.drag(760, 370, 700, 410);
    if ((await kinds()) !== "circle") throw new Error(`cruce: ${await kinds()}`);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Al volver a abrir un sketch con dibujo, la herramienta es Elegir
    const id = await b.eval(`window.__cadStore.doc().features.find((f) => f.kind.type === "sketch").id`);
    await b.eval(`window.__cadUi.editSketch(${id})`);
    await sleep(1000);
    if ((await b.eval(`window.__cadUi.tool()`)) !== "select") throw new Error(`herramienta al editar: ${await b.eval(`window.__cadUi.tool()`)}`);
    await b.shot("caja_en_el_sketch");
  },

  async "deshacer y rehacer dentro del sketch"(b) {
    await begin(b);
    const features = () => b.eval(`window.__cadStore.doc().features.length`);
    const before = await features();
    await sketchOn(b);
    const lines = () => b.eval(`window.__cadUi.session().sketch.entities.filter((e) => e.geometry.type === "line").length`);
    const points = () => b.eval(`JSON.stringify(window.__cadUi.session().sketch.points.map((p) => [p.id, +p.x.toFixed(6), +p.y.toFixed(6)]))`);
    const ctrlZ = async (shift = false) => {
      for (const type of ["keyDown", "keyUp"])
        await b.send("Input.dispatchKeyEvent", { type, key: shift ? "Z" : "z", code: "KeyZ", windowsVirtualKeyCode: 90, modifiers: 2 | (shift ? 8 : 0) });
      await sleep(800);
    };
    // Dos rectángulos; Esc cierra las cotas pedidas (con el foco en la cota, Ctrl+Z es del texto)
    await b.clickText("Rectángulo");
    await b.click(380, 320);
    await b.click(520, 460);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Rectángulo");
    await b.click(600, 320);
    await b.click(720, 440);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    if ((await lines()) !== 8) throw new Error(`dibujadas: ${await lines()}`);
    // Ctrl+Z quita el segundo rectángulo, sin tocar el documento
    await ctrlZ();
    if ((await lines()) !== 4) throw new Error(`tras deshacer: ${await lines()}`);
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error("deshacer cerró el sketch");
    if ((await features()) !== before + 1) throw new Error(`el documento cambió: ${await features()} operaciones`);
    await ctrlZ(true);
    if ((await lines()) !== 8) throw new Error(`tras rehacer: ${await lines()}`);
    // Con el botón de la barra
    await b.eval(`document.querySelector('[aria-label="Deshacer en el sketch"]').click()`);
    await sleep(800);
    if ((await lines()) !== 4) throw new Error(`con el botón: ${await lines()}`);
    // Un arrastre entero es un solo paso
    await b.clickText("Elegir");
    const still = await points();
    await b.drag(380, 320, 330, 280, 12);
    await sleep(800);
    if ((await points()) === still) throw new Error("el arrastre no movió nada");
    await ctrlZ();
    if ((await points()) !== still) throw new Error(`el arrastre no se deshizo: ${await points()} (antes ${still})`);
    if ((await lines()) !== 4) throw new Error(`deshacer el arrastre quitó más: ${await lines()}`);
    // Línea encadenada: cada clic es un paso (y cambiar el sketch no corta la cadena)
    await b.clickText("Línea");
    await b.click(600, 320);
    await b.click(700, 330);
    await b.click(720, 450);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    if ((await lines()) !== 6) throw new Error(`línea encadenada: ${(await lines()) - 4} tramos`);
    await ctrlZ();
    if ((await lines()) !== 5) throw new Error(`deshacer un tramo: ${(await lines()) - 4} tramos`);
    // Lo deshecho no queda en el documento
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const saved = await b.eval(`window.__cadStore.doc().features.find((f) => f.kind.type === "sketch").kind.sketch.entities.length`);
    if (saved !== 5) throw new Error(`guardadas: ${saved}`);
    await b.shot("deshacer_en_el_sketch");
  },

  async "mover, copiar y pegar, girar, escalar y partir en el sketch"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const lines = async () => (await sk()).entities.filter((e) => e.geometry.type === "line");
    const ctrl = async (key) => {
      for (const type of ["keyDown", "keyUp"])
        await b.send("Input.dispatchKeyEvent", { type, key, code: "Key" + key.toUpperCase(), windowsVirtualKeyCode: key.toUpperCase().charCodeAt(0), modifiers: 2 });
      await sleep(600);
    };
    /** Caja de los puntos de unas líneas: [x0, y0, x1, y1] */
    const bbox = (s, ls) => {
      const ids = new Set(ls.flatMap((e) => [e.geometry.start, e.geometry.end]));
      const ps = s.points.filter((p) => ids.has(p.id));
      return [Math.min(...ps.map((p) => p.x)), Math.min(...ps.map((p) => p.y)), Math.max(...ps.map((p) => p.x)), Math.max(...ps.map((p) => p.y))];
    };
    /** Campo y botón del bloque Transformar del panel */
    const field = (label, value) =>
      b.eval(`(() => {
        const box = document.querySelector('[aria-label="Transformar"]');
        const i = [...box.querySelectorAll("label")].find((l) => l.textContent.startsWith(${JSON.stringify(label)}))?.querySelector("input");
        i.value = ${JSON.stringify(String(value))}; i.dispatchEvent(new Event("change", { bubbles: true })); return true;
      })()`);
    const press = async (text) => {
      await b.eval(`[...document.querySelector('[aria-label="Transformar"]').querySelectorAll("button")].find((x) => x.textContent.trim() === ${JSON.stringify(text)}).click()`);
      await sleep(1000);
    };
    await b.clickText("Rectángulo");
    await b.click(380, 320);
    await b.click(520, 460);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Elegir");
    await b.drag(350, 290, 560, 490);
    const first = (await lines()).map((e) => e.id);
    const box0 = bbox(await sk(), await lines());
    const nConstraints = (await sk()).constraints.length;

    // Mover con la herramienta (V): de la esquina a 60 px a la derecha
    await b.key("v", "KeyV", 86);
    const plane = await b.eval(`window.__cadUi.session().plane`);
    const mm = (x, y) => b.eval(`window.__cadViewer.planePoint(${x}, ${y}, ${JSON.stringify(plane)})`);
    await b.click(380, 320);
    await b.click(440, 320, { wait: 1200 });
    const box1 = bbox(await sk(), await lines());
    const [p0, p1] = [await mm(380, 320), await mm(440, 320)];
    const want = p1[0] - p0[0];
    near(box1[0] - box0[0], want, Math.abs(want) * 0.1, "movido en x");
    near(box1[1] - box0[1], 0, 1e-6, "movido en y");
    near(box1[2] - box1[0], box0[2] - box0[0], 1e-6, "mismo ancho");
    if ((await sk()).constraints.length !== nConstraints) throw new Error("mover cambió las restricciones");
    if ((await b.eval(`window.__cadUi.session().report?.status`)) === "over_constrained") throw new Error("mover sobre-definió");
    // Un solo paso para deshacer
    await ctrl("z");
    near(bbox(await sk(), await lines())[0], box0[0], 1e-6, "deshacer el movimiento");

    // Copiar y pegar: Ctrl+C, Ctrl+V y clic
    await ctrl("c");
    await ctrl("v");
    if ((await b.eval(`window.__cadUi.tool()`)) !== "paste") throw new Error("Ctrl+V no quedó pegando");
    await b.click(700, 390, { wait: 1200 });
    const all = await lines();
    if (all.length !== 8) throw new Error(`tras pegar: ${all.length} líneas`);
    const copy = all.filter((e) => !first.includes(e.id));
    const s1 = await sk();
    // Las 4 de ejes y las 2 cotas de la copia (la alineación con el origen no se copia: no es interna)
    const mine = new Set(copy.map((e) => e.id));
    const copied = s1.constraints.filter((c) => mine.has(c.line));
    if (copied.length !== 6) throw new Error(`restricciones de la copia: ${JSON.stringify(copied)}`);
    const sel = await b.eval(`window.__cadUi.selection()`);
    if (!copy.every((e) => sel.includes(e.id))) throw new Error("lo pegado no quedó elegido");

    // Girar 90° desde el panel: el ancho pasa a ser el alto
    const [w, h] = [box0[2] - box0[0], box0[3] - box0[1]];
    await field("Ángulo", 90);
    await press("Girar");
    let bc = bbox(await sk(), copy);
    near(bc[2] - bc[0], h, 1e-6, "ancho girado");
    near(bc[3] - bc[1], w, 1e-6, "alto girado");
    // Escalar al doble: las cotas de largo de la copia también
    await field("Factor", 2);
    await press("Escalar");
    bc = bbox(await sk(), copy);
    near(bc[2] - bc[0], 2 * h, 1e-6, "ancho escalado");
    const copyIds = new Set(copy.map((e) => e.id));
    const lens = (await sk()).constraints.filter((c) => c.type === "length" && copyIds.has(c.line)).map((c) => c.value).sort((a, c) => a - c);
    near(lens[0], 2 * Math.min(w, h), 1e-6, "cota chica escalada");
    near(lens[1], 2 * Math.max(w, h), 1e-6, "cota grande escalada");
    if ((await b.eval(`window.__cadUi.session().report?.status`)) === "over_constrained") throw new Error("girar o escalar sobre-definió");

    // Partir (D) el lado de abajo del original por la mitad
    await b.key("d", "KeyD", 68);
    await b.click(450, 460, { wait: 1000 });
    if ((await lines()).length !== 9) throw new Error(`tras partir: ${(await lines()).length} líneas`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const r = await evaluate();
    const solved = r.sketches.find((x) => x.sketch.entities.length)?.sketch;
    if (solved.entities.filter((e) => e.geometry.type === "line").length !== 9) throw new Error("no se guardó");
    await b.shot("transformar_en_el_sketch");
  },

  async "cotas: distancia a la línea, entre paralelas, diámetro respecto del eje y largo de arco"(b) {
    await begin(b);
    // Eje vertical fijo en x = 0, un punto suelto, una línea casi paralela al eje y un arco de radio 10
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    const sketch = {
      points: [P(0, 0, 0), P(1, 0, 0), P(2, 0, 40), P(3, 12, 20), P(4, 20, 0), P(5, 21, 30), P(6, 50, 0), P(7, 60, 0), P(8, 57, 7), P(9, 36, 15)],
      entities: [
        { id: 10, geometry: { type: "line", start: 1, end: 2 }, construction: true },
        { id: 11, geometry: { type: "point", point: 3 } },
        { id: 12, geometry: { type: "line", start: 4, end: 5 } },
        { id: 13, geometry: { type: "arc", center: 6, start: 7, end: 8 } },
        { id: 14, geometry: { type: "point", point: 9 } },
      ],
      constraints: [
        { type: "fixed", point: 1, x: 0, y: 0 },
        { type: "fixed", point: 2, x: 0, y: 40 },
        { type: "fixed", point: 6, x: 50, y: 0 },
        { type: "fixed", point: 7, x: 60, y: 0 },
        { type: "horizontal_points", a: 3, b: 1 },
        { type: "horizontal_points", a: 9, b: 2 },
      ],
      next_id: 20,
      origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    const id = doc.next_id;
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadUi.editSketch(${id})`);
    await sleep(1200);
    const suggest = async (ids, label, value) => {
      await b.eval(`window.__cadUi.setSelection(${JSON.stringify(ids)})`);
      await sleep(300);
      await b.clickText(label);
      await sleep(800);
      const i = await b.eval(`window.__cadUi.session().sketch.constraints.length - 1`);
      const msg = await b.eval(`window.__cadUi.setConstraintText(${i}, ${JSON.stringify(String(value))})`);
      if (msg) throw new Error(`${label}: ${msg}`);
      await sleep(1000);
    };
    const pt = (id) => b.eval(`(() => { const p = window.__cadUi.session().sketch.points.find((q) => q.id === ${id}); return [p.x, p.y]; })()`);
    // Ø 30 respecto del eje: el punto queda a 15
    await suggest([3, 10], "Diámetro respecto del eje", 30);
    near((await pt(3))[0], 15, 1e-6, "punto a 15 del eje");
    // La línea paralela al eje, a 25
    await suggest([10, 12], "Distancia entre paralelas", 25);
    near((await pt(4))[0], 25, 1e-6, "extremo de abajo a 25");
    near((await pt(5))[0], 25, 1e-6, "extremo de arriba a 25 (paralelas)");
    // Otro punto a 5 de la línea, del lado de afuera: x = 30
    await suggest([9, 12], "Distancia a la línea", 5);
    near((await pt(9))[0], 30, 1e-6, "punto a 5 de la línea");
    // Arco de radio 10 con largo 5π: un cuarto de vuelta, termina en (50, 10)
    await suggest([13], "Largo del arco", (5 * Math.PI).toFixed(9));
    const e = await pt(8);
    near(e[0], 50, 1e-5, "fin del arco x");
    near(e[1], 10, 1e-5, "fin del arco y");
    const r = await b.eval(`window.__cadUi.session().report`);
    if (r.status === "over_constrained" || r.status === "failed") throw new Error(`estado: ${r.status}`);
    // Las etiquetas: Ø y ⌒
    const texts = await b.eval(`document.body.innerText`);
    if (!texts.includes("Ø 30")) throw new Error("no se ve la cota Ø 30");
    if (!texts.includes("⌒ 15.708")) throw new Error("no se ve el largo de arco");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.shot("cotas_nuevas");
  },

  async "rectángulo por 3 puntos, círculos por 2 y 3 puntos y tangente, polígono circunscrito, ranuras y colineal"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    const tool = async (label) => {
      await b.clickText(label);
      await sleep(200);
    };
    const esc = () => b.key("Escape", "Escape", 27);
    const status = () => b.eval(`window.__cadUi.session().report?.status`);
    const okStatus = async (what) => {
      await sleep(800);
      const st = await status();
      if (st === "over_constrained" || st === "failed") throw new Error(`${what}: ${st}`);
    };
    const count = async (type) => (await sk()).entities.filter((e) => e.geometry.type === type).length;
    // Rectángulo inclinado por 3 puntos
    await tool("Rect. 3 p.");
    await b.click(200, 520);
    await b.click(340, 430);
    await b.click(370, 480, { wait: 800 });
    await esc();
    let s = await sk();
    const rect = s.entities.filter((e) => e.geometry.type === "line").map((e) => e.id);
    if (rect.length !== 4) throw new Error(`rectángulo: ${rect.length} líneas`);
    const kinds = s.constraints.map((c) => c.type);
    if (kinds.filter((k) => k === "parallel").length !== 2 || !kinds.includes("perpendicular")) throw new Error(`restricciones del rectángulo: ${kinds}`);
    await okStatus("rectángulo");
    // Círculo tangente a tres lados del rectángulo: clic cerca del medio de cada uno
    await tool("Círculo tangente");
    const P = new Map(s.points.map((p) => [p.id, [p.x, p.y]]));
    const lineOf = (id) => s.entities.find((e) => e.id === id).geometry;
    for (const id of rect.slice(0, 3)) {
      const g = lineOf(id);
      const [a, c] = [P.get(g.start), P.get(g.end)];
      await b.click(...(await at((a[0] + c[0]) / 2, (a[1] + c[1]) / 2)), { wait: 500 });
    }
    await okStatus("círculo tangente");
    s = await sk();
    const tan = s.entities.find((e) => e.geometry.type === "circle");
    if (!tan) throw new Error("no se creó el círculo tangente");
    // Tangente a dos lados paralelos: el diámetro es el ancho (largo del segundo lado)
    const Q = new Map(s.points.map((p) => [p.id, [p.x, p.y]]));
    const g1 = s.entities.find((e) => e.id === rect[1]).geometry;
    const width = Math.hypot(Q.get(g1.end)[0] - Q.get(g1.start)[0], Q.get(g1.end)[1] - Q.get(g1.start)[1]);
    near(2 * tan.geometry.radius, width, 1e-4, "diámetro del tangente");
    // Círculos por 2 y por 3 puntos
    await tool("Círculo 2 p.");
    await b.click(500, 250);
    await b.click(580, 250, { wait: 800 });
    await esc();
    await tool("Círculo 3 p.");
    await b.click(650, 260);
    await b.click(700, 210);
    await b.click(750, 260, { wait: 800 });
    await esc();
    if ((await count("circle")) !== 3) throw new Error(`círculos: ${await count("circle")}`);
    await okStatus("círculos por puntos");
    // Polígono circunscrito de 6 lados
    await tool("Polígono");
    await b.clickText("Inscrito");
    await b.click(520, 620);
    await b.click(520, 680, { wait: 800 });
    await esc();
    s = await sk();
    const tangents = s.constraints.filter((c) => c.type === "tangent").length;
    if (tangents < 3 + 6) throw new Error(`tangencias del polígono: ${tangents - 3}`);
    await okStatus("polígono circunscrito");
    // Ranura por el centro y ranura en arco
    const arcs0 = await count("arc");
    await tool("Ranura centro");
    await b.click(720, 450);
    await b.click(790, 450);
    await b.click(790, 480, { wait: 800 });
    await esc();
    if ((await count("arc")) !== arcs0 + 2) throw new Error("ranura por el centro sin sus dos arcos");
    await okStatus("ranura por el centro");
    await tool("Ranura arco");
    await b.click(860, 700);
    await b.click(980, 700);
    await b.click(860, 580);
    await b.click(995, 700, { wait: 800 });
    await esc();
    if ((await count("arc")) !== arcs0 + 2 + 5) throw new Error(`ranura en arco: ${(await count("arc")) - arcs0 - 2} arcos`);
    await okStatus("ranura en arco");
    // Colineales: dos líneas sueltas
    await tool("Línea");
    await b.click(200, 760);
    await b.click(300, 790, { wait: 600 });
    await esc();
    await esc();
    await tool("Línea");
    await b.click(360, 820);
    await b.click(460, 800, { wait: 600 });
    await esc();
    await esc();
    s = await sk();
    const two = s.entities.filter((e) => e.geometry.type === "line" && !e.construction).slice(-2).map((e) => e.id);
    await b.eval(`window.__cadUi.setSelection(${JSON.stringify(two)})`);
    await sleep(300);
    await b.clickText("Colineales");
    await okStatus("colineales");
    s = await sk();
    const R = new Map(s.points.map((p) => [p.id, [p.x, p.y]]));
    const [la, lb] = two.map((id) => s.entities.find((e) => e.id === id).geometry);
    const [a0, a1] = [R.get(la.start), R.get(la.end)];
    const off = (p) => Math.abs((a1[0] - a0[0]) * (p[1] - a0[1]) - (a1[1] - a0[1]) * (p[0] - a0[0])) / Math.hypot(a1[0] - a0[0], a1[1] - a0[1]);
    near(off(R.get(lb.start)), 0, 1e-6, "colineal (comienzo)");
    near(off(R.get(lb.end)), 0, 1e-6, "colineal (fin)");
    await b.shot("formas_nuevas_del_sketch");
    await b.clickText("Terminar sketch");
    await sleep(1500);
  },

  async "revisión del sketch: extremos sueltos, cruces y aviso al extruir"(b) {
    await begin(b);
    await sketchOn(b);
    const problems = () => b.eval(`document.querySelector("[data-sketch-problems]")?.textContent ?? ""`);
    // Una línea suelta: dos extremos sueltos
    await b.clickText("Línea");
    await b.click(400, 300);
    await b.click(560, 330, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    if ((await problems()) !== "2 extremos sueltos") throw new Error(`una línea: "${await problems()}"`);
    // Otra que la cruza sin punto común (Mayús: sin anclajes, así no la parte)
    await b.clickText("Línea");
    await b.click(470, 250, { modifiers: 8 });
    await b.click(490, 400, { modifiers: 8, wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    if (!(await problems()).includes("1 cruce sin unir")) throw new Error(`cruce: "${await problems()}"`);
    if (!(await problems()).includes("4 extremos sueltos")) throw new Error(`extremos: "${await problems()}"`);
    await b.shot("revision_del_sketch");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Extruir ese sketch: no se crea la operación y el aviso dice por qué
    const n = await b.eval(`window.__cadStore.doc().features.length`);
    await b.clickText("Extrusión");
    await sleep(1000);
    const text = await b.eval(`document.body.innerText`);
    if (!text.includes("El sketch no tiene regiones cerradas: 4 extremos sueltos · 1 cruce sin unir")) throw new Error("falta el aviso de por qué no hay regiones");
    if ((await b.eval(`window.__cadStore.doc().features.length`)) !== n) throw new Error("se creó la extrusión igual");
  },

  async "modo construcción, mostrar y ocultar, lista de entidades, cadena y variantes en menús"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    // Modo construcción sin nada elegido: la línea sale de construcción
    await b.clickText("Construcción");
    if (!(await b.eval(`window.__cadUi.constructionMode()`))) throw new Error("no se prendió el modo construcción");
    await b.clickText("Línea");
    await b.click(300, 650);
    await b.click(800, 650, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    let s = await sk();
    if (!s.entities.some((e) => e.geometry.type === "line" && e.construction)) throw new Error("la línea no salió de construcción");
    // Apagado: el rectángulo sale normal y forma región (la construcción no cuenta)
    await b.clickText("Construcción");
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(560, 440, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    s = await sk();
    const rect = s.entities.filter((e) => e.geometry.type === "line" && !e.construction).map((e) => e.id);
    if (rect.length !== 4) throw new Error(`rectángulo: ${rect.length}`);
    await sleep(600);
    if ((await b.eval(`window.__cadUi.session().regions.length`)) !== 1) throw new Error("la región no se formó");
    // Doble clic sobre un lado: la cadena entera (sin la construcción)
    await b.clickText("Elegir");
    await b.click(480, 300, { wait: 80 });
    await b.click(480, 300, { wait: 400 });
    const sel = (await b.eval(`window.__cadUi.selection()`)).sort();
    if (JSON.stringify(sel) !== JSON.stringify([...rect].sort())) throw new Error(`cadena: ${sel} (esperado ${rect})`);
    // Elegir por tipo: construcción
    await b.eval(`[...document.querySelector('[aria-label="Elegir por tipo"]').querySelectorAll("button")].find((x) => x.textContent.trim() === "Construcción").click()`);
    await sleep(300);
    const cons = await b.eval(`window.__cadUi.selection()`);
    if (cons.length !== 1 || rect.includes(cons[0])) throw new Error(`por tipo: ${cons}`);
    // Ctrl+A: todo
    for (const type of ["keyDown", "keyUp"]) await b.send("Input.dispatchKeyEvent", { type, key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers: 2 });
    await sleep(300);
    if ((await b.eval(`window.__cadUi.selection().length`)) !== 5) throw new Error(`Ctrl+A: ${await b.eval(`window.__cadUi.selection().length`)}`);
    // Ocultar las cotas
    // Ocultar las cotas (las etiquetas son botones font-mono)
    if (!(await b.eval(`[...document.querySelectorAll("button.font-mono")].length`))) throw new Error("no hay cotas a la vista");
    await b.eval(`document.querySelector('[aria-label="Mostrar cotas"]').click()`);
    await sleep(400);
    if (await b.eval(`[...document.querySelectorAll("button.font-mono")].length`)) throw new Error("las cotas siguen a la vista");
    await b.eval(`document.querySelector('[aria-label="Mostrar cotas"]').click()`);
    await sleep(400);
    if (!(await b.eval(`[...document.querySelectorAll("button.font-mono")].length`))) throw new Error("las cotas no volvieron");
    // Un punto suelto movido desde sus propiedades
    await b.clickText("Punto");
    await b.click(700, 300, { wait: 800 });
    s = await sk();
    const pt = s.entities.filter((e) => e.geometry.type === "point").at(-1).geometry.point;
    await b.eval(`window.__cadUi.setSelection([${pt}])`);
    await sleep(300);
    await b.eval(`(() => {
      const box = document.querySelector('[aria-label="Propiedades"]');
      const i = [...box.querySelectorAll("label")].find((l) => l.textContent.startsWith("X"))?.querySelector("input");
      i.value = "42"; i.dispatchEvent(new Event("change", { bubbles: true }));
    })()`);
    await sleep(1000);
    near((await sk()).points.find((p) => p.id === pt).x, 42, 1e-6, "x del punto");
    // Variantes en menús: Círculo 3 p. desde la lista de círculos
    await b.clickText("Más círculos");
    await b.clickText("Círculo 3 p.");
    if ((await b.eval(`window.__cadUi.tool()`)) !== "circle3") throw new Error(`variante: ${await b.eval(`window.__cadUi.tool()`)}`);
    // El botón de la familia muestra la última variante usada
    if (!(await b.eval(`!!document.querySelector('button[aria-label="Círculo 3 p."]')`))) throw new Error("el botón no muestra la variante elegida");
    await b.shot("modo_construccion_y_lista");
  },

  async "agujero con rosca modelada"(b) {
    // Volumen por mm de un macho M6 × 1 (perfil ISO básico)
    const rodPerMm = (d, p) => {
      const d1 = d - 1.082532 * p, r1 = d1 / 2, h = (d - d1) / 2;
      const top = p / 8, bottom = top + 2 * h * Math.tan(Math.PI / 6);
      const rc = r1 + (h * (bottom + 2 * top)) / (3 * (bottom + top));
      return Math.PI * r1 * r1 + (((top + bottom) / 2) * h * 2 * Math.PI * rc) / p;
    };
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un punto en la cara de arriba (z = 10) para el centro
    const doc = await call("cad_get_document");
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }], entities: [{ id: 2, geometry: { type: "point", point: 1 } }], constraints: [], next_id: 3, origin: 0 };
    doc.features.push({ id: doc.next_id, name: "Centro", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 10, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Agujero");
    await sleep(2000);
    await b.clickText("Que pase el tornillo");
    await sleep(400);
    await b.clickText("Roscar (rosca modelada)");
    for (let t = 0; t < 40 && !(await b.eval(`window.__cadStore.doc().features.at(-1).kind.modeled?.pitch === 1 && !window.__cadStore.busy()`)); t++) await sleep(500);
    await sleep(1000);
    await accept(b, 4000);
    // Pasante por 20 mm: se va el macho entero
    near((await body()).volume, 8000 - 20 * rodPerMm(6, 1), 0.5, "caja con M6 roscado");
    await b.shot("agujero_roscado");
  },

  async "rosca exterior en un eje"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    // Eje de Ø6 × 10 parado en el origen
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id, name: "Eje", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 3, height: 10 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "join" } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    await b.click(...(await at([0, -3, 5])), { wait: 800 });
    await b.clickText("Rosca");
    for (let t = 0; t < 40 && !(await b.eval(`window.__cadStore.doc().features.at(-1).kind.type === "thread" && !window.__cadStore.busy()`)); t++) await sleep(500);
    await sleep(1000);
    const k = await b.eval(`window.__cadStore.doc().features.at(-1).kind`);
    if (k.pitch !== 1) throw new Error(`paso: ${k.pitch}`);
    await accept(b, 4000);
    const v = (await body()).volume;
    // El macho M6 de 10 mm: 229,6 mm³ (contra 282,7 del eje liso)
    near(v, 229.6, 0.5, "eje roscado");
    await b.shot("eje_roscado");
  },

  async "usar arista del sólido en el sketch"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Sketch sobre la cara de arriba (z = 10)
    await b.click(...(await at([3, 3, 10])), { wait: 800 });
    await b.clickText("Sketch");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()`)); t++) await sleep(250);
    await b.clickText("Usar arista");
    await b.click(...(await at([0, -10, 10])), { wait: 1500 });
    const sk = await b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    if (sk.uses?.length !== 1) throw new Error(`usadas: ${JSON.stringify(sk.uses)}`);
    const line = sk.entities.find((e) => e.id === sk.uses[0].entity);
    if (line?.geometry.type !== "line") throw new Error(`entidad: ${JSON.stringify(line)}`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // La caja más ancha: la línea usada se estira con la arista
    const doc = await call("cad_get_document");
    doc.features[0].kind.shape.dx = 30;
    const id = doc.features.find((f) => f.kind.type === "sketch").id;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const xs = await b.eval(`(() => {
      const v = window.__cadStore.sketchView(${id});
      const l = v.sketch.entities.find((e) => e.id === ${line.id}).geometry;
      return [l.start, l.end].map((p) => v.sketch.points.find((q) => q.id === p).x).sort((a, b) => a - b);
    })()`);
    near(xs[0], -15, 1e-6, "extremo izquierdo");
    near(xs[1], 15, 1e-6, "extremo derecho");
    await b.shot("usar_arista");
  },

  async "configuraciones: variante, valores y exportar todas"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Ancho por parámetro y una variante «Chica»
    const doc = await call("cad_get_document");
    doc.parameters = [{ name: "ancho", expr: "20" }];
    doc.bindings = { [`${doc.features[0].id}.kind.shape.dx`]: "ancho" };
    doc.configurations = [{ name: "Chica", values: { ancho: "10" }, suppressed: [] }];
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    near((await body()).volume, 8000, 1e-6, "base");
    await b.eval(`document.querySelector('[aria-label="Usar Chica"]').click()`);
    await sleep(2000);
    near((await body()).volume, 4000, 1e-6, "variante chica");
    // Cambiar su valor desde el panel
    await b.eval(`(() => { const i = document.querySelector('[aria-label="ancho en Chica"]'); i.value = "5"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(2000);
    near((await body()).volume, 2000, 1e-6, "variante con 5");
    if ((await call("cad_get_document")).parameters[0].expr !== "20") throw new Error("cambió lo de base");
    await b.shot("configuraciones");
    // Exportar todas: un STL por variante
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    await b.clickContains("Exportar");
    await sleep(800);
    await b.clickText("Todo el diseño");
    await sleep(400);
    await b.clickText("Todas las configuraciones (2 archivos)");
    await sleep(400);
    await b.clickText("STL");
    for (const f of ["caja-base.stl", "caja-Chica.stl"]) rmSync(`${out}/${f}`, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(`${out}/caja.stl`)}`);
    await b.clickText("Exportar STL");
    await sleep(5000);
    for (const f of ["caja-base.stl", "caja-Chica.stl"]) if (!existsSync(`${out}/${f}`)) throw new Error(`no se exportó ${f}`);
    // Vuelve a la variante que estaba
    if ((await call("cad_get_document")).active_configuration !== 0) throw new Error("no volvió a la variante elegida");
  },

  async "versiones: guardar, ver, comparar y restaurar"(b) {
    const vol = async () => (await body()).volume;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    near(await vol(), 8000, 1e-6, "caja");
    // Guardar «v1» desde el panel
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nota de la versión"]'); i.value = "enviada a imprimir"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
    await b.clickText("Guardar versión");
    await sleep(1000);
    const saved = (await call("cad_get_document")).versions;
    if (saved?.length !== 1 || saved[0].name !== "v1" || saved[0].note !== "enviada a imprimir") throw new Error(`versiones: ${JSON.stringify(saved?.map((v) => [v.name, v.note]))}`);
    if (saved[0].document.versions) throw new Error("la versión guarda versiones adentro");
    // Más ancha: 30 × 20 × 20
    await b.eval(`window.__cadStore.commit((d) => (d.features[0].kind.shape.dx = 30))`);
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "más ancha");
    // Ver v1 en solo lectura: el visor muestra la vieja y no se puede editar
    await b.eval(`document.querySelector('[aria-label="Ver v1"]').click()`);
    await sleep(1500);
    near(await vol(), 8000, 1e-6, "viendo v1");
    if (!(await b.eval(`!!document.querySelector("[data-viewing]")`))) throw new Error("sin aviso de solo lectura");
    await b.eval(`window.__cadStore.commit((d) => (d.features[0].kind.shape.dx = 50))`);
    await sleep(500);
    if ((await call("cad_get_document")).features[0].kind.shape.dx !== 30) throw new Error("se pudo editar viendo una versión");
    await b.shot("version_viendo");
    await b.clickText("Volver al diseño");
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "de vuelta");
    // Comparar: se agregaron 4000 mm³ y no se quitó nada
    await b.eval(`document.querySelector('[aria-label="Comparar con v1"]').click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadStore.comparison()`)); t++) await sleep(250);
    const c = await b.eval(`(() => { const c = window.__cadStore.comparison(); return { added: c.added, removed: c.removed, mesh: !!c.addedMesh, none: !c.removedMesh }; })()`);
    near(c.added, 4000, 1e-6, "agregado");
    near(c.removed, 0, 1e-6, "quitado");
    if (!c.mesh || !c.none) throw new Error(`mallas: ${JSON.stringify(c)}`);
    await b.shot("version_comparar");
    // Restaurar: vuelve a 20 de ancho, la versión sigue y se puede deshacer
    await b.eval(`document.querySelector('[aria-label="Restaurar v1"]').click()`);
    await sleep(1500);
    near(await vol(), 8000, 1e-6, "restaurada");
    if ((await call("cad_get_document")).versions?.length !== 1) throw new Error("restaurar borró las versiones");
    await b.eval(`window.__cadStore.undo()`);
    await sleep(1500);
    near(await vol(), 12000, 1e-6, "deshacer restaurar");
  },

  async "piezas estándar: tornillo en un agujero y tuerca roscada"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    const last = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1)))`);
    await begin(b);
    // Placa de 40×40×10 con un agujero pasante de 6,6 (M6)
    const doc = await call("cad_get_document");
    const prim = (shape, origin, op) => ({ id: doc.next_id++, name: shape.type, suppressed: false, kind: { type: "primitive", shape, origin, z: [0, 0, 1], x: [1, 0, 0], op } });
    doc.features.push(prim({ type: "box", dx: 40, dy: 40, dz: 10, centered: true }, [0, 0, 0], "join"));
    doc.features.push(prim({ type: "cylinder", radius: 3.3, height: 20 }, [0, 0, -5], "cut"));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    // Borde de arriba del agujero → Tornillo: M6, apoyado en la cara, largo para placa + tuerca
    await filter("Aristas");
    await b.click(...(await at([-3.3, 0, 10])), { wait: 800 });
    const picked = await b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
    if (picked.join() !== "edge") throw new Error(`elegido: ${picked}`);
    await b.clickText("Tornillo");
    for (let t = 0; t < 20 && (await last()).kind.shape.type !== "bolt"; t++) await sleep(250);
    await sleep(1500);
    const bolt = (await last()).kind;
    if (bolt.shape.size !== "M6" || bolt.shape.length !== 20 || bolt.op !== "new") throw new Error(`tornillo: ${JSON.stringify(bolt)}`);
    near(bolt.origin[2], 10, 1e-6, "apoyo");
    near(bolt.z[2], 1, 1e-6, "eje");
    await accept(b, 2000);
    let parts = (await evaluate()).parts;
    if (parts.length !== 2 || parts[1].name !== "Tornillo Allen M6×20") throw new Error(`piezas: ${parts.map((p) => p.name)}`);
    await b.shot("tornillo_en_agujero");
    // Tuerca suelta, y con rosca modelada pierde el filete
    await filter("Todo");
    await b.clickText("Tuerca");
    for (let t = 0; t < 20 && (await last()).kind.shape.type !== "nut"; t++) await sleep(250);
    await sleep(1500);
    await b.eval(`window.__cadStore.commit((d) => (d.features.at(-1).kind.origin = [0, 0, -12]))`);
    await sleep(1500);
    const plain = (await evaluate()).parts[2].volume;
    await b.clickText("Rosca modelada");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__cadStore.doc().features.at(-1).kind.shape.modeled && !window.__cadStore.busy()`)); t++) await sleep(250);
    await sleep(500);
    await accept(b, 3000);
    parts = (await evaluate()).parts;
    if (parts[2]?.name !== "Tuerca M6") throw new Error(`tuerca: ${parts.map((p) => p.name)}`);
    // Lo que queda entre el agujero liso de 6 y el macho M6: unos 5,3 mm³ por mm de alto (≈ 27,6)
    if (!(parts[2].volume > plain + 22 && parts[2].volume < plain + 33)) throw new Error(`tuerca roscada ${parts[2].volume} vs lisa ${plain}`);
    await b.shot("tuerca_roscada");
  },

  async "roscas coordinadas: tornillo en agujero roscado y rosca alrededor de un tornillo"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const last = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1)))`);
    const idle = async (cond) => {
      for (let t = 0; t < 60 && !(await b.eval(`(${cond}) && !window.__cadStore.busy()`)); t++) await sleep(500);
      await sleep(800);
    };
    const question = () => b.eval(`document.querySelector("[data-thread-question]")?.textContent ?? ""`);
    await begin(b);
    // 1) Placa con un agujero M6 roscado (modelado, holgura 0,2) y después un tornillo
    const doc = await call("cad_get_document");
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }], entities: [{ id: 2, geometry: { type: "point", point: 1 } }], constraints: [], next_id: 3, origin: 0 };
    const add = (name, kind) => {
      const id = doc.next_id++;
      doc.features.push({ id, name, suppressed: false, kind });
      return id;
    };
    add("Placa", { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "join" });
    const sk = add("Centro", { type: "sketch", plane: { type: "xy" }, offset: 10, sketch });
    const hole = add("Agujero roscado", {
      type: "hole", sketch: sk, points: [], diameter: 5, depth: { type: "through_all" }, style: { type: "simple" }, tip_angle: 0, thread: "M6",
      modeled: { nominal: 6, pitch: 1, clearance: 0.2, left: false },
    });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2500);
    await b.eval(`window.__cadViewer.frameAll()`);
    // Tornillo M8 suelto en el origen: pregunta si va en la rosca del agujero (está en su eje)
    await b.clickText("Tornillo");
    await idle(`window.__cadStore.doc().features.at(-1).kind.shape?.type === "bolt"`);
    const q = await question();
    if (!q.includes("Agujero roscado")) throw new Error(`pregunta: ${q}`);
    await b.shot("pregunta_rosca");
    await b.clickText("Coordinar");
    await idle(`!!window.__cadStore.doc().features.at(-1).kind.link`);
    const bolt = (await last()).kind;
    if (bolt.link.feature !== hole || bolt.shape.size !== "M6" || bolt.shape.length !== 10 || !bolt.shape.modeled) throw new Error(`tornillo: ${JSON.stringify(bolt)}`);
    await accept(b, 4000);
    let r = await evaluate();
    const errs = r.status.filter((s) => s.state === "error");
    if (errs.length) throw new Error(`errores: ${JSON.stringify(errs)}`);
    const placed = r.threads.find((t) => !t.internal).placed;
    near(placed[0][2], 10, 1e-6, "cabeza en la boca");
    near(Math.hypot(placed[0][0], placed[0][1]), 0, 1e-6, "en el eje");
    if (r.parts.length !== 2) throw new Error(`piezas: ${r.parts.length}`);
    await b.shot("tornillo_coordinado");

    // 2) Al revés: el tornillo primero, una placa con un agujero liso alrededor y
    //    la rosca del agujero toma la del tornillo (elegido con un clic)
    const doc2 = await call("cad_get_document");
    doc2.features = [];
    const add2 = (name, kind, scope) => {
      const id = doc2.next_id++;
      doc2.features.push({ id, name, suppressed: false, kind, ...(scope ? { scope } : {}) });
      return id;
    };
    add2("Perno", { type: "primitive", shape: { type: "bolt", size: "M6", length: 12, head: "socket", modeled: true }, origin: [0, 0, 10.5], z: [0, 0, 1], x: [0.3, 0.8, 0], op: "new" });
    const plate = add2("Bloque", { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" });
    const sk2 = add2("Centro", { type: "sketch", plane: { type: "xy" }, offset: 10, sketch });
    const d1 = 6 - 1.082532;
    add2("Agujero liso", { type: "hole", sketch: sk2, points: [], diameter: d1, depth: { type: "through_all" }, style: { type: "simple" }, tip_angle: 0, thread: null }, [{ feature: plate, index: 0 }]);
    await call("cad_set_document", { document: doc2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2500);
    await b.eval(`window.__cadViewer.frameAll()`);
    // La rosca en la pared del agujero (paso 1,5 a propósito: el del tornillo manda)
    await b.eval(`window.__cadStore.addFeature({ type: "thread", face: { point: [${d1 / 2}, 0, 5], normal: [-1, 0, 0], tags: [] }, pitch: 1.5, length: 0, flip: false, left: false, clearance: 0.2 })`);
    await idle(`window.__cadStore.doc().features.at(-1).kind.type === "thread"`);
    for (let t = 0; t < 20 && !(await question()); t++) await sleep(500);
    const q2 = await question();
    if (!q2.includes("Perno")) throw new Error(`pregunta de la rosca: ${q2}`);
    await b.clickText("Elegir en el visor");
    await sleep(1500);
    // Clic en la cabeza del tornillo (su costado, arriba de la placa)
    await b.click(...(await at([0, -5, 14])), { wait: 1500 });
    await idle(`!!window.__cadStore.doc().features.at(-1).kind.link`);
    const th = (await last()).kind;
    if (th.link?.feature !== doc2.features[0].id || th.pitch !== 1) throw new Error(`rosca: ${JSON.stringify(th)}`);
    await accept(b, 5000);
    r = await evaluate();
    const errs2 = r.status.filter((s) => s.state === "error");
    if (errs2.length) throw new Error(`errores: ${JSON.stringify(errs2)}`);
    const it = r.threads.find((t) => t.internal);
    if (!it || it.spec.pitch !== 1 || it.spec.nominal !== 6) throw new Error(`rosca calculada: ${JSON.stringify(it)}`);
    await b.shot("rosca_coordinada");
  },

  async "chapa: pestañas y desarrollo en DXF"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    await begin(b);
    // Chapa de 100 × 50 × 2 (radio 2, K 0,44)
    const doc = await call("cad_get_document");
    const P = [[0, 0], [100, 0], [100, 50], [0, 50]];
    const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))];
    const line = (id, a, c) => ({ id, geometry: { type: "line", start: a, end: c } });
    const sketch = { points, entities: [line(5, 1, 2), line(6, 2, 3), line(7, 3, 4), line(8, 4, 1)], constraints: [], next_id: 9, origin: 0 };
    const sk = doc.next_id++;
    doc.features.push({ id: sk, name: "Contorno", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.features.push({ id: doc.next_id++, name: "Chapa", suppressed: false, kind: { type: "sheet_metal", sketch: sk, regions: { type: "all" }, thickness: 2, radius: 2, k_factor: 0.44, flip: false, op: "join" } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    near((await body()).volume, 10000, 1e-6, "chapa");
    // Arista de arriba del borde largo de adelante → Pestaña (sube 20)
    await filter("Aristas");
    await b.click(...(await at([60, 0, 2])), { wait: 800 });
    const picked = await b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
    if (picked.join() !== "edge") throw new Error(`elegido: ${picked}`);
    await b.clickText("Pestaña");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "flange"; t++) await sleep(250);
    await sleep(1500);
    await accept(b, 2000);
    const bend = (Math.PI / 4) * (16 - 4) * 100;
    near((await body()).volume, 10000 + bend + 4000, 1e-3, "con una pestaña");
    // La segunda del otro lado, hacia el mismo lado (por documento, la arista de y = 50)
    await filter("Aristas");
    await b.click(...(await at([60, 50, 2])), { wait: 800 });
    await b.clickText("Pestaña");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.filter((f) => f.kind.type === "flange").length`)) !== 2; t++) await sleep(250);
    await sleep(1500);
    await accept(b, 2000);
    near((await body()).volume, 10000 + 2 * (bend + 4000), 1e-3, "canal en U");
    await b.shot("chapa_canal");
    // Desarrollo: 100 × (50 + 2·π/2·2,88 + 40) = 100 × 99,05
    await filter("Todo");
    await b.clickText("Desarrollo");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-flat] svg")`)); t++) await sleep(300);
    const size = await b.eval(`document.querySelector("[data-flat-size]")?.textContent ?? ""`);
    if (!size.startsWith("100 × 99,05 mm") || !size.includes("2 dobleces")) throw new Error(`desarrollo: ${size}`);
    if ((await b.eval(`document.querySelectorAll("[data-flat] [data-bend]").length`)) !== 2) throw new Error("líneas de doblez");
    await b.shot("chapa_desarrollo");
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/canal.dxf`;
    rmSync(path, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    const dxf = existsSync(path) ? readFileSync(path, "utf8") : "";
    if (!dxf.includes("CONTORNO") || (dxf.match(/\nDOBLEZ\n/g) ?? []).length < 2) throw new Error("DXF del desarrollo");
  },

  async "chapa: alivios"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    await begin(b);
    // L: 100 × 50 y un brazo de 50 × 30 que baja desde x = 50 (espesor 2, radio 2)
    const doc = await call("cad_get_document");
    const P = [[0, 0], [50, 0], [50, -30], [100, -30], [100, 50], [0, 50]];
    const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))];
    const line = (id, a, c) => ({ id, geometry: { type: "line", start: a, end: c } });
    const entities = P.map((_, i) => line(10 + i, i + 1, ((i + 1) % P.length) + 1));
    const sketch = { points, entities, constraints: [], next_id: 20, origin: 0 };
    const sk = doc.next_id++;
    doc.features.push({ id: sk, name: "Contorno", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.features.push({ id: doc.next_id++, name: "Chapa", suppressed: false, kind: { type: "sheet_metal", sketch: sk, regions: { type: "all" }, thickness: 2, radius: 2, k_factor: 0.44, flip: false, op: "join" } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    const plate = (100 * 50 + 50 * 30) * 2;
    near((await body()).volume, plate, 1e-3, "chapa en L");
    // Pestaña en el borde de y = 0 entre x = 0 y 50: en x = 50 la chapa sigue (el brazo)
    await filter("Aristas");
    await b.click(...(await at([20, 0, 2])), { wait: 800 });
    await b.clickText("Pestaña");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "flange"; t++) await sleep(250);
    await sleep(1500);
    // El diálogo ofrece el alivio y la esquina
    for (const label of ["Alivio", "Esquina", "Ancho del alivio"]) {
      if (!(await b.eval(`[...document.querySelectorAll("label, span, div")].some((e) => e.textContent.trim() === ${JSON.stringify(label)})`)))
        throw new Error("falta en el diálogo: " + label);
    }
    await accept(b, 2000);
    const bend = (Math.PI / 4) * (16 - 4) * 50;
    // Ranura recta de 2 × 6 a través del espesor
    near((await body()).volume, plate + bend + 2000 - 24, 1e-3, "con alivio recto");
    const id = await b.eval(`window.__cadStore.doc().features.at(-1).id`);
    await b.eval(`window.__cadStore.updateFeature(${id}, (f) => { f.kind.relief = "obround"; })`);
    await sleep(2000);
    near((await body()).volume, plate + bend + 2000 - (2 * 2 * 5 + (Math.PI / 2) * 2), 1e-3, "con alivio redondo");
    await b.shot("chapa_alivio");
    // El desarrollo sale con el doblez
    await filter("Todo");
    await b.clickText("Desarrollo");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-flat] svg")`)); t++) await sleep(300);
    const size = await b.eval(`document.querySelector("[data-flat-size]")?.textContent ?? ""`);
    if (!size.includes("1 doblez")) throw new Error(`desarrollo: ${size}`);
  },

  async "superficies: tubo, rellenos y coser"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const filter = (label) => b.eval(`[...document.querySelectorAll('[role=radio]')].find((e) => e.textContent === ${JSON.stringify(label)}).click()`);
    const parts = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result()?.parts ?? []))`);
    await begin(b);
    // Círculo de radio 5 en XY → Superficie extruida de 10 (desde el menú)
    const doc = await call("cad_get_document");
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }], entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 5 } }], constraints: [], next_id: 3, origin: 0 };
    doc.features.push({ id: doc.next_id++, name: "Círculo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Superficie extruida");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "surface_extrude"; t++) await sleep(250);
    await sleep(1500);
    await accept(b, 2000);
    await b.eval(`window.__cadViewer.frameAll()`);
    await sleep(500);
    let ps = await parts();
    if (ps.length !== 1 || !ps[0].surface) throw new Error(`tubo: ${JSON.stringify(ps)}`);
    near(ps[0].area, 2 * Math.PI * 5 * 10, 1e-3, "área del tubo");
    // Tapas: Relleno del círculo de abajo y del de arriba (lejos de la costura en x = 5)
    for (const z of [0, 10]) {
      await filter("Aristas");
      await b.click(...(await at([-5, 0, z])), { wait: 800 });
      const picked = await b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
      if (picked.join() !== "edge") throw new Error(`elegido: ${picked}`);
      await b.clickText("Relleno");
      for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.filter((f) => f.kind.type === "fill").length`)) !== (z ? 2 : 1); t++) await sleep(250);
      // Plano: sin tangencia
      await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.kind.tangent = false; })`);
      await sleep(1500);
      await accept(b, 2000);
    }
    ps = await parts();
    if (ps.length !== 3 || !ps.every((p) => p.surface)) throw new Error(`con tapas: ${JSON.stringify(ps.map((p) => [p.name, p.surface]))}`);
    await b.shot("superficies_tapas");
    // Coser (sin elegir: todas las superficies) → sólido
    await filter("Todo");
    await b.clickText("Coser");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "sew"; t++) await sleep(250);
    await sleep(1500);
    await accept(b, 2000);
    ps = await parts();
    if (ps.length !== 1 || ps[0].surface) throw new Error(`cosido: ${JSON.stringify(ps)}`);
    near((await body()).volume, Math.PI * 25 * 10, 1e-3, "lata cerrada");
    await b.shot("superficies_cosido");
  },

  async "patrones por tabla y de relleno, partir y recortar superficies"(b) {
    await begin(b);
    // Placa de 100 × 60 × 2, agujero de Ø4 en (10, 10) y una región de 5 a 95 por 5 a 55
    const doc = await call("cad_get_document");
    const rect = (x0, y0, x1, y1, base) => {
      const P = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
      const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: base + i, x, y }))];
      const entities = P.map((_, i) => ({ id: base + 10 + i, geometry: { type: "line", start: base + i, end: base + ((i + 1) % 4) } }));
      return { points, entities, constraints: [], next_id: base + 20, origin: 0 };
    };
    const id = () => doc.next_id++;
    const plate = id();
    doc.features.push({ id: plate, name: "Placa", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: rect(0, 0, 100, 60, 1) } });
    doc.features.push({ id: id(), name: "Extrusión", suppressed: false, kind: { type: "extrude", sketch: plate, regions: { type: "all" }, extent: { type: "blind", distance: 2 }, reverse: false, op: "join", draft: 0, thin: null } });
    const hs = id();
    doc.features.push({ id: hs, name: "Círculo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 10, y: 10 }], entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 2 } }], constraints: [], next_id: 3, origin: 0 } } });
    const hole = id();
    doc.features.push({ id: hole, name: "Agujero", suppressed: false, kind: { type: "extrude", sketch: hs, regions: { type: "all" }, extent: { type: "through_all" }, reverse: false, op: "cut", draft: 0, thin: null } });
    doc.features.push({ id: id(), name: "Región", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: rect(5, 5, 95, 55, 1) } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const holeV = Math.PI * 4 * 2;
    // Relleno: con el agujero elegido, 45 en grilla cuadrada (centros a 3 del borde)
    await b.eval(`window.__cadStore.select(${hole})`);
    await b.clickText("Patrón de relleno");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.pattern?.type`)) !== "fill"; t++) await sleep(250);

    await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.kind.pattern.margin = 3; })`);
    await sleep(2500);
    await accept(b, 2500);
    near((await body()).volume, 12000 - 45 * holeV, 1e-3, "relleno");
    // Hexagonal: 43
    await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.kind.pattern.hex = true; })`);
    await sleep(2500);
    near((await body()).volume, 12000 - 43 * holeV, 1e-3, "relleno hexagonal");
    await b.shot("patron_relleno");
    // Por tabla: suprimido el relleno, dos copias más (filas por defecto)
    await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.suppressed = true; })`);
    await sleep(1500);
    await b.eval(`window.__cadStore.select(${hole})`);
    await b.clickText("Patrón por tabla");
    for (let t = 0; t < 20 && (await b.eval(`document.querySelectorAll("[data-table-row]").length`)) !== 2; t++) await sleep(250);
    await b.clickText("Agregar fila");
    await sleep(2500);
    await accept(b, 2500);
    near((await body()).volume, 12000 - 4 * holeV, 1e-3, "tabla de 3 filas");
    // Partir con el plano a z = 1: dos placas
    await b.clickText("Partir");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "split_by"; t++) await sleep(250);
    await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.kind.tool.plane.plane.origin = [0, 0, 1]; })`);
    await sleep(2500);
    await accept(b, 2500);
    const parts = await b.eval(`window.__cadStore.result().parts.length`);
    if (parts !== 2) throw new Error(`partir: ${parts} piezas`);
    // Recortar con plano un tubo de superficie (r 5, alto 10, aparte) a z = 4
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 90, name: "Círculo tubo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 150, y: 0 }], entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 5 } }], constraints: [], next_id: 3, origin: 0 } } });
      d.features.push({ id: 91, name: "Tubo", suppressed: false, kind: { type: "surface_extrude", sketch: 90, entities: [], extent: { type: "blind", distance: 10 }, reverse: false } });
      d.next_id = 92;
    })`);
    await sleep(2500);
    await b.clickText("Recortar con plano");
    for (let t = 0; t < 20 && (await b.eval(`window.__cadStore.doc().features.at(-1).kind.type`)) !== "split"; t++) await sleep(250);
    await b.eval(`window.__cadStore.updateFeature(window.__cadStore.doc().features.at(-1).id, (f) => { f.kind.plane.plane.origin = [0, 0, 4]; })`);
    await sleep(2500);
    await accept(b, 2500);
    const tube = await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result().parts.find((p) => p.surface)))`);
    near(tube.area, 2 * Math.PI * 5 * 6, 1e-3, "tubo recortado");
    // Las placas no se tocaron (el recorte solo actúa sobre la superficie)
    if ((await b.eval(`window.__cadStore.result().parts.filter((p) => !p.surface).length`)) !== 2) throw new Error("el recorte tocó las placas");
  },

  async "chaflán de dos distancias y redondeo variable"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const last = () => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1).kind))`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Chaflán en la arista de arriba al frente: d y 2d
    await b.click(...(await at([0, -10, 10])), { wait: 800 });
    await b.clickText("Chaflán");
    await sleep(1500);
    await b.clickText("Igual a los dos lados");
    await sleep(400);
    await b.clickText("Dos distancias");
    await sleep(2000);
    const k = await last();
    if (k.second?.type !== "distance") throw new Error(`chaflán: ${JSON.stringify(k)}`);
    near((await body()).volume, 8000 - (k.distance * k.second.distance) / 2 * 20, 1e-3, "chaflán asimétrico");
    await accept(b);
    // Redondeo variable en la arista de arriba atrás
    await b.click(...(await at([0, 10, 10])), { wait: 800 });
    await b.clickText("Redondeo");
    await sleep(1500);
    const before = (await body()).volume;
    await b.clickText("Radio variable");
    await sleep(2500);
    const f = await last();
    if (!(f.radius2 > f.radius)) throw new Error(`redondeo: ${JSON.stringify(f)}`);
    if (!((await body()).volume < before)) throw new Error("el radio variable no quitó más");
    await accept(b);
    await b.shot("chaflan_y_redondeo_variable");
  },

  async "línea desde el centro"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Línea centro");
    await b.click(550, 400);
    await b.click(700, 403);
    await sleep(800);
    const sk = await b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const lines = sk.entities.filter((e) => e.geometry.type === "line");
    if (lines.length !== 1) throw new Error(`líneas: ${lines.length}`);
    const g = lines[0].geometry;
    const kinds = sk.constraints.map((c) => c.type);
    if (!kinds.includes("midpoint") || !kinds.includes("horizontal")) throw new Error(`restricciones: ${kinds}`);
    // El centro queda en el medio de los dos extremos
    const P = (id) => sk.points.find((q) => q.id === id);
    const mid = sk.constraints.find((c) => c.type === "midpoint");
    const [a, e, c] = [P(g.start), P(g.end), P(mid.point)];
    near((a.x + e.x) / 2, c.x, 1e-6, "medio en x");
    near((a.y + e.y) / 2, c.y, 1e-6, "medio en y");
    near(a.y, e.y, 1e-6, "horizontal");
    await b.shot("linea_desde_el_centro");
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

  async "caja de selección empezando sobre el sólido"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const picks = () => b.eval(`window.__cadUi.picks().map((p) => p.kind)`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un clic sin arrastrar elige solo la cara de arriba
    const [x, y] = await at([0, 0, 10]);
    await b.click(x, y, { wait: 800 });
    if ((await picks()).join() !== "face") throw new Error(`clic: ${await picks()}`);
    // Arrastrar desde la cara hacia la izquierda (cruce): todo lo que toca, sin la del clic suelto
    const [x1, y1] = await at([-14, 14, -14]);
    await b.drag(x, y, Math.min(x1, x - 150), Math.max(y1, y + 150));
    const got = await picks();
    if (!(got.length > 2 && got.includes("edge"))) throw new Error(`caja sobre el sólido: ${got}`);
    await b.shot("caja_sobre_el_solido");
  },

  async "barra de estado: atajos según lo que se hace"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const hints = () => b.eval(`[...document.querySelectorAll("[data-hints] > span")].map((s) => s.querySelector("kbd").textContent + ": " + s.lastChild.textContent.trim())`);
    const has = async (want, what) => {
      const h = await hints();
      if (!want.every((w) => h.includes(w))) throw new Error(`${what}: ${JSON.stringify(h)}`);
    };
    await begin(b);
    await has(["Clic: elegir", "Arrastrar: caja", "Mayús+1…7: vistas"], "sin nada");
    if (await b.eval(`document.body.innerText.includes("Importa un modelo para comenzar")`)) throw new Error("quedó el mensaje de importar");
    await b.clickText("Caja");
    await sleep(1500);
    await has(["Enter: aceptar", "Esc: cancelar"], "con el diálogo");
    await accept(b);
    await b.click(...(await at([0, 0, 10])), { wait: 800 });
    await has(["Mayús+clic: sumar o quitar", "Esc: limpiar"], "con algo elegido");
    await b.key("Escape", "Escape", 27);
    // Al pedir dónde va el sketch, el aviso no tapa la barra de herramientas
    await b.clickText("Sketch");
    await sleep(500);
    await has(["Clic: elegir", "Esc: cancelar"], "eligiendo");
    const overlap = await b.eval(`(() => {
      const cancel = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Cancelar (Esc)");
      const prompt = cancel.parentElement.getBoundingClientRect();
      const bar = Math.max(...[...document.querySelectorAll("[data-toolbar-menu]")].map((m) => m.getBoundingClientRect().bottom));
      return prompt.top < bar;
    })()`);
    if (overlap) throw new Error("el aviso tapa la barra");
    await b.shot("barra_de_estado_eligiendo");
    await b.key("Escape", "Escape", 27);
    await sleep(300);
    await sketchOn(b);
    await has(["Clic: dibujar", "Enter: terminar"], "en el sketch");
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
    // Sin el texto del botón Limpiar
    const summary = await b.eval(`(() => {
      const e = [...document.querySelectorAll("span")].find((x) => x.textContent?.startsWith("Elegido"));
      return [...(e?.childNodes ?? [])].filter((n) => n.nodeType === 3).map((n) => n.textContent).join("").trim();
    })()`);
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

  async "patrones del sketch: grilla, ángulo, en curva, tabla y relleno"(b) {
    await begin(b);
    await sketchOn(b);
    // Dibujo armado directo en el sketch (mm): un rectángulo de 60 × 40, un
    // camino recto y círculos sueltos para cada patrón
    const made = await b.eval(`(() => {
      const ids = {};
      window.__cadUi.change((s) => {
        let next = Math.max(...s.points.map((p) => p.id), ...s.entities.map((e) => e.id), s.next_id ?? 0) + 1;
        const pt = (x, y) => (s.points.push({ id: next, x, y }), next++);
        const ent = (geometry) => (s.entities.push({ id: next, geometry }), next++);
        const corners = [pt(0, 0), pt(60, 0), pt(60, 40), pt(0, 40)];
        ids.rect = corners.map((p, i) => ent({ type: "line", start: p, end: corners[(i + 1) % 4] }));
        const circle = (x, y, r) => ent({ type: "circle", center: pt(x, y), radius: r });
        ids.fill = circle(5, 5, 1.5);
        ids.path = ent({ type: "line", start: pt(0, -20), end: pt(60, -20) });
        ids.curve = circle(0, -15, 1);
        ids.table = circle(-30, 50, 1);
        ids.arc = circle(80, 0, 1);
        ids.grid = circle(-30, -30, 1);
        s.next_id = next;
      });
      return ids;
    })()`);
    await sleep(1200);
    const kind = async (from, to) => {
      await b.clickText(from);
      await sleep(300);
      await b.clickText(to);
      await sleep(300);
    };
    // Elegir primero (sin elección no se ve la sección de patrones), después el tipo
    const select = async (ids) => {
      await b.eval(`window.__cadUi.setSelection(${JSON.stringify(ids)})`);
      await sleep(400);
    };
    const run = async () => {
      await b.clickText("Repetir en patrón");
      await sleep(1200);
      const msg = await b.eval(`window.__cadUi.message?.() ?? null`);
      if (msg) throw new Error(`patrón: ${msg}`);
    };
    // Relleno: la región es la del primer lado elegido
    await select([made.rect[0], made.fill]);
    await kind("Lineal", "De relleno");
    await setInput(b, "Separación", 10);
    await setInput(b, "Margen al borde", 1);
    await run();
    // Tabla con las dos filas de siempre: (10, 0) y (20, 5)
    await select([made.table]);
    await kind("De relleno", "Por tabla");
    await run();
    // En curva sobre la línea: 4 de punta a punta
    await select([made.path, made.curve]);
    await kind("Por tabla", "En curva");
    await setInput(b, "Cantidad", 4);
    await run();
    // Circular en 90°: 3 de punta a punta alrededor del origen
    await select([made.arc]);
    await kind("En curva", "Circular");
    await setInput(b, "Cantidad", 3);
    await setInput(b, "Ángulo", 90);
    await run();
    // Grilla: 2 × 2 de a 20 mm (X) y 20 mm (filas, Y)
    await select([made.grid]);
    await kind("Circular", "Lineal");
    await setInput(b, "Cantidad", 2);
    await setInput(b, "Filas", 2);
    await run();
    await b.shot("sketch-patrones");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const doc = await call("cad_get_document");
    const sk = doc.features[0].kind.sketch;
    const centersOf = (s) => s.entities.filter((e) => e.geometry.type === "circle").map((e) => s.points.find((p) => p.id === e.geometry.center));
    const has = (list, x, y, what) => {
      if (!list.some((c) => Math.abs(c.x - x) < 1e-3 && Math.abs(c.y - y) < 1e-3)) throw new Error(`${what} (${x}, ${y}): ${JSON.stringify(list.map((c) => [c.x, c.y]))}`);
    };
    const all = centersOf(sk);
    const inRect = all.filter((c) => c.x > 0 && c.x < 60 && c.y > 0 && c.y < 40);
    if (inRect.length !== 24) throw new Error(`relleno: ${inRect.length} círculos (esperados 6 × 4)`);
    has(all, 55, 35, "relleno");
    has(all, -20, 50, "tabla 1");
    has(all, -10, 55, "tabla 2");
    for (const x of [20, 40, 60]) has(all, x, -15, "en curva");
    has(all, 80 * Math.SQRT1_2, 80 * Math.SQRT1_2, "circular a 45°");
    has(all, 0, 80, "circular a 90°");
    for (const [x, y] of [[-10, -30], [-30, -10], [-10, -10]]) has(all, x, y, "grilla");
    // El paso del relleno es una cota: a 12 mm la grilla la sigue
    const step = sk.constraints.find((k) => k.type === "length" && k.value === 10);
    step.value = 12;
    await call("cad_set_document", { document: doc });
    const ev = await evaluate();
    const solved = ev.sketches[0];
    if (solved.report?.status === "failed" || solved.report?.status === "over_constrained") throw new Error(`solver: ${JSON.stringify(solved.report)}`);
    const after = centersOf(solved.sketch);
    const lead = after.find((c) => Math.abs(c.x - 5) < 1e-3 && Math.abs(c.y - 5) < 1e-3) ?? after[0];
    has(after, lead.x + 12, lead.y, "relleno a 12 mm");
    has(after, lead.x, lead.y + 12, "relleno a 12 mm (arriba)");
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
    let regions = 0;
    for (let t = 0; t < 20 && !(regions >= 5); t++, await sleep(250)) regions = parseInt(await sketchText(b, "/\\d+ regiones cerradas/"));
    if (!(regions >= 5)) throw new Error(`regiones: ${regions}`);
    // Es un bloque: le quedan solo los 2 grados de libertad de su ancla
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    let s0 = await sk();
    if (s0.texts?.length !== 1 || s0.texts[0].text !== "Hola") throw new Error(`textos: ${JSON.stringify(s0.texts)}`);
    if ((await b.eval(`window.__cadUi.session().report?.dof`)) !== 2) throw new Error(`grados libres: ${await b.eval(`window.__cadUi.session().report?.dof`)}`);
    // Arrastrar el ancla lleva todo el texto
    const t0 = s0.texts[0];
    const P = (s, id) => s.points.find((q) => q.id === id);
    const a0 = P(s0, t0.anchor), q0 = P(s0, t0.points[5]);
    await b.eval(`window.__cadUi.drag(${t0.anchor}, [${a0.x + 3}, ${a0.y + 2}])`);
    await sleep(1500);
    let s1 = await sk();
    near(P(s1, t0.points[5]).x - q0.x, 3, 1e-6, "el texto se movió en x");
    near(P(s1, t0.points[5]).y - q0.y, 2, 1e-6, "el texto se movió en y");
    // Elegir el texto y cambiar lo que dice
    await b.clickText("Elegir");
    await b.eval(`window.__cadUi.setSelection([${t0.anchor}])`);
    await sleep(400);
    await b.eval(`(() => {
      const i = document.querySelector('input[aria-label="Texto elegido"]');
      i.value = "Hola!";
      i.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await b.clickText("Rehacer texto");
    await sleep(2000);
    s1 = await sk();
    if (s1.texts?.length !== 1 || s1.texts[0].text !== "Hola!" || s1.texts[0].anchor !== t0.anchor) throw new Error(`rehacer: ${JSON.stringify(s1.texts?.map((t) => t.text))}`);
    if (!(s1.texts[0].entities.length > t0.entities.length)) throw new Error("no se agregó el signo");
    await b.shot("texto_rehecho");
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

  async "remallar: los modos con la retopología adentro"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // La caja pasa sola a malla al ir a Preparar
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    const tabs = await b.eval(`[...document.querySelectorAll("nav button[aria-label]")].map((t) => t.getAttribute("aria-label"))`);
    if (tabs.includes("Retopología") || !tabs.includes("Remallar")) throw new Error(`pestañas: ${tabs}`);
    await tab(b, "Remallar");
    const modes = await b.eval(`[...document.querySelectorAll('[aria-label="Modo de remallado"] [data-mode]')].map((x) => x.dataset.mode + (x.getAttribute("aria-checked") === "true" ? "*" : ""))`);
    if (modes.join() !== "retopology*,simplify,isotropic,voxel,quads,smooth") throw new Error(`modos: ${modes}`);
    // La retopología está adentro, con su botón
    if (!(await b.eval(`[...document.querySelectorAll("button")].some((x) => x.textContent.trim() === "Retopologizar")`))) throw new Error("sin el panel de retopología");
    await b.shot("remallar");
    // Todos los modos están; otro modo cambia el panel
    if (await b.eval(`document.querySelector('[aria-label="Modo de remallado"]').innerText.includes("pronto")`)) throw new Error("queda un modo marcado como pronto");
    await b.eval(`document.querySelector('[data-mode="quads"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-quads]")`))) throw new Error("sin el panel de A quads");
    if (await b.eval(`[...document.querySelectorAll("button")].some((x) => x.textContent.trim() === "Retopologizar")`)) throw new Error("el panel de retopología sigue a la vista");
  },

  async "remallar: simplificar con vista previa, aplicar y deshacer"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Triángulos del modelo en el backend (cabecera de get_mesh_data: vértices, índices…)
    const triangles = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="simplify"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-simplify]")`))) throw new Error("sin el panel de Simplificar");
    if (await b.eval(`!!document.querySelector("[data-remesh-pending]")`)) throw new Error("Simplificar sigue marcado como pendiente");
    const before = await triangles();
    if (!(before > 500)) throw new Error(`la esfera tiene pocos triángulos: ${before}`);

    // Vista previa: el modelo no cambia; el resultado se ve en alambre encima
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    near(after, before / 4, before * 0.05, "triángulos de la vista previa");
    if ((await triangles()) !== before) throw new Error("la vista previa cambió el modelo");
    if (!(await b.eval(`window.__viewer().hasPreviewOverlay`))) throw new Error("sin el alambre de la vista previa");
    if (!(await b.eval(`!!document.querySelector("[data-preview-view]")`))) throw new Error("sin el selector de la vista previa en el visor");
    await b.eval(`document.querySelector('[data-preview-view] [data-view="result"]').click()`);
    await sleep(300);
    if (await b.eval(`window.__viewer().hasPreviewOverlay`)) throw new Error("con Resultado no debería haber alambre encima");
    await b.shot("simplificar-vista-previa");

    // Aplicar: la malla cambia y la vista previa se va
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await triangles()) === before; t++) await sleep(250);
    const applied = await triangles();
    near(applied, after, 1, "triángulos aplicados");
    for (let t = 0; t < 20 && (await b.eval(`!!document.querySelector("[data-preview-view]")`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector("[data-preview-view]") || window.__viewer().hasPreviewOverlay`)) throw new Error("la vista previa sigue después de aplicar");
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("sin el resumen de lo aplicado");

    // Deshacer: vuelve la malla de antes
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await triangles()) !== before; t++) await sleep(250);
    if ((await triangles()) !== before) throw new Error(`deshacer no devolvió la malla: ${await triangles()} (antes ${before})`);
  },

  async "remallar: suavizar sin cambiar la conectividad, aplicar y deshacer"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Cabecera de get_mesh_data (vértices, índices…) y una huella de las posiciones
    const mesh = async () => {
      const buf = await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer();
      const bytes = new Uint8Array(buf);
      let h = 0;
      for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) | 0;
      return { triangles: new Uint32Array(buf, 0, 4)[1] / 3, hash: h };
    };
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="smooth"]').click()`);
    await sleep(300);
    if (!(await b.eval(`!!document.querySelector("[data-smooth]")`))) throw new Error("sin el panel de Suavizar");
    if (await b.eval(`!!document.querySelector("[data-remesh-pending]")`)) throw new Error("Suavizar sigue marcado como pendiente");
    const before = await mesh();

    // Vista previa: mismos triángulos y el modelo no cambia
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    if (after !== before.triangles) throw new Error(`suavizar cambió los triángulos: ${before.triangles} → ${after}`);
    if ((await mesh()).hash !== before.hash) throw new Error("la vista previa cambió el modelo");
    if (!(await b.eval(`window.__viewer().hasPreviewOverlay`))) throw new Error("sin el alambre de la vista previa");
    await b.shot("suavizar-vista-previa");

    // Cambiar una opción descarta la vista previa
    await b.eval(`[...document.querySelectorAll("[data-smooth] label")].find((x) => x.textContent.includes("Sin deslizar")).click()`);
    for (let t = 0; t < 20 && (await b.eval(`!!document.querySelector("[data-preview-view]")`)); t++) await sleep(250);
    if (await b.eval(`window.__viewer().hasPreviewOverlay`)) throw new Error("cambiar la opción no descartó la vista previa");

    // Aplicar: cambian las posiciones, no los triángulos
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await mesh()).hash === before.hash; t++) await sleep(250);
    const applied = await mesh();
    if (applied.hash === before.hash) throw new Error("aplicar no movió nada");
    if (applied.triangles !== before.triangles) throw new Error(`aplicar cambió los triángulos: ${applied.triangles}`);
    for (let t = 0; t < 20 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("sin el resumen de lo aplicado");

    // Deshacer: vuelve la malla de antes
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await mesh()).hash !== before.hash; t++) await sleep(250);
    if ((await mesh()).hash !== before.hash) throw new Error("deshacer no devolvió la malla");
  },

  async "remallar: isótropo con lado en mm y aristas vivas"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 30, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    const triangles = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector("[data-isotropic-broken]")`)) throw new Error("la caja no debería figurar rota");
    const before = await triangles();
    // Lado de 2 mm: la caja de 30 × 20 × 10 (2200 mm²) da ≈ 1270 triángulos
    await b.eval(`(() => { const i = document.querySelector("[data-isotropic] input[type=number]"); i.value = "2"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
    await sleep(300);
    const estimate = await b.eval(`parseInt(document.querySelector("[data-isotropic-estimate]").textContent.replace(/\\D/g, ""))`);
    near(estimate, 1270, 30, "triángulos estimados");
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 80 && !(await b.eval(`!!document.querySelector("[data-remesh-deviation]")`)); t++) await sleep(250);
    const after = await b.eval(`parseInt(document.querySelector("[data-remesh-after]").textContent.split(" ")[0].replace(/\\D/g, ""))`);
    near(after, 1270, 400, "triángulos de la vista previa");
    // Con las aristas vivas, la caja sigue siendo caja (se aleja menos de una décima de mm)
    const stats = await call("remesh_preview", { params: { mode: "isotropic", edge_mm: 2, sharp_angle: 45, iterations: 5, thin_features: false } });
    if (!(stats.deviation.max_mm < 0.1)) throw new Error(`la caja se deformó: ${JSON.stringify(stats.deviation)}`);
    await b.shot("isotropo-vista-previa");
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 80 && (await triangles()) === before; t++) await sleep(250);
    near(await triangles(), after, 1, "triángulos aplicados");
    // (el campo del lado tiene el foco: Ctrl+Z sería su deshacer de texto)
    await b.eval(`document.activeElement?.blur()`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await triangles()) !== before; t++) await sleep(250);
    if ((await triangles()) !== before) throw new Error("deshacer no devolvió la malla");
  },

  async "remallar: vóxeles une piezas que se cruzan y avisa si es pesado"(b) {
    // STL con dos cubos de 20 mm que se cruzan (dos cáscaras metidas una en otra)
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    mkdirSync(out, { recursive: true });
    const path = `${out}/dos-cubos.stl`;
    writeFileSync(path, twoCubesStl());
    await b.eval(`window.__nextPath = [${JSON.stringify(path)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(3000);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="voxel"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-voxel-grid]")`)); t++) await sleep(250);
    const grid = await b.eval(`document.querySelector("[data-voxel-grid]").textContent`);
    if (!/Vóxel de/.test(grid) || !/MB de memoria/.test(grid)) throw new Error(`sin la estimación de la grilla: ${grid}`);
    // Volumen de las dos cajas por separado: 16000; la unión, 15000
    // (en el puente de depuración tarda ~30 s: grilla, emparejado y normales)
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 360 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    if (!(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`))) throw new Error("no se aplicó");
    const analysis = await call("analyze_print3d");
    if (!analysis.is_closed) throw new Error("la superficie rehecha no es cerrada");
    near(analysis.volume, 15000, 300, "volumen de la unión");
    await b.shot("voxeles-aplicado");
    // Con el detalle al máximo, pregunta antes de correr
    await b.eval(`(() => { const s = [...document.querySelectorAll("[data-voxel] [role=slider]")][0]; s.focus(); for (let k = 0; k < 80; k++) s.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })); })()`);
    for (let t = 0; t < 20 && !(await b.eval(`/640/.test(document.querySelector("[data-voxel] [role=slider]")?.getAttribute("aria-valuenow") ?? "")`)); t++) await sleep(150);
    await sleep(500);
    // (≈ 12 millones de triángulos estimados para esta pieza)
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    // (antes de preguntar calcula la grilla del detalle elegido)
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Vóxeles muy finos")`)); t++) await sleep(250);
    if (!(await b.eval(`document.body.innerText.includes("Vóxeles muy finos")`))) throw new Error("no preguntó antes de una grilla pesada");
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
  },

  async "remallar: triángulos a quads y OBJ con quads"(b) {
    await begin(b);
    // Una caja: cada cara es un rectángulo en dos triángulos
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 30, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    const header = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="quads"]').click()`);
    await sleep(300);
    const before = await header();
    if (before[3] !== 0) throw new Error("la caja ya traía quads");
    await b.eval(`document.querySelector("[data-remesh-preview]").click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-remesh-after]")`)); t++) await sleep(250);
    const after = await b.eval(`document.querySelector("[data-remesh-after]").textContent`);
    if (!/^6 quads/.test(after.trim())) throw new Error(`vista previa: ${after}`);
    await b.shot("quads-vista-previa");
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 40 && (await header())[3] === 0; t++) await sleep(250);
    const applied = await header();
    // Mismos vértices y triángulos; 6 caras de 4 índices para el alambre
    if (applied[0] !== before[0] || applied[1] !== before[1]) throw new Error(`cambió la malla: ${before} → ${applied}`);
    if (applied[3] !== 24) throw new Error(`índices de quads: ${applied[3]}`);
    // OBJ: seis caras de cuatro vértices
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    mkdirSync(out, { recursive: true });
    const path = `${out}/caja-quads.obj`;
    await call("export_model", { config: { format: "obj", path } });
    const faces = readFileSync(path, "utf8").split("\n").filter((l) => l.startsWith("f "));
    if (faces.length !== 6 || !faces.every((l) => l.trim().split(/\s+/).length === 5)) throw new Error(`caras del OBJ: ${faces.join(" | ")}`);
    // Deshacer: sin quads
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await header())[3] !== 0; t++) await sleep(250);
    if ((await header())[3] !== 0) throw new Error("deshacer no quitó los quads");
  },

  async "remallar: isótropo traslada textura y pesos, o los descarta"(b) {
    const model = process.env.E2E_RIGGED ?? `${process.env.HOME}/Descargas/chilesaurus_animado.glb`;
    if (!existsSync(model)) return console.log(`  (sin ${model}: se salta)`);
    await b.eval(`window.__nextPath = [${JSON.stringify(model)}]`);
    await b.clickContains("Abrir / importar");
    await sleep(4000);
    const header = async () => new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4);
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('nav button[aria-label="Remallar"]')`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    await b.eval(`(() => { const i = document.querySelector("[data-isotropic] input[type=number]"); i.value = "12"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); i.blur(); })()`);
    await sleep(300);
    const before = await header();
    if (before[2] !== 1) throw new Error("el modelo no trae UV");
    await b.shot("isotropo-antes");

    // Aplicar pregunta qué trasladar: las dos casillas, marcadas
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-confirm-choices]")`)); t++) await sleep(250);
    const choices = await b.eval(`[...document.querySelectorAll("[data-choice]")].map((c) => c.dataset.choice + (c.querySelector("input")?.checked ? "*" : ""))`);
    if (choices.join() !== "texture*,rig*") throw new Error(`casillas: ${choices}`);
    await b.shot("isotropo-que-trasladar");
    await b.eval(`document.querySelector("[data-confirm]").click()`);
    for (let t = 0; t < 240 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    const baked = await header();
    if (baked[2] !== 1) throw new Error("la malla nueva quedó sin UV: no se horneó la textura");
    if (!(await call("remesh_info")).has_rig) throw new Error("los pesos no pasaron a la malla nueva");
    await sleep(1500);
    await b.shot("isotropo-horneado");

    // Deshacer y aplicar sin trasladar nada
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await header())[1] !== before[1]; t++) await sleep(250);
    await sleep(1000);
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 20 && !(await b.eval(`!!document.querySelector("[data-confirm-choices]")`)); t++) await sleep(250);
    await b.eval(`document.querySelectorAll("[data-choice] label").forEach((x) => x.click())`);
    await sleep(200);
    const unchecked = await b.eval(`[...document.querySelectorAll("[data-choice] input")].every((i) => !i.checked)`);
    if (!unchecked) throw new Error("no se pudieron destildar las casillas");
    await b.eval(`document.querySelector("[data-confirm]").click()`);
    for (let t = 0; t < 120 && (await header())[1] === before[1]; t++) await sleep(250);
    const plain = await header();
    if (plain[2] !== 0) throw new Error("sin trasladar, la malla nueva no debería tener UV");
    if ((await call("remesh_info")).has_rig) throw new Error("sin trasladar, los pesos deberían descartarse");
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

  async "objetos: varios elegidos se exportan juntos"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 20, centered: true, centered_z: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 51, name: "Cilindro", kind: { type: "primitive", shape: { type: "cylinder", radius: 5, height: 10 }, origin: [40, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 52;
    })`);
    await sleep(2500);
    const row = (label) => `[...document.querySelector("[data-outliner]").querySelectorAll("[data-outliner-row]")].find((x) => x.innerText.split("\\n")[0] === ${JSON.stringify(label)})`;
    const selected = () => b.eval(`JSON.stringify(window.__selectedObjects())`);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    // La pieza 2 activa (así las dos tienen malla) y la 1 sumada con Ctrl + clic
    await b.eval(`${row("Pieza 2")}.click()`);
    for (let t = 0; t < 40 && !(await b.eval(`window.__objects().find((o) => o.name === "Pieza 1")?.ui != null`)); t++) await sleep(250);
    await sleep(1000);
    await b.eval(`${row("Pieza 1")}.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }))`);
    await sleep(800);
    const ids = JSON.parse(await selected());
    if (ids.length !== 2) throw new Error(`elegidos: ${JSON.stringify(ids)}`);
    // El panel lista los dos con sus medidas
    for (let t = 0; t < 20 && (await b.eval(`document.querySelectorAll("[data-selected-object]").length`)) !== 2; t++) await sleep(250);
    for (let t = 0; t < 20 && (await b.eval(`[...document.querySelectorAll("[data-selected-object]")].some((e) => e.innerText.includes("…"))`)); t++) await sleep(250);
    const sizes = await b.eval(`[...document.querySelectorAll("[data-selected-object]")].map((e) => e.innerText).join(" | ")`);
    if (!sizes.includes("20 × 20 × 20 mm") || !sizes.includes("10 × 10 × 10 mm")) throw new Error(`medidas: ${sizes}`);
    // El de la Pieza 1 en el visor, con el acento
    if (!(await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].filter((e) => e.className.includes("bg-accent")).length === 2`)))
      throw new Error("el Outliner no marca los dos");
    await b.shot("objetos-elegidos");
    // Exportar juntos a 3MF, acomodados en la cama
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const path = `${out}/juntos.3mf`;
    rmSync(path, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.clickText("Exportar juntos");
    for (let t = 0; t < 40 && !existsSync(path); t++) await sleep(250);
    if (!existsSync(path) || statSync(path).size < 500) throw new Error("no se exportó el 3MF");
    // Un clic sin Ctrl vuelve a uno solo
    await b.eval(`${row("Pieza 1")}.click()`);
    await sleep(1500);
    if (JSON.parse(await selected()).length !== 1) throw new Error(`sin Ctrl: ${await selected()}`);
  },

  async "visor principal: derecho desplaza o abre el menú, Ver todo encuadra"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    const rect = await b.eval(`(() => { const r = window.__viewer().canvas.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()`);
    const [cx, cy] = [rect[0] + rect[2] * 0.3, rect[1] + rect[3] * 0.6];
    const target = () => b.eval(`window.__viewer().controls.target.toArray()`);
    const menuOpen = () => b.eval(`!!document.querySelector('[role=menu]')`);
    // Clic derecho quieto: menú
    await b.click(cx, cy, { button: "right", buttons: 2, wait: 800 });
    if (!(await menuOpen())) throw new Error("el clic derecho no abrió el menú");
    await b.eval(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
    await sleep(400);
    if (await menuOpen()) await b.click(rect[0] + 5, rect[1] + 5, { wait: 400 });
    // Arrastrar con el derecho: desplaza y no abre el menú
    const t0 = await target();
    await b.mouse("mouseMoved", cx, cy, { buttons: 0 });
    await b.mouse("mousePressed", cx, cy, { button: "right", buttons: 2 });
    for (let i = 1; i <= 15; i++) {
      await b.mouse("mouseMoved", cx + i * 6, cy, { button: "right", buttons: 2 });
      await sleep(20);
    }
    await b.mouse("mouseReleased", cx + 90, cy, { button: "right", buttons: 0 });
    await sleep(500);
    const t1 = await target();
    if (!(Math.hypot(...t1.map((v, i) => v - t0[i])) > 1e-4)) throw new Error("el derecho no desplazó");
    if (await menuOpen()) throw new Error("arrastrar con el derecho abrió el menú");
    // Ver todo: el centro de giro vuelve al centro de la caja y entra entera
    await b.eval(`window.__viewer().resetView()`);
    await sleep(500);
    const t2 = await target();
    if (Math.hypot(...t2.map((v, i) => v - t0[i])) > Math.hypot(...t1.map((v, i) => v - t0[i]))) throw new Error(`Ver todo no volvió: ${t0} ${t2}`);
  },

  async "visor: la misma cámara en Diseñar y en Fabricar"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(1000);
    const main = () => b.eval(`JSON.parse(JSON.stringify(window.__viewer().getCameraPose()))`);
    const cadPose = () => b.eval(`JSON.parse(JSON.stringify(window.__cadViewer.cameraPose()))`);
    const close = (a, c, what) => {
      for (const k of ["position", "target"]) {
        const d = Math.hypot(...a[k].map((v, i) => v - c[k][i]));
        const size = Math.hypot(...a.position.map((v, i) => v - a.target[i]));
        if (d > 1e-3 * size) throw new Error(`${what}: ${k} ${JSON.stringify(a[k])} contra ${JSON.stringify(c[k])}`);
      }
    };
    // Mirar desde arriba en Fabricar y pasar a Diseñar: la misma vista
    await b.eval(`window.__viewer().setView("top")`);
    await sleep(800);
    const fromMain = await main();
    await b.clickText("Diseñar");
    await sleep(2000);
    close(await cadPose(), fromMain, "al entrar a Diseñar");
    // De frente en Diseñar y vuelta a Fabricar: la misma vista allá
    await b.eval(`window.__cadViewer.lookFrom([0, -1, 0])`);
    await sleep(800);
    const fromCad = await cadPose();
    await b.clickText("Fabricar");
    await sleep(2500);
    close(await main(), fromCad, "al volver a Fabricar");
  },

  async "objetos: el remallado se rehace si cambia el diseño"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 50, name: "Bola", kind: { type: "primitive", shape: { type: "sphere", radius: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 51;
    })`);
    await sleep(2500);
    // Desvío relativo del largo de las aristas: una malla isótropa lo tiene chico
    const spread = async () => {
      const buf = await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer();
      const [nv, ni] = new Uint32Array(buf, 0, 4);
      const p = new Float32Array(buf, 16, nv * 3);
      const uvs = new Uint32Array(buf, 0, 4)[2] ? nv * 2 : 0;
      const idx = new Uint32Array(buf, 16 + 4 * (nv * 6 + uvs), ni);
      const lengths = [];
      for (let t = 0; t < ni; t += 3)
        for (let k = 0; k < 3; k++) {
          const [a, c] = [idx[t + k], idx[t + ((k + 1) % 3)]];
          lengths.push(Math.hypot(p[3 * a] - p[3 * c], p[3 * a + 1] - p[3 * c + 1], p[3 * a + 2] - p[3 * c + 2]));
        }
      const mean = lengths.reduce((x, y) => x + y, 0) / lengths.length;
      return Math.sqrt(lengths.reduce((x, y) => x + (y - mean) ** 2, 0) / lengths.length) / mean;
    };
    await b.clickText("Preparar");
    for (let t = 0; t < 40 && !(await b.eval(`!!window.__objects?.()[0]?.source.hash`)); t++) await sleep(250);
    await tab(b, "Remallar");
    await b.eval(`document.querySelector('[data-mode="isotropic"]').click()`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector("[data-isotropic-estimate]")`)); t++) await sleep(250);
    const raw = await spread();
    await b.eval(`document.querySelector("[data-remesh-apply]").click()`);
    for (let t = 0; t < 120 && !(await b.eval(`document.querySelector("[data-remesh-summary]")?.innerText.includes("Aplicado")`)); t++) await sleep(250);
    const even = await spread();
    if (!(even < 0.25 && even < raw)) throw new Error(`no quedó isótropa: desvío ${even} (antes ${raw})`);
    const extent0 = (await call("remesh_info")).extent_mm;
    near(extent0, 40, 1, "tamaño");

    // El diseño cambia: la bola el doble de grande
    const hash = await b.eval(`window.__objects()[0].source.hash`);
    await b.clickText("Diseñar");
    await sleep(800);
    await b.eval(`window.__cadStore.commit((d) => { d.features.find((f) => f.id === 50).kind.shape.radius = 40; })`);
    await sleep(2000);
    await b.clickText("Preparar");
    for (let t = 0; t < 60 && (await b.eval(`window.__objects()[0].source.hash`)) === hash; t++) await sleep(250);
    for (let t = 0; t < 120 && !(await b.eval(`/El diseño cambió/.test(document.body.innerText)`)); t++) await sleep(250);
    const status = await b.eval(`document.body.innerText.match(/El diseño cambió[^\\n]*/)?.[0] ?? ""`);
    if (!status.includes("1 modificación")) throw new Error(`aviso: ${status}`);
    // La malla nueva es la bola grande, otra vez isótropa (el lado automático siguió al tamaño)
    near((await call("remesh_info")).extent_mm, 80, 2, "tamaño rehecho");
    const again = await spread();
    if (!(again < 0.25)) throw new Error(`el remallado no se rehízo: desvío ${again}`);
    // La barra del visor cuenta la malla rehecha (no la de antes del cambio)
    const faces = new Uint32Array(await (await fetch(BRIDGE + "get_mesh_data", { method: "POST", body: "{}" })).arrayBuffer(), 0, 4)[1] / 3;
    const shown = await b.eval(`parseInt(document.body.innerText.match(/Caras:\\s*([\\d.]+)/)?.[1].replace(/\\./g, "") ?? "0")`);
    if (shown !== faces) throw new Error(`la barra dice ${shown} caras, la malla tiene ${faces}`);
    const got = await b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    if (!got.some((r) => r.startsWith("Remallado isótropo (automático)"))) throw new Error(`árbol del objeto: ${JSON.stringify(got)}`);
    await b.shot("objetos-remallado-rehecho");
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
  const clickContains = b.clickContains.bind(b);
  b.clickContains = async (text) => {
    try {
      return await clickContains(text);
    } catch (e) {
      for (const name of ["Diseño", "Pieza", "Inspección", "Desde el escaneo"]) {
        try {
          await tab(b, name);
          return await clickContains(text);
        } catch {}
      }
      throw e;
    }
  };
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
