// Planetario de una vez (lib/planetary.ts), sin navegador:
//   node --test apps/web/e2e/planetary.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { CARRIER_NAME, planetCenter, planetFeatures, planetaryAssembly, ringFeature } from "../src/lib/planetary.ts";

const gearPart = (kind, teeth, extra = {}) => ({
  spec: { kind, module: 2, teeth, pressure_angle: 20, helix_angle: 0, shift: 0, backlash: 0, addendum: 1, dedendum: 1.25, root_fillet: 0.25, cone_angle: 45, shaft_angle: 90, spiral_angle: 0, worm_diameter: 0, left_hand: false },
  width: 10, bore: 0, hub_diameter: 0, hub_length: 0, keyway: false, outer: 0, hole_count: 0, hole_diameter: 0, hole_circle: 0, print_clearance: 0, ...extra,
});
const sun = { id: 1, name: "Sol", suppressed: false, kind: { type: "primitive", shape: { type: "gear", ...gearPart("external", 24) }, origin: [5, 7, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new", link: null } };
const plan = { sun_teeth: 24, planet_teeth: 12, ring_teeth: 48, planets: 3, angles: [0, 120, 240], center_distance: 36, ratio_ring_fixed: 3, ratio_carrier_fixed: -2, warnings: [] };

test("satélites enlazados al sol, portasatélites y un pasador por satélite", () => {
  const feats = planetFeatures(sun, plan, 10);
  assert.deepEqual(feats.map((f) => f.id), [10, 11, 12, 13, 14, 15, 16]);
  assert.deepEqual(feats.slice(0, 3).map((f) => f.name), ["Satélite 1", "Satélite 2", "Satélite 3"]);
  const planets = feats.slice(0, 3).map((f) => f.kind.shape);
  assert.deepEqual(planets.map((p) => p.link.angle), [0, 120, 240]);
  assert.ok(planets.every((p) => p.link.feature === 1 && p.spec.teeth === 12 && p.bore === 4 && p.spec.kind === "external"));
  // El portasatélites es un disco nuevo bajo el sol que pasa los satélites; los pasadores se le unen
  const carrier = feats[3];
  assert.equal(carrier.name, CARRIER_NAME);
  assert.equal(carrier.kind.op, "new");
  assert.equal(carrier.kind.origin[2], -4);
  assert.ok(carrier.kind.shape.radius > 36 + 14);
  assert.deepEqual(feats.slice(4).map((f) => f.kind.op), ["join", "join", "join"]);
  // El pasador de cada satélite está en su centro, a 36 mm del sol
  const c = feats[5].kind.origin;
  assert.ok(Math.abs(Math.hypot(c[0] - 5, c[1] - 7) - 36) < 1e-9);
  assert.ok(Math.abs(c[0] - 5 - 36 * Math.cos((120 * Math.PI) / 180)) < 1e-9);
});

test("el centro de un satélite sigue el marco del sol", () => {
  const rotated = { ...sun, kind: { ...sun.kind, x: [0, 1, 0] } };
  const c = planetCenter(rotated, plan, 90);
  // X del sol hacia +Y; 90° desde ella es -X
  assert.ok(Math.abs(c[0] - (5 - 36)) < 1e-9 && Math.abs(c[1] - 7) < 1e-9);
});

test("la corona queda concentrica: el ángulo del enlace resta lo que giró el satélite", () => {
  const psi = 37;
  const planet0 = { feature: 10, part: gearPart("external", 12), origin: [41, 7, 0], z: [0, 0, 1], x: [Math.cos((psi * Math.PI) / 180), Math.sin((psi * Math.PI) / 180), 0], pitch_radius: 12, center_distance: 36 };
  const ring = ringFeature(sun, plan, planet0, 20);
  const shape = ring.kind.shape;
  assert.equal(shape.spec.kind, "internal");
  assert.equal(shape.spec.teeth, 48);
  assert.equal(shape.link.feature, 10);
  // Del satélite 1 al sol hay 180° del mundo; su X está a 37°: 143°
  assert.ok(Math.abs(shape.link.angle - 143) < 1e-4, String(shape.link.angle));
  assert.ok(shape.outer >= 2 * 2 * 25 + 4);
  // Y dando la vuelta se queda en (-180, 180]
  const planet1 = { ...planet0, x: [Math.cos((200 * Math.PI) / 180), Math.sin((200 * Math.PI) / 180), 0] };
  const a = ringFeature(sun, plan, planet1, 21).kind.shape.link.angle;
  assert.ok(Math.abs(a - -20) < 1e-4, String(a));
});

test("el ensamble pone la corona fija, el sol y el portasatélites en ella y cada satélite en el portasatélites", () => {
  const feats = [sun, ...planetFeatures(sun, plan, 10)];
  const ringFeat = ringFeature(sun, plan, { feature: 10, part: gearPart("external", 12), origin: [41, 7, 0], z: [0, 0, 1], x: [1, 0, 0], pitch_radius: 12, center_distance: 36 }, 20);
  feats.push(ringFeat);
  const axis = (f, kind, g, origin) => ({ feature: f, part: { ...g, spec: { ...g.spec, kind }, link: g.link ?? null }, origin, z: [0, 0, 1], x: [1, 0, 0], pitch_radius: 1, center_distance: 0 });
  const gears = [
    axis(1, "external", sun.kind.shape, [5, 7, 0]),
    ...[0, 1, 2].map((k) => axis(10 + k, "external", feats[1 + k].kind.shape, planetCenter(sun, plan, plan.angles[k]))),
    axis(20, "internal", ringFeat.kind.shape, [5, 7, 0]),
  ];
  const parts = [1, 10, 11, 12, 13, 20].map((f) => ({ id: { feature: f, index: 0 }, name: `p${f}`, faces: [0, 0], edges: [0, 0], volume: 1, area: 1, center: [0, 0, 0] }));
  const asm = planetaryAssembly(feats, gears, parts);
  assert.equal(typeof asm, "object");
  assert.deepEqual(asm.instances.map((i) => [i.name, i.fixed]), [["Corona", true], ["Sol", false], [CARRIER_NAME, false], ["Satélite 1", false], ["Satélite 2", false], ["Satélite 3", false]]);
  // Uniones: sol y portasatélites con la corona, cada satélite con el portasatélites
  const [m0, m1, ...rest] = asm.mates;
  const name = (id) => asm.instances.find((i) => i.id === id).name;
  assert.deepEqual([m0, m1].map((m) => [name(m.a.instance), name(m.b.instance)]), [["Corona", "Sol"], ["Corona", CARRIER_NAME]]);
  assert.deepEqual(rest.map((m) => [name(m.a.instance), name(m.b.instance)]), [[CARRIER_NAME, "Satélite 1"], [CARRIER_NAME, "Satélite 2"], [CARRIER_NAME, "Satélite 3"]]);
  // Los ids de instancias y uniones no se repiten
  const ids = [...asm.instances.map((i) => i.id), ...asm.mates.map((m) => m.id)];
  assert.equal(new Set(ids).size, ids.length);
  assert.ok(asm.mates.every((m) => m.kind === "revolute"));
  assert.equal(asm.next_id, 1 + 6 + 5);
  // Los eje de los satélites pasan por sus centros
  assert.deepEqual(rest[1].a.origin, planetCenter(sun, plan, 120));
  // Errores claros: sin portasatélites o ya armado
  assert.match(planetaryAssembly(feats.filter((f) => f.name !== CARRIER_NAME), gears, parts), /Portasatélites/);
  assert.match(planetaryAssembly(feats, gears, parts, asm), /ya están/);
  assert.match(planetaryAssembly(feats, gears.slice(0, 1), parts), /no hay un sol/);
  assert.match(planetaryAssembly(feats, gears.filter((g) => g.feature !== 20), parts), /falta la corona/);
});
