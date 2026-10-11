// Operaciones avanzadas del menú (familia «operaciones:»).
//   node apps/web/e2e/cad/operaciones.mjs [filtro…] [-j N]     (solo este grupo)
//   node apps/web/e2e/cad.mjs --grupo operaciones              (lo mismo)

import { sleep } from "../cdp.mjs";
import { call, near, begin, evaluate, body, accept, clickRow, setInput, chooseOption, setInputIn, lastIs, lastKind, faceWhere, edgeNear } from "./_comun.mjs";
import { siEsPrincipal } from "./_ejecutor.mjs";

export const escenarios = {
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
    // El perfil y el camino se eligen solos
    const [perfil, camino] = doc.features.slice(-2).map((f) => f.id);
    const sw = await lastKind(b);
    if (sw.sketch !== perfil || sw.path?.sketch !== camino) throw new Error(`barrido: ${JSON.stringify(sw)}`);
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

    // Revolución hasta una cara (otro documento entero)
    // Caja en x < 0, y > 0 (su cara x = 0 a 90° del perfil) y el perfil x 10..15, z 0..20 en el frente
    const doc2 = await call("cad_get_document");
    doc2.features = [];
    const id2 = () => doc2.next_id++;
    doc2.features.push({ id: id2(), name: "Tope", suppressed: false, kind: { type: "primitive", shape: { type: "box", dx: 40, dy: 40, dz: 40, centered: false }, origin: [-40, 0, -10], z: [0, 0, 1], x: [1, 0, 0], op: "join" } });
    const P2 = [[10, 0], [15, 0], [15, 20], [10, 20]];
    doc2.features.push({
      id: id2(), name: "Perfil", suppressed: false,
      kind: { type: "sketch", plane: { type: "xz" }, offset: 0, sketch: { points: [{ id: 0, x: 0, y: 0 }, ...P2.map(([x, y], i) => ({ id: i + 1, x, y }))], entities: P2.map((_, i) => ({ id: 11 + i, geometry: { type: "line", start: i + 1, end: ((i + 1) % 4) + 1 } })), constraints: [], next_id: 20, origin: 0 } },
    });
    await call("cad_set_document", { document: doc2 });
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

siEsPrincipal(import.meta.url, escenarios);
