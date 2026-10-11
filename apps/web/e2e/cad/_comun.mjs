// Lo que comparten los escenarios de e2e/cad/: el puente con el backend
// (E2E_BRIDGE_PORT, uno por trabajador), modelos de prueba y atajos de la interfaz.

import { writeFileSync } from "node:fs";
import { sleep } from "../cdp.mjs";

/** Carpeta de capturas y archivos de las pruebas */
export const OUT = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
export const PORT = process.env.E2E_BRIDGE_PORT ?? "8766";
export const BRIDGE = `http://127.0.0.1:${PORT}/invoke/`;
export const call = (cmd, args = {}) => fetch(BRIDGE + cmd, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());
export const SPECULUM = process.env.E2E_STL ?? `${process.env.HOME}/Descargas/macho_especulo.stl`;

export function near(actual, expected, tol, what) {
  if (!(Math.abs(actual - expected) <= tol)) throw new Error(`${what}: ${actual} (esperado ${expected} ± ${tol})`);
}

/** Esfera de radio `R` con centro a `cz` mm de altura, como STL en la carpeta del e2e */
export function sphereStl(R, cz, name) {
  const [rings, segs] = [48, 96];
  const at = (i, j) => {
    const th = (Math.PI * i) / rings;
    const ph = (2 * Math.PI * j) / segs;
    return [R * Math.sin(th) * Math.cos(ph), R * Math.sin(th) * Math.sin(ph), cz + R * Math.cos(th)];
  };
  let stl = "solid esfera\n";
  const facet = (a, c, d) => (stl += `facet normal 0 0 0\nouter loop\nvertex ${a.join(" ")}\nvertex ${c.join(" ")}\nvertex ${d.join(" ")}\nendloop\nendfacet\n`);
  for (let i = 0; i < rings; i++)
    for (let j = 0; j < segs; j++) {
      const [a, c, d, e] = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
      if (i > 0) facet(a, c, e);
      if (i < rings - 1) facet(e, c, d);
    }
  stl += "endsolid esfera\n";
  const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/${name}.stl`;
  writeFileSync(path, stl);
  return path;
}

/** STL de texto con dos cubos de 20 mm, el segundo corrido 10 mm en cada eje */
export function twoCubesStl() {
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

/** Importa un modelo con Ctrl+I (reintenta: recién montada la app puede no escuchar todavía) */
export async function importModel(b, path, name = path.split("/").pop()) {
  for (let attempt = 0; attempt < 3; attempt++) {
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", ctrlKey: true }))`);
    for (let t = 0; t < 40; t++) {
      if (await b.eval(`document.body.innerText.includes(${JSON.stringify(name)}) && document.body.innerText.includes("Modelo cargado")`)) return;
      await sleep(250);
    }
  }
  throw new Error(`no se importó ${name}`);
}

export async function begin(b) {
  await b.clickText("Diseñar");
  // El diseño vacío se crea solo al entrar
  for (let t = 0; t < 40 && !(await b.eval(`!!window.__cadStore.doc()`)); t++) await sleep(250);
  await sleep(500);
}

export const evaluate = () => call("cad_evaluate");

/** Sketch nuevo en la planta: botón "Sketch" y clic en el plano base */
export async function sketchOn(b) {
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
export const body = async () => (await evaluate()).body;
/** Acepta el diálogo de la operación recién creada (✓) */
export async function accept(b, wait = 1500) {
  await b.eval(`document.querySelector('[aria-label="Aceptar"]')?.click()`);
  await sleep(wait);
}
/** Abre una pestaña del panel de Diseñar (Diseño, Pieza, Inspección, Desde el escaneo) */
export async function tab(b, name) {
  if (!(await b.eval(`(() => { const t = document.querySelector('nav button[aria-label=${JSON.stringify(name)}]'); t?.click(); return !!t; })()`)))
    throw new Error("no encontré la pestaña " + name);
  await sleep(400);
}
/** Color de una fila del árbol: "error" (rojo), "empty" (azul) o null */
export const tone = (b, name) =>
  b.eval(`[...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === ${JSON.stringify(name)})?.dataset.tone ?? null`);
/** Clic en una fila del árbol de operaciones por su nombre */
export async function clickRow(b, name) {
  const r = await b.eval(`(() => {
    const e = [...document.querySelectorAll("span")].find((x) => x.textContent === ${JSON.stringify(name)} && x.offsetParent !== null);
    if (!e) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2];
  })()`);
  if (!r) throw new Error("no encontré la operación " + name);
  await b.click(r[0], r[1], { wait: 1200 });
}
export const sketchText = (b, re) => b.eval(`(document.body.innerText.match(${re}) ?? [""])[0]`);
export const setInput = (b, labelStart, value) =>
  b.eval(`(() => {
    const i = [...document.querySelectorAll("label")].find((l) => l.textContent.startsWith(${JSON.stringify(labelStart)}))?.querySelector("input");
    if (!i) return false;
    i.value = ${JSON.stringify(String(value))};
    i.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  })()`);
/** Elige `option` en el desplegable de la fila `row` de un diálogo (Kobalte: clic en el botón y en la opción) */
export async function chooseOption(b, row, option) {
  const r = await b.eval(`(() => {
    const s = [...document.querySelectorAll("span")].find((x) => x.textContent.trim() === ${JSON.stringify(row)} && x.offsetParent !== null);
    const t = s?.parentElement?.querySelector("button");
    if (!t) return null; t.scrollIntoView({ block: "center" }); const q = t.getBoundingClientRect(); return [q.x + q.width / 2, q.y + q.height / 2];
  })()`);
  if (!r) throw new Error("no encontré el desplegable " + row);
  await b.click(r[0], r[1], { wait: 500 });
  await b.clickText(option);
  await sleep(1500);
}
/** Escribe en el campo `index` (0, 1, 2…) de los que encuentra `selector` */
export const setInputIn = (b, selector, index, value) =>
  b.eval(`(() => {
    const i = document.querySelectorAll(${JSON.stringify(selector)})[${index}];
    if (!i) return false;
    i.value = ${JSON.stringify(String(value))};
    i.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  })()`);
/** Espera a que la última operación del documento sea de tipo `type` */
export async function lastIs(b, type) {
  for (let t = 0; t < 30; t++) {
    if ((await b.eval(`window.__cadStore.doc().features.at(-1)?.kind.type`)) === type) return;
    await sleep(250);
  }
  throw new Error("no se creó la operación " + type);
}
export const lastKind = (b) => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1).kind))`);
/** Índice de la primera cara del cuerpo que cumple `test` (con lo que da `cad_face_info`: surface, point, normal) */
export async function faceWhere(test) {
  const n = (await call("cad_evaluate")).body?.faces ?? 0;
  for (let i = 0; i < n; i++) if (test(await call("cad_face_info", { face: i }))) return i;
  throw new Error("no encontré la cara");
}
/** Índice de la arista del cuerpo cuyo punto de referencia está más cerca de `p` */
export async function edgeNear(p) {
  const n = (await call("cad_evaluate")).body?.edges ?? 0;
  let best = -1;
  let dist = Infinity;
  for (let i = 0; i < n; i++) {
    const r = await call("cad_edge_ref", { edge: i });
    const d = Math.hypot(...r.point.map((c, k) => c - p[k]));
    if (d < dist) [best, dist] = [i, d];
  }
  if (best < 0) throw new Error("no hay aristas");
  return best;
}
