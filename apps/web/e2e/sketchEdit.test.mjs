// Pruebas de la edición II del sketch (lib/sketchEdit.ts), sin navegador:
//   node --test apps/web/e2e/sketchEdit.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { addEntity, addPoint, addRectangle, extendAt, filletCorner } from "../src/lib/cad.ts";
import { checkSketch } from "../src/lib/sketchCheck.ts";
import { splitEqual, chamferCorner, closeContour, joinEntities, mergePoint, offsetChain, orderChain, removeDuplicates, repairSketch, trimByStroke } from "../src/lib/sketchEdit.ts";

function sketch() {
  return { origin: 0, points: [{ id: 0, x: 0, y: 0 }], entities: [], constraints: [] };
}
const at = (s, id) => {
  const p = s.points.find((q) => q.id === id);
  return [+p.x.toFixed(9), +p.y.toFixed(9)];
};
const line = (s, a, b) => addEntity(s, { type: "line", start: addPoint(s, a), end: addPoint(s, b) });
const ends = (s, id) => {
  const g = s.entities.find((e) => e.id === id).geometry;
  return [at(s, g.start), at(s, g.end)];
};
const types = (s) => s.entities.map((e) => e.geometry.type).sort();
const clean = (s) => {
  const p = checkSketch(s);
  return p.looseEnds.length + p.crossings.length + p.overlaps.length;
};

test("recortar con trazo: un trazo que cruza dos lados del # quita esos tramos", () => {
  // Dos horizontales y dos verticales cruzadas (un numeral)
  const s = sketch();
  line(s, [0, 3], [10, 3]);
  line(s, [0, 7], [10, 7]);
  line(s, [3, 0], [3, 10]);
  line(s, [7, 0], [7, 10]);
  // Trazo vertical por x=5 entre y=1 y y=9: corta el tramo del medio de las dos horizontales
  const r = trimByStroke(s, [[5, 1], [5, 9]]);
  assert.equal(r.trimmed, 2);
  // Cuatro trozos de horizontal; las verticales quedan partidas en los cruces (cierran regiones)
  const horiz = s.entities.filter((e) => {
    const [a, b] = ends(s, e.id);
    return a[1] === b[1];
  });
  assert.equal(horiz.length, 4);
  const segs = s.entities.map((e) => ends(s, e.id));
  assert.ok(!segs.some(([a, b]) => Math.min(a[0], b[0]) < 5 && Math.max(a[0], b[0]) > 5 && a[1] === b[1]), "nada horizontal cruza x=5");
});

test("recortar con trazo: un círculo cruzado por dos líneas pierde el arco tocado", () => {
  const s = sketch();
  addEntity(s, { type: "circle", center: addPoint(s, [0, 0]), radius: 5 });
  line(s, [-10, 0], [10, 0]);
  // Trazo que entra por arriba del círculo
  trimByStroke(s, [[0, 8], [0, 3]]);
  const arc = s.entities.find((e) => e.geometry.type === "arc");
  assert.ok(arc, "el círculo pasó a ser arco");
  // Queda la mitad de abajo: de (-5,0) a (5,0) pasando por abajo (antihorario)
  assert.deepEqual([at(s, arc.geometry.start), at(s, arc.geometry.end)], [[-5, 0], [5, 0]]);
});

test("unir dos líneas alineadas: una sola, con lo de la segunda", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const m = addPoint(s, [4, 0]);
  const b = addPoint(s, [10, 0]);
  const l1 = addEntity(s, { type: "line", start: a, end: m });
  const l2 = addEntity(s, { type: "line", start: m, end: b });
  s.constraints.push({ type: "horizontal", line: l2 }, { type: "length", line: l1, value: 4 }, { type: "equal", a: l1, b: l2 });
  assert.equal(joinEntities(s, l1, l2), undefined);
  assert.equal(s.entities.length, 1);
  assert.deepEqual(ends(s, l1), [[0, 0], [10, 0]]);
  // La horizontal pasó a la unida; el largo y la igualdad (consigo misma) se fueron
  assert.deepEqual(s.constraints, [{ type: "horizontal", line: l1 }]);
  assert.ok(!s.points.some((p) => p.id === m), "el punto del medio se fue");
});

test("unir: con una tercera línea en el medio, el punto queda sobre la unida", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const m = addPoint(s, [5, 0]);
  const b = addPoint(s, [10, 0]);
  const l1 = addEntity(s, { type: "line", start: a, end: m });
  const l2 = addEntity(s, { type: "line", start: m, end: b });
  addEntity(s, { type: "line", start: m, end: addPoint(s, [5, 5]) });
  assert.equal(joinEntities(s, l1, l2), undefined);
  assert.deepEqual(s.constraints, [{ type: "point_on_line", point: m, line: l1 }]);
});

test("unir: no alineadas o sin extremo común, no", () => {
  const s = sketch();
  const m = addPoint(s, [5, 0]);
  const l1 = addEntity(s, { type: "line", start: addPoint(s, [0, 0]), end: m });
  const l2 = addEntity(s, { type: "line", start: m, end: addPoint(s, [10, 1]) });
  const l3 = line(s, [20, 0], [30, 0]);
  assert.match(joinEntities(s, l1, l2), /alineadas/);
  assert.match(joinEntities(s, l1, l3), /extremo/);
});

test("unir dos arcos del mismo círculo: un arco; con la vuelta entera, el círculo", () => {
  const s = sketch();
  const c = addPoint(s, [0, 0]);
  const p0 = addPoint(s, [5, 0]);
  const p1 = addPoint(s, [0, 5]);
  const p2 = addPoint(s, [-5, 0]);
  const a1 = addEntity(s, { type: "arc", center: c, start: p0, end: p1 });
  const a2 = addEntity(s, { type: "arc", center: c, start: p1, end: p2 });
  assert.equal(joinEntities(s, a1, a2), undefined);
  assert.deepEqual(s.entities.find((e) => e.id === a1).geometry, { type: "arc", center: c, start: p0, end: p2 });
  // La otra mitad cierra la vuelta
  const a3 = addEntity(s, { type: "arc", center: c, start: p2, end: p0 });
  assert.equal(joinEntities(s, a1, a3), undefined);
  assert.deepEqual(s.entities.find((e) => e.id === a1).geometry, { type: "circle", center: c, radius: 5 });
  assert.deepEqual(s.points.map((p) => p.id).sort(), [0, c]);
});

test("chaflán por distancia y por distancia y ángulo", () => {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 10]);
  // Esquina (10, 0): la de la primera línea (abajo) con la segunda (derecha)
  const corner = s.entities.find((e) => e.id === l[0]).geometry.end;
  assert.equal(chamferCorner(s, corner, 2), undefined);
  assert.equal(s.entities.length, 5);
  const ch = s.entities[s.entities.length - 1];
  assert.deepEqual(ends(s, ch.id), [[8, 0], [10, 2]]);
  // La esquina queda virtual: sobre las dos rectas y con las dos distancias del chaflán como cotas
  assert.ok(s.points.some((p) => p.id === corner));
  assert.equal(s.constraints.filter((k) => k.type === "point_on_line" && k.point === corner).length, 2);
  assert.deepEqual(s.constraints.filter((k) => k.type === "distance" && k.a === corner).map((k) => k.value), [2, 2]);
  assert.equal(clean(s), 0, "sigue cerrado");
  // Sin esquina virtual: se va
  const s0 = sketch();
  const l0 = addRectangle(s0, [0, 0], [10, 10]);
  const c0 = s0.entities.find((e) => e.id === l0[0]).geometry.end;
  assert.equal(chamferCorner(s0, c0, 2, { distance: 2 }, false), undefined);
  assert.ok(!s0.points.some((p) => p.id === c0));
  // Otra esquina con 2 mm y 30°: la segunda distancia es 2·sen30/sen60
  const s2 = sketch();
  const l2 = addRectangle(s2, [0, 0], [10, 10]);
  const c2 = s2.entities.find((e) => e.id === l2[0]).geometry.end;
  assert.equal(chamferCorner(s2, c2, 2, { angle: 30 }), undefined);
  const [p, q] = ends(s2, s2.entities[s2.entities.length - 1].id);
  assert.deepEqual(p, [8, 0]);
  assert.ok(Math.abs(q[1] - (2 * Math.sin(Math.PI / 6)) / Math.sin(Math.PI / 3)) < 1e-9);
  assert.match(chamferCorner(s2, addPoint(s2, [50, 50]), 1), /dos líneas/);
});

test("orden de cadena: tramos al revés y desordenados", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const b = addPoint(s, [10, 0]);
  const c = addPoint(s, [10, 10]);
  const l1 = addEntity(s, { type: "line", start: b, end: a });
  const l2 = addEntity(s, { type: "line", start: b, end: c });
  const r = orderChain(s, [l2, l1]);
  assert.equal(r.closed, false);
  // Sigue el sentido de la primera elegida (l2): l1 va al revés, de a a b
  assert.deepEqual(r.pieces, [
    { entity: l1, reversed: true },
    { entity: l2, reversed: false },
  ]);
});

test("equidistante de una L abierta a los dos lados con extremos redondos: contorno cerrado", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const b = addPoint(s, [10, 0]);
  const c = addPoint(s, [10, 10]);
  const l1 = addEntity(s, { type: "line", start: a, end: b });
  const l2 = addEntity(s, { type: "line", start: b, end: c });
  s.constraints.push({ type: "horizontal", line: l1 }, { type: "vertical", line: l2 });
  const r = offsetChain(s, [l1, l2], 1, { both: true, caps: "round" });
  assert.equal(typeof r, "object");
  // 2 líneas por lado + 2 arcos de punta
  assert.equal(r.entities.length, 6);
  // Los originales quedan como estaban; el contorno nuevo cierra solo
  const only = { ...s, entities: s.entities.filter((e) => r.entities.includes(e.id)) };
  assert.equal(checkSketch(only).looseEnds.length, 0, "el contorno nuevo no tiene extremos sueltos");
  const pts = r.entities.flatMap((id) => {
    const g = s.entities.find((e) => e.id === id).geometry;
    return g.type === "line" ? ends(s, id) : [];
  });
  // Esquina de afuera (11, -1) y de adentro (9, 1)
  assert.ok(pts.some((p) => p[0] === 11 && p[1] === -1));
  assert.ok(pts.some((p) => p[0] === 9 && p[1] === 1));
  // Horizontales y verticales copiadas
  assert.equal(s.constraints.filter((k) => k.type === "horizontal").length, 3);
  assert.equal(s.constraints.filter((k) => k.type === "vertical").length, 3);
});

test("equidistante con extremos rectos de un solo lado: cierra contra la original", () => {
  const s = sketch();
  const l = line(s, [0, 0], [10, 0]);
  const r = offsetChain(s, [l], 2, { caps: "line" });
  assert.equal(r.entities.length, 3);
  assert.equal(clean(s), 0);
  assert.deepEqual(ends(s, r.entities[0]), [[0, 2], [10, 2]]);
});

test("equidistante de línea con arco tangente: el arco comparte el centro", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const b = addPoint(s, [10, 0]);
  const c = addPoint(s, [10, 5]);
  const e = addPoint(s, [15, 5]);
  const l = addEntity(s, { type: "line", start: a, end: b });
  // Arco antihorario de (10,0) a (15,5) con centro (10,5): sale tangente a la línea
  const arc = addEntity(s, { type: "arc", center: c, start: b, end: e });
  const r = offsetChain(s, [l, arc], 1);
  const g = s.entities.find((x) => x.id === r.entities[1]).geometry;
  assert.equal(g.type, "arc");
  assert.equal(g.center, c);
  // A la izquierda de un arco antihorario es hacia el centro: radio 4
  assert.deepEqual([at(s, g.start), at(s, g.end)], [[10, 1], [14, 5]]);
  // La línea termina donde empieza el arco (tangentes: mismo punto)
  assert.equal(s.entities.find((x) => x.id === r.entities[0]).geometry.end, g.start);
  assert.match(offsetChain(s, [arc], 6), /radio/);
});

test("eliminar duplicados: la repetida se va y sus restricciones pasan a la otra", () => {
  const s = sketch();
  const l1 = line(s, [0, 0], [10, 0]);
  const l2 = line(s, [10, 0], [0, 0]);
  const c1 = addEntity(s, { type: "circle", center: addPoint(s, [5, 5]), radius: 2 });
  addEntity(s, { type: "circle", center: addPoint(s, [5, 5]), radius: 2 });
  s.constraints.push({ type: "horizontal", line: l2 });
  assert.equal(removeDuplicates(s), 2);
  assert.deepEqual(s.entities.map((e) => e.id), [l1, c1]);
  assert.deepEqual(s.constraints, [{ type: "horizontal", line: l1 }]);
  assert.equal(s.points.length, 4, "origen, dos extremos y un centro");
});

test("reparar: extremos casi juntos, una línea cortísima y una T sin partir", () => {
  const s = sketch();
  // Triángulo con un extremo corrido 0,01 y una línea de 0,001 en otra esquina
  line(s, [0, 0], [10, 0]);
  line(s, [10.01, 0], [5, 8]);
  line(s, [5, 8], [5.001, 8]);
  line(s, [5.001, 8], [0, 0]);
  // T: una línea que termina en medio de la base, desde arriba
  line(s, [5, 0], [5, 4]);
  const r = repairSketch(s, 0.05);
  assert.equal(r.removed, 1);
  assert.ok(r.joined >= 2);
  assert.equal(r.split, 1);
  const p = checkSketch(s);
  assert.equal(p.crossings.length, 0);
  // Solo el extremo de arriba de la T queda suelto
  assert.deepEqual(p.looseEnds.map((x) => x.p.map((v) => +v.toFixed(6))), [[5, 4]]);
});

test("cerrar contorno: une los extremos sueltos más cercanos", () => {
  const s = sketch();
  const a = addPoint(s, [0, 0]);
  const b = addPoint(s, [10, 0]);
  const c = addPoint(s, [10, 10]);
  addEntity(s, { type: "line", start: a, end: b });
  addEntity(s, { type: "line", start: b, end: c });
  const made = closeContour(s);
  assert.equal(made.length, 1);
  assert.deepEqual(ends(s, made[0]).sort(), [[0, 0], [10, 10]]);
  assert.equal(clean(s), 0);
  // Una línea sola no se cierra contra sí misma
  const s2 = sketch();
  line(s2, [0, 0], [5, 0]);
  assert.deepEqual(closeContour(s2), []);
});

test("juntar puntos: el origen se queda", () => {
  const s = sketch();
  const p = addPoint(s, [0, 0]);
  const l = addEntity(s, { type: "line", start: p, end: addPoint(s, [3, 0]) });
  mergePoint(s, 0, p);
  assert.ok(s.points.some((q) => q.id === 0));
  assert.equal(s.entities.find((e) => e.id === l).geometry.start, 0);
  assert.deepEqual(types(s), ["line"]);
});

test("redondeo de esquina: la esquina queda virtual (y el largo pasa a medir desde ella); sin recortar, las líneas enteras", () => {
  const s = sketch();
  const l = addRectangle(s, [0, 0], [10, 10]);
  const corner = s.entities.find((e) => e.id === l[0]).geometry.end;
  const far = s.entities.find((e) => e.id === l[0]).geometry.start;
  s.constraints.push({ type: "length", line: l[0], value: 10 });
  assert.equal(filletCorner(s, corner, 2), undefined);
  assert.ok(s.points.some((p) => p.id === corner), "la esquina sigue");
  assert.equal(s.constraints.filter((k) => k.type === "point_on_line" && k.point === corner).length, 2);
  assert.ok(s.constraints.some((k) => k.type === "distance" && k.a === corner && k.b === far && k.value === 10), "el largo, desde la esquina virtual");
  assert.ok(!s.constraints.some((k) => k.type === "length" && k.line === l[0]));
  assert.deepEqual(ends(s, l[0]), [[0, 0], [8, 0]], "recortada hasta la tangencia");
  // Sin recortar
  const t = sketch();
  const m = addRectangle(t, [0, 0], [10, 10]);
  const c2 = t.entities.find((e) => e.id === m[0]).geometry.end;
  assert.equal(filletCorner(t, c2, 2, { trim: false }), undefined);
  assert.deepEqual(ends(t, m[0]), [[0, 0], [10, 0]], "la línea sigue entera");
  assert.equal(t.constraints.filter((k) => k.type === "point_on_line" && k.point !== c2).length, 2, "el arco apoyado en las dos líneas");
});

test("extender un arco: sigue su círculo (mismo centro y radio) hasta lo primero que cruce, por la punta más cercana", () => {
  const s = sketch();
  // Arco de radio 10 con centro en el origen, de 0° a 90°; una línea vertical en x = −6 corta el círculo
  const c = addPoint(s, [0, 0]);
  const arc = addEntity(s, { type: "arc", center: c, start: addPoint(s, [10, 0]), end: addPoint(s, [0, 10]) });
  line(s, [-6, -20], [-6, 20]);
  assert.equal(extendAt(s, arc, [0.5, 9.9]), undefined);
  const g = s.entities.find((e) => e.id === arc).geometry;
  const [ex, ey] = at(s, g.end);
  assert.ok(Math.abs(ex + 6) < 1e-9 && Math.abs(ey - 8) < 1e-9, `el fin llega a (−6, 8): ${ex}, ${ey}`);
  assert.deepEqual(at(s, g.center), [0, 0]);
  assert.ok(s.constraints.some((k) => k.type === "point_on_line" && k.point === g.end) || s.entities.length > 2, "pegado a la línea");
  // Por el comienzo (horario): hasta (−6, −8)
  assert.equal(extendAt(s, arc, [9.9, -0.5]), undefined);
  const [sx, sy] = at(s, s.entities.find((e) => e.id === arc).geometry.start);
  assert.ok(Math.abs(sx + 6) < 1e-9 && Math.abs(sy + 8) < 1e-9, `el comienzo llega a (−6, −8): ${sx}, ${sy}`);
});

test("partir en N partes iguales: líneas (con el largo atado) y arcos", () => {
  const s = sketch();
  const l = line(s, [0, 0], [12, 0]);
  assert.equal(splitEqual(s, l, 3), undefined);
  const lines = s.entities.filter((e) => e.geometry.type === "line").map((e) => ends(s, e.id)).sort((a, b) => a[0][0] - b[0][0]);
  assert.deepEqual(lines, [[[0, 0], [4, 0]], [[4, 0], [8, 0]], [[8, 0], [12, 0]]]);
  assert.equal(s.constraints.filter((k) => k.type === "equal").length, 2);
  const t = sketch();
  const c = addPoint(t, [0, 0]);
  const arc = addEntity(t, { type: "arc", center: c, start: addPoint(t, [10, 0]), end: addPoint(t, [-10, 0]) });
  assert.equal(splitEqual(t, arc, 2), undefined);
  assert.equal(t.entities.filter((e) => e.geometry.type === "arc").length, 2);
  assert.ok(t.points.some((p) => Math.abs(p.x) < 1e-9 && Math.abs(p.y - 10) < 1e-9), "el medio del arco");
  assert.match(splitEqual(t, arc, 1), /dos partes/);
});
