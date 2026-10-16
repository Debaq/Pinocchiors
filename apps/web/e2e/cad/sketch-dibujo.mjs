// Sketch: herramientas de dibujo, anclajes y regiones.
//   node apps/web/e2e/cad/sketch-dibujo.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo sketch-dibujo              (lo mismo)

import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, sketchOn, body, accept, sketchText, setInput } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
    const { makeBSpline, evalAt, domain } = await import("../../src/lib/sketchCurves.ts");
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
};

siEsPrincipal(import.meta.url, escenarios);
