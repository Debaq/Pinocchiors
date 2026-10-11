// Operaciones de modelado básicas: caja, redondeo, chaflán, extrusión, agujero, rosca, chapa y superficies.
//   node apps/web/e2e/cad/modelado.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo modelado              (lo mismo)

import { existsSync, readFileSync, rmSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, sketchOn, body, accept, setInput } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
};

siEsPrincipal(import.meta.url, escenarios);
