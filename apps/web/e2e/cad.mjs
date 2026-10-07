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
  await sleep(800);
  await b.clickText("Nuevo diseño");
  await sleep(800);
}

const evaluate = () => call("cad_evaluate");

/** Sketch nuevo en la planta: botón "Sketch" y clic en el plano base */
async function sketchOn(b) {
  await b.clickText("Sketch");
  await sleep(400);
  // Un punto de la planta delante-derecha: el rayo no cruza los otros planos
  // (la cámara mira desde +X, +Z del visor) ni la caja (que está en y ≥ 0)
  const s = await b.eval(`window.__cadViewer.planeSize`);
  const [x, y] = await b.eval(`window.__cadViewer.screenOf([${0.4 * s}, ${-0.4 * s}, 0])`);
  await b.click(x, y, { wait: 1500 });
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
    for (const p of [[0, -10, 20], [10, 0, 20]]) {
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
    const A = await at([0, -10, 20]);
    const B = await at([10, 0, 20]);
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
    // Otra caja más arriba (separada) y un redondeo con una arista de cada una
    const doc = await call("cad_get_document");
    const box = doc.features[0];
    const edge = (z) => ({ point: [0, -10, z], direction: [1, 0, 0] });
    doc.features.push(
      { ...structuredClone(box), id: box.id + 1, name: "Caja arriba", kind: { ...structuredClone(box.kind), origin: [0, 0, 30] } },
      { id: box.id + 2, name: "Redondeo 1", suppressed: false, kind: { type: "fillet", edges: [edge(20), edge(50)], radius: 1 } },
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
    await b.click(...(await at([10, 0, 20])), { wait: 1500 });
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
    const hole = { type: "primitive", shape: { type: "cylinder", radius: 3, height: 40 }, origin: [0, 0, -5], z: [0, 0, 1], x: [1, 0, 0], op: "cut" };
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
    const rows = () => b.eval(`[...document.querySelectorAll("[data-feature-row]")].map((r) => { const b = r.getBoundingClientRect(); return [b.x + b.width / 2, b.top, b.bottom]; })`);
    /** Punto del hueco k del árbol (0 = antes de la primera fila) */
    const gap = async (k) => {
      const r = await rows();
      return k < r.length ? [r[k][0], r[k][1] + 2] : [r[r.length - 1][0], r[r.length - 1][2] + 6];
    };
    const bar = () => b.eval(`(() => { const r = document.querySelector("[data-rollback-bar]").getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
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
    // Arrastrar "Caja arriba" al principio: ahora el agujero (z −5..35) también
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
    const [x, y] = await b.eval(`window.__cadViewer.screenOf([3, 3, 20])`);
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
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([0, 0, 20])`)), { wait: 2500 });
    await b.clickText("Círculo");
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([0, 0, 20])`)));
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([5, 0, 20])`)), { wait: 800 });
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
    near(r.bbox_min[2], 0, 1e-6, "base en el origen");
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
    let [x, y] = await b.eval(`window.__cadViewer.screenOf([0, 0, 20])`);
    await b.click(x, y, { wait: 2500 });
    await b.clickText("Círculo");
    [x, y] = await b.eval(`window.__cadViewer.screenOf([0, 0, 20])`);
    const [x2, y2] = await b.eval(`window.__cadViewer.screenOf([5, 0, 20])`);
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
    let [x, y] = await b.eval(`window.__cadViewer.screenOf([-3, -3, 20])`);
    await b.click(x, y, { wait: 2500 });
    await b.clickText("Línea");
    const glyph = () => b.eval(`document.querySelector("[data-snap]")?.dataset.snap`);
    // Cerca de la esquina de la cara: vértice del sólido
    [x, y] = await b.eval(`window.__cadViewer.screenOf([10, 10, 20])`);
    await b.click(x - 3, y + 2);
    if ((await glyph()) !== "solid_vertex") throw new Error(`glifo en la esquina: ${await glyph()}`);
    // Cerca del medio de la arista de adelante
    [x, y] = await b.eval(`window.__cadViewer.screenOf([0, -10, 20])`);
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
    await b.click(...(await at([0, 0, 20])), { wait: 600 });
    await b.clickText("Sketch");
    await sleep(1500);
    const planes = (await evaluate()).sketches.map((v) => v.plane.origin[2]);
    if (!planes.some((z) => Math.abs(z - 20) < 1e-6)) throw new Error(`plano del sketch: ${planes}`);
    await b.clickText("Descartar");
    await sleep(800);
    // Arista superior frontal elegida → redondeo directo
    await b.key("Escape", "Escape", 27);
    const faces = (await body()).faces;
    await b.click(...(await at([0, -10, 20])), { wait: 600 });
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
};

const filter = process.argv[2];
let failed = 0;
for (const [name, run] of Object.entries(scenarios)) {
  if (filter && !name.includes(filter)) continue;
  await call("cad_close");
  const b = await launch(URL);
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
    b.close();
  }
}
process.exit(failed ? 1 : 0);
