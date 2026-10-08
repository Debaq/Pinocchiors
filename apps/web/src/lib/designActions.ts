// Acciones de Diseñar: crear operaciones con lo elegido en el visor. Las usan
// la barra de herramientas del visor, el menú del clic derecho y el panel.

import { createSignal } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import {
  METRIC_HOLES,
  emptySketch,
  offsetPlane,
  samePart,
  type AxisSpec,
  type CadStore,
  type Feature,
  type FeatureKind,
  type P2,
  type PartId,
  type PlaneSpec,
  type PrimitiveShape,
  type RegionSelection,
} from "./cad";
import type { CadUi } from "./cadUi";

export function createDesignActions(store: CadStore, ui: CadUi) {
  // Aviso corto (falta un sketch…); lo muestra el visor
  const [notice, setNotice] = createSignal<string>();

  const features = () => store.doc()?.features ?? [];
  const selectedFeature = () => features().find((f) => f.id === store.selected());
  const sketches = () => features().filter((f) => f.kind.type === "sketch");
  const toolFeatures = () => features().filter((f) => ["extrude", "revolve", "primitive", "import", "sweep", "loft"].includes(f.kind.type));

  const say = (msg: string) => {
    setNotice(msg);
    setTimeout(() => setNotice((m) => (m === msg ? undefined : m)), 4000);
  };

  // ─── Agregar operaciones ─────────────────────────────────────────────

  const newSketch = async (plane: PlaneSpec) => {
    const id = await store.addFeature({ type: "sketch", plane, offset: 0, sketch: emptySketch() });
    ui.editSketch(id);
  };

  /** Nuevo sketch en el plano o la cara elegida; sin elegir, se pide dónde */
  const startSketch = async () => {
    const picks = ui.picks();
    const plane = picks.find((p) => p.kind === "plane");
    if (plane?.kind === "plane") {
      ui.clearPicks();
      return newSketch({ type: plane.plane });
    }
    const ref = picks.find((p) => p.kind === "refplane");
    if (ref?.kind === "refplane") {
      ui.clearPicks();
      return newSketch({ type: "reference", feature: ref.feature });
    }
    const face = picks.find((p) => p.kind === "face");
    if (face?.kind === "face") {
      const ref = await store.faceRef(face.face);
      ui.clearPicks();
      return newSketch({ type: "face", face: ref });
    }
    ui.setPick({ kind: "place", prompt: "Elegir dónde va el sketch: un plano base o una cara plana", done: (spec) => void newSketch(spec) });
  };

  /** Regiones elegidas en el visor, del primer sketch que tenga alguna */
  const pickedRegions = (): { sketch: number; points: P2[] } | undefined => {
    const regs = ui.picks().flatMap((p) => (p.kind === "region" ? [p] : []));
    if (!regs.length) return undefined;
    const sketch = regs[0].sketch;
    const view = store.sketchView(sketch);
    if (!view) return undefined;
    const points = regs.filter((r) => r.sketch === sketch).map((r) => view.regions[r.region]?.sample).filter((p): p is P2 => !!p);
    return { sketch, points };
  };

  /** Sketch al que apuntan extrusiones y revoluciones nuevas: el elegido o el último */
  const targetSketch = (): Feature | undefined => {
    const sel = selectedFeature();
    if (sel?.kind.type === "sketch") return sel;
    return [...sketches()].pop();
  };

  /** Sketches con alguna región cerrada (perfiles posibles) */
  const profileSketches = () => sketches().filter((s) => (store.sketchView(s.id)?.regions.length ?? 0) > 0);

  /** Barrido: perfil = el sketch de las regiones elegidas (o el último con regiones); camino = otro sketch */
  const addSweep = () => {
    const picked = pickedRegions();
    const profile = picked ? sketches().find((f) => f.id === picked.sketch) : [...profileSketches()].pop();
    const path = [...sketches()].reverse().find((s) => s.id !== profile?.id && !profileSketches().includes(s)) ?? [...sketches()].reverse().find((s) => s.id !== profile?.id);
    if (!profile || !path) return say("Hacen falta dos sketches: el perfil (una región cerrada) y el camino");
    ui.clearPicks();
    void store.addFeature({
      type: "sweep",
      sketch: profile.id,
      regions: picked ? { type: "points", points: picked.points } : { type: "all" },
      path: { type: "sketch", sketch: path.id, entities: [] },
      op: "join",
    });
  };

  /** Engrosar las caras elegidas (si no hay, se eligen en la caja del diálogo) */
  const startThicken = async () => {
    const picked = ui.picks().flatMap((p) => (p.kind === "face" ? [p.face] : []));
    const faces = await Promise.all(picked.map((f) => store.faceRef(f)));
    ui.clearPicks();
    void store.addFeature({ type: "thicken", faces, thickness: 2, op: "join" });
  };

  const startMoveFace = async () => {
    const picked = ui.picks().flatMap((p) => (p.kind === "face" ? [p.face] : []));
    const faces = await Promise.all(picked.map((f) => store.faceRef(f)));
    ui.clearPicks();
    void store.addFeature({ type: "move_face", faces, distance: 5 });
  };

  /** Patrón a lo largo del último sketch (o de la operación elegida, si tiene herramienta) */
  const addCurvePattern = () => {
    const sel = selectedFeature();
    const features = sel && toolFeatures().includes(sel) ? [sel.id] : [];
    const path = [...sketches()].pop();
    if (!path) return say("Primero un sketch con el camino (líneas o arcos)");
    void store.addFeature({ type: "pattern", features, pattern: { type: "curve", path: { type: "sketch", sketch: path.id, entities: [] }, count: 4 } });
  };

  /** Agujero M6 pasante en los puntos del sketch elegido (o el último) */
  const addHole = () => {
    const s = targetSketch();
    if (!s) return say("Primero un sketch con puntos (o círculos) donde van los agujeros, por ejemplo sobre una cara");
    const m6 = METRIC_HOLES.find((h) => h.size === "M6")!;
    void store.addFeature({ type: "hole", sketch: s.id, points: [], diameter: m6.clearance, depth: { type: "through_all" }, style: { type: "simple" }, tip_angle: 118, thread: null });
  };

  /** Transición entre los dos últimos sketches con regiones */
  const addLoft = () => {
    const list = profileSketches().slice(-2);
    if (list.length < 2) return say("Hacen falta al menos dos sketches con una región cerrada, en planos distintos");
    void store.addFeature({ type: "loft", sections: list.map((s) => ({ sketch: s.id, regions: { type: "all" } })), ruled: false, op: "join" });
  };

  const addExtrude = () => {
    // Con regiones elegidas en el visor se extruyen esas; si no, todo el sketch
    const picked = pickedRegions();
    const s = picked ? sketches().find((f) => f.id === picked.sketch) : targetSketch();
    if (!s) return say("Primero hace falta un sketch con una región cerrada");
    ui.clearPicks();
    void store.addFeature({
      type: "extrude",
      sketch: s.id,
      regions: picked ? { type: "points", points: picked.points } : { type: "all" },
      extent: { type: "blind", distance: 10 },
      reverse: false,
      op: "join",
    });
  };

  const addRevolve = () => {
    const picked = pickedRegions();
    const s = picked ? sketches().find((f) => f.id === picked.sketch) : targetSketch();
    if (!s || s.kind.type !== "sketch") return say("Primero hace falta un sketch con una región cerrada");
    ui.clearPicks();
    // Eje: la primera línea de construcción del sketch, o Z
    const axisLine = s.kind.sketch.entities.find((e) => e.construction && e.geometry.type === "line");
    const axis: AxisSpec = axisLine ? { type: "sketch_line", sketch: s.id, line: axisLine.id } : { type: "z" };
    const regions: RegionSelection = picked ? { type: "points", points: picked.points } : { type: "all" };
    void store.addFeature({ type: "revolve", sketch: s.id, regions, axis, angle: 360, op: "join" });
  };

  const primitive = (s: Extract<FeatureKind, { type: "primitive" }>["shape"]): Extract<FeatureKind, { type: "primitive" }> => ({
    type: "primitive",
    shape: s,
    origin: [0, 0, 0],
    z: [0, 0, 1],
    x: [1, 0, 0],
    op: "join",
  });

  /** Redondeo o chaflán con las aristas ya elegidas; si no hay, se eligen en la caja del diálogo */
  const startEdges = async (kind: "fillet" | "chamfer") => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    const picked = ui.picks().flatMap((p) => (p.kind === "edge" ? [p.edge] : []));
    const edges = await Promise.all(picked.map((e) => store.edgeRef(e)));
    ui.clearPicks();
    if (kind === "fillet") void store.addFeature({ type: "fillet", edges, radius: 1 });
    else void store.addFeature({ type: "chamfer", edges, distance: 1 });
  };

  /** Vaciado o desmolde con las caras ya elegidas; si no hay, se eligen en la caja del diálogo */
  const startFaces = async (kind: "shell" | "draft") => {
    if (!store.result()?.body) return say("Primero hace falta un sólido");
    const picked = ui.picks().flatMap((p) => (p.kind === "face" ? [p.face] : []));
    const faces = await Promise.all(picked.map((f) => store.faceRef(f)));
    ui.clearPicks();
    if (kind === "shell") void store.addFeature({ type: "shell", faces, thickness: 1 });
    else void store.addFeature({ type: "draft", faces, neutral: { type: "xy" }, angle: 3 });
  };

  // El menú del visor las usa con lo elegido
  ui.setActions({
    sketch: () => void startSketch(),
    fillet: () => void startEdges("fillet"),
    chamfer: () => void startEdges("chamfer"),
    shell: () => void startFaces("shell"),
    draft: () => void startFaces("draft"),
  });

  const addPattern = (kind: "linear" | "circular") => {
    const sel = selectedFeature();
    const features = sel && toolFeatures().includes(sel) ? [sel.id] : [];
    void store.addFeature({
      type: "pattern",
      features,
      pattern: kind === "linear" ? { type: "linear", direction: [1, 0, 0], count: 3, spacing: 10 } : { type: "circular", axis: { type: "z" }, count: 6, angle: 360 },
    });
  };

  /** Piezas de las caras, aristas o vértices elegidos en el visor (en orden) */
  const pickedParts = (): PartId[] => {
    const parts = store.result()?.parts ?? [];
    const out: PartId[] = [];
    for (const p of ui.picks()) {
      const face = p.kind === "face" ? p.face : undefined;
      const edge = p.kind === "edge" ? p.edge : undefined;
      const part = parts.find((x) => (face !== undefined && x.faces[0] <= face && face < x.faces[1]) || (edge !== undefined && x.edges[0] <= edge && edge < x.edges[1]));
      if (part && !out.some((o) => samePart(o, part.id))) out.push(part.id);
    }
    return out;
  };

  /**
   * Referencia nueva a partir de lo elegido: plano desplazado de la cara o el
   * plano elegido; eje de la arista o del cilindro; punto en el vértice o el
   * centro de la arista. Sin nada elegido, valores que se ajustan en el diálogo.
   */
  const addReference = async (kind: "plane" | "axis" | "point") => {
    const picks = ui.picks();
    const face = picks.find((p) => p.kind === "face");
    const edge = picks.find((p) => p.kind === "edge");
    const vertex = picks.find((p) => p.kind === "vertex");
    const plane = picks.find((p) => p.kind === "plane" || p.kind === "refplane");
    ui.clearPicks();
    if (kind === "plane") {
      let base: PlaneSpec = { type: "xy" };
      if (plane?.kind === "plane") base = { type: plane.plane };
      else if (plane?.kind === "refplane") base = { type: "reference", feature: plane.feature };
      else if (face?.kind === "face") base = { type: "face", face: await store.faceRef(face.face) };
      return void store.addFeature({ type: "plane", def: { type: "offset", base, distance: 10 } });
    }
    if (kind === "axis") {
      if (edge?.kind === "edge") return void store.addFeature({ type: "axis", def: { type: "edge", edge: await store.edgeRef(edge.edge) } });
      if (face?.kind === "face") return void store.addFeature({ type: "axis", def: { type: "face", face: await store.faceRef(face.face) } });
      return void store.addFeature({ type: "axis", def: { type: "two_points", a: { type: "at", point: [0, 0, 0] }, b: { type: "at", point: [0, 0, 10] } } });
    }
    if (vertex?.kind === "vertex") return void store.addFeature({ type: "point", def: { type: "at", point: vertex.at } });
    if (edge?.kind === "edge") return void store.addFeature({ type: "point", def: { type: "center", edge: await store.edgeRef(edge.edge) } });
    void store.addFeature({ type: "point", def: { type: "at", point: [0, 0, 0] } });
  };

  /** Booleana: con piezas elegidas, la primera es la que queda y las demás, las que restan */
  const addBoolean = () => {
    const [first, ...rest] = pickedParts();
    ui.clearPicks();
    void store.addFeature({ type: "boolean", op: "subtract", targets: first ? [first] : [], tools: rest, keep_tools: false });
  };

  const addMirror = () => {
    const sel = selectedFeature();
    const features = sel && toolFeatures().includes(sel) ? [sel.id] : [];
    void store.addFeature({ type: "mirror", features, plane: { type: "yz" } });
  };

  // ─── Archivos ─────────────────────────────────────────────────────────

  const importStep = async () => {
    const path = await open({ filters: [{ name: "STEP", extensions: ["step", "stp"] }], multiple: false });
    if (typeof path !== "string") return;
    try {
      await store.importStep(path);
    } catch (e) {
      say(String(e));
    }
  };


  const addPrimitive = (shape: PrimitiveShape) => void store.addFeature(primitive(shape));

  return {
    notice,
    say,
    startSketch,
    addExtrude,
    addRevolve,
    addSweep,
    addLoft,
    addHole,
    addPrimitive,
    startEdges,
    startFaces,
    addPattern,
    addMirror,
    addCurvePattern,
    addBoolean,
    pickedParts,
    addReference,
    startThicken,
    startMoveFace,
    importStep,
    addSplit: () => void store.addFeature({ type: "split", plane: { type: "custom", plane: offsetPlane("xy", 0) }, flip: false }),
    addSplitParts: () => void store.addFeature({ type: "split_parts", parts: pickedParts() }),
    addDeleteParts: () => void store.addFeature({ type: "delete_parts", parts: pickedParts() }),
    addHelix: () => void store.addFeature({ type: "helix", axis: { type: "z" }, radius: 10, pitch: 5, turns: 3, left: false }),
    addScale: () => void store.addFeature({ type: "scale", factor: [2, 2, 2], center: { type: "at", point: [0, 0, 0] } }),
    sketches,
    toolFeatures,
    selectedFeature,
  };
}

export type DesignActions = ReturnType<typeof createDesignActions>;
