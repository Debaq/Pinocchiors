// Interfaz de Diseñar: selección, árbol de operaciones, Outliner, visor, medir y diálogos.
//   node apps/web/e2e/cad/interfaz.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo interfaz              (lo mismo)

import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, sketchOn, body, accept, tab, tone, clickRow, setInput, uiState } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
    const handle = (id) => b.eval(`window.__cadStore.stateOf(${id})?.handles?.[0] ?? null`);
    const tip = (h) => b.eval(`window.__cadViewer.screenOf(${JSON.stringify(h.origin.map((o, i) => o + h.dir[i] * h.length))})`);
    const kind = (id) => b.eval(`window.__cadStore.doc().features.find((f) => f.id === ${id}).kind`);
    // Abrir el bloque: su flecha sube 10 desde el centro
    await b.eval(`window.__cadStore.select(2)`);
    await sleep(2000);
    let h = await handle(2);
    if (!h || h.dir[2] !== 1 || h.length !== 10 || h.reversed) throw new Error(`flecha del bloque: ${JSON.stringify(h)}`);
    // Clic en la punta: hacia abajo
    const [x, y] = await tip(h);
    if ((await b.eval(`window.__cadViewer.handleHit(${x}, ${y})`)) < 0) throw new Error("la punta no se puede pinchar");
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
    // El botón es solo un ícono: se busca por su etiqueta
    await b.eval(`document.querySelector('[aria-label="Deshacer"]').click()`);
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
    if (await b.eval(`!!window.__cadUi.session()`)) throw new Error(`el sketch siguió abierto: ${await uiState(b)}`);
    // Un punto del círculo que no tape el panel del sketch recién terminado
    let region;
    for (const [x, y] of [[-15, -15], [-13, -16], [-12, -17], [-17, -17], [-13, -13]]) {
      const p = await at([x, y, 0]);
      if (await b.eval(`document.elementFromPoint(${p[0]}, ${p[1]})?.tagName === "CANVAS"`)) {
        region = p;
        break;
      }
    }
    if (!region) throw new Error("el panel tapa todo el círculo");
    const ante = await uiState(b, region);
    await b.click(...region, { wait: 500 });
    // Sin el texto del botón Limpiar
    const summary = await b.eval(`(() => {
      const e = [...document.querySelectorAll("span")].find((x) => x.textContent?.startsWith("Elegido"));
      return [...(e?.childNodes ?? [])].filter((n) => n.nodeType === 3).map((n) => n.textContent).join("").trim();
    })()`);
    if (summary !== "Elegido: 1 región") throw new Error(`selección: «${summary}» · antes del clic ${ante} · después ${await uiState(b)}`);
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
    const expect = ["PLANOS", "Grid", "Planta (XY)", "Frente (XZ)", "Lateral (YZ)", "Plano 1", "OPERACIONES", "OBJETOS", "Pieza 1", "Pieza 2", "PLANOS 2D"];
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
    const eye = async (label) => { const ok = await b.eval(`(() => { const r = ${row(label)}; if (!r) return false; const bs = r.parentElement.querySelectorAll("button"); bs[bs.length - 1].click(); return true; })()`); if (!ok) throw new Error(`no está la fila ${label}: ${JSON.stringify(await rows())}`); };
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
};

siEsPrincipal(import.meta.url, escenarios);
