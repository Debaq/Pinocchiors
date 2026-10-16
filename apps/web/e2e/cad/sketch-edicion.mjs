// Sketch: edición (recortar, mover, patrones, deshacer), archivos y uso del modelo.
//   node apps/web/e2e/cad/sketch-edicion.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo sketch-edicion              (lo mismo)

import { existsSync, readFileSync, rmSync } from "node:fs";
import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, sketchOn, body, accept, sketchText, setInput, uiState } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
    const { checkSketch } = await import("../../src/lib/sketchCheck.ts");
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
    if (!(await b.eval(`!!window.__cadUi.session()`))) throw new Error(`no se abrió el sketch en la planta: ${await uiState(b)}`);
    if (!(await byTitle("Las curvas donde el plano del sketch corta el sólido"))) throw new Error(`no está Intersección: ${await uiState(b)}`);
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
};

siEsPrincipal(import.meta.url, escenarios);
