// Pruebas de punta a punta del espacio Diseñar con el backend CAD real, sin
// Tauri: el arnés (cad-harness.html) manda los comandos al puente HTTP.
//
//   cargo run -p pinocchio-app --example cad_http        # backend en :8766 (E2E_BRIDGE_PORT: otro)
//   (cd apps/web && npx vite --port 5173)                 # frontend (E2E_WEB_PORT: otro)
//   node apps/web/e2e/cad.mjs [filtro]                    # Chromium headless
//
// Capturas en $E2E_OUT (por defecto /tmp/pinocchio-e2e). Cada escenario
// verifica volúmenes contra el valor teórico.

import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { launch, sleep } from "./cdp.mjs";

const PORT = process.env.E2E_BRIDGE_PORT ?? "8766";
const URL = `http://localhost:${process.env.E2E_WEB_PORT ?? "5173"}/e2e/cad-harness.html?bridge=${PORT}`;
const BRIDGE = `http://127.0.0.1:${PORT}/invoke/`;
const call = (cmd, args = {}) => fetch(BRIDGE + cmd, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());
const SPECULUM = process.env.E2E_STL ?? `${process.env.HOME}/Descargas/macho_especulo.stl`;

function near(actual, expected, tol, what) {
  if (!(Math.abs(actual - expected) <= tol)) throw new Error(`${what}: ${actual} (esperado ${expected} ± ${tol})`);
}

/** STL de texto con dos cubos de 20 mm, el segundo corrido 10 mm en cada eje */
/** Esfera de radio `R` con centro a `cz` mm de altura, como STL en la carpeta del e2e */
function sphereStl(R, cz, name) {
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

/** Importa un modelo con Ctrl+I (reintenta: recién montada la app puede no escuchar todavía) */
async function importModel(b, path, name = path.split("/").pop()) {
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
/** Color de una fila del árbol: "error" (rojo), "empty" (azul) o null */
const tone = (b, name) =>
  b.eval(`[...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === ${JSON.stringify(name)})?.dataset.tone ?? null`);
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
/** Elige `option` en el desplegable de la fila `row` de un diálogo (Kobalte: clic en el botón y en la opción) */
async function chooseOption(b, row, option) {
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
const setInputIn = (b, selector, index, value) =>
  b.eval(`(() => {
    const i = document.querySelectorAll(${JSON.stringify(selector)})[${index}];
    if (!i) return false;
    i.value = ${JSON.stringify(String(value))};
    i.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  })()`);
/** Espera a que la última operación del documento sea de tipo `type` */
async function lastIs(b, type) {
  for (let t = 0; t < 30; t++) {
    if ((await b.eval(`window.__cadStore.doc().features.at(-1)?.kind.type`)) === type) return;
    await sleep(250);
  }
  throw new Error("no se creó la operación " + type);
}
const lastKind = (b) => b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.at(-1).kind))`);
/** Índice de la primera cara del cuerpo que cumple `test` (con lo que da `cad_face_info`: surface, point, normal) */
async function faceWhere(test) {
  const n = (await call("cad_evaluate")).body?.faces ?? 0;
  for (let i = 0; i < n; i++) if (test(await call("cad_face_info", { face: i }))) return i;
  throw new Error("no encontré la cara");
}
/** Índice de la arista del cuerpo cuyo punto de referencia está más cerca de `p` */
async function edgeNear(p) {
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

  async "outliner: lo elegido en el visor se marca en su fila"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await sketchOn(b);
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    // Una línea suelta (sketch abierto: sin regiones) lejos de la caja
    await key("l");
    await b.click(...(await at([12, -15, 0])), { wait: 400 });
    await b.click(...(await at([18, -15, 0])), { wait: 400 });
    await key("Escape");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const name = (await call("cad_get_document")).features.find((f) => f.kind.type === "sketch").name;
    // Clic en la curva: se elige el sketch entero y su fila queda marcada
    await b.click(...(await at([15, -15, 0])), { wait: 600 });
    const picks = await b.eval(`JSON.stringify(window.__cadUi.picks())`);
    if (!picks.includes('"kind":"sketch"')) {
      const dbg = await b.eval(`(() => { const [x, y] = window.__cadViewer.screenOf([15, -15, 0]); const h = window.__cadViewer.pick(x, y, { regions: true, faces: true, planes: true }); const vs = window.__cadStore.result().sketches.map((v) => ({ id: v.id, n: v.sketch.entities.length, pts: v.sketch.points.map((p) => [p.x, p.y]) })); return JSON.stringify({ x, y, h: h && h.kind, vs, filter: window.__cadUi.pickFilter() }); })()`);
      throw new Error(`clic en la curva: ${picks} ${dbg}`);
    }
    if (!(await b.eval(`[...document.querySelectorAll("[data-feature-row]")].some((r) => r.textContent.includes(${JSON.stringify(name)}) && r.className.includes("bg-accent/20"))`)))
      throw new Error("la fila del sketch no se marcó");
    // Clic en una cara de la caja: se marca la fila de su pieza
    const face = await b.eval(`(() => { for (let y = 200; y < 800; y += 15) for (let x = 300; x < 1050; x += 15) { const h = window.__cadViewer.pick(x, y, { faces: true }); if (h && h.kind === "face") return [x, y]; } })()`);
    if (!face) throw new Error("no encontré una cara");
    await b.click(...face, { wait: 600 });
    const part = await b.eval(`(() => { const e = [...document.querySelectorAll("span")].find((x) => x.textContent.trim() === "Pieza 1" && x.offsetParent); const row = e?.closest("[class*='rounded']"); return row ? row.className : null; })()`);
    if (!part?.includes("bg-accent/20")) throw new Error(`la fila de la pieza no se marcó: ${part}`);
  },

  async "caja: cambiar sus medidas en el diálogo cambia el sólido"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    // Con el diálogo abierto (borrador): el ancho a 50 escribiendo en el campo
    const setField = (label, value) =>
      b.eval(`(() => {
        const l = [...document.querySelectorAll("label")].find((x) => x.querySelector("span")?.textContent.trim() === ${JSON.stringify(label)});
        const i = l?.querySelector("input");
        if (!i) return "sin campo";
        i.focus();
        i.value = ${JSON.stringify(String(value))};
        i.dispatchEvent(new Event("change", { bubbles: true }));
        return i.value;
      })()`);
    const probe = async (what) => {
      const st = await b.eval(`(() => {
        const draft = window.__cadStore.draft();
        const f = (draft?.doc ?? window.__cadStore.doc()).features.find((x) => x.kind.type === "primitive");
        const r = window.__cadStore.result();
        return { shape: f?.kind.shape, bbox: r?.body ? [r.body.bbox_min, r.body.bbox_max] : null, draft: !!draft, field: [...document.querySelectorAll("label")].find((x) => x.querySelector("span")?.textContent.trim() === "Ancho (X)")?.querySelector("input")?.value };
      })()`);
      console.log(`    ${what}: ${JSON.stringify(st)}`);
      return st;
    };
    await probe("al crear");
    console.log("    campo:", await setField("Ancho (X)", 50));
    await sleep(2500);
    const p1 = await probe("con el diálogo, ancho 50");
    if (p1.shape.dx !== 50) throw new Error("el borrador no tomó el ancho");
    if (p1.field !== "50") throw new Error(`el campo no muestra lo escrito: ${p1.field}`);
    near(p1.bbox[1][0] - p1.bbox[0][0], 50, 1e-6, "ancho de la vista previa");
    // La rueda baja de a uno desde lo que se ve (antes bajaba siempre desde el valor viejo)
    for (let i = 0; i < 3; i++) {
      await b.eval(`[...document.querySelectorAll("label")].find((x) => x.querySelector("span")?.textContent.trim() === "Ancho (X)").querySelector("input").dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true }))`);
      await sleep(900);
    }
    const p3 = await probe("tres pasos de rueda hacia abajo");
    if (p3.shape.dx !== 47 || p3.field !== "47") throw new Error(`rueda: ${p3.shape.dx} / ${p3.field}`);
    await accept(b);
    await sleep(1500);
    await probe("aceptada");
    const r = await evaluate();
    near(r.body.bbox_max[0] - r.body.bbox_min[0], 47, 1e-6, "ancho del sólido");
  },

  async "sketch: barra de restricciones arriba; la cota de ángulo mide lo que hay y dibuja su arco"(b) {
    await begin(b);
    await sketchOn(b);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    // Dos líneas desde el mismo punto: una horizontal y otra a 45°
    await key("l");
    await b.click(...(await at([12, 12])), { wait: 300 });
    await b.click(...(await at([42, 12])), { wait: 300 });
    await key("Escape");
    await b.click(...(await at([12, 12])), { wait: 300 });
    await b.click(...(await at([33, 33])), { wait: 300 });
    await key("Escape");
    await key("Escape");
    let s = await sk();
    const lines = s.entities.filter((e) => e.geometry.type === "line");
    if (lines.length !== 2) throw new Error(`líneas: ${lines.length}`);
    const before = JSON.stringify(s.points);
    // Las dos elegidas (Mayús suma) y «Cota»: el ángulo de ahora
    await key("s");
    await b.click(...(await at([27, 12])), { wait: 300 });
    await b.mouse("mouseMoved", ...(await at([22.5, 22.5])), { buttons: 0 });
    const p2 = await at([22.5, 22.5]);
    await b.mouse("mousePressed", p2[0], p2[1], { modifiers: 8 });
    await b.mouse("mouseReleased", p2[0], p2[1], { modifiers: 8, buttons: 0 });
    await sleep(300);
    if ((await b.eval(`window.__cadUi.selection().length`)) !== 2) throw new Error(`elegidas: ${await b.eval(`JSON.stringify(window.__cadUi.selection())`)}`);
    if (!(await b.eval(`document.querySelector('[data-constraint-tool="dim"]').className.includes("ring")`))) throw new Error("la Cota no se ofrece con dos líneas elegidas");
    await b.eval(`document.querySelector('[data-constraint-tool="dim"]').click()`);
    await sleep(1200);
    s = await sk();
    const angle = s.constraints.find((c) => c.type === "angle");
    if (!angle) throw new Error("no se puso la cota de ángulo");
    near(angle.degrees, 45, 0.05, "el ángulo que tienen ahora");
    const moved = JSON.parse(before).some((p) => { const q = s.points.find((x) => x.id === p.id); return !q || Math.hypot(q.x - p.x, q.y - p.y) > 1e-6; });
    if (moved) throw new Error("la cota movió las líneas");
    if (!(await b.eval(`!!document.querySelector("[data-angle-arc]")`))) throw new Error("sin el arco de la cota de ángulo");
    await b.shot("sketch_cota_angulo");
    // El botón ∠ (Ángulo) con la herramienta primero: clic en las dos líneas
    s = await sk();
    const angles = (sk) => sk.constraints.filter((c) => c.type === "angle").length;
    const a0 = angles(s);
    await b.eval(`document.querySelector('[data-constraint-tool="angle"]').click()`);
    await sleep(200);
    await b.click(...(await at([27, 12])), { wait: 500 });
    await b.click(...(await at([22.5, 22.5])), { wait: 900 });
    s = await sk();
    if (angles(s) !== a0 + 1) throw new Error(`ángulo con la herramienta primero: ${JSON.stringify(s.constraints.map((c) => c.type))}`);
    await key("Escape");
    await sleep(200);
    // Herramienta primero: «Horizontal» sin nada elegido espera; cada línea que se elige queda horizontal
    const horizontals = (sk) => sk.constraints.filter((c) => c.type === "horizontal").length;
    const h0 = horizontals(s);
    await b.eval(`document.querySelector('[data-constraint-tool="horizontal"]').click()`);
    await sleep(200);
    if (!(await b.eval(`document.querySelector('[data-constraint-tool="horizontal"]').className.includes("bg-accent")`))) throw new Error("Horizontal no quedó esperando");
    await b.click(...(await at([27, 12])), { wait: 900 });
    s = await sk();
    if (horizontals(s) !== h0 + 1) throw new Error(`horizontales: ${JSON.stringify(s.constraints.map((c) => c.type))}`);
    if (!(await b.eval(`document.querySelector('[data-constraint-tool="horizontal"]').className.includes("bg-accent")`))) throw new Error("debía seguir esperando la próxima");
    await key("Escape");
    await sleep(200);
    if (await b.eval(`document.querySelector('[data-constraint-tool="horizontal"]').className.includes("bg-accent")`)) throw new Error("Esc no la soltó");
  },

  async "sketch edición III: arrastrar una línea y el radio, recortar en rojo, similares, Tab, copias y construcción"(b) {
    await begin(b);
    await sketchOn(b);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    const drag = async (from, to) => {
      await b.mouse("mouseMoved", from[0], from[1], { buttons: 0 });
      await b.mouse("mousePressed", from[0], from[1]);
      for (let i = 1; i <= 6; i++) await b.mouse("mouseMoved", from[0] + ((to[0] - from[0]) * i) / 6, from[1] + ((to[1] - from[1]) * i) / 6);
      await b.mouse("mouseReleased", to[0], to[1], { buttons: 0 });
      await sleep(900);
    };
    // Dos líneas horizontales iguales y un círculo
    await key("l");
    await b.click(...(await at([11, 31])), { wait: 300 });
    await b.click(...(await at([31, 31])), { wait: 300 });
    await key("Escape");
    await b.click(...(await at([11, 41])), { wait: 300 });
    await b.click(...(await at([31, 41])), { wait: 300 });
    await key("Escape");
    await key("Escape");
    await key("c");
    await b.click(...(await at([51, 36])), { wait: 300 });
    await b.click(...(await at([56, 36])), { wait: 500 });
    await key("Escape");
    await key("s");
    let s = await sk();
    const [l1, l2] = s.entities.filter((e) => e.geometry.type === "line");
    const circle = s.entities.find((e) => e.geometry.type === "circle");
    // Arrastrar la primera línea (del medio): se corre entera
    const a0 = pos(s, l1.geometry.start);
    await drag(await at([21, 31]), await at([21, 25]));
    s = await sk();
    const a1 = pos(s, s.entities.find((e) => e.id === l1.id).geometry.start);
    if (!(Math.abs(a1[1] - a0[1]) > 3) || Math.abs(a1[0] - a0[0]) > 1) throw new Error(`la línea no se corrió entera: ${a0} → ${a1}`);
    // Arrastrar el borde del círculo: cambia el radio (sin la cota que se le puso al dibujarlo: si no, manda la cota)
    await b.eval(`window.__cadUi.change((s) => { s.constraints = s.constraints.filter((c) => !((c.type === "radius" || c.type === "diameter") && c.entity === ${circle.id})); })`);
    await sleep(500);
    const r0 = circle.geometry.radius;
    await drag(await at([51 + r0, 36]), await at([51 + r0 + 4, 36]));
    s = await sk();
    const r1 = s.entities.find((e) => e.id === circle.id).geometry.radius;
    if (!(r1 > r0 + 2)) throw new Error(`radio: ${r0} → ${r1}`);
    // Elegir similares: con la segunda línea, la otra del mismo largo
    // (por código: a este zoom la etiqueta de su cota tapa la línea)
    await b.eval(`window.__cadUi.setSelection([${l2.id}])`);
    await sleep(300);
    await b.clickText("Elegir similares");
    await sleep(300);
    const simil = await b.eval(`window.__cadUi.selection().length`);
    if (simil !== 2) throw new Error(`similares: ${simil}`);
    // Copiar 3 veces corrido
    await setInput(b, "Veces", 3);
    await b.eval(`window.__cadUi.setSelection([${l2.id}])`);
    await sleep(200);
    const n0 = (await sk()).entities.length;
    await b.clickText("Copiar 3 veces");
    await sleep(900);
    if ((await sk()).entities.length !== n0 + 3) throw new Error(`copias: ${(await sk()).entities.length - n0}`);
    // Construcción igualada: una línea en construcción y otra no → las dos a construcción
    await b.eval(`window.__cadUi.change((s) => { s.entities.find((e) => e.id === ${l1.id}).construction = true; })`);
    await b.eval(`window.__cadUi.setSelection([${l1.id}, ${l2.id}])`);
    await b.eval(`window.__cadUi.toggleConstruction()`);
    await sleep(600);
    s = await sk();
    if (![l1.id, l2.id].every((id) => s.entities.find((e) => e.id === id).construction)) throw new Error("construcción no se igualó");
    // Recortar: el tramo que se iría, en rojo antes del clic (una línea cruzada por otra vertical)
    await key("l");
    await b.click(...(await at([71, 21])), { wait: 300 });
    await b.click(...(await at([71, 51])), { wait: 300 });
    await key("Escape");
    await b.click(...(await at([61, 36.5])), { wait: 300 });
    await b.click(...(await at([81, 36.5])), { wait: 300 });
    await key("Escape");
    await key("Escape");
    await key("t");
    const over = await at([77, 36.5]);
    await b.mouse("mouseMoved", over[0], over[1], { buttons: 0 });
    await sleep(400);
    if (!(await b.eval(`!!window.__cadViewer`))) throw new Error("sin visor");
    await b.shot("sketch_recortar_rojo");
    // Tab: dos encimadas (la línea vertical y una copia encima)
    await key("s");
    await b.eval(`window.__cadUi.change((s) => { const v = s.entities.find((e) => e.geometry.type === "line" && Math.abs(s.points.find((p) => p.id === e.geometry.start).x - s.points.find((p) => p.id === e.geometry.end).x) < 1e-6 && s.points.find((p) => p.id === e.geometry.start).x > 60); const a = s.points.find((p) => p.id === v.geometry.start), c = s.points.find((p) => p.id === v.geometry.end); const id = Math.max(...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)) + 1; s.points.push({ id, x: a.x, y: a.y }, { id: id + 1, x: c.x, y: c.y }); s.entities.push({ id: id + 2, geometry: { type: "line", start: id, end: id + 1 }, construction: false }); s.next_id = id + 3; })`);
    await sleep(600);
    await b.click(...(await at([71, 28])), { wait: 400 });
    const first = await b.eval(`window.__cadUi.selection()[0]`);
    await b.key("Tab", "Tab", 9);
    await sleep(300);
    const second = await b.eval(`window.__cadUi.selection()[0]`);
    if (first === second || second === undefined) throw new Error(`Tab no pasó a la encimada: ${first} → ${second}`);
  },

  async "sketch edición IV: recortar splines y elipses, límite elegido con Ctrl+clic, extender una spline y a puntos de paso"(b) {
    await begin(b);
    await sketchOn(b);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    // Lo de la prueba, armado por código (puntos y entidades con ids nuevos)
    const add = (shapes) =>
      b.eval(`window.__cadUi.change((s) => {
        let id = Math.max(0, ...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)) + 1;
        const pt = (p) => (s.points.push({ id, x: p[0], y: p[1] }), id++);
        for (const g of ${JSON.stringify(shapes)}) {
          const geometry = g.type === "line" ? { type: "line", start: pt(g.a), end: pt(g.b) }
            : g.type === "spline" ? { type: "spline", points: g.pts.map(pt), closed: false }
            : { type: "ellipse", center: pt(g.c), major: pt(g.major), minor: pt(g.minor) };
          s.entities.push({ id: id++, geometry, construction: false });
        }
        s.next_id = id;
      })`);
    await add([
      { type: "spline", pts: [[10, 45], [18, 52], [26, 45], [34, 52], [42, 45]] },
      { type: "line", a: [21, 38], b: [21, 56] },
      { type: "line", a: [31, 38], b: [31, 56] },
      { type: "ellipse", c: [62, 46], major: [72, 46], minor: [62, 51] },
      { type: "line", a: [50, 46], b: [76, 46] },
      { type: "line", a: [10, 25], b: [50, 25] },
      { type: "line", a: [20, 21], b: [20, 29] },
      { type: "line", a: [30, 21], b: [30, 29] },
      { type: "line", a: [40, 21], b: [40, 29] },
    ]);
    await sleep(800);
    // Recortar la spline entre las dos verticales: quedan dos tramos por polos (misma curva)
    await key("t");
    await b.click(...(await at([26, 45])), { wait: 900 });
    let s = await sk();
    const pieces = s.entities.filter((e) => e.geometry.type === "bspline");
    if (pieces.length !== 2) throw new Error(`spline recortada: ${s.entities.map((e) => e.geometry.type)}`);
    const ends = pieces.map((e) => [pos(s, e.geometry.poles[0])[0], pos(s, e.geometry.poles.at(-1))[0]].map((x) => Math.round(x)));
    if (JSON.stringify(ends.sort((a, b) => a[0] - b[0])) !== "[[10,21],[31,42]]") throw new Error(`tramos: ${JSON.stringify(ends)}`);
    // La elipse: lo de arriba se va, queda un arco de elipse
    await b.click(...(await at([62, 51])), { wait: 900 });
    s = await sk();
    const arc = s.entities.find((e) => e.geometry.type === "ellipse_arc");
    if (!arc || s.entities.some((e) => e.geometry.type === "ellipse")) throw new Error(`elipse: ${s.entities.map((e) => e.geometry.type)}`);
    if (!(pos(s, arc.geometry.start)[1] < 46.5 && pos(s, arc.geometry.end)[1] < 46.5)) throw new Error("las puntas del arco no quedaron en la línea");
    // Límite elegido: Ctrl+clic en la vertical de x=20; el clic en x=35 quita de 20 al final (las otras no cortan)
    await b.click(...(await at([20, 27.5])), { wait: 400, modifiers: 2 });
    const msg = await b.eval(`document.body.innerText.includes("Solo lo elegido corta")`);
    if (!msg) throw new Error("no avisó que solo lo elegido corta");
    await b.click(...(await at([35, 25])), { wait: 900 });
    s = await sk();
    const horiz = s.entities.filter((e) => e.geometry.type === "line" && Math.abs(pos(s, e.geometry.start)[1] - 25) < 1e-3 && Math.abs(pos(s, e.geometry.end)[1] - 25) < 1e-3);
    const xs = horiz.map((e) => [pos(s, e.geometry.start)[0], pos(s, e.geometry.end)[0]].sort((a, b) => a - b).map(Math.round));
    if (JSON.stringify(xs) !== "[[10,20]]") throw new Error(`con el límite elegido quedó ${JSON.stringify(xs)}`);
    await b.shot("sketch_recortar_curvas");
    // Esc suelta los límites (sigue en recortar)
    await key("Escape");
    await sleep(300);
    if ((await b.eval(`window.__cadUi.selection().length`)) !== 0 || (await b.eval(`window.__cadUi.tool()`)) !== "trim") throw new Error("Esc no soltó los límites");
    // Extender una spline por puntos hasta una línea: suma un punto de paso sobre ella
    await add([
      { type: "spline", pts: [[55, 25], [62, 30], [69, 25]] },
      { type: "line", a: [78, 0], b: [78, 40] },
    ]);
    await sleep(800);
    await key("e");
    await b.click(...(await at([68.6, 25.4])), { wait: 900 });
    s = await sk();
    const ext = s.entities.find((e) => e.geometry.type === "spline");
    if (ext?.geometry.points.length !== 4 || Math.abs(pos(s, ext.geometry.points[3])[0] - 78) > 0.01) throw new Error(`extender spline: ${JSON.stringify(ext?.geometry)}`);
    // A puntos de paso: un tramo por polos vuelve a ser spline por puntos
    await key("s");
    await b.eval(`window.__cadUi.setSelection([${pieces[0].id}])`);
    await sleep(300);
    await b.clickText("A puntos de paso");
    await sleep(900);
    s = await sk();
    if (s.entities.find((e) => e.id === pieces[0].id)?.geometry.type !== "spline") throw new Error("no pasó a puntos de paso");
    if (await b.eval(`!!document.querySelector("[data-conflicts]")`)) throw new Error("quedó un conflicto");
  },

  async "sketch edición V: patrón que se edita y omite copias, encimadas que se juntan, lista de problemas y cruces"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    const add = (shapes) =>
      b.eval(`(() => { const made = []; window.__cadUi.change((s) => {
        let id = Math.max(0, ...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)) + 1;
        const pt = (p) => (s.points.push({ id, x: p[0], y: p[1] }), id++);
        for (const g of ${JSON.stringify(shapes)}) {
          const geometry = g.type === "line" ? { type: "line", start: pt(g.a), end: pt(g.b) } : { type: "circle", center: pt(g.c), radius: g.r };
          made.push(id);
          s.entities.push({ id: id++, geometry, construction: false });
        }
        s.next_id = id;
      }); return made; })()`);
    const circles = (s) => s.entities.filter((e) => e.geometry.type === "circle");
    // Patrón lineal de un círculo: 3 en total, queda anotado
    const [c] = await add([{ type: "circle", c: [10, 30], r: 2 }]);
    await sleep(600);
    await b.eval(`window.__cadUi.setSelection([${c}])`);
    await sleep(300);
    await setInput(b, "Cantidad", 3);
    await b.clickText("Repetir en patrón");
    await sleep(900);
    let s = await sk();
    if (circles(s).length !== 3 || s.patterns?.length !== 1) throw new Error(`patrón: ${circles(s).length} círculos, ${s.patterns?.length} anotados`);
    // Elegir una copia: aparece el patrón; editarlo a 5
    await b.eval(`window.__cadUi.setSelection([${s.patterns[0].copies[0][0]}])`);
    await sleep(400);
    if (!(await b.eval(`!!document.querySelector("[data-pattern-box]")`))) throw new Error("no aparece el patrón al elegir una copia");
    await b.clickText("Editar el patrón");
    await sleep(300);
    await setInput(b, "Cantidad", 5);
    await sleep(200);
    await b.clickText("Rehacer el patrón");
    await sleep(900);
    s = await sk();
    if (circles(s).length !== 5 || s.patterns[0].copies.length !== 4) throw new Error(`rehacer: ${circles(s).length} círculos`);
    const xs = circles(s).map((e) => pos(s, e.geometry.center)[0]).sort((a, b) => a - b);
    const step = xs[1] - xs[0];
    if (!(step > 1) || xs.some((x, i) => Math.abs(x - xs[0] - i * step) > 1e-3)) throw new Error(`separación: ${xs}`);
    // Omitir la segunda copia: queda de construcción
    const skipId = s.patterns[0].copies[1][0];
    await b.eval(`window.__cadUi.setSelection([${skipId}])`);
    await sleep(400);
    await b.clickText("Omitir esta copia");
    await sleep(800);
    s = await sk();
    if (!s.entities.find((e) => e.id === skipId)?.construction || JSON.stringify(s.patterns[0].skip) !== "[1]") throw new Error("no se omitió la copia");
    // Dos líneas que se pisan en parte: «Eliminar duplicados» las junta en una
    await add([
      { type: "line", a: [10, 10], b: [30, 10] },
      { type: "line", a: [20, 10], b: [40, 10] },
    ]);
    await sleep(600);
    await b.clickText("Eliminar duplicados");
    await sleep(800);
    s = await sk();
    const flat = s.entities.filter((e) => e.geometry.type === "line" && pos(s, e.geometry.start)[1] === 10 && pos(s, e.geometry.end)[1] === 10);
    if (flat.length !== 1 || JSON.stringify([pos(s, flat[0].geometry.start)[0], pos(s, flat[0].geometry.end)[0]].sort((a, b) => a - b)) !== "[10,40]") throw new Error(`encimadas: ${JSON.stringify(flat.map((e) => e.geometry))}`);
    // Una cruz sin punto común: la lista la muestra y el clic elige las dos
    await add([
      { type: "line", a: [50, 0], b: [70, 20] },
      { type: "line", a: [50, 20], b: [70, 0] },
    ]);
    await sleep(700);
    await b.eval(`document.querySelector("[data-problem-list] summary").click()`);
    await sleep(300);
    const n = await b.eval(`document.querySelectorAll("[data-problem]").length`);
    if (!(n >= 1)) throw new Error("la lista de problemas está vacía");
    await b.eval(`document.querySelector('[data-problem="info"]').click()`);
    await sleep(500);
    if ((await b.eval(`window.__cadUi.selection().length`)) !== 2) throw new Error("el renglón del cruce no eligió las dos líneas");
    await b.shot("sketch_limpieza_cruces");
    // Al terminar, el patrón queda guardado con su copia omitida
    await b.clickText("Terminar sketch");
    await sleep(1200);
    const doc = await call("cad_get_document");
    const saved = doc.features[0].kind.sketch.patterns;
    if (saved?.length !== 1 || JSON.stringify(saved[0].skip) !== "[1]") throw new Error(`patrón guardado: ${JSON.stringify(saved)}`);
  },

  async "sketch edición VI: equidistante con el cursor y escrito, campo flotante al mover, soltar, arrastre libre y filtro de selección"(b) {
    await begin(b);
    await sketchOn(b);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    const hover = async (p) => {
      const [x, y] = await at(p);
      await b.mouse("mouseMoved", x, y, { button: "none", buttons: 0 });
      await sleep(300);
    };
    const type = async (text) => {
      for (const ch of text) await key(ch);
      await sleep(150);
      await b.eval(`document.querySelector("[data-typed-field]").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }))`);
      await sleep(900);
    };
    const horizAt = (s, y) => s.entities.filter((e) => e.geometry.type === "line" && Math.abs(pos(s, e.geometry.start)[1] - y) < 0.3 && Math.abs(pos(s, e.geometry.end)[1] - y) < 0.3);
    // Una L abierta (dos líneas con la esquina compartida) y una línea suelta
    await b.eval(`window.__cadUi.change((s) => {
      let id = Math.max(0, ...s.points.map((p) => p.id), ...s.entities.map((e) => e.id)) + 1;
      const pt = (x, y) => (s.points.push({ id, x, y }), id++);
      const a = pt(10, 30), c = pt(30, 30), d = pt(30, 50), e = pt(45, 20), f = pt(60, 20);
      s.entities.push({ id: id++, geometry: { type: "line", start: a, end: c } }, { id: id++, geometry: { type: "line", start: c, end: d } }, { id: id++, geometry: { type: "line", start: e, end: f } });
      s.next_id = id;
    })`);
    await sleep(700);
    let s = await sk();
    const [top, side, loose] = s.entities.map((e) => e.id);
    // Equidistante con el cursor: clic en la L, después arriba de ella a ~3 mm
    await key("f");
    await b.click(...(await at([20, 30])), { wait: 400 });
    await hover([20, 33]);
    if (!(await b.eval(`document.body.innerText.includes("Equidistante a")`))) throw new Error("sin vista previa del equidistante");
    await b.click(...(await at([20, 33])), { wait: 900 });
    s = await sk();
    if (!horizAt(s, 33).length) throw new Error(`el equidistante no quedó del lado del cursor: ${JSON.stringify(s.entities.map((e) => e.geometry))}`);
    // Escrito: 5 mm del otro lado (abajo)
    await b.click(...(await at([20, 30])), { wait: 400 });
    await hover([20, 28]);
    await type("5");
    s = await sk();
    if (!horizAt(s, 25).some((e) => Math.abs(pos(s, e.geometry.start)[1] - 25) < 1e-6)) throw new Error("el equidistante escrito no quedó a 5 mm abajo");
    await key("Escape");
    // Mover escribiendo: la línea suelta 10 mm hacia donde está el cursor (a la derecha)
    await key("s");
    await b.eval(`window.__cadUi.setSelection([${loose}])`);
    await key("v");
    await b.click(...(await at([45, 20])), { wait: 400 });
    await hover([52, 20]);
    if (!(await b.eval(`!!document.querySelector('[data-typed-field="move"]')`))) throw new Error("no apareció el campo flotante al mover");
    await type("10");
    s = await sk();
    const lg = s.entities.find((e) => e.id === loose).geometry;
    if (Math.abs(pos(s, lg.start)[0] - 55) > 1e-6 || Math.abs(pos(s, lg.start)[1] - 20) > 1e-6) throw new Error(`mover escrito: ${pos(s, lg.start)}`);
    // Soltar de lo que no se mueve: la línea de arriba de la L se mueve sin estirar la otra
    await b.eval(`window.__cadUi.setDetachOnMove(true)`);
    await key("s");
    await b.eval(`window.__cadUi.setSelection([${top}])`);
    await key("v");
    await b.click(...(await at([10, 30])), { wait: 400 });
    await hover([10, 36]);
    await type("4");
    s = await sk();
    const corner = s.entities.find((e) => e.id === side).geometry.start;
    if (JSON.stringify(pos(s, corner)) !== "[30,30]") throw new Error(`soltar: la otra se estiró a ${pos(s, corner)}`);
    if (Math.abs(pos(s, s.entities.find((e) => e.id === top).geometry.end)[1] - 34) > 1e-6) throw new Error("soltar: la de arriba no se movió");
    await b.eval(`window.__cadUi.setDetachOnMove(false)`);
    // Arrastre libre: la línea suelta horizontal y con largo; Ctrl+Mayús+arrastrar su punta la suelta
    await key("Escape");
    await key("s");
    await b.eval(`window.__cadUi.change((s) => s.constraints.push({ type: "horizontal", line: ${loose} }, { type: "length", line: ${loose}, value: 15 }))`);
    await sleep(700);
    s = await sk();
    const tip = s.entities.find((e) => e.id === loose).geometry.end;
    const from = await at(pos(s, tip));
    const to = await at([pos(s, tip)[0] + 2, pos(s, tip)[1] + 6]);
    await b.mouse("mouseMoved", from[0], from[1], { buttons: 0 });
    await b.mouse("mousePressed", from[0], from[1], { modifiers: 10 });
    for (let i = 1; i <= 6; i++) await b.mouse("mouseMoved", from[0] + ((to[0] - from[0]) * i) / 6, from[1] + ((to[1] - from[1]) * i) / 6, { modifiers: 10 });
    await b.mouse("mouseReleased", to[0], to[1], { buttons: 0, modifiers: 10 });
    await sleep(900);
    s = await sk();
    if (s.constraints.some((k) => k.line === loose)) throw new Error("el arrastre libre no soltó las restricciones");
    if (!(pos(s, tip)[1] > 22)) throw new Error(`el arrastre libre no movió la punta: ${pos(s, tip)}`);
    // Filtro «solo líneas»: un clic sobre una punta elige la línea, no el punto
    await b.eval(`(() => { const sel = document.querySelector('select[aria-label="Filtro de selección"]'); sel.value = "lines"; sel.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(300);
    s = await sk();
    const sideStart = pos(s, s.entities.find((e) => e.id === side).geometry.start);
    await b.eval(`window.__cadUi.setSelection([])`);
    const xy = await at(sideStart);
    await b.click(...xy, { wait: 500 });
    const picked = await b.eval(`window.__cadUi.selection()`);
    if (picked.length !== 1 || !s.entities.some((e) => e.id === picked[0] && e.geometry.type === "line"))
      throw new Error(`filtro: eligió ${picked} (filtro ${await b.eval(`window.__cadUi.sketchFilter()`)}, encima ${await b.eval(`document.elementFromPoint(${xy[0]}, ${xy[1]})?.outerHTML.slice(0, 120)`)})`);
    await b.shot("sketch_equidistante_cursor");
  },

  async "preselección: lo que está bajo el mouse se resalta"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const pre = () => b.eval(`(() => { const p = window.__cadViewer.preselect; return p && { kind: p.kind, plane: p.plane, face: p.face, edge: p.edge }; })()`);
    const hover = async (x, y) => {
      // Sin botón: con "left" Chromium manda el movimiento como arrastre
      await b.mouse("mouseMoved", x, y, { button: "none", buttons: 0 });
      await sleep(250);
      return pre();
    };
    // Primer punto de la grilla de pantalla donde el visor ve `kind`
    const find = (want, kind) =>
      b.eval(`(() => {
        for (let y = 150; y < 800; y += 20) for (let x = 300; x < 1100; x += 20) {
          const h = window.__cadViewer.pick(x, y, ${JSON.stringify(want)});
          if (h && h.kind === ${JSON.stringify(kind)}) return [x, y];
        }
      })()`);
    // Dónde va el sketch: el plano base y las caras planas se tiñen
    await b.clickText("Sketch");
    await sleep(400);
    const s = await b.eval(`window.__cadViewer.planeSize`);
    let onPlane;
    for (const [u, v] of [[0.47, -0.47], [-0.47, -0.47], [0.47, 0.47], [-0.47, 0.47]]) {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf([${u * s}, ${v * s}, 0])`);
      const h = await b.eval(`window.__cadViewer.pick(${x}, ${y}, { faces: true, planes: true })`);
      if (h?.kind === "plane") onPlane = [x, y];
      if (onPlane) break;
    }
    if (!onPlane) throw new Error("no encontré un punto libre de un plano base");
    const p1 = await hover(...onPlane);
    if (p1?.kind !== "plane") throw new Error("el plano bajo el mouse no se preselecciona: " + JSON.stringify(p1));
    await b.shot("preselect_plane");
    const onFace = await find({ faces: true, planes: true }, "face");
    if (!onFace) throw new Error("no encontré una cara de la caja");
    await hover(...onFace);
    await sleep(500);
    const p2 = await pre();
    if (p2?.kind !== "face") throw new Error("la cara plana bajo el mouse no se preselecciona: " + JSON.stringify(p2));
    await b.shot("preselect_face");
    // Fuera del visor no queda nada resaltado
    await b.eval(`document.querySelector(".cursor-crosshair")?.dispatchEvent(new PointerEvent("pointerleave"))`);
    await sleep(250);
    if (await pre()) throw new Error("la preselección queda al salir del visor");
    await b.key("Escape");
    await sleep(400);
    // Sin herramienta: también las aristas
    const onEdge = await find({ edges: true }, "edge");
    if (!onEdge) throw new Error("no encontré una arista");
    const p3 = await hover(...onEdge);
    if (p3?.kind !== "edge") throw new Error("la arista bajo el mouse no se preselecciona: " + JSON.stringify(p3));
    await b.shot("preselect_edge");
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
    // Sin aristas no hay redondeo: se ve el sólido entero
    near((await body()).volume, 8000, 1e-6, "sin aristas");
    await b.click(...A, { wait: 1200 });
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
    near((await body()).volume, 8000 - (1 - Math.PI / 4) * 20, 0.05, "un redondeo");
    await b.key("Enter", "Enter", 13);
    await sleep(1500);
    const doc = await call("cad_get_document");
    if (doc.features[1]?.kind.edges.length !== 1) throw new Error("no quedó una arista");
    // Editar: la caja ya está eligiendo y se ve el redondeo mientras tanto
    await clickRow(b, "Redondeo 1");
    await sleep(2000);
    if ((await b.eval(`__cadUi.pick().kind`)) !== "edges") throw new Error("al editar no quedó eligiendo");
    near((await body()).volume, 8000 - (1 - Math.PI / 4) * 20, 0.05, "editar: con el redondeo");
    await b.click(...B, { wait: 1200 });
    if ((await items()).length !== 2) throw new Error(`editar: ${await items()}`);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    await b.key("Enter", "Enter", 13);
    await sleep(2000);
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 20, 0.2, "dos redondeos");
    if ((await call("cad_get_document")).features[1].kind.edges.length !== 2) throw new Error("no quedaron dos aristas");
  },

  async "redondeo: se ve mientras se elige y escribir el radio sigue eligiendo"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const radius = `[...document.querySelectorAll("label")].find((l) => l.textContent.startsWith("Radio"))?.querySelector("input")`;
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Redondeo");
    await sleep(1500);
    await b.click(...(await at([0, -10, 10])), { wait: 1500 });
    await b.click(...(await at([10, 0, 10])), { wait: 1500 });
    await sleep(1000);
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 20, 0.2, "eligiendo: ya redondeado");
    // Clic en el campo y escribir como el usuario (Enter lo confirma): se sigue eligiendo
    const r = await b.eval(`(() => { const r = ${radius}.getBoundingClientRect(); return [r.x + 10, r.y + r.height / 2]; })()`);
    await b.click(...r, { wait: 600 });
    if ((await b.eval(`__cadUi.pick().kind`)) !== "edges") throw new Error("tocar el campo dejó de elegir");
    await b.eval(`(() => { const i = document.activeElement; i.select(); document.execCommand("insertText", false, "4"); })()`);
    await b.key("Enter", "Enter", 13);
    await sleep(2500);
    if ((await b.eval(`${radius}.value`)) !== "4") throw new Error(`el campo volvió a ${await b.eval(`${radius}.value`)}`);
    // El empalme de la esquina suma ~6 mm³ con r=4 (crece con r³)
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 16 * 20, 10, "vista previa r=4");
    await accept(b, 2000);
    const k = (await call("cad_get_document")).features[1].kind;
    if (k.radius !== 4 || k.edges.length !== 2) throw new Error(`guardado: ${JSON.stringify(k)}`);
  },

  async "rueda del mouse sobre un campo numérico"(b) {
    const radius = `[...document.querySelectorAll("label")].find((l) => l.textContent.startsWith("Radio"))?.querySelector("input")`;
    const center = async () => b.eval(`(() => { const r = ${radius}.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    const value = () => b.eval(`${radius}.value`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`window.__cadViewer.screenOf([0, -10, 10])`).then((p) => b.click(...p, { wait: 800 }));
    await b.clickText("Redondeo");
    await sleep(1500);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    // Arriba sube un paso, abajo baja; Shift de a 10, Ctrl de a 0,1
    await b.wheel(...(await center()), -100, { wait: 1500 });
    if ((await value()) !== "2") throw new Error(`arriba: ${await value()}`);
    await b.wheel(...(await center()), -100, { modifiers: 8, wait: 1500 });
    if ((await value()) !== "12") throw new Error(`Shift: ${await value()}`);
    await b.wheel(...(await center()), 100, { modifiers: 2, wait: 1500 });
    if ((await value()) !== "11.9") throw new Error(`Ctrl: ${await value()}`);
    await b.wheel(...(await center()), 100, { modifiers: 8, wait: 1500 });
    await b.wheel(...(await center()), 100, { wait: 2500 });
    if ((await value()) !== "0.9") throw new Error(`abajo: ${await value()}`);
    const k = await b.eval(`JSON.parse(JSON.stringify(__cadStore.doc().features.at(-1).kind))`);
    if (k.radius !== 0.9) throw new Error(`el radio no se aplicó: ${k.radius}`);
    near((await body()).volume, 8000 - (1 - Math.PI / 4) * 0.81 * 20, 0.05, "vista previa r=0,9");
    // Desplazando el panel, pasar por encima del campo no lo cambia
    const [x, y] = await center();
    await b.wheel(x, y - 60, 100, { wait: 50 });
    await b.wheel(x, y, 100, { wait: 800 });
    if ((await value()) !== "0.9") throw new Error(`al desplazar cambió: ${await value()}`);
  },

  async "redondeo con arista ya elegida: clic en otra la suma"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const items = () => b.eval(`[...document.querySelector('[aria-label="Aristas"]').querySelectorAll("span")].map((s) => s.textContent)`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.click(...(await at([0, -10, 10])), { wait: 1000 });
    await b.clickText("Redondeo");
    await sleep(2000);
    if ((await items()).length !== 1) throw new Error(`preelegida: ${await items()}`);
    await b.click(...(await at([10, 0, 10])), { wait: 1500 });
    if ((await items()).length !== 2) throw new Error(`no se sumó: ${await items()}`);
    await b.key("Escape", "Escape", 27);
    await sleep(800);
    await accept(b, 2000);
    near((await body()).volume, 8000 - 2 * (1 - Math.PI / 4) * 20, 0.2, "dos redondeos");
  },

  async "árbol: sketch vacío en azul, suelto en rojo"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    if ((await tone(b, "Sketch 1")) !== "empty") throw new Error(`vacío: ${await tone(b, "Sketch 1")}`);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    if ((await tone(b, "Sketch 2")) !== "error") throw new Error(`suelto: ${await tone(b, "Sketch 2")}`);
    await b.shot("arbol_vacio_y_suelto");
    // Una esquina en el origen: queda ligado
    await b.eval(`__cadStore.commit((d) => { const s = d.features.at(-1).kind.sketch; s.constraints.push({ type: "coincident", a: s.entities[0].geometry.start, b: s.origin }); })`);
    await sleep(1500);
    if ((await tone(b, "Sketch 2")) !== null) throw new Error(`ligado: ${await tone(b, "Sketch 2")}`);
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
    if ((await tone(b, "Redondeo 1")) !== "error") throw new Error("la operación con una referencia perdida no quedó en rojo");
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

  async "piezas: booleana, corte que separa y borrar"(b) {
    const parts = async () => (await evaluate()).parts;
    /** Activa la caja de piezas con ese título y hace clic en el visor en `p` (mm) */
    const tick = async (list, p) => {
      await b.eval(`document.querySelector('[aria-label=${JSON.stringify(list)}] [data-parts-box]').click()`);
      await sleep(1500);
      await b.click(...(await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`)), { wait: 800 });
    };
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
    await tick("Piezas que quedan", [-5, -10, 0]);
    await sleep(800);
    await tick("Piezas que restan", [10, -4, 16]);
    await b.key("Escape", "Escape", 27);
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
    // El corte parte la pieza en dos: cada pedazo queda como pieza, sin Separar
    ps = await parts();
    if (ps.length !== 2) throw new Error(`la ranura no separó las piezas: ${ps.length}`);
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
    if (kinds.join(",") !== "primitive,primitive,boolean,primitive,delete_parts") throw new Error(`historial: ${kinds}`);
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
    const chosen = () => b.eval(`document.querySelector('[aria-label="En las piezas"] [data-parts-box]').querySelectorAll("button").length`);
    if ((await chosen()) !== 1) throw new Error(`piezas: ${await chosen()}`);
    // Sumar la de abajo con un clic en ella: atraviesa las dos
    await b.eval(`document.querySelector('[aria-label="En las piezas"] [data-parts-box]').click()`);
    await sleep(1500);
    // La de abajo queda fuera de cuadro: encuadrar todo
    await b.key("Home", "Home", 36);
    await sleep(800);
    await b.click(...(await at([0, -10, -40])), { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await sleep(2000);
    near((await body()).volume, 16000 - 2 * hole, 0.05, "las dos");
    if ((await chosen()) !== 2) throw new Error("la de abajo no quedó elegida");
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
    if (info.views.join(",") !== "front,top,left,iso") throw new Error(`vistas: ${info.views}`);
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
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
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
    if ((await call("cad_get_document")).drawing?.sheets?.[0]?.dims?.length !== 2) throw new Error("no se guardaron las cotas");
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
    // Corte A-A: el frente rayado y la línea en la planta
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
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
    // El «Cerrar» del plano (hay otros en la página)
    await b.eval(`[...document.querySelectorAll('div[aria-label="Plano 2D"] button')].find((x) => x.textContent.trim() === "Cerrar").click()`);
    await sleep(500);
    if (await b.eval(`!!document.querySelector('div[aria-label="Plano 2D"]')`)) throw new Error("no se cerró");
  },

  async "plano 2D: dibujar sobre la hoja con el editor del sketch"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    const sheet = await b.eval(`(() => { const s = window.__cadUi.session(); return s && { ...s.sheet, open: !!document.querySelector('div[aria-label="Plano 2D"]') }; })()`);
    if (!sheet || sheet.width !== 297 || sheet.height !== 210 || !String(sheet.backdrop).startsWith("blob:")) throw new Error(`sesión de la hoja: ${JSON.stringify(sheet)}`);
    if (sheet.open) throw new Error("el plano sigue tapando el visor");
    if (await b.eval(`[...document.querySelectorAll("button")].some((x) => x.title?.startsWith("Usar arista") && x.offsetParent)`)) throw new Error("Usar arista no va sobre la hoja");
    await sleep(800);
    await b.shot("plano_dibujar");
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    // Una línea (mm de hoja) y un círculo
    await key("l");
    await b.click(...(await at([40, 40])), { wait: 400 });
    await b.click(...(await at([100, 40])), { wait: 400 });
    await key("Escape");
    await key("c");
    await b.click(...(await at([150, 60])), { wait: 400 });
    await b.click(...(await at([160, 60])), { wait: 800 });
    await key("Escape");
    const kinds = await b.eval(`window.__cadUi.session().sketch.entities.map((e) => e.geometry.type).sort().join(",")`);
    if (kinds !== "circle,line") throw new Error(`dibujado: ${kinds}`);
    const ends = await b.eval(`(() => { const s = window.__cadUi.session().sketch; const l = s.entities.find((e) => e.geometry.type === "line").geometry; return [l.start, l.end].map((id) => { const p = s.points.find((q) => q.id === id); return [p.x, p.y]; }); })()`);
    near(Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]), 60, 1.5, "largo de la línea");
    // De vuelta al plano: lo dibujado está en la hoja y en el documento
    await b.clickText("Volver al plano");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-layer]")`)); t++) await sleep(250);
    const layer = await b.eval(`document.querySelectorAll("[data-sheet] svg [data-layer] polyline").length`);
    if (layer !== 2) throw new Error(`en la hoja: ${layer}`);
    const saved = (await call("cad_get_document")).drawing?.sheets?.[0]?.sketch;
    if (saved?.entities?.length !== 2) throw new Error(`guardado: ${JSON.stringify(saved?.entities)}`);
    await b.shot("plano_con_dibujo");
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const dxf = `${out}/plano_dibujo.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    if (!(existsSync(dxf) && readFileSync(dxf, "utf8").includes("\nDIBUJO\n10\n"))) throw new Error("DXF sin lo dibujado");
    // Se vuelve a abrir con lo guardado; descartar no cambia nada y vuelve al plano
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    if ((await b.eval(`window.__cadUi.session().sketch.entities.length`)) !== 2) throw new Error("no se abrió lo guardado");
    await b.clickText("Descartar");
    await sleep(500);
    if (!(await b.eval(`!!document.querySelector('div[aria-label="Plano 2D"]') && !window.__cadUi.session()`))) throw new Error("descartar no volvió al plano");
  },

  async "plano 2D: medida al dibujar sobre la hoja (queda solo si se escribe o con Enter) y cotas donde se dejaron"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(800);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const asked = () => b.eval(`document.activeElement?.tagName === "INPUT"`);
    const lengths = () => b.eval(`window.__cadUi.session().sketch.constraints.filter((c) => c.type === "length").map((c) => c.value)`);
    const line = async (p, q) => {
      await key("l");
      await b.click(...(await at(p)), { wait: 400 });
      await b.click(...(await at(q)), { wait: 800 });
      if (!(await asked())) throw new Error("no se pidió la medida sobre la hoja");
    };
    // 1. Se escribe 50 + Enter: queda la cota y la línea mide 50
    await line([40, 40], [100, 40]);
    if ((await lengths()).length) throw new Error("la cota se agregó antes de escribirla");
    await b.send("Input.insertText", { text: "50" });
    await b.key("Enter", "Enter", 13);
    await sleep(1200);
    await key("Escape");
    if (JSON.stringify(await lengths()) !== "[50]") throw new Error(`con 50: ${JSON.stringify(await lengths())}`);
    // 2. Esc en el campo: no queda nada
    await line([40, 100], [100, 100]);
    await b.key("Escape", "Escape", 27);
    await sleep(600);
    await key("Escape");
    if ((await lengths()).length !== 1) throw new Error(`con Esc: ${JSON.stringify(await lengths())}`);
    // 3. Enter sin escribir: queda con lo dibujado
    await line([40, 150], [80, 150]);
    await b.key("Enter", "Enter", 13);
    await sleep(1200);
    await key("Escape");
    const ls = await lengths();
    if (ls.length !== 2) throw new Error(`con Enter: ${JSON.stringify(ls)}`);
    near(ls[1], 40, 1, "cota con lo dibujado");
    const ends = await b.eval(`(() => { const s = window.__cadUi.session().sketch; const l = s.entities.find((e) => e.geometry.type === "line").geometry; return [l.start, l.end].map((id) => { const p = s.points.find((q) => q.id === id); return [p.x, p.y]; }); })()`);
    near(Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]), 50, 1e-3, "la línea acotada");
    // 4. El texto de la cota de 50 arrastrado a mano: en la hoja sale ahí (y la línea de cota se alarga)
    await b.eval(`window.__cadUi.change((sk) => { const c = sk.constraints.find((c) => c.type === "length" && Math.abs(c.value - 50) < 1e-6); c.opts = { offset: [60, 15] }; })`);
    await sleep(800);
    await b.clickText("Volver al plano");
    const dimTexts = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-layer-dims] text")].map((t) => [t.textContent, +t.getAttribute("x"), +t.getAttribute("y")])`);
    for (let t = 0; t < 30 && (await dimTexts()).length < 2; t++) await sleep(250);
    const texts = await dimTexts();
    const fifty = texts.find((t) => t[0] === "50");
    if (!fifty || !texts.some((t) => t[0] === "40")) throw new Error(`cotas en la hoja: ${JSON.stringify(texts)}`);
    // Medio de la línea + (60, 15) en mm de hoja; en el SVG Y va hacia abajo (alto 210) y el renglón 1 mm arriba
    const mid = [(ends[0][0] + ends[1][0]) / 2, (ends[0][1] + ends[1][1]) / 2];
    near(fifty[1], mid[0] + 60, 0.5, "texto de la cota (x)");
    near(fifty[2], 210 - (mid[1] + 15) - 1, 0.5, "texto de la cota (y)");
    await b.shot("plano_medida_al_dibujar");
  },

  async "planos 2D en el Outliner: varios, elementos, ventana de propiedades"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    const ready = async () => {
      for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
      await sleep(300);
    };
    await ready();
    const row = (id) => `document.querySelector('[data-outliner-row=${JSON.stringify(id)}]')`;
    // Con el plano abierto, la barra izquierda es la del plano (no la de Diseñar)
    const side = () => b.eval(`({ cota: !!document.querySelector('button[aria-label="Cota"]'), extrude: [...document.querySelectorAll("button[aria-label]")].some((x) => x.getAttribute("aria-label").startsWith("Extrusión") && x.offsetParent) })`);
    const s1 = await side();
    if (!s1.cota || s1.extrude) throw new Error(`barra izquierda con el plano: ${JSON.stringify(s1)}`);
    if (!(await b.eval(`!!${row("group-drawings")} && !!${row("drawing-1")} && !!${row("drawing-1:view:iso")}`))) throw new Error("no está el grupo Planos 2D con el plano y sus vistas");
    // Otro plano con «+»: se abre con su hoja elegida
    await b.eval(`document.querySelector('[aria-label="Plano nuevo"]').click()`);
    for (let t = 0; t < 20 && (await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 2"; t++) await sleep(250);
    await ready();
    if (!(await b.eval(`!!document.querySelector('[data-drawing-props="sheet"]')`))) throw new Error("el plano nuevo no abrió su hoja");
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Nombre del plano"]'); i.value = "Cortes"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    for (let t = 0; t < 20 && !(await b.eval(`${row("drawing-2")}?.textContent.includes("Cortes")`)); t++) await sleep(250);
    if (!(await b.eval(`${row("drawing-2")}?.textContent.includes("Cortes")`))) throw new Error("el nombre no llegó al Outliner");
    // El ojo de la isométrica la saca de la hoja
    await b.eval(`[...${row("drawing-2:view:iso")}.querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 30 && (await b.eval(`!!document.querySelector('[data-sheet] [data-view="iso"]')`)); t++) await sleep(250);
    if (await b.eval(`!!document.querySelector('[data-sheet] [data-view="iso"]')`)) throw new Error("la isométrica sigue");
    // Una cota en el frente; clic en su texto la elige (ventana y fila del Outliner)
    await b.eval(`document.querySelector('button[aria-label="Cota"]').click()`);
    await sleep(200);
    const mid = await b.eval(`(() => {
      const svg = document.querySelector("[data-sheet] svg");
      const l = [...svg.querySelectorAll('[data-view="front"] polyline[data-kind="visible"]')].map((x) => x.getAttribute("points").split(" ").map((p) => p.split(",").map(Number))).find((x) => x.length === 2);
      const p = new DOMPoint((l[0][0] + l[1][0]) / 2, (l[0][1] + l[1][1]) / 2).matrixTransform(svg.getScreenCTM());
      return [p.x, p.y];
    })()`);
    await b.click(...mid, { wait: 1000 });
    await b.key("Escape", "Escape", 27);
    const txt = await b.eval(`(() => { const t = document.querySelector('[data-sheet] [data-user-dim="0"] text'); if (!t) return null; const r = t.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    if (!txt) throw new Error("no se puso la cota");
    await b.click(...txt, { wait: 500 });
    const picked = await b.eval(`({ props: document.querySelector('[data-drawing-props]')?.dataset.drawingProps, value: document.querySelector("[data-dim-value]")?.textContent, row: ${row("drawing-2:dim:0")}?.className.includes("bg-accent/20") })`);
    if (picked.props !== "dim" || picked.value !== "20" || !picked.row) throw new Error(`cota elegida: ${JSON.stringify(picked)}`);
    // Supr la quita
    await b.key("Delete", "Delete", 46);
    for (let t = 0; t < 20 && (await b.eval(`!!${row("drawing-2:dim:0")}`)); t++) await sleep(250);
    if (await b.eval(`!!${row("drawing-2:dim:0")}`)) throw new Error("Supr no quitó la cota");
    // Clic en el otro plano del Outliner: se abre ese
    await b.eval(`${row("drawing-1")}.click()`);
    for (let t = 0; t < 20 && (await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 1"; t++) await sleep(250);
    if ((await b.eval(`document.querySelector("[data-drawing-name]")?.textContent.trim()`)) !== "Plano 1") throw new Error("no se abrió el Plano 1");
    await b.eval(`[...document.querySelectorAll('div[aria-label="Plano 2D"] button')].find((x) => x.textContent.trim() === "Cerrar").click()`);
    await sleep(500);
    const s2 = await side();
    if (s2.cota || !s2.extrude) throw new Error(`barra izquierda al cerrar el plano: ${JSON.stringify(s2)}`);
    const doc = (await call("cad_get_document")).drawing;
    if (doc?.sheets?.length !== 2 || !doc.sheets[1].hiddenViews.includes("iso") || doc.sheets[1].name !== "Cortes") throw new Error(`guardado: ${JSON.stringify(doc)}`);
    await b.shot("planos_outliner");
  },

  async "plano 2D: seis vistas en primer y tercer diedro"(b) {
    await begin(b);
    // Caja de 30 × 20 × 10 corrida: cada vista tiene otra forma
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    const shown = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-view]")].map((g) => g.dataset.view).join(",")`);
    if ((await shown()) !== "front,top,left,iso") throw new Error(`al empezar: ${await shown()}`);
    // Prender las que faltan con el ojo del Outliner
    for (const v of ["bottom", "right", "back"]) await b.eval(`[...document.querySelector('[data-outliner-row="drawing-1:view:${v}"]').querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 40 && (await shown()) !== "front,top,bottom,left,right,back,iso"; t++) await sleep(250);
    if ((await shown()) !== "front,top,bottom,left,right,back,iso") throw new Error(`las seis: ${await shown()}`);
    const boxes = () =>
      b.eval(`Object.fromEntries([...document.querySelectorAll("[data-sheet] svg [data-view]")].map((g) => { const r = g.getBBox(); return [g.dataset.view, { cx: r.x + r.width / 2, cy: r.y + r.height / 2 }]; }))`);
    const check = (v, first) => {
      const [up, down, l, r] = first ? ["bottom", "top", "right", "left"] : ["top", "bottom", "left", "right"];
      if (!(v[up].cy < v.front.cy && v[down].cy > v.front.cy)) throw new Error(`${first ? "1.er" : "3.er"} diedro arriba/abajo: ${JSON.stringify(v)}`);
      if (!(v[l].cx < v.front.cx && v[r].cx > v.front.cx && v.back.cx > v[r].cx)) throw new Error(`${first ? "1.er" : "3.er"} diedro costados: ${JSON.stringify(v)}`);
    };
    check(await boxes(), true);
    await b.shot("plano_seis_vistas_iso_e");
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].projection = "third"; })`);
    for (let t = 0; t < 40; t++) {
      const v = await boxes();
      if (v.top && v.front && v.top.cy < v.front.cy) break;
      await sleep(250);
    }
    await sleep(500);
    check(await boxes(), false);
    await b.shot("plano_seis_vistas_iso_a");
    // Escala propia de la posterior: rótulo debajo y la fila del Outliner lo dice
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].viewScales = { back: 2 }; })`);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-view-label="back"]')`)); t++) await sleep(250);
    const label = await b.eval(`document.querySelector('[data-sheet] [data-view-label="back"]')?.textContent`);
    if (label !== "Posterior (2:1)") throw new Error(`rótulo: ${label}`);
    if (!(await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:back"]')?.textContent.includes("(2:1)")`))) throw new Error("el Outliner no muestra la escala");
    await b.shot("plano_escala_por_vista");
  },

  async "plano 2D: las vistas como referencia del dibujo (anclar, usar, recortar, cotas que siguen al modelo)"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    // Escala fija 1:1: las medidas en la hoja son las de la pieza
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 1; })`);
    await sleep(1500);
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(800);
    // Esquinas del frente en mm de hoja (Y hacia arriba)
    const front = await b.eval(`(() => {
      const sh = window.__cadUi.session().sheet;
      const v = sh.views.find((x) => x.name === "front");
      const pts = v.lines.flatMap((l) => l.points);
      const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
      const S = (p) => [v.x + p[0] * v.scale, sh.height - v.y + p[1] * v.scale];
      return { lo: S([Math.min(...xs), Math.min(...ys)]), hi: S([Math.max(...xs), Math.max(...ys)]) };
    })()`);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const pos = (s, id) => { const q = s.points.find((p) => p.id === id); return [q.x, q.y]; };
    // 1. Anclar: una línea que empieza cerca de la esquina de arriba a la derecha del frente queda justo en ella
    await key("l");
    await b.click(...(await at([front.hi[0] + 0.4, front.hi[1] + 0.4])), { wait: 400 });
    await b.click(...(await at([front.hi[0] + 30, front.hi[1] + 25])), { wait: 400 });
    await key("Escape");
    await key("Escape");
    let s = await sk();
    const first = s.entities.find((e) => e.geometry.type === "line");
    const p0 = pos(s, first.geometry.start);
    near(p0[0], front.hi[0], 1e-6, "anclado a la esquina (x)");
    near(p0[1], front.hi[1], 1e-6, "anclado a la esquina (y)");
    // 2. Usar el borde de abajo del frente: queda ligado
    await key("j");
    await b.click(...(await at([(front.lo[0] + front.hi[0]) / 2, front.lo[1]])), { wait: 600 });
    s = await sk();
    if ((s.uses ?? []).length !== 1) throw new Error(`usar: ${JSON.stringify(s.uses)}`);
    const used = s.entities.find((e) => e.id === s.uses[0].entity);
    near(Math.abs(pos(s, used.geometry.end)[0] - pos(s, used.geometry.start)[0]), 20, 1e-6, "el borde usado mide 20");
    if ((await b.eval(`window.__cadUi.session().sheet.links.length`)) !== 1) throw new Error("no quedó el vínculo");
    // 3. Recortar contra la vista: una línea que entra al frente por la izquierda; clic adentro quita ese tramo
    const y = (front.lo[1] + front.hi[1]) / 2;
    await key("l");
    await b.click(...(await at([front.lo[0] - 15, y + 3])), { wait: 400 });
    await b.click(...(await at([front.lo[0] + 8, y + 3])), { wait: 400 });
    await key("Escape");
    await key("Escape");
    await key("t");
    await b.click(...(await at([front.lo[0] + 5, y + 3])), { wait: 600 });
    s = await sk();
    const cut = s.entities.filter((e) => e.geometry.type === "line" && !(s.uses ?? []).some((u) => u.entity === e.id)).map((e) => [pos(s, e.geometry.start), pos(s, e.geometry.end)]).find((l) => Math.abs(l[0][1] - (y + 3)) < 1 && Math.abs(l[1][1] - (y + 3)) < 1);
    if (!cut) throw new Error(`recortar: ${JSON.stringify(s.entities.map((e) => e.geometry.type))}`);
    near(Math.max(cut[0][0], cut[1][0]), front.lo[0], 1e-6, "recortada en el borde de la vista");
    // 4. Una cota de largo en la línea usada y de vuelta al plano: sale en la hoja
    await b.eval(`window.__cadUi.change((sk) => sk.constraints.push({ type: "length", line: ${used.id}, value: 20, reference: true }))`);
    await sleep(800);
    await b.clickText("Volver al plano");
    const dimTexts = () => b.eval(`[...document.querySelectorAll("[data-sheet] svg [data-layer-dims] text")].map((t) => t.textContent)`);
    for (let t = 0; t < 30 && !(await dimTexts()).includes("20"); t++) await sleep(250);
    if (!(await dimTexts()).includes("20")) throw new Error(`cotas del dibujo: ${await dimTexts()}`);
    const saved = (await call("cad_get_document")).drawing.sheets[0];
    if (saved.links?.length !== 1) throw new Error(`vínculos guardados: ${JSON.stringify(saved.links)}`);
    await b.shot("plano_referencias");
    // 5. La caja pasa a 30 de ancho: lo ligado (y su cota) la sigue
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    box.kind.shape.dx = 30;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    for (let t = 0; t < 40 && !(await dimTexts()).includes("30"); t++) await sleep(250);
    if (!(await dimTexts()).includes("30")) throw new Error(`la cota no siguió al modelo: ${await dimTexts()}`);
    await b.shot("plano_referencias_modelo");
  },

  async "plano 2D: las escalas dicen la verdad (lo dibujado mide la pieza por la escala)"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    Object.assign(box.kind.shape, { dx: 100, dy: 60, dz: 40 });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await sleep(500);
    // Tamaño de cada vista en la pieza (ancho × alto en la hoja)
    const size = { front: [100, 40], back: [100, 40], top: [100, 60], bottom: [100, 60], left: [60, 40], right: [60, 40] };
    const measure = () =>
      b.eval(`(() => {
        const svg = document.querySelector("[data-sheet] svg");
        const scale = [...svg.querySelectorAll("[data-title-block] text")].map((t) => t.textContent).find((t) => /^\\d+:\\d+$/.test(t));
        const views = Object.fromEntries([...svg.querySelectorAll("[data-view]")].map((g) => { const r = g.getBBox(); return [g.dataset.view, [r.width, r.height]]; }));
        return { scale, views };
      })()`);
    const check = (m, what) => {
      const [a, c] = m.scale.split(":").map(Number);
      const k = a / c;
      for (const [name, [w, h]] of Object.entries(m.views)) {
        if (!size[name]) continue;
        near(w, size[name][0] * k, 0.05, `${what}: ancho de ${name} a ${m.scale}`);
        near(h, size[name][1] * k, 0.05, `${what}: alto de ${name} a ${m.scale}`);
      }
    };
    const four = await measure();
    check(four, "4 vistas");
    // Lo que dicen las ventanas (Hoja y la de una vista) también es la escala de verdad
    const said = () => b.eval(`[...document.querySelectorAll('[data-drawing-props] button, [data-drawing-props] [data-scale-now]')].map((x) => x.textContent.trim()).join(" | ")`);
    await b.clickText("Hoja");
    await sleep(400);
    const sheetBefore = await said();
    for (const v of ["bottom", "right", "back"]) await b.eval(`[...document.querySelector('[data-outliner-row="drawing-1:view:${v}"]').querySelectorAll("button")].pop().click()`);
    for (let t = 0; t < 40 && Object.keys((await measure()).views).length < 7; t++) await sleep(250);
    await sleep(500);
    const seven = await measure();
    if (Object.keys(seven.views).length !== 7) throw new Error(`vistas: ${Object.keys(seven.views)}`);
    check(seven, "7 vistas");
    const sheetAfter = await said();
    if (four.scale === seven.scale) throw new Error("con más vistas la automática debía bajar");
    if (!sheetBefore.includes(four.scale) || !sheetAfter.includes(seven.scale) || sheetAfter.includes(four.scale)) throw new Error(`la ventana Hoja no dice la escala: ${sheetBefore} → ${sheetAfter}`);
    const bar = await b.eval(`document.querySelector("[data-sheet-scale]")?.textContent`);
    if (bar !== `Escala ${seven.scale} (auto)`) throw new Error(`barra: ${bar}`);
  },

  async "plano 2D: mover vistas (y lo dibujado con ellas), vista nueva y girar en 3D"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const doc = await call("cad_get_document");
    Object.assign(doc.features.find((f) => f.kind.type === "primitive").kind.shape, { dx: 40, dy: 20, dz: 10 });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 1; })`);
    await sleep(1500);
    // En pantalla (para arrastrar) y en mm de la hoja (para medir: la hoja en pantalla puede cambiar de tamaño)
    const box = (name) => b.eval(`(() => { const g = document.querySelector('[data-sheet] [data-view="${name}"]'); if (!g) return null; const r = g.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; })()`);
    const mm = (name) => b.eval(`(() => { const g = document.querySelector('[data-sheet] [data-view="${name}"]'); if (!g) return null; const r = g.getBBox(); return [r.x, r.y, r.width, r.height]; })()`);
    const screenSize = () => b.eval(`(() => { const r = document.querySelector("[data-sheet]").getBoundingClientRect(); return [r.width, r.height]; })()`);
    const drag = async (from, to, modifiers = 0) => {
      await b.mouse("mouseMoved", from[0], from[1], { buttons: 0 });
      await b.mouse("mousePressed", from[0], from[1], { modifiers });
      for (let i = 1; i <= 6; i++) await b.mouse("mouseMoved", from[0] + ((to[0] - from[0]) * i) / 6, from[1] + ((to[1] - from[1]) * i) / 6, { modifiers });
      await sleep(200);
      await b.mouse("mouseReleased", to[0], to[1], { modifiers, buttons: 0 });
      await sleep(1200);
    };
    const sheets = async () => (await call("cad_get_document")).drawing.sheets[0];
    // La lateral se arrastra en diagonal: solo se corre en horizontal (sigue alineada con el frente)
    const l0 = await box("left");
    const m0 = await mm("left");
    const s0 = await screenSize();
    const c0 = [l0[0] + l0[2] / 2, l0[1] + l0[3] / 2];
    await drag(c0, [c0[0] + 60, c0[1] + 40]);
    const m1 = await mm("left");
    if (!(m1[0] > m0[0] + 10) || Math.abs(m1[1] - m0[1]) > 1e-6) throw new Error(`arrastre alineado (mm): ${m0} → ${m1}`);
    if (!(await sheets()).viewAt?.left) throw new Error("no se guardó dónde quedó");
    const s1 = await screenSize();
    if (Math.abs(s1[0] - s0[0]) > 0.5 || Math.abs(s1[1] - s0[1]) > 0.5) throw new Error(`la hoja cambió de tamaño en pantalla: ${s0} → ${s1}`);
    // Acomodar la devuelve
    await b.clickText("Acomodar");
    for (let t = 0; t < 20 && Math.abs((await mm("left"))[0] - m0[0]) > 1e-6; t++) await sleep(250);
    if (Math.abs((await mm("left"))[0] - m0[0]) > 1e-6) throw new Error("Acomodar no la devolvió");
    // Una línea dibujada encima del frente se mueve con él
    await b.clickText("Dibujar");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()?.sheet`)); t++) await sleep(250);
    await sleep(600);
    // Caja del frente en mm de hoja (Y hacia arriba)
    const fr = await b.eval(`(() => {
      const sh = window.__cadUi.session().sheet;
      const v = sh.views.find((x) => x.name === "front");
      const pts = v.lines.flatMap((l) => l.points);
      const S = (p) => [v.x + p[0] * v.scale, sh.height - v.y + p[1] * v.scale];
      return { lo: S([Math.min(...pts.map((p) => p[0])), Math.min(...pts.map((p) => p[1]))]), hi: S([Math.max(...pts.map((p) => p[0])), Math.max(...pts.map((p) => p[1]))]) };
    })()`);
    const at = (p) => b.eval(`window.__cadViewer.screenOf([${p[0]}, ${p[1]}, 0])`);
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)} }))`);
    const lerp = (u, v) => [fr.lo[0] + (fr.hi[0] - fr.lo[0]) * u, fr.lo[1] + (fr.hi[1] - fr.lo[1]) * v];
    await key("l");
    // Adentro del frente, lejos de sus bordes y de sus medios
    await b.click(...(await at(lerp(0.17, 0.31))), { wait: 400 });
    await b.click(...(await at(lerp(0.71, 0.77))), { wait: 400 });
    await key("Escape");
    await key("Escape");
    await b.clickText("Volver al plano");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-layer] polyline")`)); t++) await sleep(250);
    if ((await sheets()).owners?.length !== 1) throw new Error(`dueños: ${JSON.stringify((await sheets()).owners)}`);
    const linePts = () => b.eval(`document.querySelector("[data-sheet] svg [data-layer] polyline").getAttribute("points")`);
    const before = (await linePts()).split(" ").map((p) => p.split(",").map(Number));
    const f0 = await box("front");
    await drag([f0[0] + f0[2] * 0.8, f0[1] + f0[3] * 0.8], [f0[0] + f0[2] * 0.8 + 50, f0[1] + f0[3] * 0.8 + 30]);
    for (let t = 0; t < 20 && (await linePts()) === before.map((p) => p.join(",")).join(" "); t++) await sleep(250);
    const after = (await linePts()).split(" ").map((p) => p.split(",").map(Number));
    const sv = (await sheets()).viewAt.front;
    const dx = after[0][0] - before[0][0];
    const dy = after[0][1] - before[0][1];
    if (!(Math.abs(dx) > 10 && Math.abs(dy) > 5)) throw new Error(`lo dibujado no siguió al frente: ${before} → ${after} (frente en ${sv})`);
    near(after[1][0] - after[0][0], before[1][0] - before[0][0], 1e-6, "misma línea, corrida");
    // Vista nueva (mira como la isométrica) y un giro de 90° con su botón
    await b.clickText("Vista nueva");
    for (let t = 0; t < 30 && !(await box("vista-1")); t++) await sleep(250);
    if (!(await box("vista-1"))) throw new Error("no apareció la vista nueva");
    if ((await b.eval(`document.querySelector('[data-sheet] [data-view-label="vista-1"]')?.textContent`)) !== "Vista 1") throw new Error("la vista nueva no lleva su nombre");
    const dir0 = (await sheets()).viewDirs["vista-1"];
    await b.eval(`[...document.querySelectorAll('[aria-label="Girar la vista"] button')].find((x) => x.textContent === "→").click()`);
    for (let t = 0; t < 20 && JSON.stringify((await sheets()).viewDirs["vista-1"]) === JSON.stringify(dir0); t++) await sleep(250);
    const dir1 = (await sheets()).viewDirs["vista-1"];
    if (JSON.stringify(dir1) === JSON.stringify(dir0)) throw new Error("el botón no giró la vista");
    // Ctrl+arrastrar la isométrica la gira: durante el arrastre se ve la vista previa
    const iso = await box("iso");
    const ci = [iso[0] + iso[2] / 2, iso[1] + iso[3] / 2];
    await b.mouse("mouseMoved", ci[0], ci[1], { buttons: 0 });
    await b.mouse("mousePressed", ci[0], ci[1], { modifiers: 2 });
    for (let i = 1; i <= 5; i++) await b.mouse("mouseMoved", ci[0] + i * 12, ci[1], { modifiers: 2 });
    await sleep(300);
    const preview = await b.eval(`document.querySelectorAll("[data-orbit-preview] polyline").length`);
    await b.mouse("mouseReleased", ci[0] + 60, ci[1], { modifiers: 2, buttons: 0 });
    await sleep(1500);
    if (!(preview > 0)) throw new Error("sin vista previa al girar");
    const isoDir = (await sheets()).viewDirs?.iso;
    if (!isoDir) throw new Error("no se guardó el giro de la isométrica");
    await b.shot("plano_girar_mover");
  },

  async "plano 2D: cortes por línea y escalonados, medio corte, rotura, vista 3D cortada y sombreada"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Caja de 40 × 20 × 10 con dos agujeros pasantes verticales (en x = −10 y x = 10)
    const doc = await call("cad_get_document");
    Object.assign(doc.features.find((f) => f.kind.type === "primitive").kind.shape, { dx: 40, dy: 20, dz: 10 });
    for (const [x, y] of [[-10, -3], [10, 3]]) {
      doc.features.push({ id: doc.next_id, name: `Agujero ${x}`, suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 2, height: 40 }, origin: [x, y, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
      doc.next_id += 1;
    }
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].scale = 2; })`);
    await sleep(1500);
    // Punto de una vista (mm de la pieza) → pantalla
    const at = (view, p) =>
      b.eval(`(() => {
        const sh = window.__cadStore.doc().drawing.sheets[0];
        const svg = document.querySelector("[data-sheet] svg");
        const g = svg.querySelector('[data-view="${view}"]');
        const pts = [...g.querySelectorAll("polyline")].flatMap((l) => l.getAttribute("points").split(" ").map((q) => q.split(",").map(Number)));
        const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
        // Centro de la vista en la hoja y la escala (2:1)
        const [cx, cy] = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
        const q = new DOMPoint(cx + ${p[0]} * 2, cy - ${p[1]} * 2).matrixTransform(svg.getScreenCTM());
        return [q.x, q.y];
      })()`);
    const tool = (name) => b.eval(`document.querySelector('button[aria-label="${name}"]').click()`);
    const count = (sel) => b.eval(`document.querySelectorAll('[data-sheet] ${sel}').length`);
    const wait = async (sel, what) => {
      for (let t = 0; t < 40 && !(await count(sel)); t++) await sleep(300);
      if (!(await count(sel))) throw new Error(`no apareció: ${what}`);
    };
    // 1. Corte recto B-B por el medio de la superior (de izquierda a derecha: se mira hacia atrás)
    await tool("Corte");
    await b.click(...(await at("top", [-22, 0])), { wait: 300 });
    await b.click(...(await at("top", [22, 0])), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-hatch="corte-B"] polygon', "rayado del corte B-B");
    await wait('[data-cut-line="B"]', "línea de corte B");
    if (!(await b.eval(`[...document.querySelectorAll("[data-sheet] [data-view-label]")].some((t) => t.textContent === "Corte B-B")`))) throw new Error("sin rótulo Corte B-B");
    // 2. Escalonado C-C: pasa por los dos agujeros (y = −3 y y = 3)
    await tool("Corte");
    for (const p of [[-22, -3], [0, -3], [0, 3], [22, 3]]) await b.click(...(await at("top", p)), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-hatch="corte-C"] polygon', "rayado del corte escalonado");
    if (!(await b.eval(`document.querySelector('[data-outliner-row="drawing-1:cut:C"]')?.textContent.includes("escalonado")`))) throw new Error("el Outliner no dice escalonado");
    // 3. Medio corte del frente (derecha)
    await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:front"]').click()`);
    await sleep(400);
    await b.eval(`[...document.querySelectorAll('[aria-label="Medio corte"] button')].find((x) => x.textContent === "Derecha").click()`);
    await wait('[data-hatch="front"] polygon', "rayado del medio corte");
    await wait('[data-center-line="front"]', "línea central del medio corte");
    // 4. Rotura en la lateral izquierda
    await tool("Rotura");
    for (const p of [[-4, -2], [4, -2], [4, 3], [-4, 3]]) await b.click(...(await at("left", p)), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    await wait('[data-break-line="left"]', "contorno de la rotura");
    await wait('[data-hatch="left"] polygon', "rayado de la rotura");
    // 5. Isométrica cortada en un cuarto y sombreada
    await b.eval(`document.querySelector('[data-outliner-row="drawing-1:view:iso"]').click()`);
    await sleep(400);
    await b.eval(`[...document.querySelectorAll('[aria-label="Cortar la vista 3D"] button')].find((x) => x.textContent === "Un cuarto").click()`);
    await wait('[data-hatch="iso"] polygon', "rayado de la isométrica cortada");
    await b.eval(`[...document.querySelectorAll('[data-drawing-props] label')].find((l) => l.textContent.trim() === "Sombreada").querySelector("input").click()`);
    await wait('[data-view-image="iso"]', "imagen sombreada");
    await b.shot("plano_cortes");
    // Las vistas de los cortes entran en la hoja
    const inside = await b.eval(`(() => { const svg = document.querySelector("[data-sheet] svg"); const W = svg.viewBox.baseVal.width, H = svg.viewBox.baseVal.height; return ["corte-B", "corte-C"].map((n) => { const r = svg.querySelector('[data-view="' + n + '"]').getBBox(); return r.x >= 10 && r.y >= 10 && r.x + r.width <= W - 10 && r.y + r.height <= H - 10; }); })()`);
    if (inside.some((x) => !x)) throw new Error(`las vistas de los cortes se salen de la hoja: ${inside}`);
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const png = `${out}/iso_sombreada.png`;
    rmSync(png, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(png)}`);
    await b.clickText("Guardar imagen PNG");
    await sleep(1500);
    if (!existsSync(png) || readFileSync(png).subarray(1, 4).toString() !== "PNG") throw new Error("no se guardó el PNG");
    // El PDF lleva la imagen
    const pdf = `${out}/plano_cortes.pdf`;
    rmSync(pdf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(pdf)}`);
    await b.clickText("Exportar PDF");
    await sleep(3000);
    if (!existsSync(pdf)) throw new Error("no se exportó el PDF");
    const { execSync } = await import("node:child_process");
    try {
      const list = execSync(`pdfimages -list ${pdf}`).toString();
      if (list.trim().split("\n").length < 3) throw new Error(`el PDF no tiene la imagen: ${list}`);
    } catch (e) {
      if (String(e).includes("no tiene")) throw e;
    }
    await b.key("Escape", "Escape", 27);
    await sleep(400);
    await b.shot("plano_cortes_hoja");
  },

  async "plano 2D: rayado por pieza según su material, DXF de los cortes y vista del corte alineada"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Dos piezas: la caja de acero y otra al lado de PLA
    const doc = await call("cad_get_document");
    const box = doc.features.find((f) => f.kind.type === "primitive");
    Object.assign(box.kind.shape, { dx: 20, dy: 20, dz: 10 });
    doc.features.push({ id: doc.next_id++, name: "Caja 2", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 10, centered: true, centered_z: true }, origin: [20, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const parts = (await evaluate()).parts;
    if (parts.length !== 2) throw new Error(`piezas: ${parts.length}`);
    await b.eval(`window.__cadStore.commit((d) => { d.parts = ${JSON.stringify(parts.map((p, i) => ({ part: p.id, material: i === 0 ? { name: "Acero", density: 7850 } : { name: "PLA", density: 1240 } })))}; })`);
    await sleep(1500);
    await b.clickText("Plano 2D");
    for (let t = 0; t < 30 && !(await b.eval(`!!document.querySelector("[data-sheet] svg [data-view]")`)); t++) await sleep(500);
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
    for (let t = 0; t < 40 && (await b.eval(`document.querySelectorAll('[data-sheet] [data-hatch="front"]').length`)) < 2; t++) await sleep(300);
    const groups = await b.eval(`[...document.querySelectorAll('[data-sheet] [data-hatch="front"]')].map((g) => { const id = g.getAttribute("fill").slice(5, -1); const p = document.querySelector('[data-sheet] pattern[id="' + id + '"]'); return { lines: p.querySelectorAll("line").length, rot: p.getAttribute("patternTransform") }; })`);
    // Acero de a pares (dos líneas); PLA cruzado (dos líneas, una horizontal) y con el ángulo al revés por ser la vecina
    if (groups.length !== 2 || groups[0].rot === groups[1].rot) throw new Error(`rayados por pieza: ${JSON.stringify(groups)}`);
    if (!groups.every((g) => g.lines === 2)) throw new Error(`dibujo de los rayados: ${JSON.stringify(groups)}`);
    // Corte por una línea vertical en el frente: su vista va a un costado, alineada (sin la lateral ni la isométrica, hay lugar)
    await b.eval(`window.__cadStore.commit((d) => { d.drawing.sheets[0].hiddenViews = ["bottom", "right", "back", "left", "iso"]; })`);
    await sleep(1500);
    await b.eval(`document.querySelector('button[aria-label="Corte"]').click()`);
    const at = (p) => b.eval(`(() => { const svg = document.querySelector("[data-sheet] svg"); const g = svg.querySelector('[data-view="front"]').getBBox(); const q = new DOMPoint(g.x + g.width * ${p[0]}, g.y + g.height * ${p[1]}).matrixTransform(svg.getScreenCTM()); return [q.x, q.y]; })()`);
    await b.click(...(await at([0.25, -0.15])), { wait: 300 });
    await b.click(...(await at([0.25, 1.15])), { wait: 300 });
    await b.key("Enter", "Enter", 13);
    for (let t = 0; t < 40 && !(await b.eval(`!!document.querySelector('[data-sheet] [data-view="corte-B"]')`)); t++) await sleep(300);
    const al = await b.eval(`(() => { const f = document.querySelector('[data-sheet] [data-view="front"]').getBBox(); const c = document.querySelector('[data-sheet] [data-view="corte-B"]').getBBox(); return { fy: f.y + f.height / 2, cy: c.y + c.height / 2, fx: f.x, cx: c.x }; })()`);
    near(al.cy, al.fy, 0.05, "la vista del corte a la altura del frente");
    // DXF con las líneas de corte, las letras y el rayado
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    const dxf = `${out}/plano_cortes.dxf`;
    rmSync(dxf, { force: true });
    await b.eval(`window.__nextPath = ${JSON.stringify(dxf)}`);
    await b.clickText("Exportar DXF");
    await sleep(1500);
    const text = existsSync(dxf) ? readFileSync(dxf, "utf8") : "";
    if (!text.includes("\nRAYADO\n") || !text.includes("\nCORTES\n")) throw new Error("el DXF no trae el rayado o los cortes");
    await b.key("Escape", "Escape", 27);
    await sleep(300);
    await b.shot("plano_material");
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
    const toolBtn = (name) => b.eval(`document.querySelector('button[aria-label=${JSON.stringify(name)}]').click()`);
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
      return { view: svg.querySelectorAll('[data-view="detalle-B"] polyline').length, mark: !!svg.querySelector('[data-detail-mark="B"]'), label: svg.querySelector('[data-detail="B"] text')?.textContent, chip: !!document.querySelector('[data-outliner-row="drawing-1:detail:B"]') && !!document.querySelector('[data-drawing-props="detail"]') };
    })()`);
    if (!(det.view > 0) || !det.mark || !det.chip || !/^Detalle B \(\d+:1\)$/.test(det.label ?? "")) throw new Error(`detalle: ${JSON.stringify(det)}`);
    if ((await call("cad_get_document")).drawing?.sheets?.[0]?.details?.length !== 1) throw new Error("no se guardó el detalle");
    // Corte A-A por el medio (Y = 5) y después en Y = 2: el rayado es el ancho del cono ahí
    const hatchW = () =>
      b.eval(`(() => {
        const ps = [...document.querySelectorAll('[data-sheet] [data-hatch="front"] polygon')].flatMap((p) => p.getAttribute("points").split(" ").map((q) => Number(q.split(",")[0])));
        return ps.length ? Math.max(...ps) - Math.min(...ps) : 0;
      })()`);
    await b.eval(`document.querySelector('button[aria-label="Corte A-A"]').click()`);
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
    // Por sus atajos (P y U): «Polígono» y «Ranura» también salen en otras listas
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "p" }))`);
    await b.click(350, 400);
    await b.click(450, 400);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "u" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
    await b.click(700, 300, { wait: 500 });
    await setInput(b, "Radio", 10);
    // Pasar el mouse por el botón: vista previa del arco, sin cambiar el sketch
    const before = await b.eval(`window.__cadUi.session().sketch.entities.length`);
    const btn = await b.eval(`(() => { const e = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Redondear esquina"); e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.mouse("mouseMoved", btn[0], btn[1], { buttons: 0 });
    await sleep(400);
    const prev = await b.eval(`window.__cadUi.panelPreview()?.length ?? 0`);
    if (!(prev > 0)) throw new Error("sin vista previa al pasar por Redondear esquina");
    if ((await b.eval(`window.__cadUi.session().sketch.entities.length`)) !== before) throw new Error("la vista previa cambió el sketch");
    await b.mouse("mouseMoved", 600, 650, { buttons: 0 });
    await sleep(300);
    if (await b.eval(`window.__cadUi.panelPreview()?.length ?? 0`)) throw new Error("la vista previa no se fue al salir");
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
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }))`);
    await b.click(680, 390);
    await b.click(730, 390);
    await b.key("Escape", "Escape", 27);
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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

  async "cotas II y restricciones II: suplementario, largo total en pulgadas, mínima entre círculos, coradiales, intersección, bloqueo, ordenadas y texto movido"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    const sketch = {
      points: [
        P(0, 0, 0), P(1, 0, 0), P(2, 20, 0), P(3, 14, 14), P(4, 60, 0), P(5, 80, 0), P(6, 100, 10), P(7, 104, 12), P(8, 54, 1),
        P(9, 0, 30), P(10, 10, 35), P(11, 0, -20), P(12, 10, -20), P(13, 10, -10), P(14, 0, -40), P(15, 7, -40), P(16, 15, -45),
      ],
      entities: [
        { id: 20, geometry: { type: "line", start: 1, end: 2 } },
        { id: 21, geometry: { type: "line", start: 1, end: 3 } },
        { id: 22, geometry: { type: "circle", center: 4, radius: 5 } },
        { id: 23, geometry: { type: "circle", center: 5, radius: 2 } },
        { id: 24, geometry: { type: "circle", center: 6, radius: 3 } },
        { id: 25, geometry: { type: "circle", center: 7, radius: 1 } },
        { id: 26, geometry: { type: "point", point: 8 } },
        { id: 27, geometry: { type: "line", start: 9, end: 10 } },
        { id: 28, geometry: { type: "line", start: 11, end: 12 } },
        { id: 29, geometry: { type: "line", start: 12, end: 13 } },
        { id: 30, geometry: { type: "point", point: 14 } },
        { id: 31, geometry: { type: "point", point: 15 } },
        { id: 32, geometry: { type: "point", point: 16 } },
      ],
      constraints: [
        { type: "fixed", point: 1, x: 0, y: 0 },
        { type: "fixed", point: 2, x: 20, y: 0 },
        { type: "fixed", point: 4, x: 60, y: 0 },
        { type: "radius", entity: 22, value: 5 },
        { type: "fixed", point: 5, x: 80, y: 0 },
        { type: "fixed", point: 6, x: 100, y: 10 },
        { type: "radius", entity: 24, value: 3 },
        { type: "fixed", point: 11, x: 0, y: -20 },
        { type: "horizontal", line: 28 },
        { type: "vertical", line: 29 },
      ],
      next_id: 40,
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
    const count = () => b.eval(`window.__cadUi.session().sketch.constraints.length`);
    /** Elige, aprieta la sugerencia y (si se pasa) escribe el valor de la última cota */
    const suggest = async (ids, label, value) => {
      await b.eval(`window.__cadUi.setSelection(${JSON.stringify(ids)})`);
      await sleep(300);
      const before = await count();
      await b.clickText(label);
      await sleep(800);
      if ((await count()) <= before) throw new Error(`${label}: no agregó nada`);
      if (value === undefined) return;
      const i = (await count()) - 1;
      const msg = await b.eval(`window.__cadUi.setConstraintText(${i}, ${JSON.stringify(String(value))})`);
      if (msg) throw new Error(`${label}: ${msg}`);
      await sleep(1000);
    };
    const pt = (id) => b.eval(`(() => { const p = window.__cadUi.session().sketch.points.find((q) => q.id === ${id}); return [p.x, p.y]; })()`);
    const radius = (id) => b.eval(`window.__cadUi.session().sketch.entities.find((e) => e.id === ${id}).geometry.radius`);
    // Suplementario de −150°: la segunda línea queda a 30°
    await suggest([20, 21], "Ángulo suplementario", -150);
    const p3 = await pt(3);
    near((Math.atan2(p3[1], p3[0]) * 180) / Math.PI, 30, 1e-5, "línea a 30°");
    // Mínima entre círculos 10: el segundo pasa a radio 5
    await suggest([22, 23], "Distancia mínima", 10);
    near(await radius(23), 5, 1e-6, "radio por la distancia mínima");
    // Coradiales: el chico se va al centro del grande con su radio
    await suggest([24, 25], "Coradiales");
    near(await radius(25), 3, 1e-6, "radio coradial");
    const c7 = await pt(7);
    near(c7[0], 100, 1e-6, "centro coradial x");
    near(c7[1], 10, 1e-6, "centro coradial y");
    // Punto en la intersección de la recta y = 0 y el círculo de (60, 0) radio 5: (55, 0)
    await suggest([8, 20, 22], "Punto en la intersección");
    const p8 = await pt(8);
    near(p8[0], 55, 1e-6, "intersección x");
    near(p8[1], 0, 1e-6, "intersección y");
    // Largo total en pulgadas: 1 in = 25,4 mm entre las dos líneas
    await suggest([28, 29], "Largo total", "1 in");
    const [a, m, z] = [await pt(11), await pt(12), await pt(13)];
    near(Math.hypot(m[0] - a[0], m[1] - a[1]) + Math.hypot(z[0] - m[0], z[1] - m[1]), 25.4, 1e-5, "largo total");
    // Bloquear una línea: no se mueve al transformarla
    await suggest([27], "Bloquear la entidad");
    const glyphs = await b.eval(`[...document.querySelectorAll("button")].filter((x) => x.textContent === "⊞").length`);
    if (glyphs < 1) throw new Error("no se ve el ícono del bloqueo");
    // Ordenadas horizontales desde el primer punto: dos cotas con el texto en fila
    await suggest([14, 15, 16], "Ordenadas horizontales");
    const ords = await b.eval(`window.__cadUi.session().sketch.constraints.filter((c) => c.opts?.ordinate).map((c) => [c.b, c.value])`);
    if (JSON.stringify(ords) !== JSON.stringify([[15, 7], [16, 15]])) throw new Error("ordenadas: " + JSON.stringify(ords));
    const r = await b.eval(`window.__cadUi.session().report`);
    if (r.status === "over_constrained" || r.status === "failed") throw new Error(`estado: ${r.status}`);
    // Nombres en las etiquetas
    const labelMode = (t) => b.eval(`[...document.querySelectorAll('[aria-label="Qué dicen las cotas"] button')].find((x) => x.textContent === ${JSON.stringify(t)})?.click()`);
    await labelMode("Nombre");
    await sleep(400);
    if (!(await b.eval(`document.body.innerText`)).includes("mín d")) throw new Error("no se ven los nombres de las cotas");
    await labelMode("Fórmula");
    await sleep(400);
    const texts = await b.eval(`document.body.innerText`);
    if (!texts.includes("1 in = 25.4")) throw new Error("no se ve la fórmula con unidades");
    // Arrastrar el texto de la cota mínima: queda corrido, con guía
    const di = await b.eval(`window.__cadUi.session().sketch.constraints.findIndex((c) => c.type === "circle_distance")`);
    const box = await b.eval(`(() => { const r = document.querySelector('[data-dim="${di}"]').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    await b.drag(box[0], box[1], box[0] + 60, box[1] - 40);
    await sleep(1200);
    const off = await b.eval(`window.__cadUi.session().sketch.constraints[${di}].opts?.offset`);
    if (!off) throw new Error("el texto no se movió");
    if ((await b.eval(`document.querySelectorAll("[data-dim-leaders] line").length`)) < 1) throw new Error("falta la guía de la cota movida");
    // Bloquear la cota: el clic ya no la edita
    await b.eval(`window.__cadUi.setDimOpts(${di}, { locked: true })`);
    await sleep(800);
    await b.eval(`document.querySelector('[data-dim="${di}"]').click()`);
    await sleep(300);
    if (await b.eval(`!!document.querySelector("input.w-20")`)) throw new Error("la cota bloqueada se pudo editar");
    await b.shot("cotas_restricciones_2_sketch");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // El documento guarda las opciones y vuelve a abrir igual
    const saved = (await call("cad_get_document")).features.find((f) => f.id === id).kind.sketch.constraints;
    const dim = saved.find((c) => c.type === "circle_distance");
    if (!dim?.opts?.locked || !dim.opts.offset) throw new Error("no se guardaron las opciones de la cota: " + JSON.stringify(dim));
    if (!saved.some((c) => c.type === "angle" && c.supplementary)) throw new Error("no se guardó el suplementario");
    await b.shot("cotas_restricciones_2");
  },

  async "sketch 3D: clics en el plano y en un vértice, ejes, largo, conflicto, perforar, barrido y sketch envuelto"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    // Después de que llegue la vista previa de lo último que se cambió
    const view3d = () => b.eval(`window.__cadStore.settled().then(() => { const d = window.__cadStore.draft(); return (window.__cadStore.result()?.sketches3d ?? []).find((v) => v.id === d?.feature); })`);
    const clickAt = async (p, wait = 1200) => b.click(...(await at(p)), { wait });
    const clickButton = async (text) => {
      const r = await b.eval(`(() => { const e = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === ${JSON.stringify(text)} && x.offsetParent !== null); if (!e) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      if (!r) throw new Error("no encontré: " + text);
      await b.click(...r, { wait: 1200 });
    };
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un vértice de arriba de la caja que se vea (y se pueda elegir)
    let v;
    for (const c of [[10, -10, 10], [-10, -10, 10], [10, 10, 10], [-10, 10, 10]]) {
      const [x, y] = await at(c);
      if ((await b.eval(`window.__cadViewer.pick(${x}, ${y}, { vertices: true })?.kind`)) === "vertex") {
        v = c;
        break;
      }
    }
    if (!v) throw new Error("no se ve ningún vértice de arriba");
    const sx = Math.sign(v[0]);
    const sy = -Math.sign(v[1]);
    const B = [v[0] + 12 * sx, v[1], 10];
    const C = [B[0], v[1] + 16 * sy, 10];
    // Todo dentro del lienzo (a la derecha está el panel)
    const canvas = await b.eval(`(() => { const r = window.__cadViewer.canvas.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; })()`);
    for (const p of [v, B, C]) {
      const [x, y] = await at(p);
      if (x < canvas[0] + 10 || x > canvas[2] - 10 || y < canvas[1] + 10 || y > canvas[3] - 10) throw new Error(`fuera del visor: ${p} → ${x}, ${y}`);
    }
    await b.clickText("Sketch 3D");
    await sleep(1500);
    // Plano activo: la planta corrida 10 (la tapa de la caja)
    if (!(await setInput(b, "Desplazado", 10))) throw new Error("falta el desplazamiento del plano activo");
    await sleep(400);
    await clickAt(v);
    await clickAt(B);
    await clickAt(C);
    await clickButton("Terminar trazo");
    let w = await view3d();
    if (!w || w.sketch.points.length !== 3 || w.sketch.entities.length !== 2) throw new Error(`trazo: ${JSON.stringify(w?.sketch)}`);
    const types = w.sketch.constraints.map((c) => c.type).sort();
    if (JSON.stringify(types) !== JSON.stringify(["along_axis", "along_axis", "attach"])) throw new Error(`restricciones solas: ${types}`);
    if (w.report.dof !== 2) throw new Error(`grados: ${w.report.dof}`);
    // Largos: elegir cada línea en el visor y acotarla
    await b.eval(`[...document.querySelectorAll('[aria-label="Herramienta del sketch 3D"] [role=radio]')].find((e) => e.textContent === "Elegir").click()`);
    await sleep(300);
    const mid = (p, q) => p.map((x, i) => (x + q[i]) / 2);
    const solved = async (i) => {
      const s = (await view3d()).sketch.points[i];
      return [s.x, s.y, s.z];
    };
    const setLength = async (i, j, value) => {
      const m = mid(await solved(i), await solved(j));
      await clickAt(m, 600);
      const sel = await b.eval(`window.__cadUi.sel3d()`);
      if (sel.length !== 1) throw new Error(`elegir la línea ${i}-${j}: ${JSON.stringify(sel)}`);
      await clickButton("Largo");
      const ok = await b.eval(`(() => {
        const rows = [...document.querySelectorAll('[data-constraint3d="length"]')];
        const i = rows[rows.length - 1]?.querySelector("input");
        if (!i) return false;
        i.value = "${value}";
        i.dispatchEvent(new Event("change", { bubbles: true }));
        return true;
      })()`);
      if (!ok) throw new Error("no está el largo en la lista");
      await sleep(1500);
    };
    await setLength(0, 1, 14);
    await setLength(1, 2, 18);
    w = await view3d();
    if (w.report.status !== "well_constrained") throw new Error(`definido: ${JSON.stringify(w.report)}`);
    const pt = (i) => [w.sketch.points[i].x, w.sketch.points[i].y, w.sketch.points[i].z];
    const end = pt(2);
    near(end[0], v[0] + 14 * sx, 1e-6, "fin x");
    near(end[1], v[1] + 18 * sy, 1e-6, "fin y");
    near(end[2], 10, 1e-6, "fin z");
    // Paralela a Y en la primera: choca; lo demás sigue y se marca
    await clickAt(mid(v, pt(1)), 600);
    await clickButton("Paralela a Y");
    await sleep(800);
    w = await view3d();
    if (w.report.status !== "over_constrained" || w.report.conflicting.length !== 1) throw new Error(`conflicto: ${JSON.stringify(w.report)}`);
    if (!(await b.eval(`document.querySelector("[data-sketch3d-status]").textContent.includes("conflicto")`))) throw new Error("no avisa el conflicto");
    await b.eval(`[...document.querySelectorAll('[data-constraint3d]')].at(-1).querySelector('[aria-label="Quitar"]').click()`);
    await sleep(1500);
    w = await view3d();
    if (w.report.status !== "well_constrained") throw new Error(`sin el conflicto: ${w.report.status}`);
    await b.shot("sketch3d");
    await accept(b);
    let r = await evaluate();
    const path = r.references.find((x) => x.kind === "curves");
    if (!path || path.lines.length !== 2) throw new Error(`curvas del sketch 3D: ${JSON.stringify(path?.kind)}`);
    const pathId = path.id;
    // Perfil en el plano lateral corrido hasta el vértice (x = v.x): círculo cuyo centro perfora el camino
    const doc = await call("cad_get_document");
    const prof = {
      points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 3, y: 3 }],
      entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 1.5 } }],
      constraints: [{ type: "radius", entity: 2, value: 1.5 }],
      next_id: 3,
      origin: 0,
    };
    const profId = doc.next_id;
    doc.features.push({ id: profId, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "yz" }, offset: v[0], sketch: prof } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadUi.editSketch(${profId})`);
    await sleep(1000);
    await b.eval(`window.__cadUi.setSelection([1])`);
    await sleep(400);
    const name = doc.features.find((f) => f.id === pathId).name;
    const done = await b.eval(`(() => { const e = [...document.querySelectorAll('[aria-label="Perforar"] button')].find((x) => x.textContent.trim() === ${JSON.stringify(name)}); e?.click(); return !!e; })()`);
    if (!done) {
      await b.shot("sketch3d-perforar");
      throw new Error("falta «Perforar» con un punto elegido");
    }
    await sleep(1200);
    const c = await b.eval(`window.__cadUi.session().sketch.points.find((p) => p.id === 1)`);
    // En el plano lateral: x del sketch = Y del mundo, y = Z
    near(c.x, v[1], 1e-4, "perforación en Y");
    near(c.y, 10, 1e-4, "perforación en Z");
    await b.eval(`window.__cadUi.finishSketch()`);
    await sleep(1500);
    const doc2 = await call("cad_get_document");
    doc2.features.push({ id: doc2.next_id, name: "Tubo", suppressed: false, kind: { type: "sweep", sketch: profId, regions: { type: "all" }, path: { type: "curve", feature: pathId }, op: "new" } });
    doc2.next_id += 1;
    await call("cad_set_document", { document: doc2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    r = await evaluate();
    const bad = r.status.filter((x) => x.state.type === "error");
    if (bad.length) throw new Error(`barrido: ${JSON.stringify(bad)}`);
    const tube = Math.PI * 1.5 * 1.5;
    if (!(r.body.volume > 8000 + tube * 28 && r.body.volume < 8000 + tube * 32.5)) throw new Error(`volumen con el tubo: ${r.body.volume}`);
    // Sketch envuelto sobre la tapa de la caja: una recta de 10 queda a z = 10
    await b.clickText("Sketch envuelto");
    await sleep(600);
    await clickAt([2, 2, 10], 2500);
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error("no abrió el sketch envuelto");
    await b.eval(`window.__cadUi.change((s) => { const a = window.__cadUi.addPoint(s, [-5, 0]); const c = window.__cadUi.addPoint(s, [5, 0]); window.__cadUi.addEntity(s, { type: "line", start: a, end: c }); })`);
    await sleep(800);
    await b.eval(`window.__cadUi.finishSketch()`);
    await sleep(2000);
    r = await evaluate();
    const wrapped = r.references.filter((x) => x.kind === "curves").at(-1);
    if (!wrapped || wrapped.id === pathId || wrapped.lines.length !== 1) throw new Error(`envuelto: ${JSON.stringify(wrapped && { id: wrapped.id, n: wrapped.lines.length })}`);
    for (const p of wrapped.lines[0]) near(p[2], 10, 1e-6, "sobre la tapa");
    const L = wrapped.lines[0].slice(1).reduce((acc, p, i) => acc + Math.hypot(...p.map((x, k) => x - wrapped.lines[0][i][k])), 0);
    near(L, 10, 1e-3, "largo sobre la tapa");
  },

  async "solver II: grados libres, sugerir, definir todo, cambio grande sin dar vuelta y conflicto parcial"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    // Rectángulo dibujado un poco torcido con una esquina en el origen, y un círculo suelto
    const sketch = {
      points: [P(0, 0, 0), P(1, 20, 0.25), P(2, 20.3, 10), P(3, -0.2, 10.1), P(4, 8, 5)],
      entities: [
        { id: 10, geometry: { type: "line", start: 0, end: 1 } },
        { id: 11, geometry: { type: "line", start: 1, end: 2 } },
        { id: 12, geometry: { type: "line", start: 2, end: 3 } },
        { id: 13, geometry: { type: "line", start: 3, end: 0 } },
        { id: 14, geometry: { type: "circle", center: 4, radius: 2.5 } },
      ],
      constraints: [],
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
    const report = () => b.eval(`window.__cadUi.session().report`);
    const pt = (id) => b.eval(`(() => { const p = window.__cadUi.session().sketch.points.find((q) => q.id === ${id}); return [p.x, p.y]; })()`);
    let r = await report();
    if (r.status !== "under_constrained") throw new Error(`sin restricciones: ${r.status}`);
    if (!(r.free_radius ?? []).includes(14)) throw new Error("el radio del círculo está libre");
    if (!(r.entity_dof ?? []).some(([e, n]) => e === 10 && n === 2)) throw new Error(`la línea del origen tiene 2 grados libres: ${JSON.stringify(r.entity_dof)}`);
    if (!(await b.eval(`!!document.querySelector('button[aria-label="Mostrar libres"]')`))) throw new Error("falta el botón de las flechas");
    // Lo elegido dice cuántos grados le faltan
    await b.eval(`window.__cadUi.setSelection([10])`);
    await sleep(300);
    const dof = await b.eval(`document.querySelector("[data-entity-dof]")?.getAttribute("data-entity-dof")`);
    if (dof !== "2") throw new Error(`grados de lo elegido: ${dof}`);
    await b.eval(`window.__cadUi.setSelection([])`);
    await sleep(300);
    await b.shot("solver-ii-libres");
    await sleep(200);
    // Sugerencias: las cuatro casi horizontales/verticales, sin paralelas que sobran
    await b.clickText("Sugerir restricciones");
    await sleep(1200);
    const kinds = await b.eval(`[...document.querySelectorAll("[data-suggestion]")].map((e) => e.dataset.suggestion)`);
    for (const k of ["horizontal", "vertical"]) {
      if (kinds.filter((x) => x === k).length !== 2) throw new Error(`sugerencias ${k}: ${kinds}`);
    }
    if (kinds.includes("parallel") || kinds.includes("perpendicular")) throw new Error(`sugiere lo que sobra: ${kinds}`);
    await b.clickContains("Agregar todas");
    await sleep(1500);
    near((await pt(1))[1], 0, 1e-6, "la de abajo quedó horizontal");
    near((await pt(2))[0], (await pt(1))[0], 1e-6, "la derecha quedó vertical");
    // Definir todo: totalmente definido
    await b.clickText("Definir todo");
    await sleep(1500);
    r = await report();
    if (r.status !== "well_constrained") throw new Error(`después de definir todo: ${r.status} (${r.dof})`);
    // Cambio grande del ancho (×8): llega en pasos y no se da vuelta
    const iLen = await b.eval(`window.__cadUi.session().sketch.constraints.findIndex((c) => c.type === "length" && c.line === 10)`);
    if (iLen < 0) throw new Error("falta el largo de abajo");
    const msg = await b.eval(`window.__cadUi.setConstraintText(${iLen}, "160")`);
    if (msg) throw new Error(msg);
    await sleep(1500);
    near((await pt(1))[0], 160, 1e-5, "ancho nuevo");
    near((await pt(2))[0], 160, 1e-5, "esquina de arriba");
    if (!((await pt(2))[1] > 5)) throw new Error(`se dio vuelta: ${await pt(2)}`);
    // Conflicto: un segundo largo en la derecha; lo demás se cumple entero
    await b.eval(`window.__cadUi.change((s) => s.constraints.push({ type: "length", line: 11, value: 30 }))`);
    await sleep(1500);
    r = await report();
    if (r.status !== "over_constrained" || !r.partial || r.conflicting.length === 0) throw new Error(`conflicto: ${JSON.stringify({ s: r.status, p: r.partial, c: r.conflicting })}`);
    near((await pt(1))[1], 0, 1e-6, "sigue horizontal con el conflicto");
    near((await pt(2))[0], 160, 1e-5, "sigue vertical con el conflicto");
    const text = await b.eval(`document.body.innerText`);
    if (!text.includes("lo demás se cumple")) throw new Error("el panel no dice que lo demás se cumple");
    await b.shot("solver-ii");
  },

  async "edición II: recortar arrastrando, unir, chaflán, equidistante a los dos lados, estirar, duplicados, reparar y cerrar"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    const L = (id, start, end) => ({ id, geometry: { type: "line", start, end } });
    const sketch = {
      points: [
        P(0, 0, 0),
        // Numeral: dos horizontales y dos verticales cruzadas
        P(1, 0, 30), P(2, 40, 30), P(3, 0, 50), P(4, 40, 50), P(5, 10, 20), P(6, 10, 60), P(7, 30, 20), P(8, 30, 60),
        // Dos líneas alineadas seguidas
        P(9, 60, 0), P(10, 70, 0), P(11, 85, 0),
        // Esquina para el chaflán
        P(12, 0, -30), P(13, 20, -30), P(14, 20, -10),
        // Cadena abierta para el equidistante
        P(15, 50, -40), P(16, 70, -40), P(17, 70, -20),
        // Línea repetida (al revés, con sus propios puntos)
        P(18, 100, 0), P(19, 110, 0), P(20, 110, 0), P(21, 100, 0),
        // Triángulo con un extremo corrido 0,01
        P(22, 100, 20), P(23, 120, 20), P(24, 120.01, 20), P(25, 110, 35),
      ],
      entities: [
        L(40, 1, 2), L(41, 3, 4), L(42, 5, 6), L(43, 7, 8),
        L(44, 9, 10), L(45, 10, 11),
        L(46, 12, 13), L(47, 13, 14),
        L(48, 15, 16), L(49, 16, 17),
        L(50, 18, 19), L(51, 20, 21),
        L(52, 22, 23), L(53, 24, 25), L(54, 25, 22),
      ],
      constraints: [{ type: "horizontal", line: 45 }],
      next_id: 60,
      origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Edición", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    const id = doc.next_id;
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadUi.editSketch(${id})`);
    await sleep(1500);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    const pos = (s, id) => {
      const p = s.points.find((q) => q.id === id);
      return [p.x, p.y];
    };
    const ends = (s, e) => [pos(s, e.geometry.start), pos(s, e.geometry.end)];
    const select = async (ids) => {
      await b.eval(`window.__cadUi.setTool("select"); window.__cadUi.setSelection(${JSON.stringify(ids)})`);
      await sleep(400);
    };

    // Recortar arrastrando: un trazo por x = 20 quita el tramo del medio de las dos horizontales
    await b.key("t", "KeyT");
    const [x0, y0] = await at(20, 24);
    const [x1, y1] = await at(20, 56);
    await b.drag(x0, y0, x1, y1, 12);
    await sleep(800);
    let s = await sk();
    const horiz = s.entities.filter((e) => e.geometry.type === "line" && [30, 50].includes(Math.round(pos(s, e.geometry.start)[1])) && Math.abs(pos(s, e.geometry.start)[1] - pos(s, e.geometry.end)[1]) < 1e-9);
    if (horiz.length !== 4) throw new Error(`trozos de horizontal después del trazo: ${horiz.length}`);
    if (horiz.some((e) => Math.min(...ends(s, e).map((p) => p[0])) < 20 && Math.max(...ends(s, e).map((p) => p[0])) > 20)) throw new Error("quedó algo cruzando x = 20");
    // Un clic sigue recortando un solo tramo: la punta de arriba de la vertical izquierda
    await b.click(...(await at(10, 57)), { wait: 800 });
    s = await sk();
    if (s.entities.some((e) => e.geometry.type === "line" && ends(s, e).some((p) => p[0] === 10 && p[1] === 60))) throw new Error("el clic no recortó la punta");

    // Unir las dos alineadas
    await select([44, 45]);
    await b.clickText("Unir");
    await sleep(900);
    s = await sk();
    const joined = s.entities.find((e) => e.id === 44);
    if (s.entities.some((e) => e.id === 45) || JSON.stringify(ends(s, joined)) !== "[[60,0],[85,0]]") throw new Error(`unir: ${JSON.stringify(joined && ends(s, joined))}`);
    if (!s.constraints.some((c) => c.type === "horizontal" && c.line === 44)) throw new Error("la horizontal no pasó a la unida");

    // Chaflán de 4 mm en la esquina (20, -30)
    await select([13]);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Chaflán de la esquina"] input'); i.focus(); })()`);
    await b.eval(`(() => { const i = document.querySelector('[aria-label="Chaflán de la esquina"] input'); const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set; set.call(i, "4"); i.dispatchEvent(new Event("input", { bubbles: true })); i.dispatchEvent(new Event("change", { bubbles: true })); i.blur(); })()`);
    await sleep(300);
    await b.clickText("Achaflanar");
    await sleep(900);
    s = await sk();
    const chamfer = s.entities.find((e) => e.geometry.type === "line" && JSON.stringify(ends(s, e).map((p) => p.map((v) => +v.toFixed(6)))) === "[[16,-30],[20,-26]]");
    if (!chamfer) throw new Error("no está el chaflán de 4 mm: " + JSON.stringify(s.entities.filter((e) => e.id > 59).map((e) => ends(s, e))));

    // Equidistante a los dos lados con extremos redondos: contorno cerrado alrededor de la cadena
    await select([48, 49]);
    await b.clickText("A los dos lados");
    await b.clickText("Extremos abiertos");
    await sleep(300);
    await b.clickText("Extremos redondos");
    await sleep(300);
    const before = (await sk()).entities.length;
    // El botón del panel (la barra izquierda también tiene la herramienta «Equidistante»)
    await b.eval(`[...document.querySelectorAll("button[data-preview]")].find((x) => x.textContent.trim() === "Equidistante").click()`);
    await sleep(1000);
    s = await sk();
    const made = s.entities.slice(before);
    if (made.length !== 6 || made.filter((e) => e.geometry.type === "arc").length !== 2) throw new Error(`equidistante: ${made.map((e) => e.geometry.type)}`);
    const { checkSketch } = await import("../src/lib/sketchCheck.ts");
    if (checkSketch({ ...s, entities: made }).looseEnds.length) throw new Error("el contorno equidistante quedó abierto");
    if (!(await b.eval(`window.__cadUi.session().regions.length`))) throw new Error("el contorno equidistante no forma región");

    // Estirar: caja sobre la punta derecha de la unida y 10 mm a la derecha
    await select([]);
    await b.key("w", "KeyW");
    const [bx0, by0] = await at(82, 3);
    const [bx1, by1] = await at(88, -3);
    await b.drag(bx0, by0, bx1, by1, 6);
    await sleep(400);
    const taken = await b.eval(`window.__cadUi.selection()`);
    if (taken.length !== 1) throw new Error(`puntos tomados por la caja: ${taken}`);
    await b.click(...(await at(85, 0)), { wait: 400 });
    await b.click(...(await at(95, 0)), { wait: 1000 });
    s = await sk();
    const stretched = s.entities.find((e) => e.id === 44);
    const xs = ends(s, stretched).map((p) => +p[0].toFixed(3)).sort((a, c) => a - c);
    if (xs[0] !== 60 || Math.abs(xs[1] - 95) > 0.5) throw new Error(`estirar: ${xs}`);

    // Eliminar duplicados: la línea repetida se va
    await select([]);
    await b.clickText("Eliminar duplicados");
    await sleep(900);
    s = await sk();
    if (s.entities.some((e) => e.id === 51) || !s.entities.some((e) => e.id === 50)) throw new Error("duplicados: sigue la repetida");

    // Reparar: el triángulo cierra y forma región
    const regionsBefore = await b.eval(`window.__cadUi.session().regions.length`);
    await b.clickText("Reparar");
    await sleep(1200);
    s = await sk();
    if (s.points.some((p) => p.id === 24) && s.points.some((p) => p.id === 23)) throw new Error("reparar: los extremos del triángulo siguen separados");
    const regionsAfter = await b.eval(`window.__cadUi.session().regions.length`);
    if (!(regionsAfter > regionsBefore)) throw new Error(`reparar: regiones ${regionsBefore} → ${regionsAfter}`);

    // Cerrar contorno sobre una U suelta (lo demás borrado)
    await b.eval(`window.__cadUi.change((s) => {
      s.entities = []; s.constraints = []; s.points = s.points.filter((p) => p.id === 0);
      s.points.push({ id: 200, x: 0, y: 0.5 }, { id: 201, x: 20, y: 0.5 }, { id: 202, x: 20, y: 10 }, { id: 203, x: 0, y: 10 });
      s.entities.push({ id: 204, geometry: { type: "line", start: 200, end: 201 } }, { id: 205, geometry: { type: "line", start: 201, end: 202 } }, { id: 206, geometry: { type: "line", start: 202, end: 203 } });
      s.next_id = 210;
    })`);
    await sleep(900);
    await b.clickText("Cerrar contorno");
    await sleep(1200);
    if ((await b.eval(`window.__cadUi.session().regions.length`)) !== 1) throw new Error("cerrar contorno: no quedó una región");
    await b.shot("edicion-ii");
  },

  async "asistencia al dibujo: coordenadas escritas, polilínea línea-arco, rejilla sin inferencias, simetría al dibujar y línea central"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const type = async (text) => {
      await b.eval(`(() => {
        const i = document.querySelector('[aria-label="Coordenadas del próximo punto"]');
        i.value = ${JSON.stringify(text)};
        i.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      })()`);
      await sleep(700);
    };
    const key = async (k) => {
      await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)}, bubbles: true }))`);
      await sleep(300);
    };
    const pos = (s, id) => {
      const p = s.points.find((q) => q.id === id);
      return [+p.x.toFixed(6), +p.y.toFixed(6)];
    };
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    await b.eval(`window.__cadUi.setTool("line")`);
    // Coordenadas: absoluta (el origen), relativa y polar
    await type("0, 0");
    await type("@20, 0");
    await type("@10<90");
    let s = await sk();
    const lines = s.entities.filter((e) => e.geometry.type === "line");
    const segs = lines.map((e) => [pos(s, e.geometry.start), pos(s, e.geometry.end)]);
    if (JSON.stringify(segs) !== "[[[0,0],[20,0]],[[20,0],[20,10]]]") throw new Error(`coordenadas: ${JSON.stringify(segs)}`);
    if (lines[0].geometry.start !== s.origin) throw new Error("el primer punto no tomó el origen");
    // A: el próximo tramo es un arco tangente (de (20,10) hacia arriba, hasta (10,20): centro (10,10))
    await key("a");
    await type("@-10, 10");
    s = await sk();
    const arc = s.entities.find((e) => e.geometry.type === "arc");
    if (!arc || JSON.stringify(pos(s, arc.geometry.center)) !== "[10,10]") throw new Error("arco tangente: " + JSON.stringify(arc && pos(s, arc.geometry.center)));
    if (!s.constraints.some((c) => c.type === "tangent" && c.b === arc.id)) throw new Error("el arco no quedó tangente");
    // Vuelve a línea solo
    await type("@-10, 0");
    s = await sk();
    if (s.entities.filter((e) => e.geometry.type === "line").length !== 3) throw new Error("después del arco no siguió con línea");
    await key("Escape");

    // Sin inferencias y anclado a la rejilla de 5: un punto cerca de (31,2) cae en (30,0)
    await b.eval(`window.__cadUi.setAssist({ infer: false, grid: true, gridSnap: true, spacing: 5 })`);
    await sleep(500);
    if ((await b.eval(`document.querySelector('[aria-label="Inferencias"]').getAttribute("aria-pressed")`)) !== "false") throw new Error("el botón de inferencias no se apagó");
    await b.eval(`window.__cadUi.setTool("point")`);
    await b.click(...(await at(31.2, 1.9)), { wait: 800 });
    s = await sk();
    const pt = s.entities.find((e) => e.geometry.type === "point");
    if (!pt || JSON.stringify(pos(s, pt.geometry.point)) !== "[30,0]") throw new Error("punto en la rejilla: " + JSON.stringify(pt && pos(s, pt.geometry.point)));
    await b.shot("asistencia-rejilla");
    await b.eval(`window.__cadUi.setAssist({ infer: true, grid: false, gridSnap: false })`);

    // Línea central vertical por x = 40 y simetría al dibujar en ella
    await b.eval(`window.__cadUi.setTool("centerline")`);
    await type("40, -10");
    await type("40, 40");
    s = await sk();
    const axis = s.entities.find((e) => e.axis);
    if (!axis || !axis.construction) throw new Error("no hay línea central");
    await b.eval(`window.__cadUi.setTool("select"); window.__cadUi.setSelection([${axis.id}])`);
    await sleep(400);
    await b.clickContains("Simetría al dibujar");
    await sleep(300);
    await b.eval(`window.__cadUi.setTool("line")`);
    await type("35, 30");
    await type("25, 30");
    await key("Escape");
    s = await sk();
    const mirrored = s.entities.filter((e) => e.geometry.type === "line" && !e.construction).map((e) => [pos(s, e.geometry.start), pos(s, e.geometry.end)]);
    if (!mirrored.some(([p, q]) => JSON.stringify([p, q].sort()) === "[[45,30],[55,30]]")) throw new Error("no salió reflejada: " + JSON.stringify(mirrored));
    if (s.constraints.filter((c) => c.type === "symmetric" && c.line === axis.id).length !== 2) throw new Error("faltan las simetrías");
    // Diámetro respecto de la línea central sin elegirla: el punto (25, 30) está a 15 → 30
    const far = s.points.find((p) => Math.abs(p.x - 25) < 1e-6 && Math.abs(p.y - 30) < 1e-6);
    await b.eval(`window.__cadUi.setSymmetryAxis(undefined); window.__cadUi.setTool("select"); window.__cadUi.setSelection([${far.id}])`);
    await sleep(400);
    await b.clickText("Diámetro respecto de la línea central");
    await sleep(900);
    s = await sk();
    const d = s.constraints.find((c) => c.type === "axis_diameter" && c.line === axis.id);
    if (!d || Math.abs(d.value - 30) > 1e-6) throw new Error("diámetro al eje: " + JSON.stringify(d));
    await b.shot("asistencia");
  },

  async "plano del sketch: horizontal según un eje y normal invertida sin mover el dibujo (ni las regiones ni los textos), corte y modelo oculto al dibujar"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    const L = (id, start, end) => ({ id, geometry: { type: "line", start, end } });
    const sketch = {
      points: [P(0, 0, 0), P(1, 20, 0), P(2, 20, 4), P(3, 0, 4)],
      entities: [L(10, 0, 1), L(11, 1, 2), L(12, 2, 3), L(13, 3, 0)],
      constraints: [{ type: "horizontal", line: 10 }, { type: "vertical", line: 11 }],
      next_id: 20,
      origin: 0,
    };
    const sid = doc.next_id;
    doc.features.push({ id: sid, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    // Solo la región del rectángulo (elegida por un punto): el texto no se extruye
    doc.features.push({ id: sid + 1, name: "Bloque", suppressed: false, kind: { type: "extrude", sketch: sid, regions: { type: "points", points: [[10, 2]] }, extent: { type: "blind", distance: 5 }, reverse: false, op: "join", draft: 0 } });
    doc.next_id += 2;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const box = async () => {
      const bd = await body();
      return [...bd.bbox_min, ...bd.bbox_max].map((v) => +v.toFixed(4));
    };
    if (JSON.stringify(await box()) !== "[0,0,0,20,4,5]") throw new Error("bloque al empezar: " + (await box()));
    // Un texto arriba del rectángulo
    await b.eval(`window.__cadUi.editSketch(${sid})`);
    await sleep(1200);
    await b.clickText("Texto");
    await b.eval(`(() => {
      const i = document.querySelector('input[aria-label="Texto"]');
      i.value = "Fa";
      i.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([2, 8, 0])`)), { wait: 2000 });
    // Caja del texto en el mundo (con el plano del sketch abierto)
    const textBox = () =>
      b.eval(`(() => {
        const s = window.__cadUi.session(); const t = s.sketch.texts[0]; const P = s.plane;
        const n = P.normal, x = P.x_dir, y = [n[1] * x[2] - n[2] * x[1], n[2] * x[0] - n[0] * x[2], n[0] * x[1] - n[1] * x[0]];
        const ids = new Set(t.points);
        const w = s.sketch.points.filter((p) => ids.has(p.id)).map((p) => [0, 1, 2].map((i) => P.origin[i] + x[i] * p.x + y[i] * p.y));
        const lo = [0, 1, 2].map((i) => Math.min(...w.map((q) => q[i]))), hi = [0, 1, 2].map((i) => Math.max(...w.map((q) => q[i])));
        return [...lo, ...hi].map((v) => +v.toFixed(4));
      })()`);
    const text0 = await textBox();
    if (!(text0[4] > 8)) throw new Error("texto: " + text0);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    if (JSON.stringify(await box()) !== "[0,0,0,20,4,5]") throw new Error("el texto se extruyó: " + (await box()));

    // Horizontal según el eje Y: el dibujo queda en su lugar (girado al revés en el sketch)
    await clickRow(b, "Perfil");
    await b.clickText("La del plano");
    await sleep(300);
    await b.clickText("Eje Y");
    await sleep(1500);
    await accept(b, 2000);
    if (JSON.stringify(await box()) !== "[0,0,0,20,4,5]") throw new Error("con la horizontal en Y se movió: " + (await box()));
    let saved = (await call("cad_get_document")).features.find((f) => f.id === sid).kind.sketch;
    if (saved.x_axis?.type !== "y") throw new Error("no se guardó la horizontal");
    // La de abajo era horizontal: con x a lo largo de Y del mundo pasa a vertical
    if (!saved.constraints.some((c) => c.type === "vertical" && c.line === 10)) throw new Error("las horizontales no pasaron a verticales: " + JSON.stringify(saved.constraints));
    // Invertir la normal: el dibujo queda en el mismo lugar y la extrusión va hacia −Z
    await clickRow(b, "Perfil");
    // Seguía elegida: el clic la soltó
    if (!(await b.eval(`[...document.querySelectorAll("label")].some((l) => l.textContent === "Invertir la normal" && l.offsetParent !== null)`))) await clickRow(b, "Perfil");
    await b.clickText("Invertir la normal");
    await sleep(1500);
    await accept(b, 2000);
    const bb2 = await box();
    if (JSON.stringify(bb2) !== "[0,0,-5,20,4,0]") throw new Error("con la normal invertida: " + bb2);
    saved = (await call("cad_get_document")).features.find((f) => f.id === sid).kind.sketch;
    if (!saved.flip_normal || !saved.texts?.[0]?.style?.frame) throw new Error("no se guardó: " + JSON.stringify({ f: saved.flip_normal, t: saved.texts?.[0]?.style }));

    // Dibujando: corte en el plano y modelo oculto; el texto rehecho queda igual que al principio
    await b.eval(`window.__cadUi.setAssist({ section: true, hideModel: true })`);
    await b.eval(`window.__cadUi.editSketch(${sid})`);
    await sleep(1200);
    const state = () => b.eval(`({ section: !!window.__cadViewer.section, visible: window.__cadViewer.body?.visible })`);
    let st = await state();
    if (!st.section || st.visible !== false) throw new Error("dibujando: " + JSON.stringify(st));
    const text1 = await textBox();
    if (text1.some((v, i) => Math.abs(v - text0[i]) > 1e-3)) throw new Error(`el texto se movió: ${text0} → ${text1}`);
    const anchor = await b.eval(`window.__cadUi.session().sketch.texts[0].anchor`);
    await b.eval(`window.__cadUi.setTool("select"); window.__cadUi.setSelection([${anchor}])`);
    await sleep(400);
    await b.clickText("Rehacer texto");
    await sleep(2000);
    const text2 = await textBox();
    if (text2.some((v, i) => Math.abs(v - text0[i]) > 1e-3)) throw new Error(`el texto rehecho quedó distinto: ${text0} → ${text2}`);
    await b.shot("plano-sketch");
    await b.eval(`window.__cadUi.cancelSketch()`);
    await sleep(1500);
    st = await state();
    if (st.section || st.visible !== true) throw new Error("al salir: " + JSON.stringify(st));
    await b.eval(`window.__cadUi.setAssist({ section: false, hideModel: false })`);
  },

  async "archivos y fórmulas: curva por ecuación con parámetro, CSV, DXF importado y exportado, SVG y calco calibrado"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.addParameter("amp", "5")`);
    await sleep(1200);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    // Archivos: el selector del sistema se reemplaza por el archivo que prepara la prueba
    await b.eval(`(() => {
      const orig = HTMLInputElement.prototype.click;
      HTMLInputElement.prototype.click = function () {
        if (this.type === "file" && window.__nextFile) {
          const dt = new DataTransfer();
          dt.items.add(window.__nextFile);
          window.__nextFile = undefined;
          this.files = dt.files;
          this.dispatchEvent(new Event("change"));
          return;
        }
        return orig.call(this);
      };
    })()`);
    const giveFile = (name, content, type = "text/plain") => b.eval(`window.__nextFile = new File([${JSON.stringify(content)}], ${JSON.stringify(name)}, { type: ${JSON.stringify(type)} })`);
    const setField = (label, value) =>
      b.eval(`(() => {
        const i = document.querySelector('input[aria-label=${JSON.stringify(label)}]');
        i.value = ${JSON.stringify(value)};
        i.dispatchEvent(new Event("change", { bubbles: true }));
      })()`);
    // y = amp·sin(x/5) de 0 a 30: la spline pasa por los picos de altura 5 (el parámetro)
    await setField("y(x)", "amp * sin(x / 5)");
    await setInput(b, "x desde", 0);
    await setInput(b, "hasta", 30);
    await setInput(b, "Puntos", 31);
    await sleep(300);
    await b.clickText("Agregar curva");
    await sleep(1200);
    let s = await sk();
    const curve = s.entities.find((e) => e.geometry.type === "spline");
    if (!curve || curve.geometry.points.length !== 31) throw new Error("curva por ecuación: " + JSON.stringify(curve?.geometry.points.length));
    const ys = curve.geometry.points.map((id) => s.points.find((p) => p.id === id).y);
    near(Math.max(...ys), 5 * Math.sin(8 / 5), 0.2, "pico");
    // CSV con punto y coma y coma decimal, como polilínea
    await b.clickText("Como polilínea");
    await giveFile("perfil.csv", "x;y\n0;-10\n10;-10\n10;-15,5\n0;-15,5\n");
    await b.clickText("Curva desde archivo…");
    await sleep(1200);
    s = await sk();
    if (!s.points.some((p) => p.x === 10 && p.y === -15.5)) throw new Error("CSV: falta (10; -15,5)");
    // DXF: un cuadrado cerrado de 20 en (40, 0) en una polilínea: una región más
    const regions0 = await b.eval(`window.__cadUi.session().regions.length`);
    const dxf = ["0", "SECTION", "2", "ENTITIES", "0", "LWPOLYLINE", "90", "4", "70", "1", "10", "40", "20", "0", "10", "60", "20", "0", "10", "60", "20", "20", "10", "40", "20", "20", "0", "ENDSEC", "0", "EOF"].join("\n");
    await giveFile("cuadrado.dxf", dxf);
    await b.clickText("Importar DXF…");
    await sleep(1500);
    if ((await b.eval(`window.__cadUi.session().regions.length`)) !== regions0 + 1) throw new Error("el DXF no dio una región");
    // Exportar DXF y SVG del sketch
    const out = process.env.E2E_OUT ?? "/tmp/pinocchio-e2e";
    for (const ext of ["dxf", "svg"]) {
      const path = `${out}/sketch.${ext}`;
      rmSync(path, { force: true });
      await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
      await b.clickText(`Exportar ${ext.toUpperCase()}`);
      await sleep(1200);
      const text = existsSync(path) ? readFileSync(path, "utf8") : "";
      if (ext === "dxf" && !(text.includes("LWPOLYLINE") === false && (text.match(/\nLINE\n/g) ?? []).length >= 7 && text.includes("POLYLINE"))) throw new Error("DXF exportado raro");
      if (ext === "svg" && !text.includes("<polyline")) throw new Error("SVG exportado raro");
    }
    // Calco: imagen de 200 × 100 px, 100 mm de ancho; calibrar: lo que hay de 0 a 50 mide 100 → el doble
    const png = await b.eval(`(() => { const c = document.createElement("canvas"); c.width = 200; c.height = 100; const g = c.getContext("2d"); g.fillStyle = "#c33"; g.fillRect(0, 0, 200, 100); return c.toDataURL("image/png"); })()`);
    await b.eval(`window.__nextFile = new File([Uint8Array.from(atob(${JSON.stringify(png.split(",")[1])}), (c) => c.charCodeAt(0))], "plano.png", { type: "image/png" })`);
    await b.clickText("Agregar calco…");
    await sleep(1500);
    s = await sk();
    if (s.images?.length !== 1 || s.images[0].width !== 100 || s.images[0].height !== 50) throw new Error("calco: " + JSON.stringify(s.images?.map((i) => [i.width, i.height])));
    await b.clickText("Calibrar");
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([0, 0, 0])`)), { modifiers: 8, wait: 400 });
    await b.click(...(await b.eval(`window.__cadViewer.screenOf([50, 0, 0])`)), { modifiers: 8, wait: 600 });
    await setInput(b, "Eso mide", 100);
    await sleep(300);
    await b.clickText("Aplicar la medida");
    await sleep(1000);
    s = await sk();
    near(s.images[0].width, 200, 1e-3, "ancho calibrado");
    near(s.images[0].at[0], 0, 1e-3, "esquina");
    await b.shot("archivos-formulas");
    // Se guarda con el sketch
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const saved = (await call("cad_get_document")).features.find((f) => f.kind.type === "sketch").kind.sketch;
    if (!(Math.abs((saved.images?.[0]?.width ?? 0) - 200) < 1e-3)) throw new Error("el calco no se guardó");
  },

  async "organización: bloques con copias que cambian juntas, capas ocultas y bloqueadas, pegar el sketch en otro plano y validar la revolución"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = (id, x, y) => ({ id, x, y });
    const L = (id, start, end) => ({ id, geometry: { type: "line", start, end } });
    const sketch = {
      points: [P(0, 0, 0), P(1, 0, 0.0001), P(2, 10, 0), P(3, 10, 6), P(4, 0, 6), P(5, 0, 20), P(6, 30, 20)],
      entities: [L(10, 1, 2), L(11, 2, 3), L(12, 3, 4), L(13, 4, 1), L(14, 5, 6)],
      constraints: [{ type: "horizontal", line: 10 }],
      next_id: 30,
      origin: 0,
    };
    const sid = doc.next_id;
    doc.features.push({ id: sid, name: "Piezas", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.eval(`window.__cadUi.editSketch(${sid})`);
    await sleep(1200);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    // Bloque con el rectángulo (origen: su esquina de abajo a la izquierda)
    await b.eval(`window.__cadUi.setTool("select"); window.__cadUi.setSelection([10, 11, 12, 13, 1])`);
    await sleep(400);
    await b.eval(`(() => { const i = document.querySelector('input[aria-label="Nombre del bloque"]'); i.value = "Placa"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
    await b.clickText("Hacer bloque");
    await sleep(900);
    let s = await sk();
    if (s.blocks?.length !== 1 || s.texts?.[0]?.block !== s.blocks[0].id) throw new Error("bloque: " + JSON.stringify({ b: s.blocks?.length, t: s.texts }));
    // Insertar una copia con el origen en (40, 0)
    await b.clickText("Insertar");
    await b.click(...(await at(40, 0)), { modifiers: 8, wait: 900 });
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
    s = await sk();
    if (s.texts.length !== 2) throw new Error("copias: " + s.texts.length);
    if (!s.points.some((p) => Math.abs(p.x - 50) < 0.1 && Math.abs(p.y - 6) < 0.1)) throw new Error("la copia no está en (40, 0)");
    // Editar el bloque: 10 → 14 de ancho; las dos copias cambian
    await b.clickText("Editar");
    await sleep(900);
    if (!(await b.eval(`!!window.__cadUi.blockEdit()`))) throw new Error("no abrió el bloque");
    await b.eval(`window.__cadUi.change((k) => { for (const p of k.points) if (Math.abs(p.x - 10) < 1e-6) p.x = 14; })`);
    await sleep(800);
    await b.clickText("Terminar bloque");
    await sleep(1200);
    s = await sk();
    // La copia se insertó con un clic (el origen cae a centésimas de 40)
    const anchor2 = s.points.find((p) => p.id === s.texts[1].anchor);
    for (const x of [14, anchor2.x + 14]) if (!s.points.some((p) => Math.abs(p.x - x) < 1e-6)) throw new Error(`falta el ancho nuevo en x = ${x}: ${JSON.stringify(s.texts[1].points.map((id) => s.points.find((p) => p.id === id)))}`);
    if (!s.entities.some((e) => e.id === 14)) throw new Error("la línea suelta se perdió al editar el bloque");

    // Capas: la línea suelta a una capa; oculta no se elige, bloqueada tampoco
    await b.clickText("Nueva capa");
    await sleep(500);
    await b.eval(`window.__cadUi.setSelection([14])`);
    await sleep(300);
    await b.eval(`[...document.querySelectorAll('[aria-label="Pasar lo elegido a una capa"] button')].find((e) => e.textContent === "Capa 1").click()`);
    await sleep(800);
    s = await sk();
    const layer = s.layers?.[0]?.id;
    if (s.entities.find((e) => e.id === 14).layer !== layer) throw new Error("no pasó a la capa");
    const clickLine = async () => {
      await b.eval(`window.__cadUi.setTool("select"); window.__cadUi.setSelection([])`);
      await b.click(...(await at(15, 20)), { wait: 500 });
      return b.eval(`window.__cadUi.selection()`);
    };
    if (!(await clickLine()).includes(14)) throw new Error("visible y libre se elige");
    await b.eval(`document.querySelector('[aria-label="Ver Capa 1"]').click()`);
    await sleep(500);
    if ((await clickLine()).includes(14)) throw new Error("oculta se eligió");
    await b.eval(`document.querySelector('[aria-label="Ver Capa 1"]').click(); document.querySelector('[aria-label="Bloquear Capa 1"]').click()`);
    await sleep(500);
    if ((await clickLine()).includes(14)) throw new Error("bloqueada se eligió");
    await b.shot("organizacion");

    // Copiar el sketch entero y pegarlo en el frente
    await b.clickText("Copiar el sketch entero");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    await b.eval(`window.__cadUi.pickToggle({ kind: "plane", plane: "xz" }, false)`);
    await b.clickText("Pegar sketch");
    await sleep(2500);
    const feats = (await call("cad_get_document")).features.filter((f) => f.kind.type === "sketch");
    if (feats.length !== 2 || feats[1].kind.plane.type !== "xz") throw new Error("pegar: " + JSON.stringify(feats.map((f) => f.kind.plane)));
    const pasted = feats[1].kind.sketch;
    if (pasted.entities.length !== feats[0].kind.sketch.entities.length || pasted.blocks?.length !== 1) throw new Error("el pegado no trae lo mismo");
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error("no abrió el pegado");
    await b.eval(`window.__cadUi.cancelSketch()`);

    // Validación: revolución de un rectángulo a los dos lados del eje Z (en el frente)
    const d2 = await call("cad_get_document");
    const rid = d2.next_id;
    d2.features.push({ id: rid, name: "Perfil cruzado", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 30, sketch: { points: [P(0, 0, 0), P(1, -2, 0), P(2, 5, 0), P(3, 5, 8), P(4, -2, 8)], entities: [L(10, 1, 2), L(11, 2, 3), L(12, 3, 4), L(13, 4, 1)], constraints: [], next_id: 20, origin: 0 } } });
    d2.features.push({ id: rid + 1, name: "Giro", suppressed: false, kind: { type: "revolve", sketch: rid, regions: { type: "all" }, axis: { type: "z" }, angle: 360, op: "join" } });
    d2.next_id += 2;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const st = await b.eval(`JSON.stringify(window.__cadStore.result().status.find((x) => x.id === ${rid + 1}))`);
    if (!st.includes("cruza el eje")) throw new Error("la revolución no avisó: " + st);
  },

  async "curvas: spline por polos, punto en spline, tangente en la unión, cónica, paralelogramo, arco elíptico, línea infinita, ranura por 3 puntos, convertir, peine y texto sobre curva"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const at = (x, y) => b.eval(`window.__cadViewer.screenOf([${x}, ${y}, 0])`);
    const tool = async (label) => {
      await b.clickText(label);
      await sleep(250);
    };
    const esc = () => b.key("Escape", "Escape", 27);
    const status = () => b.eval(`window.__cadUi.session().report?.status`);
    const okStatus = async (what) => {
      await sleep(900);
      const st = await status();
      if (st === "over_constrained" || st === "failed") throw new Error(`${what}: ${st}`);
    };
    const of = async (type) => (await sk()).entities.filter((e) => e.geometry.type === type);
    const P = (s) => new Map(s.points.map((p) => [p.id, [p.x, p.y]]));
    const { makeBSpline, evalAt, domain } = await import("../src/lib/sketchCurves.ts");
    // Spline por polos: cuatro clics y Esc
    await tool("Spline por polos");
    for (const [x, y] of [[180, 520], [240, 400], [320, 540], [400, 420]]) await b.click(x, y, { wait: 250 });
    await esc();
    await sleep(800);
    let s = await sk();
    let bs = s.entities.find((e) => e.geometry.type === "bspline");
    if (!bs || bs.geometry.poles.length !== 4 || bs.geometry.degree !== 3) throw new Error("spline por polos: " + JSON.stringify(bs));
    // Un polo más con clic sobre la curva: la forma no cambia
    const curveNow = (s, g) => makeBSpline({ poles: g.poles.map((id) => P(s).get(id)), degree: g.degree, knots: g.knots, weights: g.weights });
    const before = curveNow(s, bs.geometry);
    const [u0, u1] = domain(before);
    const mid = evalAt(before, (u0 + u1) / 2);
    await tool("Punto en spline");
    await b.click(...(await at(mid[0], mid[1])), { wait: 1000 });
    s = await sk();
    bs = s.entities.find((e) => e.id === bs.id);
    if (bs.geometry.poles.length !== 5) throw new Error(`polos después de agregar: ${bs.geometry.poles.length}`);
    const after = curveNow(s, bs.geometry);
    for (const u of [0.1, 0.4, 0.8]) {
      const [p, q] = [evalAt(before, u0 + (u1 - u0) * u), evalAt(after, u0 + (u1 - u0) * u)];
      near(Math.hypot(p[0] - q[0], p[1] - q[1]), 0, 1e-6, "misma forma");
    }
    // Línea que llega al primer polo: tangente en la unión
    const first = P(s).get(bs.geometry.poles[0]);
    await tool("Línea");
    await b.click(...(await at(first[0] - 15, first[1] - 6)), { wait: 300 });
    await b.click(...(await at(first[0], first[1])), { wait: 600 });
    await esc();
    await esc();
    s = await sk();
    const line = s.entities.find((e) => e.geometry.type === "line" && e.geometry.end === bs.geometry.poles[0]);
    if (!line) throw new Error("la línea no quedó unida al primer polo");
    await b.eval(`window.__cadUi.setSelection(${JSON.stringify([line.id, bs.id])})`);
    await sleep(300);
    await b.clickText("Tangentes en la unión");
    await okStatus("tangentes en la unión");
    s = await sk();
    {
      const Q = P(s);
      const [a, c, d] = [Q.get(line.geometry.start), Q.get(bs.geometry.poles[0]), Q.get(bs.geometry.poles[1])];
      const cross = (c[0] - a[0]) * (d[1] - c[1]) - (c[1] - a[1]) * (d[0] - c[0]);
      near(cross / (Math.hypot(c[0] - a[0], c[1] - a[1]) * Math.hypot(d[0] - c[0], d[1] - c[1])), 0, 1e-6, "tangente en la unión");
    }
    // Peine de curvatura de la spline por polos
    await b.eval(`window.__cadUi.setSelection([${bs.id}])`);
    await sleep(300);
    await b.clickText("Peine de curvatura");
    await sleep(500);
    if (!(await b.eval(`!!document.querySelector("[data-curvature]")?.textContent.includes("Radio mínimo")`))) throw new Error("no se ve la curvatura");
    // Texto sobre la curva elegida
    await tool("Texto");
    await b.eval(`(() => { const i = document.querySelector('input[aria-label="Texto"]'); i.value = "AB"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
    await b.click(...(await at(first[0] + 2, first[1])), { wait: 1500 });
    s = await sk();
    const txt = (s.texts ?? []).find((t) => t.text === "AB");
    if (!txt || txt.style?.path !== bs.id) throw new Error("texto sobre la curva: " + JSON.stringify(txt?.style));
    await b.eval(`window.__cadUi.setSelection([])`);
    // Cónica con rho 0,3
    await tool("Cónica");
    await b.eval(`(() => { const i = document.querySelector('input[aria-label="Factor rho"]'); i.value = "0.3"; i.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    for (const [x, y] of [[500, 560], [640, 560], [570, 470]]) await b.click(x, y, { wait: 300 });
    await sleep(600);
    const conic = (await of("bspline")).find((e) => e.geometry.weights?.length === 3);
    if (!conic) throw new Error("no se creó la cónica");
    near(conic.geometry.weights[1], 0.3 / 0.7, 1e-9, "peso de la cónica");
    // Paralelogramo
    await tool("Paralelogramo");
    for (const [x, y] of [[700, 560], [800, 560], [830, 480]]) await b.click(x, y, { wait: 300 });
    await sleep(600);
    s = await sk();
    if (s.constraints.filter((c) => c.type === "parallel").length < 2) throw new Error("paralelogramo sin paralelas");
    // Arco elíptico: centro, eje mayor, ancho, comienzo, fin
    await tool("Arco elíptico");
    for (const [x, y] of [[300, 260], [380, 260], [300, 225], [380, 250], [230, 250]]) await b.click(x, y, { wait: 300 });
    await sleep(500);
    await esc();
    await okStatus("arco elíptico");
    s = await sk();
    const ea = s.entities.find((e) => e.geometry.type === "ellipse_arc");
    if (!ea) throw new Error("no se creó el arco elíptico");
    {
      const Q = P(s);
      const g = ea.geometry;
      const [c, ma, mi] = [Q.get(g.center), Q.get(g.major), Q.get(g.minor)];
      const u = [ma[0] - c[0], ma[1] - c[1]];
      const [A, B] = [Math.hypot(...u), Math.hypot(mi[0] - c[0], mi[1] - c[1])];
      for (const id of [g.start, g.end]) {
        const q = [Q.get(id)[0] - c[0], Q.get(id)[1] - c[1]];
        const x = (q[0] * u[0] + q[1] * u[1]) / A;
        const y = (u[0] * q[1] - u[1] * q[0]) / A;
        near((x / A) ** 2 + (y / B) ** 2, 1, 1e-6, "extremo sobre la elipse");
      }
    }
    // Línea infinita
    await tool("Línea infinita");
    await b.click(600, 200, { wait: 300 });
    await b.click(650, 230, { wait: 600 });
    s = await sk();
    const inf = s.entities.find((e) => e.infinite);
    if (!inf || !inf.construction) throw new Error("línea infinita: " + JSON.stringify(inf));
    // Ranura en arco por 3 puntos y el ancho
    const arcsBefore = (await of("arc")).length;
    await tool("Ranura arco 3 p.");
    for (const [x, y] of [[860, 300], [930, 240], [1000, 300], [930, 255]]) await b.click(x, y, { wait: 300 });
    await sleep(500);
    await esc();
    await okStatus("ranura por 3 puntos");
    if ((await of("arc")).length < arcsBefore + 4) throw new Error("la ranura no armó sus arcos");
    // Spline por puntos convertida a polos («Spline» también es una fila de la lista de entidades: atajo N)
    await b.eval(`window.__cadUi.setSelection([])`);
    await b.key("n", "KeyN", 78);
    await sleep(250);
    for (const [x, y] of [[200, 680], [260, 630], [330, 690]]) await b.click(x, y, { wait: 250 });
    await esc();
    await sleep(700);
    s = await sk();
    const sp = s.entities.find((e) => e.geometry.type === "spline" && !(s.texts ?? []).some((t) => t.entities.includes(e.id)));
    if (!sp) throw new Error("no se creó la spline por puntos");
    await b.eval(`window.__cadUi.setSelection([${sp.id}])`);
    await sleep(300);
    await b.clickText("Convertir a spline por polos");
    await sleep(800);
    s = await sk();
    if (s.entities.some((e) => e.id === sp.id)) throw new Error("la spline por puntos sigue ahí");
    await okStatus("todo");
    await b.shot("curvas_16_17");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const ev = await evaluate();
    const errs = (ev.status ?? []).filter((x) => x.state === "error");
    if (errs.length) throw new Error("errores al recalcular: " + JSON.stringify(errs));
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

  async "revisión del sketch: extremos sueltos y aviso al extruir"(b) {
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
    // El cruce no es un problema (las regiones se parten ahí): siguen los extremos sueltos
    if ((await problems()) !== "4 extremos sueltos") throw new Error(`extremos: "${await problems()}"`);
    await b.shot("revision_del_sketch");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Extruir ese sketch: no se crea la operación y el aviso dice por qué
    const n = await b.eval(`window.__cadStore.doc().features.length`);
    await b.clickText("Extrusión");
    await sleep(1000);
    const text = await b.eval(`document.body.innerText`);
    if (!text.includes("El sketch no tiene regiones cerradas: 4 extremos sueltos")) throw new Error("falta el aviso de por qué no hay regiones");
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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

  async "íconos de restricciones: elegir, quitar, ocultar, todas las cotas y lazo"(b) {
    await begin(b);
    await sketchOn(b);
    const sk = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const glyphs = () => b.eval(`[...document.querySelectorAll("[data-constraint-glyph]")].map((g) => g.textContent).sort().join("")`);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(540, 440, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.clickText("Círculo");
    await b.click(760, 370);
    await b.click(800, 370, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
    await sleep(500);
    // Rectángulo: dos H y dos V
    if ((await glyphs()) !== "HHVV") throw new Error(`íconos: "${await glyphs()}"`);
    // Clic en un ícono: elige la restricción; Supr la quita
    const n = (await sk()).constraints.length;
    const g = await b.eval(`(() => { const e = document.querySelector("[data-constraint-glyph]"); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2, +e.dataset.constraintGlyph]; })()`);
    await b.click(g[0], g[1], { wait: 300 });
    if (JSON.stringify(await b.eval(`window.__cadUi.selectedConstraints()`)) !== JSON.stringify([g[2]])) throw new Error("el clic no eligió la restricción");
    await b.key("Delete", "Delete", 46);
    await sleep(800);
    if ((await sk()).constraints.length !== n - 1) throw new Error("Supr no quitó la restricción");
    if ((await glyphs()).length !== 3) throw new Error(`íconos tras quitar: "${await glyphs()}"`);
    // Ocultar y mostrar
    await b.eval(`document.querySelector('[aria-label="Mostrar restric."]').click()`);
    await sleep(300);
    if ((await glyphs()) !== "") throw new Error("los íconos siguen a la vista");
    await b.eval(`document.querySelector('[aria-label="Mostrar restric."]').click()`);
    await sleep(300);
    // Todas las cotas desde el panel y quitarlas
    const dims = (await sk()).constraints.filter((c) => "value" in c).length;
    if (dims < 2) throw new Error(`cotas: ${dims}`);
    await b.eval(`[...document.querySelector('[aria-label="Elegir restricciones"]').querySelectorAll("button")].find((x) => x.textContent.trim() === "Todas las cotas").click()`);
    await sleep(300);
    if ((await b.eval(`window.__cadUi.selectedConstraints().length`)) !== dims) throw new Error("no eligió todas las cotas");
    await b.clickContains("Quitar elegidas");
    await sleep(800);
    if ((await sk()).constraints.some((c) => "value" in c)) throw new Error("quedaron cotas");
    // Lazo con Ctrl+arrastrar alrededor del círculo: solo el círculo (y su centro)
    await b.mouse("mouseMoved", 700, 310, { buttons: 0 });
    await b.mouse("mousePressed", 700, 310, { modifiers: 2 });
    for (const [x, y] of [[860, 310], [860, 430], [700, 430], [700, 320]]) {
      for (let k = 1; k <= 6; k++) await b.mouse("mouseMoved", x, y, { buttons: 1, modifiers: 2 });
      await sleep(40);
    }
    await b.mouse("mouseReleased", 700, 320, { modifiers: 2 });
    await sleep(400);
    const s = await sk();
    const sel = await b.eval(`window.__cadUi.selection()`);
    const circle = s.entities.find((e) => e.geometry.type === "circle");
    if (!sel.includes(circle.id)) throw new Error(`el lazo no eligió el círculo: ${sel}`);
    if (s.entities.some((e) => e.geometry.type === "line" && sel.includes(e.id))) throw new Error("el lazo eligió líneas de afuera");
    await b.shot("iconos_de_restricciones");
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
    await b.clickText("Usar");
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

  async "usar del modelo: cara, silueta, intersección, otro sketch y romper vínculo"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const byTitle = (t) => b.eval(`(() => { const e = [...document.querySelectorAll("button")].find((x) => x.title === ${JSON.stringify(t)} && x.offsetParent !== null); e?.click(); return !!e; })()`);
    const session = () => b.eval(`JSON.parse(JSON.stringify(window.__cadUi.session().sketch))`);
    const waitUses = async (n) => {
      for (let t = 0; t < 40 && ((await session()).uses?.length ?? 0) !== n; t++) await sleep(250);
      const sk = await session();
      if ((sk.uses?.length ?? 0) !== n) throw new Error(`ligadas: ${sk.uses?.length} (esperaba ${n}); aviso: ${await b.eval(`window.__cadUi.message?.()`)}`);
      return sk;
    };
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Sketch sobre la cara de arriba (z = 10): Usar con clic en la cara trae sus 4 aristas
    await b.click(...(await at([3, 3, 10])), { wait: 800 });
    await b.clickText("Sketch");
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()`)); t++) await sleep(250);
    await b.clickText("Usar");
    await b.click(...(await at([3, 3, 10])), { wait: 1500 });
    let sk = await waitUses(4);
    if (!sk.uses.every((u) => u.edge)) throw new Error(`sin arista: ${JSON.stringify(sk.uses)}`);
    // Las esquinas se comparten: 4 puntos (más el origen)
    if (sk.points.length > 5) throw new Error(`puntos: ${sk.points.length}`);
    // Romper el vínculo de una
    const broken = sk.uses[0].entity;
    await b.eval(`window.__cadUi.setSelection([${broken}])`);
    await sleep(300);
    if (!(await byTitle("Lo elegido deja de seguir al modelo y queda editable"))) throw new Error("no está Romper vínculo");
    sk = await waitUses(3);
    await b.shot("usar_cara");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Sketch en la planta (z = 0, a media altura de la caja)
    await sketchOn(b);
    for (let t = 0; t < 20 && !(await b.eval(`!!window.__cadUi.session()`)); t++) await sleep(250);
    if (!(await byTitle("Las curvas donde el plano del sketch corta el sólido"))) throw new Error("no está Intersección");
    await waitUses(4);
    if (!(await byTitle("El contorno del sólido visto desde la normal del plano, con los agujeros pasantes"))) throw new Error("no está Silueta");
    await waitUses(8);
    if (!(await byTitle("Todas sus curvas y puntos, proyectados a este plano"))) throw new Error("no está el otro sketch");
    sk = await waitUses(12);
    const kinds = sk.uses.map((u) => u.source?.type ?? "edge").join(",");
    if (kinds !== "section,section,section,section,silhouette,silhouette,silhouette,silhouette,sketch,sketch,sketch,sketch") throw new Error(`fuentes: ${kinds}`);
    await b.shot("usar_silueta");
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // La caja más ancha: la intersección y la silueta se estiran; lo del otro
    // sketch sigue a sus aristas (la del vínculo roto no)
    const doc = await call("cad_get_document");
    doc.features[0].kind.shape.dx = 30;
    const id = doc.features.filter((f) => f.kind.type === "sketch")[1].id;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2500);
    const xs = await b.eval(`(() => {
      const v = window.__cadStore.sketchView(${id});
      const pt = (p) => v.sketch.points.find((q) => q.id === p);
      const out = {};
      for (const u of v.sketch.uses) {
        const g = v.sketch.entities.find((e) => e.id === u.entity).geometry;
        const k = u.source.type;
        out[k] = [...(out[k] ?? []), pt(g.start).x, pt(g.end).x];
      }
      return Object.fromEntries(Object.entries(out).map(([k, v]) => [k, [Math.min(...v), Math.max(...v)]]));
    })()`);
    for (const k of ["section", "silhouette", "sketch"]) {
      near(xs[k][0], -15, 1e-6, `${k}: izquierda`);
      near(xs[k][1], 15, 1e-6, `${k}: derecha`);
    }
    const st = (await evaluate()).status?.find((s) => s.id === id);
    if (st && st.state !== "ok") throw new Error(`estado del sketch: ${JSON.stringify(st)}`);
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
    // La caja de aristas queda activa para sumar más: Esc muestra la vista previa
    await b.key("Escape", "Escape", 27);
    await sleep(800);
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
    await b.key("Escape", "Escape", 27);
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
    // Con el atajo: «Línea» también es una fila de la lista de entidades
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "l" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
    await b.click(550, 500, { wait: 500 });
    await setInput(b, "Distancia", 5);
    // El botón del panel (la barra izquierda también tiene la herramienta «Equidistante»)
    await b.eval(`[...document.querySelectorAll("button[data-preview]")].find((x) => x.textContent.trim() === "Equidistante").click()`);
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
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "l" }))`);
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
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "l" }))`);
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
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }))`);
    await b.click(700, 400);
    await b.click(750, 400);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "t" }))`);
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
    // La caja de aristas queda activa para sumar más: Esc muestra la vista previa
    await b.key("Escape", "Escape", 27);
    await sleep(1500);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // La herramienta Elegir del sketch por su atajo (hay otros «Elegir» en la página)
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "s" }))`);
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
    // Recién aplicado la app a veces sigue ocupada y no toma el atajo (inestable antes de E4)
    await sleep(3000);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", code: "KeyZ", ctrlKey: true, bubbles: true }))`);
    for (let t = 0; t < 40 && (await triangles()) !== before; t++) await sleep(250);
    if ((await triangles()) !== before) throw new Error(`deshacer no devolvió la malla: ${await triangles()} triángulos (antes ${before}, remallada ${after})`);
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

  async "escaneo: sigue de referencia con una pieza activa"(b) {
    // El "escaneo": dos cubos de 20 mm en STL (24 triángulos)
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-dos-cubos.stl`;
    writeFileSync(path, twoCubesStl());
    await importModel(b, path);
    await begin(b);
    const scanCount = () => b.eval(`window.__cadViewer?.scan?.geometry.index.count / 3`);
    for (let t = 0; t < 20 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`no se ve el escaneo: ${await scanCount()}`);
    // Una pieza, a Fabricar (queda activa) y de vuelta a Diseñar
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    await sleep(1000);
    const rows = () => b.eval(`[...document.querySelectorAll("[data-outliner-row]")].map((d) => d.innerText.split("\\n")[0])`);
    await b.eval(`[...document.querySelector("[data-outliner]").querySelectorAll("span")].find((x) => x.textContent === "Pieza 1").click()`);
    for (let t = 0; t < 40 && (await rows()).indexOf("Malla generada") !== (await rows()).indexOf("Pieza 1") + 1; t++) await sleep(250);
    const got = await rows();
    if (got.indexOf("Malla generada") !== got.indexOf("Pieza 1") + 1) throw new Error(`la pieza no quedó activa: ${JSON.stringify(got)}`);
    await b.clickText("Diseñar");
    await sleep(1000);
    for (let t = 0; t < 20 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`el escaneo desapareció con la pieza activa: ${await scanCount()}`);
    // Y las medidas se toman del escaneo, no de la pieza: 12 caras planas (6 por cubo, sin unir)
    const found = await b.eval(`window.__cadStore.scanDetect().then((d) => d.filter((x) => x.shape.type === "plane").length)`);
    if (found < 7) throw new Error(`planos detectados: ${found} (esperaba los de los dos cubos)`);
    await tab(b, "Desde el escaneo");
    await sleep(500);
    await b.shot("escaneo-pieza-activa");
    // Un segundo escaneo (un solo cubo): pasa a ser el de referencia al
    // importarlo (queda activo) y se puede volver a elegir el primero
    const one = path.replace("dos-cubos", "un-cubo");
    writeFileSync(one, twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n");
    await b.eval(`window.__nextPath = ${JSON.stringify(one)}`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", ctrlKey: true }))`);
    for (let t = 0; t < 60 && !(await b.eval(`document.body.innerText.includes("escaneo-un-cubo")`)); t++) await sleep(250);
    await sleep(1000);
    await b.clickText("Diseñar");
    await sleep(1000);
    await tab(b, "Desde el escaneo");
    for (let t = 0; t < 40 && (await scanCount()) !== 12; t++) await sleep(250);
    if ((await scanCount()) !== 12) throw new Error(`el escaneo nuevo no es la referencia: ${await scanCount()}`);
    if (!(await b.eval(`!!document.querySelector('[aria-label="Escaneo de referencia"]')`))) throw new Error("falta el selector de escaneo");
    const first = await b.eval(`window.__objects().find((o) => o.name === "escaneo-dos-cubos.stl").id`);
    await b.eval(`window.__cadStore.commit((d) => { d.scan_object = ${first}; })`);
    for (let t = 0; t < 40 && (await scanCount()) !== 24; t++) await sleep(250);
    if ((await scanCount()) !== 24) throw new Error(`no volvió al primer escaneo: ${await scanCount()}`);
    const planes = await b.eval(`window.__cadStore.scanDetect().then((d) => d.filter((x) => x.shape.type === "plane").length)`);
    if (planes < 7) throw new Error(`las medidas no son del escaneo elegido: ${planes} planos`);
    await b.shot("escaneo-elegido");
  },

  async "escaneo: alinear por cara al piso y punto al origen"(b) {
    // Un cubo de 20 mm girado 30° en X y 20° en Z y corrido: el "escaneo"
    const rot = (p) => {
      const [ax, az] = [Math.PI / 6, Math.PI / 9];
      const [x, y, z] = p;
      const [y1, z1] = [y * Math.cos(ax) - z * Math.sin(ax), y * Math.sin(ax) + z * Math.cos(ax)];
      const [x2, y2] = [x * Math.cos(az) - y1 * Math.sin(az), x * Math.sin(az) + y1 * Math.cos(az)];
      return [x2 + 15, y2 - 5, z1 + 12];
    };
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const tilted = stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => "vertex " + rot([+x, +y, +z]).join(" "));
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-inclinado.stl`;
    writeFileSync(path, tilted);
    await importModel(b, path);
    await begin(b);
    await tab(b, "Desde el escaneo");
    await sleep(800);
    const planes = () => b.eval(`window.__cadStore.scanDetect().then((d) => JSON.stringify(d.filter((x) => x.shape.type === "plane").map((x) => x.shape.plane)))`).then(JSON.parse);
    // Cara de abajo del cubo original (z = 0): su centro girado
    const bottom = rot([10, 10, 0]);
    const clickAt = async (p) => {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
      await b.click(x, y);
    };
    // Que se vea la cara de abajo: vista desde abajo
    await b.eval(`window.__cadViewer.lookFrom([0.3, -0.4, -1])`);
    await sleep(1200);
    await b.clickText("Cara al piso");
    await sleep(300);
    await clickAt(bottom);
    let floor;
    for (let t = 0; t < 40; t++) {
      floor = (await planes()).find((p) => p.normal[2] < -0.999);
      if (floor && Math.abs(floor.origin[2]) < 0.05) break;
      await sleep(250);
    }
    if (!floor || Math.abs(floor.origin[2]) > 0.05) throw new Error(`la cara no quedó en el piso: ${JSON.stringify(floor)} ${JSON.stringify(await planes())}`);
    // Punto al origen: el centro de la cara de arriba (ahora z = 20)
    const top = (await planes()).find((p) => p.normal[2] > 0.999);
    if (!top || Math.abs(top.origin[2] - 20) > 0.05) throw new Error(`cara de arriba: ${JSON.stringify(top)}`);
    await b.eval(`window.__cadViewer.lookFrom([0.2, -0.3, 1])`);
    await sleep(1200);
    await b.clickText("Punto al origen");
    await sleep(300);
    await clickAt(top.origin);
    let after;
    for (let t = 0; t < 40; t++) {
      after = (await planes()).find((p) => p.normal[2] > 0.999);
      if (after && Math.abs(after.origin[2]) < 0.05) break;
      await sleep(250);
    }
    if (!after || Math.abs(after.origin[2]) > 0.05) throw new Error(`la cara de arriba no pasó por el origen: ${JSON.stringify(after)}`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(500);
    await b.shot("escaneo-alineado");
  },

  async "escaneo: referencias ligadas siguen al alinear"(b) {
    const rot = (p) => {
      const [ax, az] = [Math.PI / 6, Math.PI / 9];
      const [x, y, z] = p;
      const [y1, z1] = [y * Math.cos(ax) - z * Math.sin(ax), y * Math.sin(ax) + z * Math.cos(ax)];
      const [x2, y2] = [x * Math.cos(az) - y1 * Math.sin(az), x * Math.sin(az) + y1 * Math.cos(az)];
      return [x2 + 15, y2 - 5, z1 + 12];
    };
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-inclinado-refs.stl`;
    writeFileSync(path, stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => "vertex " + rot([+x, +y, +z]).join(" ")));
    await importModel(b, path);
    await begin(b);
    await tab(b, "Desde el escaneo");
    await sleep(800);
    const clickAt = async (p) => {
      const [x, y] = await b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
      await b.click(x, y);
    };
    const refs = () => b.eval(`JSON.stringify(window.__cadStore.result()?.references ?? [])`).then(JSON.parse);
    // Desde arriba se ve la cara de arriba del cubo (z = 20 sin girar)
    await b.eval(`window.__cadViewer.lookFrom([0.2, -0.3, 1])`);
    await sleep(1200);
    const top = rot([10, 10, 20]);
    await b.clickText("Plano de zona");
    await sleep(300);
    await clickAt(top);
    for (let t = 0; t < 40 && (await refs()).length < 1; t++) await sleep(250);
    await b.clickText("Punto");
    await sleep(300);
    await clickAt(top);
    for (let t = 0; t < 40 && (await refs()).length < 2; t++) await sleep(250);
    let r = await refs();
    const plane = r.find((x) => x.kind === "plane");
    const point = r.find((x) => x.kind === "point");
    if (!plane || !point) throw new Error(`referencias: ${JSON.stringify(r)}`);
    // Inclinado: la normal del plano es la de la cara de arriba girada
    if (plane.plane.normal[2] > 0.95) throw new Error(`el plano ya estaba horizontal: ${JSON.stringify(plane.plane)}`);
    // Alinear: la cara de abajo al piso
    await b.eval(`window.__cadViewer.lookFrom([0.3, -0.4, -1])`);
    await sleep(1200);
    await b.clickText("Cara al piso");
    await sleep(300);
    await clickAt(rot([10, 10, 0]));
    // Las referencias van con el escaneo: plano horizontal a 20 mm, punto a 20 mm
    let ok = false;
    for (let t = 0; t < 60 && !ok; t++) {
      await sleep(250);
      r = await refs();
      const p = r.find((x) => x.kind === "plane")?.plane;
      const q = r.find((x) => x.kind === "point")?.point;
      ok = !!p && p.normal[2] > 0.999 && Math.abs(p.origin[2] - 20) < 0.05 && !!q && Math.abs(q[2] - 20) < 0.05;
    }
    if (!ok) throw new Error(`las referencias no siguieron al escaneo: ${JSON.stringify(r)}`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-referencias");
  },

  async "escaneo: el sketch se ancla al corte del escaneo"(b) {
    // Cubo de 20 mm centrado en el origen: la planta lo corta en un cuadrado
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-centrado.stl`;
    writeFileSync(path, stl.replace(/vertex (\S+) (\S+) (\S+)/g, (_, x, y, z) => `vertex ${+x - 10} ${+y - 10} ${+z - 10}`));
    await importModel(b, path);
    await begin(b);
    await sketchOn(b);
    const outline = () => b.eval(`window.__cadUi.scanOutline()?.sketch.entities.length ?? 0`);
    for (let t = 0; t < 40 && (await outline()) < 4; t++) await sleep(250);
    if ((await outline()) !== 4) throw new Error(`el corte no salió como un cuadrado: ${await outline()} entidades`);
    if (!(await b.eval(`!!window.__cadViewer.scanSection`))) throw new Error("no se dibuja el corte");
    const before = await b.eval(`__cadUi.session().sketch.entities.length`);
    await b.clickText("Usar el corte");
    await sleep(1500);
    const lines = await b.eval(`__cadUi.session().sketch.entities.filter((e) => e.geometry.type === "line").length`);
    if (lines - before < 4) throw new Error(`no se copió el corte: ${before} → ${lines}`);
    // Las líneas copiadas están sobre el cuadrado de ±10 mm
    const xs = await b.eval(`JSON.stringify(__cadUi.session().sketch.points.map((p) => [Math.round(p.x), Math.round(p.y)]))`);
    if (!xs.includes("[10,10]") || !xs.includes("[-10,-10]")) throw new Error(`puntos del corte: ${xs}`);
    await b.shot("escaneo-corte-sketch");
  },

  async "escaneo: cuerpo del escaneo deja la cuna en un bloque"(b) {
    // Cubo de 20 mm (0..20) como escaneo y un bloque que toma su mitad de abajo
    const stl = twoCubesStl().split("endfacet\n").slice(0, 12).join("endfacet\n") + "endfacet\nendsolid uno\n";
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/escaneo-cuna.stl`;
    writeFileSync(path, stl);
    await importModel(b, path);
    await begin(b);
    const doc = await call("cad_get_document");
    const id = doc.next_id;
    doc.features.push({ id, name: "Bloque", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 15, centered: false }, origin: [-10, -10, -5], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
    doc.next_id = id + 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await tab(b, "Desde el escaneo");
    await b.clickText("Escaneo como cuerpo");
    // Prepara el escaneo (vóxeles) y resta: unos segundos
    const state = () => b.eval(`JSON.stringify(window.__cadStore.result()?.status.map((s) => s.state.type ?? s.state))`).then(JSON.parse);
    for (let t = 0; t < 120 && (await state())?.length !== 2; t++) await sleep(250);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const r = await evaluate();
    const st = r.status.map((s) => s.state.type ?? s.state);
    if (st[1] !== "ok") throw new Error(`cuerpo del escaneo: ${JSON.stringify(r.status[1])}`);
    if (r.parts.length !== 1) throw new Error(`piezas: ${r.parts.length}`);
    // Se lleva la mitad de abajo del cubo con 0,2 mm de holgura: 20,4 × 20,4 × 10,2
    // (las aristas del cubo quedan algo redondeadas por los vóxeles)
    near(r.parts[0].volume, 40 * 40 * 15 - 20.4 * 20.4 * 10.2, 80, "bloque menos el escaneo");
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 1.2])`);
    await sleep(800);
    await b.shot("escaneo-cuna");
  },

  async "escaneo: extruir y llevar una cara hasta el escaneo"(b) {
    // Esfera de radio 20 con centro a 40 mm de altura (la panza en z = 20)
    const [R, cz] = [20, 40];
    await importModel(b, sphereStl(R, cz, "escaneo-esfera"));
    await begin(b);
    // Círculo de radio 5 en la planta, extruido hasta el escaneo con 0,5 mm de separación
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }],
      entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 5 } }],
      constraints: [],
      next_id: 3,
      origin: 0,
    };
    const doc = await call("cad_get_document");
    doc.features = [
      { id: 1, name: "Sketch 1", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } },
      { id: 2, name: "Columna", suppressed: false, kind: { type: "extrude", sketch: 1, regions: { type: "all" }, extent: { type: "up_to_scan", offset: 0.5 }, reverse: false, op: "new" } },
    ];
    doc.next_id = 3;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    let r;
    for (let t = 0; t < 120; t++) {
      r = await evaluate();
      if (r.status?.[1]?.state === "ok" || r.status?.[1]?.state === "error") break;
      await sleep(250);
    }
    if (r.status[1].state !== "ok") throw new Error(`hasta el escaneo: ${JSON.stringify(r.status[1])}`);
    // π r² · 40 − ∫ 2πρ √(R² − ρ²) dρ con R = 20,5
    const big = R + 0.5;
    const column = Math.PI * 25 * cz - ((2 * Math.PI) / 3) * (big ** 3 - (big * big - 25) ** 1.5);
    near(r.parts[0].volume, column, column * 0.01, "columna hasta la panza");
    // El diálogo muestra el modo y la separación
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(1500);
    const text = await b.eval(`document.body.innerText`);
    if (!text.includes("Hasta el escaneo") || !text.includes("Separación")) throw new Error("el diálogo no muestra «Hasta el escaneo» con su separación");
    await b.key("Escape", "Escape", 27);
    await sleep(500);

    // Cara al escaneo: la tapa de un bloque de 10 sube hasta la panza (sin separación)
    const d2 = await call("cad_get_document");
    d2.features = [
      { id: 1, name: "Bloque", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 10, dy: 10, dz: 10, centered: true }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } },
      { id: 2, name: "Cara al escaneo 1", suppressed: false, kind: { type: "face_to_scan", faces: [{ point: [0, 0, 10], normal: [0, 0, 1] }], offset: 0 } },
    ];
    d2.next_id = 3;
    await call("cad_set_document", { document: d2 });
    await b.eval(`window.__cadStore.reload()`);
    for (let t = 0; t < 120; t++) {
      r = await evaluate();
      if (r.status?.[1]?.state === "ok" || r.status?.[1]?.state === "error") break;
      await sleep(250);
    }
    if (r.status[1].state !== "ok") throw new Error(`cara al escaneo: ${JSON.stringify(r.status[1])}`);
    // Bloque + entre z = 10 y la panza (grilla)
    let added = 0;
    for (let i = 0; i < 100; i++)
      for (let j = 0; j < 100; j++) {
        const [x, y] = [-5 + (i + 0.5) * 0.1, -5 + (j + 0.5) * 0.1];
        added += (cz - Math.sqrt(R * R - x * x - y * y) - 10) * 0.01;
      }
    near(r.parts[0].volume, 1000 + added, (1000 + added) * 0.01, "bloque hasta la panza");
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.4])`);
    await sleep(800);
    await b.shot("escaneo-cara");
  },

  async "escaneo: carcasa y parche bajo un sketch"(b) {
    // Esfera de radio 20 con centro a 40 mm; círculo de radio 10 arriba, en z = 70
    const [R, cz, r] = [20, 40, 10];
    await importModel(b, sphereStl(R, cz, "escaneo-carcasa"));
    await begin(b);
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }],
      entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: r } }],
      constraints: [],
      next_id: 3,
      origin: 0,
    };
    const doc = await call("cad_get_document");
    doc.features = [{ id: 1, name: "Sketch 1", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 70, sketch } }];
    doc.next_id = 2;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const settle = async (n) => {
      let res;
      for (let t = 0; t < 240; t++) {
        res = await evaluate();
        if (res.status?.length === n && ["ok", "error"].includes(res.status[n - 1].state?.type ?? res.status[n - 1].state)) break;
        await sleep(250);
      }
      const st = res.status[n - 1].state;
      if ((st.type ?? st) !== "ok") throw new Error(`operación ${n}: ${JSON.stringify(res.status[n - 1])}`);
      return res;
    };
    // Parche desde el sketch: 0,2 mm de holgura y 2 mm hacia afuera, solo la tapa de arriba
    await tab(b, "Desde el escaneo");
    await b.clickText("Parche desde sketch");
    let res = await settle(2);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const [r1, r2] = [R + 0.2, R + 2.2];
    const cap = (big) => ((2 * Math.PI) / 3) * (big ** 3 - (big * big - r * r) ** 1.5);
    near(res.parts[0].volume, cap(r2) - cap(r1), (cap(r2) - cap(r1)) * 0.03, "parche bajo el círculo");
    if (res.body.bbox_min[2] < cz) throw new Error(`el parche baja hasta ${res.body.bbox_min[2]}: tomó también la cara de abajo`);
    await b.eval(`window.__cadViewer.lookFrom([1, -1, 0.8])`);
    await sleep(800);
    await b.shot("escaneo-parche");
    // El diálogo muestra la forma, el espesor y el sketch
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(1500);
    const text = await b.eval(`document.body.innerText`);
    for (const w of ["Carcasa hacia afuera", "Espesor", "Solo bajo"]) if (!text.includes(w)) throw new Error(`el diálogo no muestra «${w}»`);
    await b.key("Escape", "Escape", 27);
    await sleep(500);
    // Carcasa entera: otra pieza, con el hueco de la esfera adentro
    await tab(b, "Desde el escaneo");
    await b.clickText("Carcasa");
    res = await settle(3);
    await b.eval(`window.__cadStore.settled()`);
    await accept(b, 3000);
    const ball = (x) => (4 / 3) * Math.PI * x ** 3;
    const shell = res.parts.find((p) => Math.abs(p.volume - (ball(r2) - ball(r1))) < (ball(r2) - ball(r1)) * 0.03);
    if (!shell) throw new Error(`no hay una carcasa de ${(ball(r2) - ball(r1)).toFixed(0)} mm³: ${res.parts.map((p) => p.volume.toFixed(0))}`);
  },

  async "mallas: caja del diseño en Fabricar"(b) {
    // La malla soldada de una pieza trae una sola normal por vértice en las
    // aristas vivas: la marca por vértice hace que las caras se dibujen planas
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Fabricar");
    for (let t = 0; t < 40 && !(await b.eval(`document.body.innerText.includes("Malla generada")`)); t++) await sleep(250);
    await sleep(2000);
    const info = JSON.parse(await b.eval(`JSON.stringify((() => { let out = null; window.__viewer().scene.traverse((o) => { const g = o.geometry; if (!out && o.isMesh && g?.attributes?.position?.count === 8) out = { crease: g.attributes.crease ? [...g.attributes.crease.array] : null, smooth: "AUTO_SMOOTH" in (o.material.defines ?? {}) }; }); return out; })())`));
    if (!info) throw new Error("no está la malla de la caja");
    if (JSON.stringify(info.crease) !== JSON.stringify(Array(8).fill(1))) throw new Error(`marcas de aristas vivas: ${JSON.stringify(info.crease)}`);
    if (!info.smooth) throw new Error("el material no tiene el suavizado automático");
    // Las unidades llegan: la grilla en mm y la cámara afuera de la caja
    if (!(await b.eval(`document.body.innerText.includes("Grilla: 1 mm")`))) throw new Error("la grilla no está en mm");
    if ((await b.eval(`window.__viewer().camera.position.length()`)) < 20) throw new Error("la cámara quedó dentro de la caja");
    await b.shot("mallas-caja-fabricar");
  },

  async "mallas: STL del diseño importado"(b) {
    await begin(b);
    await b.eval(`window.__cadStore.commit((d) => {
      d.features.push({ id: 61, name: "Caja", kind: { type: "primitive", shape: { type: "box", dx: 20, dy: 20, dz: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 62, name: "Cilindro", kind: { type: "primitive", shape: { type: "cylinder", radius: 8, height: 20 }, origin: [35, 0, -10], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 63, name: "Esfera", kind: { type: "primitive", shape: { type: "sphere", radius: 10 }, origin: [-35, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.features.push({ id: 64, name: "Cono", kind: { type: "primitive", shape: { type: "cone", r1: 9, r2: 3, height: 20 }, origin: [0, 35, -10], z: [0, 0, 1], x: [1, 0, 0], op: "new" } });
      d.next_id = 65;
    })`);
    await sleep(3000);
    const path = `${process.env.E2E_OUT ?? "/tmp/pinocchio-e2e"}/mallas-diseno.stl`;
    const r = await call("cad_export", { path, format: "stl" });
    if (r?.error) throw new Error(JSON.stringify(r));
    await b.eval(`window.__nextPath = ${JSON.stringify(path)}`);
    await b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "i", ctrlKey: true }))`);
    for (let t = 0; t < 60 && !(await b.eval(`document.body.innerText.includes("mallas-diseno")`)); t++) await sleep(250);
    await sleep(2500);
    // Caja y tapas planas; la esfera y los costados curvos quedan suaves
    const share = await b.eval(`(() => { let out = null; window.__viewer().scene.traverse((o) => { const c = o.geometry?.attributes?.crease; if (!out && o.isMesh && c && o.geometry.attributes.position.count > 1000) out = c.array.reduce((s, x) => s + x, 0) / c.count; }); return out; })()`);
    if (share === null || !(share > 0.05 && share < 0.6)) throw new Error(`vértices en aristas vivas: ${share}`);
    await b.click(31, 276);
    await sleep(1500);
    await b.shot("mallas-stl-importado");
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

  async "redondeo en vivo: cara entera y quitar con clic"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const items = () => b.eval(`[...document.querySelector('[aria-label="Aristas"]').querySelectorAll("span")].length`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Redondeo");
    await sleep(1500);
    // Clic en la tapa: sus cuatro aristas
    await b.click(...(await at([3, -4, 10])), { wait: 2500 });
    if ((await items()) !== 4) throw new Error(`cara: ${await items()} aristas`);
    near((await body()).volume, 8000 - 4 * (1 - Math.PI / 4) * 20, 1, "tapa redondeada");
    // Clic en lo redondeado de adelante: se quita esa
    await b.click(...(await at([0, -10, 10])), { wait: 2500 });
    if ((await items()) !== 3) throw new Error(`quitar: ${await items()} aristas`);
    // Otra vez en el mismo lugar (ahora arista recta): vuelve
    await b.click(...(await at([0, -10, 10])), { wait: 2500 });
    if ((await items()) !== 4) throw new Error(`sumar: ${await items()} aristas`);
    await accept(b, 2000);
    if ((await call("cad_get_document")).features[1].kind.edges.length !== 4) throw new Error("no quedaron 4 aristas");
  },

  async "chaflán en vivo: una cara entera"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    const items = () => b.eval(`[...document.querySelector('[aria-label="Aristas"]').querySelectorAll("span")].length`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.clickText("Chaflán");
    await sleep(1500);
    await b.click(...(await at([3, -10, -3])), { wait: 2500 });
    if ((await items()) !== 4) throw new Error(`chaflán: ${await items()} aristas`);
    await accept(b, 2000);
    const r = await evaluate();
    if (r.status.some((x) => x.state !== "ok")) throw new Error(JSON.stringify(r.status));
    // Las esquinas comparten un pedacito (≈1,3 mm³)
    near(r.body.volume, 8000 - 4 * 0.5 * 20, 2, "cuatro chaflanes de 1 mm");
  },

  async "agujero en una cara sin sketch y extruir una cara"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Agujero: se pide la cara; cerca del centro se ancla al centro
    await b.clickText("Agujero");
    await sleep(500);
    if ((await b.eval(`__cadUi.pick().kind`)) !== "hole") throw new Error("no pidió la cara");
    await b.click(...(await at([0.3, 0.2, 10])), { wait: 3000 });
    const doc = await call("cad_get_document");
    const sk = doc.features.find((f) => f.name === "Centros del agujero");
    if (!sk) throw new Error("no se creó el sketch de los centros");
    const p = sk.kind.sketch.points.find((q) => sk.kind.sketch.entities.some((e) => e.geometry.type === "point" && e.geometry.point === q.id));
    if (!p || Math.hypot(p.x, p.y) > 1e-6) throw new Error(`centro: ${JSON.stringify(p)}`);
    if (await b.eval(`!!__cadUi.session()`)) throw new Error("entró al sketch");
    await accept(b, 2500);
    const r = await evaluate();
    if (r.status.some((x) => x.state !== "ok")) throw new Error(JSON.stringify(r.status));
    if (!(r.body.volume < 8000 - 100)) throw new Error(`no agujereó: ${r.body.volume}`);
    // Extruir la cara de adelante: elegirla y Extrusión
    await b.click(...(await at([5, -10, -5])), { wait: 800 });
    await b.clickText("Extrusión");
    await sleep(3000);
    await accept(b, 2500);
    const after = await evaluate();
    if (after.status.some((x) => x.state !== "ok")) throw new Error(JSON.stringify(after.status));
    near(after.body.volume, r.body.volume + 20 * 20 * 10, 1, "cara extruida 10 mm");
  },

  async "sketch encima de otro sketch, anclado a sus puntos"(b) {
    // Pantalla de un punto del primer sketch (coordenadas del plano)
    const screen = (p) =>
      b.eval(`(() => { const v = __cadStore.result().sketches[0]; const pl = v.plane, p = ${JSON.stringify(p)}; const [o, n, x] = [pl.origin, pl.normal, pl.x_dir]; const y = [n[1]*x[2]-n[2]*x[1], n[2]*x[0]-n[0]*x[2], n[0]*x[1]-n[1]*x[0]]; return __cadViewer.screenOf([0,1,2].map((k) => o[k] + x[k]*p[0] + y[k]*p[1])); })()`);
    await begin(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300);
    await b.click(700, 500);
    await sleep(800);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const a = (await b.eval(`JSON.parse(JSON.stringify(__cadStore.doc()))`)).features[0];
    const sample = await b.eval(`__cadStore.result().sketches[0].regions[0].sample`);
    // Sketch nuevo: clic dentro de la región del primero
    await b.clickText("Sketch");
    await sleep(500);
    await b.click(...(await screen(sample)), { wait: 2500 });
    const doc = await b.eval(`JSON.parse(JSON.stringify(__cadStore.doc()))`);
    const c = doc.features.filter((f) => f.kind.type === "sketch")[1];
    if (!c || JSON.stringify(a.kind.plane) !== JSON.stringify(c.kind.plane)) throw new Error("no quedó en el mismo plano");
    if (!(await b.eval(`!!__cadUi.session()`))) throw new Error("no entró al sketch nuevo");
    // Línea desde una esquina del primero: se ancla y el primero no cambia
    const corner = a.kind.sketch.points.find((p) => p.id !== a.kind.sketch.origin);
    const [cx, cy] = await screen([corner.x, corner.y]);
    await b.clickText("Línea");
    await b.click(cx + 2, cy + 2);
    await b.click(cx + 150, cy - 90, { wait: 800 });
    await b.key("Escape", "Escape", 27);
    await b.key("Escape", "Escape", 27);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    const after = await b.eval(`JSON.parse(JSON.stringify(__cadStore.doc()))`);
    if (JSON.stringify(after.features[0].kind.sketch) !== JSON.stringify(a.kind.sketch)) throw new Error("cambió el primer sketch");
    const s2 = after.features[1].kind.sketch;
    if (!s2.points.some((p) => Math.hypot(p.x - corner.x, p.y - corner.y) < 1e-6)) throw new Error("la línea no se ancló a la esquina del primero");
  },

  async "regiones de formas que se cruzan"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Rectángulo");
    await b.click(400, 300, { modifiers: 8 });
    await b.click(600, 450, { modifiers: 8 });
    await sleep(600);
    await b.clickText("Rectángulo");
    await b.click(500, 375, { modifiers: 8 });
    await b.click(700, 520, { modifiers: 8 });
    await sleep(1200);
    const n = await b.eval(`__cadUi.session().regions.length`);
    if (n !== 3) throw new Error(`regiones: ${n}`);
    await b.clickText("Terminar sketch");
    await sleep(1500);
    // Elegir solo lo común y extruirlo
    const pick = await b.eval(`(() => { const v = __cadStore.result().sketches[0]; const r = v.regions.reduce((m, x) => (x.outer.area < m.outer.area ? x : m)); return __cadViewer.screenOf((() => { const pl = v.plane, p = r.sample; const [o, n, x] = [pl.origin, pl.normal, pl.x_dir]; const y = [n[1]*x[2]-n[2]*x[1], n[2]*x[0]-n[0]*x[2], n[0]*x[1]-n[1]*x[0]]; return [0,1,2].map((k) => o[k] + x[k]*p[0] + y[k]*p[1]); })()); })()`);
    await b.click(...pick, { wait: 800 });
    await b.clickText("Extrusión");
    await sleep(2500);
    await accept(b, 2500);
    const r = await evaluate();
    if (r.status.some((x) => x.state !== "ok")) throw new Error(JSON.stringify(r.status));
    const small = await b.eval(`Math.min(...__cadStore.result().sketches[0].regions.map((x) => x.outer.area))`);
    near(r.body.volume, small * 10, small * 0.01, "solo lo común");
  },

  async "texto: caja, se elige entero y doble clic lo cambia"(b) {
    await begin(b);
    await sketchOn(b);
    await b.clickText("Texto");
    await sleep(300);
    await b.click(450, 400, { wait: 2500 });
    await b.key("Escape", "Escape", 27);
    await sleep(300);
    await b.eval(`document.querySelector('[aria-label="Elegir"]').click()`);
    await sleep(300);
    const t = await b.eval(`JSON.parse(JSON.stringify(__cadUi.session().sketch.texts[0]))`);
    if (!t) throw new Error("no se creó el texto");
    // Clic dentro de la caja: el ancla (todo el texto)
    const box = await b.eval(`(() => { const s = __cadUi.session(); const t = s.sketch.texts[0]; const at = new Map(s.sketch.points.map((p) => [p.id, p])); const xs = t.points.map((i) => at.get(i).x), ys = t.points.map((i) => at.get(i).y); return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2]; })()`);
    const pl = await b.eval(`__cadUi.session().plane`);
    const world = [0, 1, 2].map((k) => pl.origin[k] + pl.x_dir[k] * box[0] + [pl.normal[1] * pl.x_dir[2] - pl.normal[2] * pl.x_dir[1], pl.normal[2] * pl.x_dir[0] - pl.normal[0] * pl.x_dir[2], pl.normal[0] * pl.x_dir[1] - pl.normal[1] * pl.x_dir[0]][k] * box[1]);
    const [x, y] = await b.eval(`__cadViewer.screenOf(${JSON.stringify(world)})`);
    await b.click(x, y, { wait: 900 });
    const sel = await b.eval(`__cadUi.selection()`);
    if (sel.length !== 1 || sel[0] !== t.anchor) throw new Error(`elegido: ${JSON.stringify(sel)}`);
    // Doble clic: el campo encima de la caja
    await b.click(x, y, { wait: 50 });
    await b.click(x, y, { wait: 0 });
    await sleep(600);
    if (!(await b.eval(`!!document.querySelector("[data-text-edit]")`))) throw new Error("no apareció el campo");
    const focused = await b.eval(`document.activeElement?.matches("[data-text-edit]")`);
    if (!focused) throw new Error("el campo no tiene el foco");
    await b.eval(`(() => { const i = document.querySelector("[data-text-edit]"); i.select(); document.execCommand("insertText", false, "Hola"); })()`);
    await b.key("Enter", "Enter", 13);
    await sleep(2500);
    const now = await b.eval(`__cadUi.session().sketch.texts[0].text`);
    if (now !== "Hola") throw new Error(`texto: ${now}`);
    await b.shot("texto-caja");
  },
  // ─── Plan de operaciones (F1–F3, las fáciles) ──────────────────────────
  // node apps/web/e2e/cad.mjs operaciones

  async "operaciones: simetría del cuerpo como pieza aparte"(b) {
    await begin(b);
    // Caja de 20 apoyada en el plano lateral (x de 0 a 20): el reflejo la toca
    await b.clickText("Caja");
    await sleep(1500);
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.origin = [10, 0, 0]; })`);
    await sleep(1200);
    await accept(b);
    await b.clickText("Simetría");
    await lastIs(b, "mirror");
    // Todo el cuerpo, en el plano lateral (YZ, el de por defecto)
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.features = []; })`);
    await sleep(2000);
    let ps = (await evaluate()).parts;
    if (ps.length !== 1) throw new Error(`unido: ${ps.length} piezas`);
    near((await body()).volume, 16000, 1e-3, "unido");
    // Como pieza aparte: dos piezas de 8000
    await b.clickText("El reflejo como pieza aparte");
    await sleep(2000);
    ps = (await evaluate()).parts;
    if (ps.length !== 2) throw new Error(`aparte: ${ps.length} piezas`);
    for (const p of ps) near(p.volume, 8000, 1e-3, "cada pieza");
    const r = await body();
    near(r.bbox_min[0], -20, 1e-6, "reflejo en −X");
    await accept(b);
    if ((await lastKind(b)).op !== "new") throw new Error("no quedó op: new");
    await b.shot("operaciones_simetria_aparte");
  },

  async "operaciones: mover, girar y copiar el cuerpo"(b) {
    await begin(b);
    // Caja de 20 en x de 0 a 20 (y, z de −10 a 10)
    await b.clickText("Caja");
    await sleep(1500);
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.origin = [10, 0, 0]; })`);
    await sleep(1200);
    await accept(b);
    await b.clickText("Mover cuerpo");
    await lastIs(b, "transform");
    await sleep(2000);
    // Por defecto 20 en X
    let r = await body();
    near(r.bbox_min[0], 20, 1e-6, "desplazado por defecto");
    // X = 0, Z = 15
    await setInputIn(b, '[aria-label="Desplazamiento"] input', 0, 0);
    await sleep(1200);
    await setInputIn(b, '[aria-label="Desplazamiento"] input', 2, 15);
    await sleep(2000);
    r = await body();
    near(r.bbox_min[0], 0, 1e-6, "x sin mover");
    near(r.bbox_min[2], 5, 1e-6, "subido 15");
    near(r.volume, 8000, 1e-3, "mismo volumen");
    // Girar 90° alrededor de Z: (x, y) → (−y, x)
    await chooseOption(b, "Movimiento", "Girar");
    r = await body();
    near(r.bbox_min[0], -10, 1e-6, "girado: x");
    near(r.bbox_min[1], 0, 1e-6, "girado: y desde 0");
    near(r.bbox_max[1], 20, 1e-6, "girado: y hasta 20");
    await setInput(b, "Ángulo", 180);
    await sleep(2000);
    r = await body();
    near(r.bbox_min[0], -20, 1e-6, "media vuelta");
    near(r.bbox_max[0], 0, 1e-6, "media vuelta");
    // De un punto a otro: del origen a (20, 0, 0)
    await chooseOption(b, "Movimiento", "De un punto a otro");
    r = await body();
    near(r.bbox_min[0], 20, 1e-6, "de punto a punto");
    // Copiar: la original queda y la copia es otra pieza
    await b.clickText("Copiar (las originales quedan)");
    await sleep(2000);
    const ps = (await evaluate()).parts;
    if (ps.length !== 2) throw new Error(`copia: ${ps.length} piezas`);
    r = await body();
    near(r.bbox_min[0], 0, 1e-6, "la original");
    near(r.bbox_max[0], 40, 1e-6, "la copia");
    near(r.volume, 16000, 1e-3, "dos cajas");
    await accept(b);
    const k = await lastKind(b);
    if (!k.copy || k.motion.type !== "points") throw new Error(`guardado: ${JSON.stringify(k)}`);
    await b.shot("operaciones_mover_copiar");
  },

  async "operaciones: patrón lineal en dos direcciones y copias omitidas"(b) {
    await begin(b);
    // Placa de 100 × 60 × 2 y un agujero de Ø4 en (10, 10)
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    const P = [[0, 0], [100, 0], [100, 60], [0, 60]];
    const plate = id();
    doc.features.push({
      id: plate, name: "Placa", suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    doc.features.push({ id: id(), name: "Extrusión", suppressed: false, kind: { type: "extrude", sketch: plate, regions: { type: "all" }, extent: { type: "blind", distance: 2 }, reverse: false, op: "join", draft: 0, thin: null } });
    const hs = id();
    doc.features.push({ id: hs, name: "Círculo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 10, y: 10 }], entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 2 } }], constraints: [], next_id: 3, origin: 0 } } });
    const hole = id();
    doc.features.push({ id: hole, name: "Agujero", suppressed: false, kind: { type: "extrude", sketch: hs, regions: { type: "all" }, extent: { type: "through_all" }, reverse: false, op: "cut", draft: 0, thin: null } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const holeV = Math.PI * 4 * 2;
    await b.eval(`window.__cadStore.select(${hole})`);
    await b.clickText("Patrón lineal");
    await lastIs(b, "pattern");
    await sleep(1500);
    // 3 en X cada 20
    await setInput(b, "Separación", 20);
    await sleep(2000);
    near((await body()).volume, 12000 - 3 * holeV, 1e-3, "3 en X");
    // Segunda dirección: Y por defecto, 2 cada 30 → grilla de 3 × 2
    await b.clickText("También en otra dirección");
    await sleep(1500);
    await setInput(b, "Separación 2", 30);
    await sleep(2000);
    near((await body()).volume, 12000 - 6 * holeV, 1e-3, "grilla 3 × 2");
    const k = await lastKind(b);
    if (k.pattern.second?.direction?.[1] !== 1) throw new Error(`segunda dirección: ${JSON.stringify(k.pattern.second)}`);
    // Omitir la copia 4 (segunda fila, segunda columna) y la 2
    await setInput(b, "Omitir copias", "4, 2");
    await sleep(2000);
    near((await body()).volume, 12000 - 4 * holeV, 1e-3, "sin las copias 2 y 4");
    await b.shot("operaciones_patron_grilla_omitidas");
    await accept(b);
    const saved = await lastKind(b);
    if (JSON.stringify(saved.skip) !== "[2,4]") throw new Error(`omitidas: ${JSON.stringify(saved.skip)}`);
    // Volver a abrir y quitar las omitidas: vuelven las 6
    await clickRow(b, (await b.eval(`window.__cadStore.doc().features.at(-1).name`)));
    await sleep(1200);
    await setInput(b, "Omitir copias", "");
    await sleep(2000);
    near((await body()).volume, 12000 - 6 * holeV, 1e-3, "sin omitir");
    await accept(b);
  },

  async "operaciones: revolución simétrica y a dos ángulos"(b) {
    await begin(b);
    // En el frente: rectángulo x 10..15, z 0..20; gira alrededor de Z
    const doc = await call("cad_get_document");
    const P = [[10, 0], [15, 0], [15, 20], [10, 20]];
    const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))];
    const line = (id, a, c) => ({ id, geometry: { type: "line", start: a, end: c } });
    const sketch = { points, entities: [line(7, 1, 2), line(8, 2, 3), line(9, 3, 4), line(10, 4, 1)], constraints: [], next_id: 12, origin: 0 };
    doc.features.push({ id: doc.next_id, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Revolución");
    await lastIs(b, "revolve");
    await sleep(2000);
    const full = Math.PI * (15 ** 2 - 10 ** 2) * 20;
    near((await body()).volume, full, 1e-3, "vuelta completa");
    // 90° hacia un lado: todo en y ≥ 0 (o ≤ 0)
    await setInput(b, "Ángulo", 90);
    await sleep(2000);
    let r = await body();
    near(r.volume, full / 4, 1e-3, "un lado");
    if (Math.min(Math.abs(r.bbox_min[1]), Math.abs(r.bbox_max[1])) > 1e-6) throw new Error(`un lado: y de ${r.bbox_min[1]} a ${r.bbox_max[1]}`);
    // Simétrica: 45° a cada lado del plano del sketch
    await chooseOption(b, "Hacia", "Simétrica");
    r = await body();
    near(r.volume, full / 4, 1e-3, "simétrica");
    near(r.bbox_min[1], -r.bbox_max[1], 1e-6, "simétrica respecto del frente");
    if (r.bbox_max[1] < 5) throw new Error(`simétrica: y hasta ${r.bbox_max[1]}`);
    // Dos ángulos: 90 + 45 por defecto, luego 90 + 90
    await chooseOption(b, "Hacia", "Dos ángulos");
    near((await body()).volume, (full * 135) / 360, 1e-3, "90 + 45");
    await setInput(b, "Hacia atrás", 90);
    await sleep(2000);
    near((await body()).volume, full / 2, 1e-3, "90 + 90");
    // Entre los dos más de una vuelta: la operación da error
    await setInput(b, "Hacia atrás", 300);
    await sleep(2000);
    const st = (await evaluate()).status.at(-1);
    if (st.state !== "error") throw new Error(`más de 360°: ${JSON.stringify(st)}`);
    await setInput(b, "Hacia atrás", 90);
    await sleep(2000);
    await accept(b);
    const k = await lastKind(b);
    if (k.extent?.type !== "two_angles" || k.extent.second !== 90) throw new Error(`guardado: ${JSON.stringify(k.extent)}`);
    await b.shot("operaciones_revolucion_dos_angulos");
  },

  async "operaciones: extrusión hasta un punto"(b) {
    await begin(b);
    // Cuadrado de 10 centrado en la planta
    const doc = await call("cad_get_document");
    const P = [[-5, -5], [5, -5], [5, 5], [-5, 5]];
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))],
      entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })),
      constraints: [], next_id: 20, origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Cuadrado", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Extrusión");
    await lastIs(b, "extrude");
    await sleep(2000);
    await chooseOption(b, "Hasta", "Hasta un punto");
    // Por defecto el punto (0, 0, 10)
    let r = await body();
    near(r.volume, 1000, 1e-3, "hasta z = 10");
    near(r.bbox_max[2], 10, 1e-6, "hasta z = 10");
    // El punto a z = −6: va hacia abajo
    const point = `[data-point-field="Hasta el punto"] input[type=number]`;
    if (!(await setInputIn(b, point, 2, -6))) throw new Error("no encontré las coordenadas del punto");
    await sleep(2000);
    r = await body();
    near(r.volume, 600, 1e-3, "hasta z = −6");
    near(r.bbox_min[2], -6, 1e-6, "hacia abajo");
    near(r.bbox_max[2], 0, 1e-6, "desde la planta");
    await accept(b);
    const k = await lastKind(b);
    if (k.extent.type !== "up_to_vertex" || k.extent.point.point[2] !== -6) throw new Error(`guardado: ${JSON.stringify(k.extent)}`);
    await b.shot("operaciones_extrusion_hasta_punto");
  },

  async "operaciones: agujero cónico y rosca en pulgadas"(b) {
    await begin(b);
    // Caja de 20 (z de −10 a 10) y un sketch arriba con un punto en el centro
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`__cadStore.commit((d) => {
      d.features.push({ id: d.next_id, name: "Centros", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 10, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 0 }], entities: [{ id: 2, geometry: { type: "point", point: 1 }, construction: false }], constraints: [], next_id: 3, origin: 0 } } });
      d.next_id += 1;
    })`);
    await sleep(1500);
    await b.clickText("Agujero");
    await lastIs(b, "hole");
    await sleep(2000);
    const frustum = (h, r1, r2) => ((Math.PI * h) / 3) * (r1 * r1 + r1 * r2 + r2 * r2);
    const tan = (deg) => Math.tan((deg * Math.PI) / 180);
    // M6 pasante (Ø 6,6) cónico a 1,7833° (NPT): se angosta los 20 de alto
    await chooseOption(b, "Tipo", "Cónico");
    near((await body()).volume, 8000 - frustum(20, 3.3, 3.3 - 20 * tan(1.7833)), 0.05, "cónico NPT");
    await setInput(b, "Cono: ángulo", 3);
    await sleep(2000);
    near((await body()).volume, 8000 - frustum(20, 3.3, 3.3 - 20 * tan(3)), 0.05, "cónico a 3°");
    // Tanto ángulo que se cierra: error
    await setInput(b, "Cono: ángulo", 20);
    await sleep(2000);
    if ((await evaluate()).status.at(-1).state !== "error") throw new Error("el cono cerrado no dio error");
    await setInput(b, "Cono: ángulo", 3);
    await sleep(2000);
    // Pulgadas: la medida más parecida a M6 es 1/4" (pasante de 0,266")
    await chooseOption(b, "Sistema", "Pulgadas (UNC/UNF)");
    let k = await lastKind(b);
    near(k.diameter, 0.266 * 25.4, 1e-6, "pasante de 1/4\"");
    // Para roscar 1/4-20 UNC: broca #7 (0,201") y la rosca cosmética
    await chooseOption(b, "Para", "Roscar (rosca cosmética)");
    k = await lastKind(b);
    if (k.thread !== "1/4-20 UNC") throw new Error(`rosca: ${k.thread}`);
    const r = (0.201 * 25.4) / 2;
    near((await body()).volume, 8000 - frustum(20, r, r - 20 * tan(3)), 0.05, "broca de 1/4-20 cónica");
    if (!(await b.eval(`document.body.innerText.includes("Rosca 1/4-20 UNC")`))) throw new Error("no se muestra la rosca");
    await accept(b);
    const threads = (await evaluate()).threads;
    if (threads.length !== 1) throw new Error(`roscas: ${threads.length}`);
    near(threads[0].spec.nominal, 6.35, 1e-6, "rosca de 1/4\"");
    near(threads[0].spec.pitch, 1.27, 1e-6, "20 hilos por pulgada");
    await b.shot("operaciones_agujero_conico_pulgadas");
  },

  async "operaciones: patrón por puntos de un sketch"(b) {
    await begin(b);
    // Placa de 100 × 60 × 2, agujero de Ø4 en (10, 10) y un sketch con cuatro puntos (el primero sobre el agujero)
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    const P = [[0, 0], [100, 0], [100, 60], [0, 60]];
    const plate = id();
    doc.features.push({
      id: plate, name: "Placa", suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    doc.features.push({ id: id(), name: "Extrusión", suppressed: false, kind: { type: "extrude", sketch: plate, regions: { type: "all" }, extent: { type: "blind", distance: 2 }, reverse: false, op: "join", draft: 0, thin: null } });
    const hs = id();
    doc.features.push({ id: hs, name: "Círculo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 10, y: 10 }], entities: [{ id: 2, geometry: { type: "circle", center: 1, radius: 2 } }], constraints: [], next_id: 3, origin: 0 } } });
    const hole = id();
    doc.features.push({ id: hole, name: "Agujero", suppressed: false, kind: { type: "extrude", sketch: hs, regions: { type: "all" }, extent: { type: "through_all" }, reverse: false, op: "cut", draft: 0, thin: null } });
    const Q = [[10, 10], [40, 10], [70, 30], [90, 50]];
    doc.features.push({
      id: id(), name: "Puntos", suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...Q.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: Q.map((_, i) => ({ id: 11 + i, geometry: { type: "point", point: i + 1 }, construction: false })), constraints: [], next_id: 20, origin: 0 } },
    });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    const holeV = Math.PI * 4 * 2;
    await b.eval(`window.__cadStore.select(${hole})`);
    await b.clickText("Patrón por puntos");
    await lastIs(b, "pattern");
    await sleep(2500);
    const k = await lastKind(b);
    if (k.pattern.type !== "points" || JSON.stringify(k.features) !== `[${hole}]`) throw new Error(`patrón: ${JSON.stringify(k)}`);
    // En el punto del original no se copia: 4 agujeros
    near((await body()).volume, 12000 - 4 * holeV, 1e-3, "un agujero por punto");
    if (!(await b.eval(`document.body.innerText.includes("4 puntos sueltos")`))) throw new Error("no dice cuántos puntos");
    // Omitir la segunda copia (el punto de (70, 30))
    await setInput(b, "Omitir copias", "2");
    await sleep(2000);
    near((await body()).volume, 12000 - 3 * holeV, 1e-3, "sin la copia 2");
    await accept(b);
    await b.shot("operaciones_patron_por_puntos");
  },

  async "operaciones: revolución delgada"(b) {
    await begin(b);
    // En el frente: rectángulo x 10..15, z 0..20; gira alrededor de Z
    const doc = await call("cad_get_document");
    const P = [[10, 0], [15, 0], [15, 20], [10, 20]];
    const points = [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))];
    const line = (id, a, c) => ({ id, geometry: { type: "line", start: a, end: c } });
    const sketch = { points, entities: [line(7, 1, 2), line(8, 2, 3), line(9, 3, 4), line(10, 4, 1)], constraints: [], next_id: 12, origin: 0 };
    doc.features.push({ id: doc.next_id, name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Revolución");
    await lastIs(b, "revolve");
    await sleep(2000);
    await b.clickText("Delgada (solo una pared)");
    await sleep(2000);
    // Banda de 1 alrededor del rectángulo de 5 × 20 (esquinas de afuera redondeadas), centrada en x = 12,5: Pappus
    const band = (t) => 100 + (t / 2) * 50 + Math.PI * (t / 2) ** 2 - (5 - t) * (20 - t);
    let v = 2 * Math.PI * 12.5 * band(1);
    near((await body()).volume, v, v * 1e-3, "delgada de 1");
    await setInput(b, "Espesor", 2);
    await sleep(2000);
    v = 2 * Math.PI * 12.5 * band(2);
    near((await body()).volume, v, v * 1e-3, "delgada de 2");
    // Con 90° simétrica sigue siendo delgada: un cuarto
    await setInput(b, "Ángulo", 90);
    await sleep(1500);
    await chooseOption(b, "Hacia", "Simétrica");
    near((await body()).volume, v / 4, v * 1e-3, "un cuarto, delgada");
    await accept(b);
    if ((await lastKind(b)).thin !== 2) throw new Error("no quedó delgada");
    await b.shot("operaciones_revolucion_delgada");
  },

  async "operaciones: espiral plana y hélice cónica"(b) {
    await begin(b);
    await b.clickText("Espiral");
    await lastIs(b, "helix");
    await sleep(1500);
    const curve = async () => {
      const refs = (await evaluate()).references;
      const c = refs.find((r) => r.kind === "curve");
      if (!c) throw new Error(`sin curva: ${JSON.stringify(refs.map((r) => r.kind))}`);
      return c.points;
    };
    const radial = (p) => Math.hypot(p[0], p[1]);
    // Espiral plana de radio 2 a 20 en 4 vueltas: todo en z = 0
    let pts = await curve();
    if (pts.some((p) => Math.abs(p[2]) > 1e-6)) throw new Error("la espiral no es plana");
    near(Math.max(...pts.map(radial)), 20, 0.05, "radio final");
    near(Math.min(...pts.map(radial)), 2, 0.05, "radio inicial");
    // Cónica: sin «Espiral plana», sube 4 vueltas de 5
    await b.clickText("Espiral plana");
    await sleep(1500);
    pts = await curve();
    const top = pts.reduce((a, p) => (p[2] > a[2] ? p : a));
    near(top[2], 20, 1e-3, "alto de la cónica");
    near(radial(top), 20, 0.05, "arriba, el radio final");
    await setInput(b, "Radio final", 8);
    await sleep(1500);
    pts = await curve();
    near(radial(pts.reduce((a, p) => (p[2] > a[2] ? p : a))), 8, 0.05, "radio final 8");
    // Sin «Cónica»: la hélice de siempre, de radio 2
    await b.clickText("Cónica (dos radios)");
    await sleep(1500);
    pts = await curve();
    if (pts.some((p) => Math.abs(radial(p) - 2) > 0.05)) throw new Error("la hélice no quedó de radio 2");
    await accept(b);
    const k = await lastKind(b);
    if (k.radius2 != null || k.flat) throw new Error(`guardado: ${JSON.stringify(k)}`);
  },

  async "operaciones: superficie plana y desfasada"(b) {
    await begin(b);
    // Rectángulo de 10 × 20 en la planta
    const doc = await call("cad_get_document");
    const P = [[0, 0], [10, 0], [10, 20], [0, 20]];
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))],
      entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })),
      constraints: [], next_id: 20, origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Rectángulo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Superficie plana");
    await lastIs(b, "surface_planar");
    await sleep(2000);
    await accept(b);
    let ps = await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result().parts))`);
    if (ps.length !== 1 || !ps[0].surface) throw new Error(`plana: ${JSON.stringify(ps.map((p) => p.surface))}`);
    near(ps[0].area, 200, 1e-6, "área de la plana");
    // Desfasada 5 (por defecto): otra igual a 5 del plano
    await b.clickText("Superficie desfasada");
    await lastIs(b, "surface_offset");
    await sleep(2000);
    ps = await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result().parts))`);
    if (ps.length !== 2 || !ps[1].surface) throw new Error(`desfasada: ${ps.length} piezas`);
    near(ps[1].area, 200, 1e-3, "área de la desfasada");
    const reach = async () => {
      const r = await body();
      return Math.max(Math.abs(r.bbox_min[2]), Math.abs(r.bbox_max[2]));
    };
    near(await reach(), 5, 1e-6, "a 5 del plano");
    await setInput(b, "Distancia", 3);
    await sleep(2000);
    near(await reach(), 3, 1e-6, "a 3 del plano");
    await accept(b);
    await b.shot("operaciones_superficie_plana_desfasada");
  },

  // ─── Tanda media ───

  async "operaciones: alinear un cuerpo a un plano"(b) {
    await begin(b);
    // Caja de 20 centrada (de −10 a 10 en cada eje)
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const top = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    const ref = await b.eval(`window.__cadStore.faceRef(${top})`);
    await b.clickText("Mover cuerpo");
    await lastIs(b, "transform");
    await sleep(1500);
    // La cara de arriba apoyada en la planta: la caja queda de z = 0 a 20
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.motion = { type: "align", face: ${JSON.stringify(ref)}, to: { type: "xy" }, flip: false }; })`);
    await sleep(2000);
    let r = await body();
    near(r.bbox_min[2], 0, 1e-6, "apoyada en la planta");
    near(r.bbox_max[2], 20, 1e-6, "arriba");
    // En el frente (normal −Y): la caja queda del lado −Y, de y = −20 a 0
    await chooseOption(b, "Sobre el plano", "Frente (XZ)");
    r = await body();
    near(r.bbox_max[1], 0, 1e-6, "contra el frente");
    near(r.bbox_min[1], -20, 1e-6, "del lado −Y");
    // Del otro lado: de y = 0 a 20
    await b.clickText("Del otro lado del plano");
    await sleep(2000);
    r = await body();
    near(r.bbox_min[1], 0, 1e-6, "del otro lado");
    near(r.volume, 8000, 1e-3, "mismo volumen");
    await accept(b);
    await b.shot("operaciones_alinear_cuerpo");
  },

  async "operaciones: eliminar cara y vaciado con espesor por cara"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Un agujero de Ø8 que la atraviesa
    await b.eval(`__cadStore.commit((d) => {
      d.features.push({ id: d.next_id, name: "Agujero", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 4, height: 40 }, origin: [0, 0, -20], z: [0, 0, 1], x: [1, 0, 0], op: "cut" } });
      d.next_id += 1;
    })`);
    await sleep(2000);
    near((await body()).volume, 8000 - Math.PI * 16 * 20, 1e-3, "con agujero");
    // Eliminar la pared del agujero: se tapa
    const wall = await faceWhere((f) => f.surface === "cylinder");
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${wall} }])`);
    await b.clickText("Eliminar cara");
    await lastIs(b, "delete_face");
    await sleep(2500);
    near((await body()).volume, 8000, 1e-3, "agujero tapado");
    await accept(b);
    // Vaciado abierto arriba, paredes de 1; el fondo con 3 y después 5
    const topFace = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    const bottom = await b.eval(`window.__cadStore.faceRef(${await faceWhere((f) => f.surface === "plane" && f.normal[2] < -0.99)})`);
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${topFace} }])`);
    await b.clickText("Vaciado");
    await lastIs(b, "shell");
    await sleep(2500);
    near((await body()).volume, 8000 - 18 * 18 * 19, 1e-3, "vaciado de 1");
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.overrides = [{ face: ${JSON.stringify(bottom)}, thickness: 3 }]; })`);
    await sleep(2500);
    near((await body()).volume, 8000 - 18 * 18 * 17, 1e-3, "fondo de 3");
    if (!(await setInput(b, "Espesor cara 1", 5))) throw new Error("no está el espesor de la cara");
    await sleep(2500);
    near((await body()).volume, 8000 - 18 * 18 * 15, 1e-3, "fondo de 5");
    await accept(b);
    await b.shot("operaciones_vaciado_por_cara");
  },

  async "operaciones: redondeo con radios en el medio"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // La arista de arriba al frente, radio 2
    await b.click(...(await at([0, -10, 10])), { wait: 800 });
    await b.clickText("Redondeo");
    await lastIs(b, "fillet");
    await sleep(1500);
    await setInput(b, "Radio", 2);
    await sleep(1500);
    const cut = (r) => (1 - Math.PI / 4) * r * r * 20;
    near((await body()).volume, 8000 - cut(2), 0.05, "radio 2");
    // Un radio de 4 en el medio: saca más que el de 2 y menos que el de 4 entero
    await b.clickText("Agregar radio en el medio");
    await sleep(2000);
    let k = await lastKind(b);
    if (k.points?.length !== 1 || k.points[0].at !== 0.5) throw new Error(`puntos: ${JSON.stringify(k.points)}`);
    const v4 = (await body()).volume;
    if (!(v4 < 8000 - cut(2) && v4 > 8000 - cut(4))) throw new Error(`con 4 en el medio: ${v4}`);
    // 5 en el medio: saca más todavía
    await setInputIn(b, '[data-fillet-point="0"] input', 1, 5);
    await sleep(2000);
    const v5 = (await body()).volume;
    if (!(v5 < v4 && v5 > 8000 - cut(5))) throw new Error(`con 5 en el medio: ${v5}`);
    await accept(b);
    k = await lastKind(b);
    if (k.points[0].radius !== 5) throw new Error(`guardado: ${JSON.stringify(k.points)}`);
    await b.shot("operaciones_redondeo_por_puntos");
  },

  async "operaciones: extrusión con dirección propia y desde un plano corrido"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = [[0, 0], [10, 0], [10, 10], [0, 10]];
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))],
      entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })),
      constraints: [], next_id: 20, origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Cuadrado", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Extrusión");
    await lastIs(b, "extrude");
    await sleep(1500);
    await setInput(b, "Distancia", 10);
    await sleep(1500);
    // Desde 3 sobre el plano: de z = 3 a 13
    await setInput(b, "Desde el plano a", 3);
    await sleep(2000);
    let r = await body();
    near(r.bbox_min[2], 3, 1e-6, "arranca en 3");
    near(r.bbox_max[2], 13, 1e-6, "llega a 13");
    // Dirección propia: Z (la misma) y después a 45° en X
    await b.clickText("Dirección propia");
    await sleep(1500);
    near((await body()).volume, 1000, 1e-3, "a lo largo de Z");
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.direction = { type: "custom", origin: [0, 0, 0], direction: [1, 0, 1] }; })`);
    await sleep(2000);
    const h = 10 / Math.SQRT2;
    r = await body();
    near(r.volume, 100 * h, 1e-3, "prisma oblicuo");
    near(r.bbox_max[0], 10 + h + 3, 1e-6, "corrido en X");
    await accept(b);
    await b.shot("operaciones_extrusion_direccion");
  },

  async "operaciones: tubo, barrido delgado y superficie por barrido"(b) {
    await begin(b);
    // Perfil: una recta de 6 en la planta; camino: recta de 20 hacia arriba en el frente
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    doc.features.push({ id: id(), name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: -3, y: 0 }, { id: 2, x: 3, y: 0 }], entities: [{ id: 3, geometry: { type: "line", start: 1, end: 2 } }], constraints: [], next_id: 4, origin: 0 } } });
    doc.features.push({ id: id(), name: "Camino", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 20 }], entities: [{ id: 2, geometry: { type: "line", start: 0, end: 1 } }], constraints: [], next_id: 3, origin: 0 } } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    // Tubo por el último sketch (el camino), radio 2 por defecto
    await b.clickText("Tubo");
    await lastIs(b, "sweep");
    await sleep(2500);
    near((await body()).volume, Math.PI * 4 * 20, 1e-3, "tubo de radio 2");
    await setInput(b, "Radio del tubo", 3);
    await sleep(2000);
    near((await body()).volume, Math.PI * 9 * 20, 1e-3, "tubo de radio 3");
    await b.clickText("Delgado (solo una pared)");
    await sleep(2000);
    near((await body()).volume, Math.PI * (3.5 ** 2 - 2.5 ** 2) * 20, 1e-3, "caño de pared 1");
    await chooseOption(b, "Orientación", "Fija (no gira)");
    near((await body()).volume, Math.PI * (3.5 ** 2 - 2.5 ** 2) * 20, 1e-3, "fijo en una recta: lo mismo");
    await accept(b);
    if ((await lastKind(b)).orient !== "fixed") throw new Error("no quedó fija");
    // Superficie por barrido: la recta del perfil por el camino (los dos últimos sketches)
    await b.clickText("Superficie por barrido");
    await lastIs(b, "surface_sweep");
    await sleep(2500);
    await accept(b);
    const ps = await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result().parts))`);
    const sheet = ps.find((p) => p.surface);
    if (!sheet) throw new Error(`sin superficie: ${JSON.stringify(ps.map((p) => p.surface))}`);
    near(sheet.area, 120, 1e-3, "superficie de 6 × 20");
    await b.shot("operaciones_tubo_y_superficie_por_barrido");
  },

  async "operaciones: barrido con torsión"(b) {
    await begin(b);
    // Perfil: cuadrado de 4 en la planta; camino: recta de 20 hacia arriba en el frente
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    const P = [[-2, -2], [2, -2], [2, 2], [-2, 2]];
    const square = { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 };
    doc.features.push({ id: id(), name: "Perfil", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: square } });
    doc.features.push({ id: id(), name: "Camino", suppressed: false, kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 0, y: 20 }], entities: [{ id: 2, geometry: { type: "line", start: 0, end: 1 } }], constraints: [], next_id: 3, origin: 0 } } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Barrido");
    await lastIs(b, "sweep");
    await sleep(2500);
    near((await body()).volume, 16 * 20, 1e-3, "prisma de 4 × 4 × 20");
    near((await body()).bbox_max[0], 2, 1e-3, "sin torsión");
    // 45°: el volumen no cambia y la esquina pasa por el eje x (2√2)
    await setInput(b, "Torsión", 45);
    await sleep(3000);
    near((await body()).volume, 16 * 20, 1e-2, "con torsión, mismo volumen");
    near((await body()).bbox_max[0], 2 * Math.SQRT2, 2e-2, "la esquina gira");
    await accept(b);
    if ((await lastKind(b)).twist !== 45) throw new Error("no quedó la torsión");
    await b.shot("operaciones_barrido_con_torsion");
  },

  async "operaciones: extender superficie"(b) {
    await begin(b);
    const doc = await call("cad_get_document");
    const P = [[0, 0], [10, 0], [10, 20], [0, 20]];
    const sketch = {
      points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))],
      entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })),
      constraints: [], next_id: 20, origin: 0,
    };
    doc.features.push({ id: doc.next_id, name: "Rectángulo", suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch } });
    doc.next_id += 1;
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Superficie plana");
    await lastIs(b, "surface_planar");
    await sleep(2000);
    await accept(b);
    // El borde de x = 10, 5 más allá (por defecto): de 10 × 20 a 15 × 20
    const edge = await edgeNear([10, 10, 0]);
    await b.eval(`window.__cadUi.setPicks([{ kind: "edge", edge: ${edge} }])`);
    await b.clickText("Extender superficie");
    await lastIs(b, "surface_extend");
    await sleep(2500);
    const area = async () => (await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.result().parts))`))[0].area;
    near(await area(), 300, 1e-3, "extendida 5");
    near((await body()).bbox_max[0], 15, 1e-6, "hasta x = 15");
    await setInput(b, "Distancia", 10);
    await sleep(2000);
    near(await area(), 400, 1e-3, "extendida 10");
    await accept(b);
    await b.shot("operaciones_extender_superficie");
  },

  async "operaciones: copiar, pegar y duplicar operaciones"(b) {
    await begin(b);
    // Cuadrado de 10 y su extrusión de 5
    const doc = await call("cad_get_document");
    const P = [[0, 0], [10, 0], [10, 10], [0, 10]];
    const sk = doc.next_id++;
    doc.features.push({
      id: sk, name: "Cuadrado", suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    const ex = doc.next_id++;
    doc.features.push({ id: ex, name: "Bloque", suppressed: false, kind: { type: "extrude", sketch: sk, regions: { type: "all" }, extent: { type: "blind", distance: 5 }, reverse: false, op: "join", draft: 0, thin: null } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    near((await body()).volume, 500, 1e-3, "un bloque");
    const rowAt = (name) =>
      b.eval(`(() => { const e = [...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === ${JSON.stringify(name)}); if (!e) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
    // Elegir el sketch y, con Mayús, la extrusión: el tramo de los dos
    await clickRow(b, "Cuadrado");
    await b.click(...(await rowAt("Bloque")), { wait: 800, modifiers: 8 });
    const key = (k) => b.eval(`window.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)}, ctrlKey: true, bubbles: true }))`);
    await key("c");
    await sleep(500);
    await key("v");
    await sleep(2500);
    const feats = await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features.map((f) => ({ id: f.id, name: f.name, kind: f.kind }))))`);
    const sCopy = feats.find((f) => f.name === "Cuadrado (copia)");
    const eCopy = feats.find((f) => f.name === "Bloque (copia)");
    if (!sCopy || !eCopy) throw new Error(`pegado: ${feats.map((f) => f.name)}`);
    if (eCopy.kind.sketch !== sCopy.id) throw new Error(`la extrusión copiada usa ${eCopy.kind.sketch}, no la copia del sketch ${sCopy.id}`);
    // La copia del sketch 10 más arriba: dos bloques
    await b.eval(`__cadStore.commit((d) => { d.features.find((f) => f.id === ${sCopy.id}).kind.offset = 10; })`);
    await sleep(2000);
    near((await body()).volume, 1000, 1e-3, "dos bloques");
    // Duplicar solo la extrusión original (un clic normal deja de lado el tramo): la copia sigue usando el sketch de afuera
    await clickRow(b, "Bloque");
    await b.eval(`(() => { const s = [...document.querySelectorAll("[data-feature-row] span")].find((x) => x.textContent === "Bloque"); s.closest("[data-feature-row]").querySelector('[aria-label="Duplicar"]').click(); })()`);
    await sleep(2500);
    const dup = (await b.eval(`JSON.parse(JSON.stringify(window.__cadStore.doc().features))`)).filter((f) => f.name === "Bloque (copia)").pop();
    if (dup.kind.sketch !== sk) throw new Error(`duplicada usa ${dup.kind.sketch} (esperado ${sk})`);
    await b.shot("operaciones_copiar_pegar");
  },

  // ─── Los difíciles ───

  async "operaciones: planos, ejes y puntos por clics"(b) {
    const at = (p) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(p)})`);
    await begin(b);
    // Caja de 20 centrada (de −10 a 10)
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const ref = async () => {
      const id = await b.eval(`window.__cadStore.doc().features.at(-1).id`);
      return (await evaluate()).references.find((r) => r.id === id);
    };
    const top = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    const bottom = await faceWhere((f) => f.surface === "plane" && f.normal[2] < -0.99);
    const side = await faceWhere((f) => f.surface === "plane" && f.normal[0] > 0.99);
    // Arriba y abajo elegidas antes: plano medio (z = 0)
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${top} }, { kind: "face", face: ${bottom} }])`);
    await b.clickText("Plano");
    await lastIs(b, "plane");
    await sleep(2000);
    let r = await ref();
    if (r?.how !== "Plano medio") throw new Error(`deducido: ${r?.how}`);
    near(r.plane.origin[2], 0, 1e-6, "plano medio a z = 0");
    if (!(await b.eval(`document.querySelector("[data-ref-how]")?.textContent.includes("Plano medio")`))) throw new Error("el diálogo no dice qué se armó");
    await accept(b);

    // Sin nada elegido: desfasado de la planta; se cambia por clics en el visor
    await b.clickText("Plano");
    await lastIs(b, "plane");
    await sleep(1500);
    if ((await ref())?.how !== "Desfasado de un plano o una cara") throw new Error("el plano nuevo no sale de la planta");
    await setInput(b, "Distancia", 0);
    await b.eval(`document.querySelector('[aria-label="Quitar Planta (XY)"]').click()`);
    await sleep(800);
    await b.clickText("Elegir en el visor");
    await sleep(500);
    // La cara de arriba y la arista de arriba al frente: por la arista, en ángulo con la cara
    await b.click(...(await at([3, 3, 10])), { wait: 1500 });
    await b.click(...(await at([0, -10, 10])), { wait: 1500 });
    await sleep(1500);
    const kinds = await b.eval(`JSON.stringify(window.__cadStore.doc().features.at(-1).kind.def.refs.map((x) => x.type))`);
    if (kinds !== '["face","edge"]') throw new Error(`elegido: ${kinds}`);
    r = await ref();
    if (r?.how !== "Por una línea, en ángulo con el plano") throw new Error(`deducido: ${r?.how}`);
    near(Math.abs(r.plane.normal[2]), 1, 1e-6, "a 0°, paralelo a la cara");
    await setInput(b, "Ángulo", 90);
    await sleep(2000);
    r = await ref();
    near(Math.abs(r.plane.normal[1]), 1, 1e-6, "a 90°, vertical");
    near(r.plane.origin[1], -10, 1e-6, "por la arista del frente");
    await accept(b);

    // Eje: el cruce de la cara de arriba con la del costado
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${top} }, { kind: "face", face: ${side} }])`);
    await b.clickText("Eje");
    await lastIs(b, "axis");
    await sleep(2000);
    r = await ref();
    if (r?.how !== "Cruce de dos planos") throw new Error(`eje: ${r?.how}`);
    near(Math.abs(r.dir[1]), 1, 1e-6, "a lo largo de Y");
    near(r.origin[0], 10, 1e-6, "en x = 10");
    near(r.origin[2], 10, 1e-6, "en z = 10");
    await accept(b);

    // Punto: el medio entre dos vértices opuestos (el centro de la caja)
    await b.eval(`window.__cadUi.setPicks([{ kind: "vertex", at: [10, 10, 10] }, { kind: "vertex", at: [-10, -10, -10] }])`);
    await b.clickText("Punto");
    await lastIs(b, "point");
    await sleep(2000);
    r = await ref();
    if (r?.how !== "Punto medio") throw new Error(`punto: ${r?.how}`);
    for (const k of [0, 1, 2]) near(r.point[k], 0, 1e-6, "centro");
    const def = await b.eval(`JSON.stringify(window.__cadStore.doc().features.at(-1).kind.def.refs[0].point.type)`);
    if (def !== '"edge_end"') throw new Error(`el vértice no quedó ligado: ${def}`);
    await accept(b);
    await b.shot("operaciones_referencias_por_clics");
  },

  async "operaciones: curvas compuesta y por puntos como caminos"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const lines = async () => {
      const id = await b.eval(`window.__cadStore.doc().features.at(-1).id`);
      const r = (await evaluate()).references.find((x) => x.id === id);
      if (r?.kind !== "curves") throw new Error(`sin curva: ${JSON.stringify(r?.kind)}`);
      return r.lines.flat();
    };
    // Las dos aristas de arriba del frente y del costado derecho, encadenadas
    const a = await edgeNear([0, -10, 10]);
    const c = await edgeNear([10, 0, 10]);
    await b.eval(`window.__cadUi.setPicks([{ kind: "edge", edge: ${a} }, { kind: "edge", edge: ${c} }])`);
    await b.clickText("Curva compuesta");
    await lastIs(b, "curve");
    await sleep(2000);
    let pts = await lines();
    if (!pts.every((p) => Math.abs(p[2] - 10) < 1e-6)) throw new Error("la compuesta no está arriba");
    if (!pts.some((p) => Math.abs(p[0] - 10) < 1e-6 && Math.abs(p[1] - 10) < 1e-6)) throw new Error("no llega a la esquina de atrás");
    await accept(b);
    // Un tubo de radio 2 por ella (sin errores)
    await b.clickText("Tubo");
    await lastIs(b, "sweep");
    await sleep(2500);
    let st = (await evaluate()).status.at(-1);
    if (st.state !== "ok") throw new Error(`tubo por la compuesta: ${JSON.stringify(st)}`);
    await accept(b);
    // Curva por puntos (sin nada elegido: tres puntos escritos, hasta z = 10)
    await b.clickText("Curva por puntos");
    await lastIs(b, "curve");
    await sleep(2000);
    pts = await lines();
    near(Math.max(...pts.map((p) => p[2])), 10, 1e-3, "pasa por z = 10");
    await b.clickText("Agregar punto");
    await sleep(1500);
    if ((await lastKind(b)).def.points.length !== 4) throw new Error("no sumó el punto");
    await accept(b);
    await b.shot("operaciones_curvas");
  },

  async "operaciones: extrusión hasta una cara curva y revolución hasta una cara"(b) {
    await begin(b);
    // Cilindro de radio 20 acostado sobre X y un cuadrado de 10 a z = −30
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    doc.features.push({ id: id(), name: "Rollo", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 20, height: 100 }, origin: [-50, 0, 0], z: [1, 0, 0], x: [0, 1, 0], op: "join" } });
    const P = [[-5, -5], [5, -5], [5, 5], [-5, 5]];
    doc.features.push({
      id: id(), name: "Cuadrado", suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: -30, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const roll = Math.PI * 400 * 100;
    const belly = await b.eval(`window.__cadStore.faceRef(${await faceWhere((f) => f.surface === "cylinder")})`);
    await b.clickText("Extrusión");
    await lastIs(b, "extrude");
    await sleep(1500);
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.extent = { type: "up_to_face", face: ${JSON.stringify(belly)} }; })`);
    await sleep(3000);
    const under = 5 * Math.sqrt(375) + 400 * Math.asin(0.25);
    near((await body()).volume, roll + 10 * (300 - under), 0.5, "hasta la panza del cilindro");
    await accept(b);
    await b.shot("operaciones_hasta_cara_curva");
  },

  async "operaciones: revolución hasta una cara"(b) {
    await begin(b);
    // Caja en x < 0, y > 0 (su cara x = 0 a 90° del perfil) y el perfil x 10..15, z 0..20 en el frente
    const doc = await call("cad_get_document");
    const id = () => doc.next_id++;
    doc.features.push({ id: id(), name: "Tope", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 40, centered: false }, origin: [-40, 0, -10], z: [0, 0, 1], x: [1, 0, 0], op: "join" } });
    const P = [[10, 0], [15, 0], [15, 20], [10, 20]];
    doc.features.push({
      id: id(), name: "Perfil", suppressed: false,
      kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const wall = await b.eval(`window.__cadStore.faceRef(${await faceWhere((f) => f.surface === "plane" && f.normal[0] > 0.99)})`);
    await b.clickText("Revolución");
    await lastIs(b, "revolve");
    await sleep(2000);
    await chooseOption(b, "Hacia", "Hasta una cara");
    if (!(await b.eval(`document.body.innerText.includes("Hasta la cara")`))) throw new Error("no apareció la caja de la cara");
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.angle = 90; d.features.at(-1).kind.extent = { type: "up_to_face", face: ${JSON.stringify(wall)} }; })`);
    await sleep(3000);
    near((await body()).volume, 64000 + (Math.PI * (225 - 100) * 20) / 4, 0.5, "un cuarto de vuelta hasta la caja");
    await accept(b);
    await b.shot("operaciones_revolucion_hasta_cara");
  },

  async "operaciones: transición perpendicular en las puntas"(b) {
    await begin(b);
    // Cuadrado de 4 abajo y otro arriba (z = 20) corrido 10 en x
    const doc = await call("cad_get_document");
    const sq = (cx, z) => {
      const P = [[cx - 2, -2], [cx + 2, -2], [cx + 2, 2], [cx - 2, 2]];
      return { id: doc.next_id++, name: `Cuadrado ${z}`, suppressed: false, kind: { type: "sketch", plane: { type: "xy" }, offset: z, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } } };
    };
    doc.features.push(sq(0, 0), sq(10, 20));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Transición");
    await lastIs(b, "loft");
    await sleep(2000);
    // Cada corte horizontal es el cuadrado: 16 × 20 (Cavalieri), recta o en S
    near((await body()).volume, 320, 0.05, "transición recta");
    const straight = (await body()).area;
    await b.clickText("Sale perpendicular a la primera sección");
    await sleep(2500);
    await b.clickText("Llega perpendicular a la última sección");
    await sleep(2500);
    const k = await lastKind(b);
    if (k.start_tangent !== 1 || k.end_tangent !== 1) throw new Error(`tangencias: ${JSON.stringify(k)}`);
    near((await body()).volume, 320, 0.05, "en S, mismo volumen");
    // En S los costados son más largos que en la recta
    if (!((await body()).area > straight + 1)) throw new Error(`área ${(await body()).area} contra ${straight}`);
    await accept(b);
    await b.shot("operaciones_transicion_perpendicular");
  },

  async "operaciones: transición delgada, a un punto y cerrada"(b) {
    await begin(b);
    // Dos cuadrados de 10 (z = 0 y z = 10)
    const doc = await call("cad_get_document");
    const P = [[-5, -5], [5, -5], [5, 5], [-5, 5]];
    const square = (z) => ({
      id: doc.next_id++, name: `Cuadrado ${z}`, suppressed: false,
      kind: { type: "sketch", plane: { type: "xy" }, offset: z, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    doc.features.push(square(0), square(10));
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(1500);
    await b.clickText("Transición");
    await lastIs(b, "loft");
    await sleep(2000);
    near((await body()).volume, 1000, 0.5, "prisma");
    await b.clickText("Caras planas entre secciones");
    await sleep(1500);
    // Delgada de 1: caño cuadrado
    await b.clickText("Delgada (solo una pared)");
    await sleep(2500);
    near((await body()).volume, (100 + 20 + Math.PI * 0.25 - 81) * 10, 0.5, "caño de pared 1");
    await b.clickText("Delgada (solo una pared)");
    await sleep(1500);
    // Termina en (0, 0, 20): prisma más pirámide
    await b.clickText("Termina en un punto");
    await sleep(2500);
    near((await body()).volume, 1000 + 1000 / 3, 0.5, "con punta");
    // Cerrada con dos secciones y punta: error
    await b.clickText("Cerrada (vuelve a la primera)");
    await sleep(1500);
    if ((await evaluate()).status.at(-1).state !== "error") throw new Error("cerrada con punta no dio error");
    await b.clickText("Cerrada (vuelve a la primera)");
    await sleep(1500);
    await accept(b);
    const k = await lastKind(b);
    if (!k.end_point || !k.ruled) throw new Error(`guardado: ${JSON.stringify(k)}`);
    await b.shot("operaciones_transicion");
  },

  async "operaciones: girar cara con bisagra"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    const top = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${top} }])`);
    await b.clickText("Girar cara");
    await lastIs(b, "rotate_face");
    await sleep(1500);
    // Bisagra en la arista de arriba de x = −10 (a lo largo de Y), 10°: baja el lado de x = 10
    await b.eval(`__cadStore.commit((d) => { d.features.at(-1).kind.axis = { type: "custom", origin: [-10, 0, 10], direction: [0, 1, 0] }; })`);
    await sleep(2500);
    const wedge = 0.5 * 20 * 20 * Math.tan((10 * Math.PI) / 180) * 20;
    near((await body()).volume, 8000 - wedge, 0.05, "baja 10°");
    await setInput(b, "Ángulo", -10);
    await sleep(2500);
    near((await body()).volume, 8000 + wedge, 0.05, "sube 10°");
    await accept(b);
    await b.shot("operaciones_girar_cara");
  },

  async "operaciones: chaflán de vértice"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    await b.eval(`window.__cadUi.setPicks([{ kind: "vertex", at: [10, 10, 10] }])`);
    await b.clickText("Chaflán de vértice");
    await lastIs(b, "vertex_chamfer");
    await sleep(2500);
    near((await body()).volume, 8000 - 8 / 6, 1e-3, "esquina de 2");
    await setInput(b, "Distancia", 3);
    await sleep(2000);
    near((await body()).volume, 8000 - 27 / 6, 1e-3, "esquina de 3");
    await accept(b);
    const k = await lastKind(b);
    if (k.vertex.type !== "picked" || k.vertex.refs[0].point.type !== "edge_end") throw new Error(`vértice: ${JSON.stringify(k.vertex)}`);
    await b.shot("operaciones_chaflan_vertice");
  },

  async "operaciones: redondeo completo"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Los costados y = ±10 y la cara de arriba: medio cilindro de radio 10 a lo largo de X
    const a = await faceWhere((f) => f.surface === "plane" && f.normal[1] < -0.99);
    const m = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    const c = await faceWhere((f) => f.surface === "plane" && f.normal[1] > 0.99);
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${m} }, { kind: "face", face: ${a} }, { kind: "face", face: ${c} }])`);
    await b.clickText("Redondeo completo");
    await lastIs(b, "full_round");
    await sleep(2500);
    near((await body()).volume, 8000 - 4000 + (Math.PI * 100 * 20) / 2, 0.05, "medio cilindro arriba");
    near((await body()).bbox_max[2], 10, 1e-6, "misma altura");
    await accept(b);
    await b.shot("operaciones_redondeo_completo");
  },

  async "operaciones: redondeo de cara (bola que rueda)"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Arriba y el costado x+ de la caja de 20: canto de radio 2 a lo largo de 20
    const top = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    const side = await faceWhere((f) => f.surface === "plane" && f.normal[0] > 0.99);
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${top} }, { kind: "face", face: ${side} }])`);
    await b.clickText("Redondeo de cara");
    await lastIs(b, "face_blend");
    await sleep(3000);
    const lost = (1 - Math.PI / 4) * 4 * 20;
    near((await body()).volume, 8000 - lost, 0.05, "canto redondeado");
    // Rincón donde hay un canto: la bola no toca las dos caras desde afuera, da error
    await chooseOption(b, "Lado", "Rincón (agrega material)");
    await sleep(2500);
    const r = await evaluate();
    if (!r.status.some((s) => s.state === "error")) throw new Error("se esperaba un error al pedir rincón en un canto");
    await chooseOption(b, "Lado", "Según la geometría");
    await sleep(2500);
    near((await body()).volume, 8000 - lost, 0.05, "vuelve el canto");
    await accept(b);
    await b.shot("operaciones_redondeo_de_cara");
  },

  async "operaciones: grabado y relieve sobre una cara curva"(b) {
    await begin(b);
    // Cilindro de radio 10 y alto 20; en su pared, un cuadrado de 4 envuelto
    const doc = await call("cad_get_document");
    doc.features.push({ id: doc.next_id++, name: "Rollo", suppressed: false, kind: { type: "primitive", shape: { type: "cylinder", radius: 10, height: 20 }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "join" } });
    await call("cad_set_document", { document: doc });
    await b.eval(`window.__cadStore.reload()`);
    await sleep(2000);
    const wall = await b.eval(`window.__cadStore.faceRef(${await faceWhere((f) => f.surface === "cylinder")})`);
    const P = [[-2, -2], [2, -2], [2, 2], [-2, 2]];
    const sketch = { points: [{ id: 0, x: 0, y: 0 }, ...P.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 };
    await b.eval(`__cadStore.commit((d) => { d.features.push({ id: d.next_id, name: "Marca", suppressed: false, kind: { type: "surface_sketch", face: ${JSON.stringify(wall)}, sketch: ${JSON.stringify(sketch)} } }); d.next_id += 1; })`);
    await sleep(2500);
    const full = Math.PI * 100 * 20;
    await b.clickText("Grabado");
    await lastIs(b, "wrap");
    await sleep(3000);
    const cut = full - (await body()).volume;
    if (!(cut > 14 && cut < 16)) throw new Error(`grabado: ${cut}`);
    await b.clickText("Relieve (sobresale)");
    await sleep(3000);
    const added = (await body()).volume - full;
    if (!(added > 16 && added < 18)) throw new Error(`relieve: ${added}`);
    await accept(b);
    await b.shot("operaciones_grabado_relieve");
  },

  async "operaciones: línea de partición por silueta"(b) {
    await begin(b);
    await b.clickText("Esfera");
    await sleep(1500);
    await accept(b);
    const ball = await faceWhere((f) => f.surface === "sphere");
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${ball} }])`);
    await b.clickText("Línea de partición");
    await lastIs(b, "split_faces");
    await sleep(2000);
    // Mirada desde arriba (eje Z, el de entrada): la silueta es el ecuador → dos hemisferios
    await chooseOption(b, "Con", "La silueta (vista desde un eje)");
    await sleep(3000);
    const k = await lastKind(b);
    if (k.tool.type !== "silhouette" || k.tool.direction.type !== "z") throw new Error(`herramienta: ${JSON.stringify(k.tool)}`);
    const r = await body();
    if (r.faces !== 2) throw new Error(`caras: ${r.faces}`);
    near(r.volume, (4 / 3) * Math.PI * 1000, 0.5, "la esfera sigue entera");
    await accept(b);
    await b.shot("operaciones_particion_por_silueta");
  },

  async "operaciones: línea de partición y flechas en más operaciones"(b) {
    await begin(b);
    await b.clickText("Caja");
    await sleep(1500);
    await accept(b);
    // Los cuatro costados partidos por la planta (z = 0, a media altura)
    const sides = [];
    for (const test of [(f) => f.normal[0] > 0.99, (f) => f.normal[0] < -0.99, (f) => f.normal[1] > 0.99, (f) => f.normal[1] < -0.99]) sides.push(await faceWhere((f) => f.surface === "plane" && test(f)));
    await b.eval(`window.__cadUi.setPicks(${JSON.stringify(sides.map((face) => ({ kind: "face", face })))})`);
    await b.clickText("Línea de partición");
    await lastIs(b, "split_faces");
    await sleep(2500);
    let r = await body();
    if (r.faces !== 10) throw new Error(`caras: ${r.faces}`);
    near(r.volume, 8000, 1e-3, "la pieza sigue entera");
    if ((await evaluate()).parts.length !== 1) throw new Error("se separó la pieza");
    await accept(b);
    // Mover cara: con flecha (la del cálculo)
    const top = await faceWhere((f) => f.surface === "plane" && f.normal[2] > 0.99);
    await b.eval(`window.__cadUi.setPicks([{ kind: "face", face: ${top} }])`);
    await b.clickText("Mover cara");
    await lastIs(b, "move_face");
    await sleep(2500);
    const h = await b.eval(`(() => { const id = window.__cadStore.doc().features.at(-1).id; return JSON.parse(JSON.stringify(window.__cadStore.stateOf(id)?.handle ?? null)); })()`);
    if (!h || Math.abs(h.dir[2]) < 0.99) throw new Error(`flecha: ${JSON.stringify(h)}`);
    await b.shot("operaciones_flecha_mover_cara");
    await accept(b);
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
        await sleep(200);
        // En la lista de la familia cada ítem lleva su atajo al lado (el texto no es exacto)
        const r = await b.eval(`(() => { const e = [...document.querySelectorAll("[data-family-list] button")].find((x) => [...x.childNodes].some((n) => n.textContent.trim() === ${JSON.stringify(text)})); if (!e) return null; const q = e.getBoundingClientRect(); return [q.x + q.width / 2, q.y + q.height / 2]; })()`);
        if (r) return b.click(r[0], r[1]);
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
