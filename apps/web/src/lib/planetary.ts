// Tren planetario de una vez: las operaciones (satélites, portasatélites con sus pasadores y corona) que
// acompañan a un sol, y el ensamble con sus uniones. Todo es geometría pura; el cálculo del plan (dientes,
// condición de montaje) lo hace el backend.
import type { AsmInstance, Assembly, Feature, GearAxis, GearPart, Mate, P3, PartView } from "./cad";

export interface PlanetaryPlan {
  sun_teeth: number;
  planet_teeth: number;
  ring_teeth: number;
  planets: number;
  /** Dirección del centro de cada satélite visto desde el sol (°, desde su X) */
  angles: number[];
  center_distance: number;
  ratio_ring_fixed: number;
  ratio_carrier_fixed: number;
  warnings: string[];
}

export const CARRIER_NAME = "Portasatélites";
export const RING_NAME = "Corona";
export const SUN_NAME = "Sol";
export const planetName = (k: number) => `Satélite ${k}`;

const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x)) as T;
const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a: P3): P3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/** Marco del sol en el mundo: origen, eje Z y X (en el plano), como los trae su operación */
function frameOf(sun: Feature): { origin: P3; z: P3; x: P3 } {
  if (sun.kind.type !== "primitive") throw new Error("el sol tiene que ser un engrane");
  const z = unit(sun.kind.z);
  const k = dot(sun.kind.x, z);
  const x = unit([sun.kind.x[0] - z[0] * k, sun.kind.x[1] - z[1] * k, sun.kind.x[2] - z[2] * k]);
  return { origin: sun.kind.origin, z, x };
}

/** Centro del satélite que está a `angle`° del X del sol, a la distancia entre centros */
export function planetCenter(sun: Feature, plan: PlanetaryPlan, angle: number): P3 {
  const f = frameOf(sun);
  const y = cross(f.z, f.x);
  const t = (angle * Math.PI) / 180;
  const d = plan.center_distance;
  return [0, 1, 2].map((i) => f.origin[i] + d * (Math.cos(t) * f.x[i] + Math.sin(t) * y[i])) as P3;
}

/**
 * Satélites (enlazados al sol, cada uno a su ángulo), portasatélites (un disco bajo el conjunto) y los
 * pasadores que lo unen a cada satélite. `firstId` es el primer id libre; salen en ese orden.
 */
export function planetFeatures(sun: Feature, plan: PlanetaryPlan, firstId: number): Feature[] {
  if (sun.kind.type !== "primitive" || sun.kind.shape.type !== "gear") throw new Error("el sol tiene que ser un engrane");
  const { type: _t, ...sunPart } = sun.kind.shape;
  const f = frameOf(sun);
  const out: Feature[] = [];
  let id = firstId;
  const m = sunPart.spec.module;
  // Pasador de 3,6 mm en un agujero de 4: el satélite gira libre
  const bore = 4;
  const pin = 3.6;
  for (let k = 0; k < plan.planets; k++) {
    const part = clone(sunPart) as GearPart;
    part.spec = { ...part.spec, teeth: plan.planet_teeth, shift: 0 };
    part.bore = bore;
    part.hub_diameter = 0;
    part.hub_length = 0;
    part.keyway = false;
    part.set_screw = 0;
    part.link = { feature: sun.id, angle: plan.angles[k], offset: 0, distance: null };
    out.push({
      id: id++,
      name: planetName(k + 1),
      suppressed: false,
      kind: { type: "primitive", shape: { type: "gear", ...part }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new", link: null },
    });
  }
  // Disco del portasatélites: 1 mm bajo la cara de abajo de los engranes, hasta pasar los satélites
  const tip = (m * (plan.planet_teeth + 2)) / 2;
  const radius = plan.center_distance + tip + 3;
  const base = (z: number): P3 => [0, 1, 2].map((i) => f.origin[i] + z * f.z[i]) as P3;
  out.push({
    id: id++,
    name: CARRIER_NAME,
    suppressed: false,
    kind: { type: "primitive", shape: { type: "cylinder", radius, height: 3 }, origin: base(-4), z: f.z, x: f.x, op: "new", link: null },
  });
  for (let k = 0; k < plan.planets; k++) {
    const c = planetCenter(sun, plan, plan.angles[k]);
    out.push({
      id: id++,
      name: `Pasador ${k + 1}`,
      suppressed: false,
      kind: {
        type: "primitive",
        shape: { type: "cylinder", radius: pin / 2, height: sunPart.width + 3.5 },
        origin: [0, 1, 2].map((i) => c[i] - 2 * f.z[i]) as P3,
        z: f.z,
        x: f.x,
        op: "join",
        link: null,
      },
    });
  }
  return out;
}

/**
 * Corona concentrica con el sol, enlazada al primer satélite. La dirección del enlace se mide desde la X
 * del satélite (que gira para calzar con el sol), así que hay que restar lo que giró: hacia el centro del sol.
 */
export function ringFeature(sun: Feature, plan: PlanetaryPlan, planet0: GearAxis, firstId: number): Feature {
  if (sun.kind.type !== "primitive" || sun.kind.shape.type !== "gear") throw new Error("el sol tiene que ser un engrane");
  const f = frameOf(sun);
  const world = (v: P3) => (Math.atan2(dot(v, cross(f.z, f.x)), dot(v, f.x)) * 180) / Math.PI;
  // Dirección del primer satélite visto desde el sol (plan.angles[0]) y el X del satélite en el marco del sol
  const toSun = plan.angles[0] + 180;
  const angle = toSun - world(planet0.x);
  const { type: _t, ...sunPart } = sun.kind.shape;
  const part = clone(sunPart) as GearPart;
  const m = part.spec.module;
  part.spec = { ...part.spec, kind: "internal", teeth: plan.ring_teeth, shift: 0 };
  part.outer = Math.ceil(m * (plan.ring_teeth + 6) + 4);
  part.bore = 0;
  part.hub_diameter = 0;
  part.hub_length = 0;
  part.keyway = false;
  part.set_screw = 0;
  part.hole_count = 0;
  part.spokes = 0;
  part.link = { feature: planet0.feature, angle: +(((angle % 360) + 540) % 360 - 180).toFixed(6), offset: 0, distance: null };
  return {
    id: firstId,
    name: RING_NAME,
    suppressed: false,
    kind: { type: "primitive", shape: { type: "gear", ...part }, origin: [0, 0, 0], z: [0, 0, 1], x: [1, 0, 0], op: "new", link: null },
  };
}

/**
 * Ensamble del planetario: la corona fija como carcasa, el sol y el portasatélites girando en ella, y
 * cada satélite girando en el portasatélites. Las relaciones (con la fórmula de Willis) las detecta
 * después el backend. Devuelve un mensaje si el diseño no tiene esa estructura.
 */
export function planetaryAssembly(features: Feature[], gears: GearAxis[], parts: PartView[], base?: Assembly): Assembly | string {
  const sun = gears.find((g) => g.part.spec.kind === "external" && !g.part.link && gears.some((o) => o.part.link?.feature === g.feature && o.part.spec.kind === "external"));
  if (!sun) return "no hay un sol: un engrane exterior con satélites enlazados a él";
  const planets = gears.filter((g) => g.part.link?.feature === sun.feature && g.part.spec.kind === "external");
  const ring = gears.find((g) => g.part.spec.kind === "internal" && planets.some((p) => p.feature === g.part.link?.feature));
  if (!ring) return "falta la corona: un engrane interno enlazado a un satélite";
  const carrier = features.find((f) => f.name === CARRIER_NAME && f.kind.type === "primitive");
  if (!carrier) return `falta el «${CARRIER_NAME}»: créalo con «Armar planetario» en el sol`;
  const partOf = (feature: number) => parts.find((p) => p.id.feature === feature)?.id;
  const name = (feature: number) => features.find((f) => f.id === feature)?.name ?? `#${feature}`;
  const next = base ? { ...base, instances: [...base.instances], mates: [...base.mates] } : { instances: [], mates: [], relations: [], next_id: 1 };
  if (next.instances.some((i) => [sun, ring, ...planets].some((g) => i.part.feature === g.feature))) return "esas piezas ya están en el ensamble";
  const instances: AsmInstance[] = [];
  const mates: Mate[] = [];
  let id = next.next_id;
  const instance = (feature: number, fixed: boolean): number | string => {
    const part = partOf(feature);
    if (!part) return `la pieza «${name(feature)}» todavía no existe`;
    instances.push({ id, part, name: name(feature), position: [0, 0, 0], rotation: [0, 0, 0], fixed });
    return id++;
  };
  const house = instance(ring.feature, true);
  const sunId = instance(sun.feature, false);
  const carrierId = instance(carrier.id, false);
  if ([house, sunId, carrierId].some((v) => typeof v === "string")) return [house, sunId, carrierId].find((v) => typeof v === "string") as string;
  const conn = (instanceId: number, g: GearAxis) => ({ instance: instanceId, origin: g.origin, z: g.z, x: g.x });
  const mate = (label: string, a: ReturnType<typeof conn>, b: ReturnType<typeof conn>) => {
    mates.push({ id: id++, name: label, kind: "revolute", a, b, flip: false });
  };
  mate(name(sun.feature), conn(house as number, sun), conn(sunId as number, sun));
  mate(name(carrier.id), conn(house as number, sun), conn(carrierId as number, sun));
  for (const p of planets) {
    const pid = instance(p.feature, false);
    if (typeof pid === "string") return pid;
    mate(name(p.feature), conn(carrierId as number, p), conn(pid, p));
  }
  return { ...next, instances: [...next.instances, ...instances], mates: [...next.mates, ...mates], next_id: id };
}
