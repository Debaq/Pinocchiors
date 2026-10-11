// Sketch: cotas, restricciones, solver y revisión.
//   node apps/web/e2e/cad/sketch-restricciones.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo sketch-restricciones              (lo mismo)

import { sleep } from "../cdp.mjs";
import { call, near, begin, sketchOn, body, accept, clickRow } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
};

siEsPrincipal(import.meta.url, escenarios);
