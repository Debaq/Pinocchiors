import { Component, For, type JSX, Index, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { CadViewer, entityPolyline, planeToWorld } from "../../lib/CadViewer";
import type { LightSettings } from "../../lib/lightRig";
import type { CameraPose } from "../../lib/cameraRig";
import { parse as parseFont, type Font } from "opentype.js";
import { outlineContours } from "../../lib/sketchText";
import { addPoint, addText, removeText, textOf, constraintIds, ellipsePolyline, splineOf, splinePolyline, constraintValue, isReference, extendLine, isSolidPoint, leavingDirection, placeSnap, tangentArc, trimAt, type CadStore, type Geometry, designMass, dragByHandle, flipByHandle, handleField, partColor, partHidden, samePart, type FeatureHandle, type MeasureItem, type Measurement, type P2, type P3, type Sketch, type SketchConstraint } from "../../lib/cad";
import { infer, solidRefs, SNAP_GLYPHS, type Snap, type SnapKind } from "../../lib/sketchSnap";
import type { CadUi, Pick3d, PickFilter, SketchTool } from "../../lib/cadUi";
import type { MeshData } from "../../lib/Viewer3D";
import { Button, IconButton, Slider, Tooltip } from "../ui";
import { ContextMenu, type MenuEntry } from "../ui/ContextMenu";
import { DrawingView } from "./DrawingView";
import { FlatPatternView } from "./FlatPatternView";
import { DesignToolbar } from "../design/DesignToolbar";
import { FeatureEditor, pickSummary } from "../steps/DesignStep";
import type { DesignActions } from "../../lib/designActions";
import * as Icons from "../icons";
import * as SketchIcons from "../icons/sketch";

/** mm³ o cm³ según el tamaño */
const formatVolume = (v: number) => (v >= 1000 ? `${(v / 1000).toLocaleString("es", { maximumFractionDigits: 2 })} cm³` : `${v.toLocaleString("es", { maximumFractionDigits: 1 })} mm³`);

export interface CadViewProps {
  store: CadStore;
  ui: CadUi;
  /** Modelo cargado, de referencia (escaneo a calcar) */
  scanMesh?: MeshData | null;
  /** Grilla visible (el ojo del Outliner) */
  showGrid?: boolean;
  /** Acciones para la barra de herramientas */
  actions: DesignActions;
  /** Luces del panel Luces (las mismas que en el visor principal) */
  lights?: LightSettings;
  /** Modo notebook: la rueda gira la vista */
  trackpad?: boolean;
  /** La cámara del visor principal al entrar (se lee una vez) */
  initialPose?: CameraPose;
  /** Al salir, la cámara de acá para el visor principal */
  onLeave?: (pose: CameraPose) => void;
}

// Grupos de la barra: elegir · dibujar · modificar
const TOOLS: { id: SketchTool; short: string; label: string; key?: string; icon: (p: { size?: number }) => JSX.Element; group: number }[] = [
  { id: "select", short: "Elegir", label: "Elegir y arrastrar", key: "S", icon: SketchIcons.Select, group: 0 },
  { id: "line", short: "Línea", label: "Línea", key: "L", icon: SketchIcons.Line, group: 1 },
  { id: "line_mid", short: "Línea centro", label: "Línea desde el centro (centro, extremo: crece igual a los dos lados)", key: "M", icon: SketchIcons.LineMid, group: 1 },
  { id: "rect", short: "Rectángulo", label: "Rectángulo", key: "R", icon: SketchIcons.Rect, group: 1 },
  { id: "rect_center", short: "Rect. centro", label: "Rectángulo por el centro (centro, esquina)", icon: SketchIcons.RectCenter, group: 1 },
  { id: "circle", short: "Círculo", label: "Círculo", key: "C", icon: SketchIcons.Circle, group: 1 },
  { id: "arc", short: "Arco", label: "Arco (centro, inicio, fin)", key: "A", icon: SketchIcons.ArcCenter, group: 1 },
  { id: "arc3", short: "Arco 3 p.", label: "Arco por 3 puntos (inicio, fin, uno por donde pasa)", key: "3", icon: SketchIcons.Arc3, group: 1 },
  { id: "tangent", short: "Tangente", label: "Arco tangente (desde el extremo de una línea o arco)", key: "G", icon: SketchIcons.TangentArc, group: 1 },
  { id: "ellipse", short: "Elipse", label: "Elipse (centro, extremo del eje mayor, ancho)", key: "I", icon: SketchIcons.Ellipse, group: 1 },
  { id: "polygon", short: "Polígono", label: "Polígono regular (centro, vértice)", key: "P", icon: SketchIcons.Polygon, group: 1 },
  { id: "slot", short: "Ranura", label: "Ranura (centro, centro, ancho)", key: "U", icon: SketchIcons.Slot, group: 1 },
  { id: "spline", short: "Spline", label: "Spline (clics por donde pasa; clic en el primero la cierra, Esc la termina)", key: "N", icon: SketchIcons.Spline, group: 1 },
  { id: "point", short: "Punto", label: "Punto suelto (para agujeros y referencias)", key: "O", icon: SketchIcons.Point, group: 1 },
  { id: "text", short: "Texto", label: "Texto (clic donde empieza la línea base)", key: "X", icon: SketchIcons.Text, group: 1 },
  { id: "trim", short: "Recortar", label: "Recortar (clic en el tramo a quitar)", key: "T", icon: SketchIcons.Trim, group: 2 },
  { id: "extend", short: "Extender", label: "Extender (clic cerca del extremo)", key: "E", icon: SketchIcons.Extend, group: 2 },
  { id: "use", short: "Usar arista", label: "Usar arista del sólido (queda ligada: si el sólido cambia, se mueve con él)", key: "J", icon: SketchIcons.Use, group: 2 },
];

const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

// Fuente del texto: la incluida (Liberation Sans, OFL) o la que se elija
let textFont: { name: string; font: Font } | undefined;
async function loadTextFont(): Promise<Font> {
  if (!textFont) {
    const res = await fetch(`${import.meta.env.BASE_URL}fonts/LiberationSans-Regular.ttf`);
    textFont = { name: "Liberation Sans", font: parseFont(await res.arrayBuffer()) };
  }
  return textFont.font;
}

/** Centro del círculo que pasa por tres puntos (nada si están en línea) */
function circumcenter(a: P2, b: P2, c: P2): P2 | undefined {
  const d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  if (Math.abs(d) < 1e-12 * Math.max(1, dist(a, b) * dist(b, c))) return undefined;
  const [a2, b2, c2] = [a, b, c].map((p) => p[0] * p[0] + p[1] * p[1]);
  return [(a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d, (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d];
}

/** Extremo del semieje menor de una elipse (centro `c`, eje mayor hasta `a`) con el ancho hasta `p` */
function ellipseMinor(c: P2, a: P2, p: P2): P2 | undefined {
  const l = dist(c, a);
  if (l < 1e-12) return undefined;
  const n: P2 = [-(a[1] - c[1]) / l, (a[0] - c[0]) / l];
  const b = (p[0] - c[0]) * n[0] + (p[1] - c[1]) * n[1];
  if (Math.abs(b) < 1e-9) return undefined;
  return [c[0] + n[0] * Math.abs(b), c[1] + n[1] * Math.abs(b)];
}

/** Arco de `a` a `b` que pasa por `m`: centro, radio, ángulo inicial y barrido (con signo) */
function arcThrough(a: P2, b: P2, m: P2): { c: P2; r: number; a0: number; sweep: number } | undefined {
  const c = circumcenter(a, b, m);
  if (!c) return undefined;
  const ang = (p: P2) => Math.atan2(p[1] - c[1], p[0] - c[0]);
  const rel = (t: number) => ((t - ang(a)) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI);
  // Antihorario de a a b si m queda en ese recorrido; si no, horario
  const sb = rel(ang(b));
  const sweep = rel(ang(m)) < sb ? sb : sb - 2 * Math.PI;
  return { c, r: dist(c, a), a0: ang(a), sweep };
}

/** Distancia de un punto a un segmento */
function segDist(p: P2, a: P2, b: P2): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 > 0 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2)) : 0;
  return dist(p, [a[0] + t * dx, a[1] + t * dy]);
}

/** Entidad o punto bajo el cursor (ids), los puntos primero */
function hitTest(s: Sketch, p: P2, tol: number, withPoints = true): { point?: number; entity?: number } {
  const pt = new Map(s.points.map((q) => [q.id, [q.x, q.y] as P2]));
  let best: { id: number; d: number } | undefined;
  for (const q of withPoints ? s.points : []) {
    const d = dist(p, [q.x, q.y]);
    if (d <= tol && (!best || d < best.d)) best = { id: q.id, d };
  }
  if (best) return { point: best.id };
  for (const e of s.entities) {
    const g = e.geometry;
    let d = Infinity;
    if (g.type === "line") d = segDist(p, pt.get(g.start)!, pt.get(g.end)!);
    else if (g.type === "circle") d = Math.abs(dist(p, pt.get(g.center)!) - g.radius);
    else if (g.type === "arc") d = Math.abs(dist(p, pt.get(g.center)!) - dist(pt.get(g.start)!, pt.get(g.center)!));
    else if (g.type === "point") d = dist(p, pt.get(g.point)!);
    else if (g.type === "ellipse") {
      const pts = ellipsePolyline(pt.get(g.center)!, pt.get(g.major)!, pt.get(g.minor)!);
      for (let i = 0; i + 1 < pts.length; i++) d = Math.min(d, segDist(p, pts[i], pts[i + 1]));
    }
    else {
      const pts = splineOf(g, (x) => pt.get(x)) ?? [];
      for (let i = 0; i + 1 < pts.length; i++) d = Math.min(d, segDist(p, pts[i], pts[i + 1]));
    }
    if (d <= tol && (!best || d < best.d)) best = { id: e.id, d };
  }
  return best ? { entity: best.id } : {};
}

/** Vértices de un polígono regular de centro `c` con un vértice en `v` */
function polygonPoints(c: P2, v: P2, n: number, closed = false): P2[] {
  const r = dist(c, v);
  const a0 = Math.atan2(v[1] - c[1], v[0] - c[0]);
  const pts: P2[] = Array.from({ length: n }, (_, i) => [c[0] + r * Math.cos(a0 + (2 * Math.PI * i) / n), c[1] + r * Math.sin(a0 + (2 * Math.PI * i) / n)]);
  if (closed) pts.push(pts[0]);
  return pts;
}

/** Radio de una ranura: distancia del cursor a la recta entre centros */
function slotRadius(a: P2, b: P2, p: P2): number {
  const d = dist(a, b) || 1;
  return Math.abs(((b[0] - a[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (b[1] - a[1])) / d);
}

/** Contorno de una ranura para la vista previa */
function slotOutline(a: P2, b: P2, r: number): P2[] {
  const ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
  const out: P2[] = [];
  for (let i = 0; i <= 16; i++) {
    const t = ang - Math.PI / 2 + (Math.PI * i) / 16;
    out.push([b[0] + r * Math.cos(t), b[1] + r * Math.sin(t)]);
  }
  for (let i = 0; i <= 16; i++) {
    const t = ang + Math.PI / 2 + (Math.PI * i) / 16;
    out.push([a[0] + r * Math.cos(t), a[1] + r * Math.sin(t)]);
  }
  out.push(out[0]);
  return out;
}

export const CadView: Component<CadViewProps> = (props) => {
  let container!: HTMLDivElement;
  let viewer: CadViewer | undefined;
  const [cursor, setCursor] = createSignal<P2>();
  // Estado de la herramienta en curso (clics ya dados)
  // `lastSnapped`: el último punto ya existía (se ancló a él) al hacer clic
  const [chain, setChain] = createSignal<{ first: number; last: number; lastSnapped: boolean }>();
  // Arco tangente: desde qué punto, en qué dirección y de qué entidad viene
  const [tangentFrom, setTangentFrom] = createSignal<{ point: number; dir: P2; entity: number }>();
  const [anchor, setAnchor] = createSignal<Snap[]>([]);
  // Anclaje bajo el cursor: punto resaltado y su glifo junto al puntero
  const [snapView, setSnapView] = createSignal<{ kind: SnapKind; p: P2; x: number; y: number; guides: [P2, P2][]; refs: number[] }>();
  const [polygonSides, setPolygonSides] = createSignal(6);
  /** Texto elegido con «Elegir» (una de sus curvas o su ancla): se puede rehacer */
  const selectedText = () => {
    const s = ui.session();
    if (!s || ui.tool() !== "select") return undefined;
    for (const id of ui.selection()) {
      const t = textOf(s.sketch, id);
      if (t) return t;
    }
    return undefined;
  };
  /** Rehace las curvas de un texto en su ancla (mismo ancla y mismo id) */
  const rewriteText = async (id: number, text: string, size: number) => {
    try {
      const font = await loadTextFont();
      const s = ui.session();
      const t = s?.sketch.texts?.find((x) => x.id === id);
      const a = t && s!.sketch.points.find((p) => p.id === t.anchor);
      if (!t || !a || !text.trim()) return;
      const at: P2 = [a.x, a.y];
      const contours = outlineContours(font.getPath(text, 0, 0, size).commands, at);
      ui.change((sk) => {
        const cur = sk.texts?.find((x) => x.id === id);
        if (!cur) return;
        removeText(sk, cur, true);
        addText(sk, contours, at, { text, size, font: fontName() }, cur.anchor, cur.id);
      });
      ui.setSelection([t.anchor]);
      ui.setMessage(undefined);
    } catch (err) {
      ui.setMessage(`No se pudo leer la fuente: ${err}`);
    }
  };
  const [textValue, setTextValue] = createSignal("Texto");
  const [textSize, setTextSize] = createSignal(10);
  const [fontName, setFontName] = createSignal("Liberation Sans");
  const [scanVisible, setScanVisible] = createSignal(true);
  const [scanOpacity, setScanOpacity] = createSignal(0.35);
  let dragging: number | undefined;
  // Cambia con cada cuadro dibujado: las cotas HTML siguen a la cámara
  const [viewTick, setViewTick] = createSignal(0);
  const [editingDim, setEditingDim] = createSignal<number>();
  // Cotas que se piden al terminar una forma, en orden (índices de restricciones)
  const [dimQueue, setDimQueue] = createSignal<number[]>([]);
  /** Pide las cotas recién creadas: la primera queda en edición */
  const askDims = (indices: number[]) => {
    setDimQueue(indices.slice(1));
    setEditingDim(indices[0]);
  };
  /** Pasa a la siguiente cota pedida (o termina) */
  const nextDim = () => {
    const q = dimQueue();
    setDimQueue(q.slice(1));
    setEditingDim(q[0]);
  };

  const ui = props.ui;
  const store = props.store;

  const resetTool = () => {
    setChain(undefined);
    setAnchor([]);
    setTangentFrom(undefined);
  };

  const startPose = untrack(() => props.initialPose);
  onMount(async () => {
    viewer = new CadViewer(container);
    viewer.onRender = () => setViewTick((t) => t + 1);
    // Para las pruebas en navegador (scratch de desarrollo)
    if (import.meta.env.DEV) Object.assign(window, { __cadViewer: viewer, __cadStore: store, __cadUi: ui });
    try {
      viewer.mmPerUnit = await invoke<number>("cad_mm_per_unit");
    } catch {
      viewer.mmPerUnit = 1;
    }
    viewer.setScan(props.scanMesh);
    viewer.setBody(store.mesh());
    viewer.frameAll();
    // La misma vista que en el visor principal (grilla y luces quedan del encuadre)
    if (startPose) viewer.setCameraPose(startPose);
    setPlanesReady((n) => n + 1);
  });
  onCleanup(() => {
    if (viewer && store.mesh()) props.onLeave?.(viewer.cameraPose());
    viewer?.dispose();
  });

  createEffect(
    on(
      () => props.scanMesh,
      async (m) => {
        if (!viewer) return;
        try {
          viewer.mmPerUnit = await invoke<number>("cad_mm_per_unit");
        } catch {
          // sin backend: queda la escala anterior
        }
        viewer.setScan(m);
        viewer.setBody(store.mesh());
        viewer.frameAll();
        setPlanesReady((n) => n + 1);
      },
      { defer: true },
    ),
  );
  // Encuadrar cuando aparece el primer sólido (no en cada recálculo); al entrar
  // con la vista del visor principal, ya hay uno
  let hadBody = !!startPose;
  createEffect(() => {
    const m = store.mesh();
    // En el ensamble el visor muestra las instancias (efecto de abajo)
    if (ui.assemblyMode()) return;
    viewer?.setBody(m);
    if (m && !hadBody && !ui.session()) viewer?.frameAll();
    hadBody = !!m;
  });
  // Ensamble: cada instancia en su lugar, con el color de su pieza
  let asmSeq = 0;
  createEffect(
    on([() => ui.assemblyMode(), () => store.result()?.assembly?.version], async ([on_]) => {
      const n = ++asmSeq;
      if (!on_) {
        viewer?.setBody(store.mesh());
        return;
      }
      if (!store.result()?.assembly) return viewer?.setBody(null);
      try {
        const [mesh, faces] = await Promise.all([store.assemblyMesh(), store.assemblyFaces()]);
        if (n !== asmSeq || !viewer) return;
        viewer.setBody(mesh);
        const doc = store.doc();
        const parts = store.result()?.parts ?? [];
        viewer.setParts(
          faces.map(([id, a, b]) => {
            const inst = doc?.assembly?.instances.find((i) => i.id === id);
            const k = parts.findIndex((p) => inst && samePart(p.id, inst.part));
            return { faces: [a, b] as [number, number], edges: [0, 0] as [number, number], color: parseInt((k >= 0 ? partColor(doc, parts[k], k) : "#9aa4b8").slice(1), 16), hidden: false };
          }),
        );
      } catch (e) {
        ui.setMessage(String(e));
      }
    }),
  );
  createEffect(() => {
    const h = ui.highlight();
    const picks = ui.picks();
    const faces = [...h.faces, ...picks.flatMap((p) => (p.kind === "face" ? [p.face] : []))];
    const edges = [...h.edges, ...picks.flatMap((p) => (p.kind === "edge" ? [p.edge] : []))];
    viewer?.setHighlight(faces, edges);
  });
  createEffect(() => {
    store.mesh();
    const body = store.result()?.body;
    // Con materiales por pieza, el centro ponderado por la masa de cada una
    const c = designMass(store.doc(), store.result()).center ?? body?.center;
    viewer?.setCenterOfMass(ui.showCenterOfMass() && c ? c : null);
  });
  // Planos, ejes y puntos de referencia (el ojo del árbol los oculta)
  createEffect(() => {
    const hidden = ui.hiddenSketches();
    const refs = (store.result()?.references ?? []).filter((r) => !hidden.includes(r.id) && r.id !== ui.session()?.feature);
    const chosen = ui.picks().flatMap((p) => (p.kind === "refplane" ? [p.feature] : []));
    viewer?.setReferences(refs, chosen);
  });
  // Color y visibilidad de cada pieza (con una sola, el color de siempre)
  createEffect(() => {
    store.mesh();
    if (ui.assemblyMode()) return;
    const parts = store.result()?.parts ?? [];
    const doc = store.doc();
    viewer?.setParts(
      parts.map((p, i) => ({
        faces: p.faces,
        edges: p.edges,
        color: parseInt(partColor(doc, p, i).slice(1), 16),
        hidden: partHidden(doc, p),
        surface: p.surface,
      })),
    );
  });
  // Vista de corte: el plano recorre la caja envolvente del sólido
  createEffect(() => {
    store.mesh();
    const s = ui.section();
    const body = store.result()?.body;
    if (!s || !body) return viewer?.setSection(null);
    const axis = { xy: 2, xz: 1, yz: 0 }[s.plane];
    const normal: P3 = [0, 0, 0];
    normal[axis] = s.flip ? -1 : 1;
    const origin: P3 = body.bbox_min.map((v, i) => (v + body.bbox_max[i]) / 2) as P3;
    origin[axis] = body.bbox_min[axis] + s.at * (body.bbox_max[axis] - body.bbox_min[axis]);
    viewer?.setSection({ origin, normal });
  });

  // Medidas de lo elegido en el sólido (una o dos cosas), como en Onshape
  const [measurement, setMeasurement] = createSignal<Measurement>();
  let measureSeq = 0;
  createEffect(() => {
    const picks = ui.picks();
    const version = store.result()?.version;
    const items: MeasureItem[] = picks.flatMap((p): MeasureItem[] =>
      p.kind === "face" ? [{ kind: "face", index: p.face }] : p.kind === "edge" ? [{ kind: "edge", index: p.edge }] : p.kind === "vertex" ? [{ kind: "vertex", point: p.at }] : [],
    );
    const n = ++measureSeq;
    if (ui.session() || version === undefined || items.length === 0 || items.length > 2 || items.length !== picks.length) return setMeasurement(undefined);
    store
      .measure(items)
      .then((m) => n === measureSeq && setMeasurement(m))
      .catch(() => n === measureSeq && setMeasurement(undefined));
  });
  createEffect(() => {
    const vertices = ui.picks().flatMap((p) => (p.kind === "vertex" ? [p.at] : []));
    const d = measurement()?.distance;
    viewer?.setMarks(vertices, d && d.value > 1e-9 ? [d.a, d.b] : undefined);
  });

  // Sketches visibles (todos menos los ocultos y el que se edita), con sus regiones elegibles
  createEffect(() => {
    const editing = ui.session()?.feature;
    const hidden = ui.hiddenSketches();
    const picks = ui.picks();
    const list = (store.result()?.sketches ?? [])
      .filter((v) => v.id !== editing && !hidden.includes(v.id))
      .map((v) => ({
        id: v.id,
        plane: v.plane,
        sketch: v.sketch,
        regions: v.regions,
        selected: picks.flatMap((p) => (p.kind === "region" && p.sketch === v.id ? [p.region] : [])),
      }));
    viewer?.setVisibleSketches(list);
  });
  createEffect(() => viewer?.setGridVisible(props.showGrid !== false));
  createEffect(() => props.lights && viewer?.setLights(props.lights));
  createEffect(() => viewer && (viewer.trackpad = props.trackpad === true));
  // Planos base: visibles fuera de la edición (y al elegir dónde va un sketch)
  const [planesReady, setPlanesReady] = createSignal(0);
  createEffect(() => {
    planesReady();
    const show = ui.showPlanes() && !ui.session();
    const selected = ui.picks().flatMap((p) => (p.kind === "plane" ? [p.plane] : []));
    const place = ui.pick().kind === "place";
    viewer?.setPlanes(show || place, selected, place ? [] : ui.hiddenPlanes());
  });
  // Herramienta de la operación en el diálogo (translúcida, verde o roja)
  createEffect(() => {
    const t = store.tool();
    viewer?.setTool(t?.mesh ?? null, t?.op);
  });
  // Flecha de la operación del diálogo (extrusión, chapa, agujero), como en
  // Onshape: clic la da vuelta; arrastrándola cambia la distancia
  const handleOf = createMemo(() => {
    const d = store.draft();
    if (!d || d.selecting || ui.session()) return undefined;
    const f = d.doc.features.find((x) => x.id === d.feature);
    const h = f && store.stateOf(f.id)?.handle;
    return f && h && ["extrude", "hole", "sheet_metal"].includes(f.kind.type) ? { id: f.id, h } : undefined;
  });
  const [handleHot, setHandleHot] = createSignal(false);
  // Arrastre en curso: la flecha del comienzo y dónde va la punta (mm, con signo)
  let handleDrag: { id: number; h: FeatureHandle; x: number; y: number; moved: boolean; canDrag: boolean } | undefined;
  const [dragAt, setDragAt] = createSignal<{ h: FeatureHandle; t: number }>();
  createEffect(() => {
    const drag = dragAt();
    const cur = handleOf()?.h;
    // Mientras se arrastra sigue al puntero (el recálculo llega después)
    const h = drag
      ? { origin: drag.h.origin, dir: drag.h.dir.map((v) => (drag.t < 0 ? -v : v)) as P3, length: Math.abs(drag.t) }
      : cur;
    viewer?.setHandle(h ?? null, handleHot() || !!drag);
  });
  // Comparación con una versión: lo agregado en verde y lo quitado en rojo
  createEffect(() => {
    const c = store.comparison();
    viewer?.setComparison(c?.addedMesh ?? null, c?.removedMesh ?? null);
  });
  createEffect(() => viewer?.setScanHighlight(ui.scanHighlight()));
  createEffect(() => {
    props.scanMesh;
    viewer?.setScanDeviation(ui.deviation());
  });
  createEffect(() => viewer?.setScanVisible(scanVisible(), scanOpacity()));

  // Sketch: vista de frente al entrar, normal al salir
  // Memo: el efecto corre al entrar o salir de un sketch, no con cada cambio
  // del sketch (cortaría la polilínea en cada clic)
  const editingFeature = createMemo(() => ui.session()?.feature);
  createEffect(
    on(
      editingFeature,
      (feature) => {
        resetTool();
        if (!viewer) return;
        const s = ui.session();
        if (feature !== undefined && s) {
          viewer.saveView();
          // Un sketch con algo dibujado se encuadra entero (puede estar lejos del origen)
          const pts = s.sketch.points.filter((p) => p.id !== s.sketch.origin);
          if (s.sketch.entities.length && pts.length) {
            const xs = pts.map((p) => p.x);
            const ys = pts.map((p) => p.y);
            const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
            viewer.lookAtPlane(s.plane, { center: [(x0 + x1) / 2, (y0 + y1) / 2], size: Math.max(x1 - x0, y1 - y0) });
          } else viewer.lookAtPlane(s.plane);
        } else viewer.restoreView();
      },
    ),
  );

  // Dibujo del sketch con la vista previa de la herramienta
  createEffect(() => {
    const s = ui.session();
    if (!viewer) return;
    if (!s) {
      // Eligiendo regiones: el sketch de esa operación con las elegidas resaltadas
      const mode = ui.pick();
      if (mode.kind === "region" || mode.kind === "axis") {
        const view = store.sketchView(mode.sketch);
        viewer.setSketch(view ? { plane: view.plane, sketch: view.sketch, regions: view.regions, chosen: mode.kind === "region" ? mode.chosen() : [] } : null);
        return;
      }
      // Fuera de la edición los sketches se ven como sketches visibles
      viewer.setSketch(null);
      return;
    }
    const c = cursor();
    const preview: P2[][] = [];
    const pt = (id: number): P2 | undefined => {
      const p = s.sketch.points.find((q) => q.id === id);
      return p ? [p.x, p.y] : undefined;
    };
    const ch = chain();
    const an = anchor().map((a) => a.p);
    if (c) {
      const t = ui.tool();
      const from = ch && pt(ch.last);
      if (t === "line" && from) preview.push([from, c]);
      if (t === "line_mid" && from) preview.push([[2 * from[0] - c[0], 2 * from[1] - c[1]], c]);
      if (t === "rect" && an.length === 1) {
        const [a] = an;
        preview.push([a, [c[0], a[1]], c, [a[0], c[1]], a]);
      }
      if (t === "circle" && an.length === 1) {
        const r = dist(an[0], c);
        preview.push(Array.from({ length: 49 }, (_, i) => [an[0][0] + r * Math.cos((i / 48) * 2 * Math.PI), an[0][1] + r * Math.sin((i / 48) * 2 * Math.PI)] as P2));
      }
      if (t === "arc" && an.length >= 1) preview.push([an[0], an.length === 2 ? an[1] : c]);
      if (t === "polygon" && an.length === 1) preview.push(polygonPoints(an[0], c, polygonSides(), true));
      const tf = tangentFrom();
      const tp = tf && pt(tf.point);
      if (t === "tangent" && tf && tp) {
        const arc = tangentArc(tp, tf.dir, c);
        if (arc) {
          const r = dist(arc.center, tp);
          const a0 = Math.atan2(tp[1] - arc.center[1], tp[0] - arc.center[0]);
          let sweep = Math.atan2(c[1] - arc.center[1], c[0] - arc.center[0]) - a0;
          if (arc.ccw) while (sweep <= 0) sweep += 2 * Math.PI;
          else while (sweep >= 0) sweep -= 2 * Math.PI;
          preview.push(Array.from({ length: 33 }, (_, i) => [arc.center[0] + r * Math.cos(a0 + (sweep * i) / 32), arc.center[1] + r * Math.sin(a0 + (sweep * i) / 32)] as P2));
        } else preview.push([tp, c]);
      }
      if (t === "rect_center" && an.length === 1) {
        const [o] = an;
        const m: P2 = [2 * o[0] - c[0], 2 * o[1] - c[1]];
        preview.push([c, [m[0], c[1]], m, [c[0], m[1]], c]);
      }
      if (t === "spline" && an.length >= 1) preview.push(splinePolyline([...an, c], false));
      if (t === "ellipse" && an.length === 1) preview.push([an[0], c]);
      if (t === "ellipse" && an.length === 2) {
        const e = ellipseMinor(an[0], an[1], c);
        if (e) preview.push(ellipsePolyline(an[0], an[1], e));
      }
      if (t === "arc3" && an.length === 1) preview.push([an[0], c]);
      if (t === "arc3" && an.length === 2) {
        const arc = arcThrough(an[0], an[1], c);
        if (arc) preview.push(Array.from({ length: 49 }, (_, i) => [arc.c[0] + arc.r * Math.cos(arc.a0 + (arc.sweep * i) / 48), arc.c[1] + arc.r * Math.sin(arc.a0 + (arc.sweep * i) / 48)] as P2));
        else preview.push([an[0], an[1]]);
      }
      if (t === "slot" && an.length === 1) preview.push([an[0], c]);
      if (t === "slot" && an.length === 2) preview.push(slotOutline(an[0], an[1], slotRadius(an[0], an[1], c)));
    }
    viewer.setSketch({
      plane: s.plane,
      sketch: s.sketch,
      regions: s.regions,
      selected: ui.selection(),
      hover: [...ui.hoverIds(), ...(snapView()?.refs ?? [])],
      freePoints: s.report?.free_points,
      freeEntities: s.report?.free_entities,
      conflictEntities: (s.report?.conflicting ?? []).flatMap((i) => {
        const c = s.sketch.constraints[i];
        return c ? constraintIds(c).filter((id) => s.sketch.entities.some((e) => e.id === id)) : [];
      }),
      preview,
      snap: snapView()?.p,
      guides: snapView()?.guides,
    });
  });

  // Aristas del sólido proyectadas al plano del sketch en curso (anclajes)
  const solid = createMemo(() => {
    const plane = ui.session()?.plane;
    const m = store.mesh();
    if (!plane || !m || !viewer) return undefined;
    const s = viewer.mmPerUnit;
    const edges: P3[][] = [];
    let start = 0;
    for (const end of m.edgeEnds) {
      const poly: P3[] = [];
      for (let i = start; i < end; i++) {
        const [x, y, z] = m.edgePoints.subarray(i * 3, i * 3 + 3);
        poly.push([x * s, -z * s, y * s]);
      }
      edges.push(poly);
      start = end;
    }
    return solidRefs(edges, plane);
  });

  /**
   * Posición del cursor sobre el plano con su anclaje (punto, origen, medio,
   * centro, cuadrante, sobre una curva). Con Mayús se dibuja libre.
   */
  const snapped = (e: PointerEvent): Snap | undefined => {
    const s = ui.session();
    if (!s || !viewer) return undefined;
    const p = viewer.planePoint(e.clientX, e.clientY, s.plane);
    if (!p) return undefined;
    if (e.shiftKey) return { p, kind: "free" };
    // Dibujando una línea: puede salir paralela, perpendicular o tangente
    // (la línea desde el centro sale del centro con la misma dirección)
    const from = ui.tool() === "line" || ui.tool() === "line_mid" ? chain()?.last : undefined;
    return infer(s.sketch, p, viewer.pixelSizeMm() * 8, {
      exclude: dragging !== undefined ? [dragging] : [],
      from,
      solid: solid(),
      // Un rectángulo alineado con su primera esquina (o su centro) tendría ancho o alto cero
      noAlign: ui.tool() === "rect" || ui.tool() === "rect_center" ? anchor().flatMap((a) => (a.id !== undefined ? [a.id] : [])) : [],
    });
  };

  /** Crea la spline con los puntos marcados (abierta o cerrada) */
  const finishSpline = (closed: boolean) => {
    const an = anchor();
    setAnchor([]);
    if (an.length < 2) return;
    ui.change((sk) => {
      const points = an.map((a) => placeSnap(sk, a));
      ui.addEntity(sk, { type: "spline", points, closed });
    });
  };

  /** Agrega una cota y devuelve su índice */
  const dim = (sk: Sketch, c: SketchConstraint): number => {
    sk.constraints.push(c);
    return sk.constraints.length - 1;
  };
  /** Medida dibujada, redondeada a algo legible según el zoom */
  const round = (v: number) => {
    const px = viewer?.pixelSizeMm() ?? 0.1;
    const step = Math.pow(10, Math.floor(Math.log10(Math.max(px, 1e-6))));
    return Math.round(v / step) * step;
  };

  const sketchClick = (e: PointerEvent) => {
    const s = ui.session();
    const hit = snapped(e);
    if (!s || !hit || !viewer) return;
    const t = ui.tool();
    if (t === "select") {
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const h = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8);
      if (h.point !== undefined) {
        dragging = h.point;
        if (e.shiftKey) ui.setSelection((sel) => (sel.includes(h.point!) ? sel.filter((x) => x !== h.point) : [...sel, h.point!]));
        else if (!ui.selection().includes(h.point)) ui.setSelection([h.point]);
        return;
      }
      if (h.entity !== undefined) {
        const id = h.entity;
        ui.setSelection((sel) => (e.shiftKey ? (sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id]) : [id]));
        return;
      }
      // En vacío: arrastrar elige por caja (Mayús suma)
      boxStart = { x: e.clientX, y: e.clientY, additive: e.shiftKey, before: [] };
      sketchBoxBefore = ui.selection();
      if (!e.shiftKey) ui.setSelection([]);
      return;
    }
    if (t === "use") {
      // La arista del sólido bajo el cursor, proyectada al plano y ligada a él
      const pick = viewer.pick(e.clientX, e.clientY, { edges: true });
      if (pick?.kind !== "edge") return ui.setMessage("Clic sobre una arista del sólido");
      store
        .projectEdge(pick.edge, s.plane)
        .then((pe) => {
          ui.change((sk) => {
            const p = pe.points.map((q) => addPoint(sk, q));
            const geometry: Geometry =
              pe.kind === "line"
                ? { type: "line", start: p[0], end: p[1] }
                : pe.kind === "circle"
                  ? { type: "circle", center: p[0], radius: pe.radius }
                  : { type: "arc", center: p[0], start: p[1], end: p[2] };
            const entity = ui.addEntity(sk, geometry);
            sk.uses = [...(sk.uses ?? []), { edge: pe.ref, entity }];
          });
          ui.setMessage(undefined);
        })
        .catch((err) => ui.setMessage(String(err)));
      return;
    }
    // Cotas de la forma recién dibujada (se piden enseguida)
    const dims: number[] = [];
    if (t === "line") {
      const ch = chain();
      let closed = false;
      const snappedToPoint = hit.id !== undefined || isSolidPoint(hit);
      ui.change((sk) => {
        const id = placeSnap(sk, hit);
        if (!ch) {
          setChain({ first: id, last: id, lastSnapped: snappedToPoint });
          return;
        }
        if (id === ch.last) return;
        const a = sk.points.find((q) => q.id === ch.last)!;
        const line = ui.addEntity(sk, { type: "line", start: ch.last, end: id });
        // Entre dos puntos que ya estaban, la línea queda definida por ellos
        const between = snappedToPoint && ch.lastSnapped;
        // Lo que se vio al dibujar es lo que queda restringido
        const dir = hit.direction;
        if (hit.axis) sk.constraints.push({ type: hit.axis, line });
        else if (dir?.kind === "tangent") sk.constraints.push({ type: "tangent", a: line, b: dir.entity });
        else if (dir) sk.constraints.push({ type: dir.kind, a: dir.entity, b: line });
        closed = id === ch.first;
        // El tramo que cierra queda determinado por los demás: sin cota propia
        if (!closed && !between) dims.push(dim(sk, { type: "length", line, value: round(dist([a.x, a.y], hit.p)) }));
        setChain(closed ? undefined : { first: ch.first, last: id, lastSnapped: snappedToPoint });
      });
      askDims(dims);
      return;
    }
    if (t === "line_mid") {
      const ch = chain();
      ui.change((sk) => {
        const id = placeSnap(sk, hit);
        // Primer clic: el centro (queda como punto del sketch)
        if (!ch) return void setChain({ first: id, last: id, lastSnapped: false });
        if (id === ch.last) return;
        const c = sk.points.find((q) => q.id === ch.last)!;
        const other = addPoint(sk, [2 * c.x - hit.p[0], 2 * c.y - hit.p[1]]);
        const line = ui.addEntity(sk, { type: "line", start: other, end: id });
        sk.constraints.push({ type: "midpoint", point: ch.last, line });
        const dir = hit.direction;
        if (hit.axis) sk.constraints.push({ type: hit.axis, line });
        else if (dir && dir.kind !== "tangent") sk.constraints.push({ type: dir.kind, a: dir.entity, b: line });
        if (hit.id === undefined && !isSolidPoint(hit)) dims.push(dim(sk, { type: "length", line, value: round(2 * dist([c.x, c.y], hit.p)) }));
        setChain(undefined);
      });
      askDims(dims);
      return;
    }
    if (t === "rect") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      const [A, B] = [an[0], hit];
      if (Math.abs(A.p[0] - B.p[0]) > 1e-9 && Math.abs(A.p[1] - B.p[1]) > 1e-9)
        ui.change((sk) => {
          // Las esquinas elegidas usan sus anclajes; las otras dos son nuevas
          const pa = placeSnap(sk, A);
          const pb = placeSnap(sk, B);
          const pc = addPoint(sk, [B.p[0], A.p[1]]);
          const pd = addPoint(sk, [A.p[0], B.p[1]]);
          const l = [
            [pa, pc],
            [pc, pb],
            [pb, pd],
            [pd, pa],
          ].map(([start, end]) => ui.addEntity(sk, { type: "line", start, end }));
          sk.constraints.push({ type: "horizontal", line: l[0] }, { type: "horizontal", line: l[2] });
          sk.constraints.push({ type: "vertical", line: l[1] }, { type: "vertical", line: l[3] });
          // Entre dos puntos que ya estaban, el rectángulo queda definido por ellos
          if (A.id === undefined || B.id === undefined) {
            dims.push(dim(sk, { type: "length", line: l[0], value: round(Math.abs(B.p[0] - A.p[0])) }));
            dims.push(dim(sk, { type: "length", line: l[1], value: round(Math.abs(B.p[1] - A.p[1])) }));
          }
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "rect_center") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      const [C, K] = [an[0], hit];
      const [dx, dy] = [K.p[0] - C.p[0], K.p[1] - C.p[1]];
      if (Math.abs(dx) > 1e-9 && Math.abs(dy) > 1e-9)
        ui.change((sk) => {
          const c = placeSnap(sk, C);
          // Esquinas en orden: la del clic y las otras tres reflejadas en el centro
          const p0 = placeSnap(sk, K);
          const p1 = addPoint(sk, [C.p[0] - dx, C.p[1] + dy]);
          const p2 = addPoint(sk, [C.p[0] - dx, C.p[1] - dy]);
          const p3 = addPoint(sk, [C.p[0] + dx, C.p[1] - dy]);
          const l = [
            [p0, p1],
            [p1, p2],
            [p2, p3],
            [p3, p0],
          ].map(([start, end]) => ui.addEntity(sk, { type: "line", start, end }));
          sk.constraints.push({ type: "horizontal", line: l[0] }, { type: "horizontal", line: l[2] });
          sk.constraints.push({ type: "vertical", line: l[1] }, { type: "vertical", line: l[3] });
          // Diagonal de construcción partida en el centro: dos mitades alineadas e iguales
          const d1 = ui.addEntity(sk, { type: "line", start: p0, end: c });
          const d2 = ui.addEntity(sk, { type: "line", start: c, end: p2 });
          for (const d of [d1, d2]) sk.entities.find((e) => e.id === d)!.construction = true;
          sk.constraints.push({ type: "parallel", a: d1, b: d2 }, { type: "equal", a: d1, b: d2 });
          if (C.id === undefined || K.id === undefined) {
            dims.push(dim(sk, { type: "length", line: l[0], value: round(Math.abs(2 * dx)) }));
            dims.push(dim(sk, { type: "length", line: l[1], value: round(Math.abs(2 * dy)) }));
          }
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "text") {
      const text = textValue().trim();
      if (!text) return ui.setMessage("Escribir el texto en la barra del sketch");
      const at = hit.p;
      loadTextFont()
        .then((font) => {
          const contours = outlineContours(font.getPath(text, 0, 0, textSize()).commands, at);
          ui.change((sk) => addText(sk, contours, at, { text, size: textSize(), font: fontName() }));
          ui.setMessage(undefined);
        })
        .catch((err) => ui.setMessage(`No se pudo leer la fuente: ${err}`));
      return;
    }
    if (t === "spline") {
      const an = anchor();
      // Clic sobre el primer punto: se cierra
      if (an.length >= 3 && dist(an[0].p, hit.p) <= viewer.pixelSizeMm() * 8) return finishSpline(true);
      return setAnchor([...an, hit]);
    }
    if (t === "ellipse") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [C, M] = an;
      const n = ellipseMinor(C.p, M.p, hit.p);
      if (!n) return ui.setMessage("La elipse necesita ancho: mover el punto hacia un costado del eje");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const center = placeSnap(sk, C);
        const major = placeSnap(sk, M);
        const minor = addPoint(sk, n);
        ui.addEntity(sk, { type: "ellipse", center, major, minor });
        if (C.id === undefined || M.id === undefined) dims.push(dim(sk, { type: "distance", a: center, b: major, value: round(dist(C.p, M.p)) }));
        dims.push(dim(sk, { type: "distance", a: center, b: minor, value: round(dist(C.p, n)) }));
      });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "arc3") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [A, B] = an;
      const arc = arcThrough(A.p, B.p, hit.p);
      if (!arc) return ui.setMessage("Los tres puntos están en línea: mover el tercero hacia un costado");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const a = placeSnap(sk, A);
        const b = placeSnap(sk, B);
        const center = addPoint(sk, arc.c);
        // Los arcos van antihorarios: si pasa por el otro lado, de b a a
        const id = ui.addEntity(sk, arc.sweep > 0 ? { type: "arc", center, start: a, end: b } : { type: "arc", center, start: b, end: a });
        dims.push(dim(sk, { type: "radius", entity: id, value: round(arc.r) }));
      });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "point") {
      ui.change((sk) => {
        const id = placeSnap(sk, hit);
        if (!sk.entities.some((e) => e.geometry.type === "point" && e.geometry.point === id)) ui.addEntity(sk, { type: "point", point: id });
      });
      return;
    }
    if (t === "circle") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      const r = dist(an[0].p, hit.p);
      if (r > 1e-9)
        ui.change((sk) => {
          const center = placeSnap(sk, an[0]);
          // Por un punto que ya estaba: el círculo pasa por él, sin cota
          const through = hit.id !== undefined && hit.id !== center ? hit.id : undefined;
          const c = ui.addEntity(sk, { type: "circle", center, radius: through !== undefined ? r : round(r) });
          if (through !== undefined) sk.constraints.push({ type: "point_on_circle", point: through, circle: c });
          else dims.push(dim(sk, { type: "diameter", entity: c, value: round(2 * r) }));
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "extend") {
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      if (target === undefined) return;
      let msg: string | undefined;
      ui.change((sk) => (msg = extendLine(sk, target, raw)));
      ui.setMessage(msg);
      return;
    }
    if (t === "tangent") {
      const from = tangentFrom();
      if (!from) {
        // Primer clic: un extremo de una línea o un arco
        if (hit.id === undefined) return ui.setMessage("Empezar en el extremo de una línea o de un arco");
        const owner = s.sketch.entities.find(
          (en) => (en.geometry.type === "line" || en.geometry.type === "arc") && [en.geometry.start, en.geometry.end].includes(hit.id!),
        );
        const dir = owner && leavingDirection(s.sketch, owner.id, hit.id);
        if (!owner || !dir) return ui.setMessage("Empezar en el extremo de una línea o de un arco");
        ui.setMessage(undefined);
        setTangentFrom({ point: hit.id, dir, entity: owner.id });
        return;
      }
      const startPos = s.sketch.points.find((q) => q.id === from.point);
      if (!startPos) return setTangentFrom(undefined);
      const arc = tangentArc([startPos.x, startPos.y], from.dir, hit.p);
      if (!arc) return ui.setMessage("En línea recta no hay arco: mover el punto hacia un costado");
      let next: { point: number; dir: P2; entity: number } | undefined;
      ui.change((sk) => {
        const end = placeSnap(sk, hit);
        const center = ui.addPoint(sk, arc.center);
        const id = ui.addEntity(sk, arc.ccw ? { type: "arc", center, start: from.point, end } : { type: "arc", center, start: end, end: from.point });
        sk.constraints.push({ type: "tangent", a: from.entity, b: id });
        dims.push(dim(sk, { type: "radius", entity: id, value: round(dist(arc.center, hit.p)) }));
        next = { point: end, dir: arc.outDir, entity: id };
      });
      // Encadenar: el próximo arco sale tangente a este
      setTangentFrom(next);
      return askDims(dims);
    }
    if (t === "trim") {
      // El enganche a puntos no sirve acá: la línea bajo el cursor
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      if (target === undefined) return;
      let msg: string | undefined;
      ui.change((sk) => (msg = trimAt(sk, target, raw)));
      ui.setMessage(msg);
      return;
    }
    if (t === "polygon") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      const n = Math.max(3, Math.round(polygonSides()));
      const r = dist(an[0].p, hit.p);
      if (r > 1e-9)
        ui.change((sk) => {
          const center = placeSnap(sk, an[0]);
          const circle = ui.addEntity(sk, { type: "circle", center, radius: r });
          sk.entities.find((e) => e.id === circle)!.construction = true;
          // El primer vértice es el del clic (con su anclaje)
          const pts = polygonPoints(an[0].p, hit.p, n).map((p, i) => (i === 0 ? placeSnap(sk, hit) : ui.addPoint(sk, p)));
          const lines = pts.map((p, i) => ui.addEntity(sk, { type: "line", start: p, end: pts[(i + 1) % n] }));
          pts.forEach((p) => sk.constraints.push({ type: "point_on_circle", point: p, circle }));
          for (let i = 1; i < n; i++) sk.constraints.push({ type: "equal", a: lines[0], b: lines[i] });
          if (an[0].id === undefined || hit.id === undefined) dims.push(dim(sk, { type: "radius", entity: circle, value: round(r) }));
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "slot") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [a, b] = [an[0].p, an[1].p];
      const r = slotRadius(a, b, hit.p);
      const len = dist(a, b);
      if (r > 1e-9 && len > 1e-9)
        ui.change((sk) => {
          const n: P2 = [-(b[1] - a[1]) / len, (b[0] - a[0]) / len];
          const off = (p: P2, s: number): P2 => [p[0] + n[0] * r * s, p[1] + n[1] * r * s];
          const ca = placeSnap(sk, an[0]);
          const cb = placeSnap(sk, an[1]);
          const a1 = ui.addPoint(sk, off(a, 1));
          const a2 = ui.addPoint(sk, off(a, -1));
          const b1 = ui.addPoint(sk, off(b, 1));
          const b2 = ui.addPoint(sk, off(b, -1));
          const bottom = ui.addEntity(sk, { type: "line", start: a2, end: b2 });
          const arcB = ui.addEntity(sk, { type: "arc", center: cb, start: b2, end: b1 });
          const top = ui.addEntity(sk, { type: "line", start: b1, end: a1 });
          const arcA = ui.addEntity(sk, { type: "arc", center: ca, start: a1, end: a2 });
          sk.constraints.push(
            { type: "tangent", a: bottom, b: arcB },
            { type: "tangent", a: top, b: arcB },
            { type: "tangent", a: top, b: arcA },
            { type: "tangent", a: bottom, b: arcA },
            { type: "equal", a: arcA, b: arcB },
          );
          if (an[0].id === undefined || an[1].id === undefined) dims.push(dim(sk, { type: "distance", a: ca, b: cb, value: round(len) }));
          dims.push(dim(sk, { type: "radius", entity: arcA, value: round(r) }));
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "arc") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [C, A] = an;
      const [c, a] = [C.p, A.p];
      const r = dist(c, a);
      const ang = Math.atan2(hit.p[1] - c[1], hit.p[0] - c[0]);
      const b: P2 = [c[0] + r * Math.cos(ang), c[1] + r * Math.sin(ang)];
      if (r > 1e-9)
        ui.change((sk) => {
          const center = placeSnap(sk, C);
          const start = placeSnap(sk, A);
          // El fin solo se pega a un punto que ya estaba (el resto queda en el radio)
          const end = hit.id !== undefined && hit.id !== start && hit.id !== center ? hit.id : ui.addPoint(sk, b);
          const id = ui.addEntity(sk, { type: "arc", center, start, end });
          if (C.id === undefined || A.id === undefined) dims.push(dim(sk, { type: "radius", entity: id, value: round(r) }));
        });
      setAnchor([]);
      askDims(dims);
    }
  };

  const pickClick = async (e: PointerEvent) => {
    if (!viewer) return;
    const mode = ui.pick();
    try {
      if (mode.kind === "face") {
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true });
        if (hit?.kind !== "face") return;
        const ref = await store.faceRef(hit.face);
        // Las cajas de selección resaltan lo suyo
        if (!mode.owner) ui.setHighlight({ faces: [hit.face], edges: [] });
        ui.setPick({ kind: "none" });
        mode.done(ref, hit.face);
      } else if (mode.kind === "edges") {
        // La caja de selección resalta lo que tiene
        const hit = viewer.pick(e.clientX, e.clientY, { edges: true });
        if (hit?.kind !== "edge") return;
        mode.toggle(await store.edgeRef(hit.edge), hit.edge);
      } else if (mode.kind === "faces") {
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true });
        if (hit?.kind !== "face") return;
        mode.toggle(await store.faceRef(hit.face), hit.face);
      } else if (mode.kind === "region") {
        const view = store.sketchView(mode.sketch);
        const p = view && viewer.planePoint(e.clientX, e.clientY, view.plane);
        if (p) mode.toggle(p);
      } else if (mode.kind === "axis") {
        // Primero una línea del sketch (se ve mientras se elige); si no, una arista del sólido
        const view = store.sketchView(mode.sketch);
        const p = view && viewer.planePoint(e.clientX, e.clientY, view.plane);
        const h = view && p ? hitTest(view.sketch, p, viewer.pixelSizeMm() * 8, false) : {};
        const line = view?.sketch.entities.find((x) => x.id === h.entity && x.geometry.type === "line");
        if (line) {
          ui.setPick({ kind: "none" });
          return mode.done({ type: "sketch_line", sketch: mode.sketch, line: line.id });
        }
        const hit = viewer.pick(e.clientX, e.clientY, { edges: true });
        if (hit?.kind !== "edge") return;
        const edge = await store.edgeRef(hit.edge);
        ui.setPick({ kind: "none" });
        mode.done({ type: "edge", edge });
      } else if (mode.kind === "scan") {
        const hit = viewer.pick(e.clientX, e.clientY, { scan: true });
        if (hit?.kind !== "scan") return;
        ui.setMessage("Buscando la zona...");
        const result = await store.scanPick(mode.shape, hit.triangle);
        ui.setScanHighlight(result.faces);
        ui.setMessage(undefined);
        mode.done(result, hit.triangle);
      } else if (mode.kind === "asm_face") {
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true });
        if (hit?.kind !== "face") return;
        ui.setPick({ kind: "none" });
        mode.done(hit.face);
      } else if (mode.kind === "place") {
        // Dónde va el sketch: un plano base o una cara plana
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true, planes: true, refPlanes: true });
        if (hit?.kind === "plane") {
          ui.setPick({ kind: "none" });
          mode.done({ type: hit.plane });
        } else if (hit?.kind === "refplane") {
          ui.setPick({ kind: "none" });
          mode.done({ type: "reference", feature: hit.feature });
        } else if (hit?.kind === "face") {
          const info = await invoke<{ surface: string }>("cad_face_info", { face: hit.face });
          if (info.surface !== "plane") return ui.setMessage("Esa cara no es plana: elegir una cara plana o un plano base");
          const face = await store.faceRef(hit.face);
          ui.setPick({ kind: "none" });
          mode.done({ type: "face", face });
        }
      } else {
        // Sin herramienta: elegir caras, aristas, regiones de sketches y planos
        // (Mayús o Ctrl suma a la selección), como en Onshape
        const hit = viewer.pick(e.clientX, e.clientY, FILTER_WANT[ui.pickFilter()]);
        const additive = e.shiftKey || e.ctrlKey || e.metaKey;
        if (!hit || hit.kind === "scan") {
          if (!additive) ui.clearPicks();
          return ui.setMessage(undefined);
        }
        if (hit.kind === "face") ui.pickToggle({ kind: "face", face: hit.face }, additive);
        else if (hit.kind === "edge") ui.pickToggle({ kind: "edge", edge: hit.edge }, additive);
        else if (hit.kind === "vertex") ui.pickToggle({ kind: "vertex", at: hit.at }, additive);
        else if (hit.kind === "region") ui.pickToggle({ kind: "region", sketch: hit.sketch, region: hit.region }, additive);
        else if (hit.kind === "refplane") ui.pickToggle({ kind: "refplane", feature: hit.feature }, additive);
        else ui.pickToggle({ kind: "plane", plane: hit.plane }, additive);
        ui.setMessage(undefined);
      }
    } catch (err) {
      ui.setMessage(String(err));
    }
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 2) rightDown = { x: e.clientX, y: e.clientY };
    if (e.button !== 0 || e.altKey) return;
    if (viewer?.cubeDown(e.clientX, e.clientY)) return;
    if (ui.session()) return sketchClick(e);
    const handle = handleOf();
    if (handle && viewer?.handleHit(e.clientX, e.clientY)) {
      const f = store.doc()?.features.find((x) => x.id === handle.id);
      const field = f && handleField(f.kind, handle.h);
      // Con la distancia dada por una fórmula, la flecha solo se da vuelta
      const canDrag = !!field && !store.bindingOf(`${handle.id}.${field}`);
      handleDrag = { id: handle.id, h: handle.h, x: e.clientX, y: e.clientY, moved: false, canDrag };
      container.setPointerCapture(e.pointerId);
      return;
    }
    // Arrastrar desde cualquier lado elige por caja (como la herramienta de selección de
    // Blender); si no se arrastra queda el clic
    if (ui.pick().kind === "none") boxStart = { x: e.clientX, y: e.clientY, additive: e.shiftKey || e.ctrlKey || e.metaKey, before: ui.picks() };
    void pickClick(e);
  };

  const onPointerMove = (e: PointerEvent) => {
    if (e.buttons === 0) setOverCube(viewer?.cubeHover(e.clientX, e.clientY) ?? false);
    if (handleDrag) {
      const d = handleDrag;
      if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) return;
      d.moved = true;
      const raw = d.canDrag ? viewer?.handleParam(e.clientX, e.clientY) : null;
      if (raw == null || !viewer) return;
      // Redondeado a lo que se distingue en pantalla (1, 0,1, 0,01 mm...)
      const step = 10 ** Math.ceil(Math.log10(viewer.pixelSizeMm() * 2));
      const t = Math.round(raw / step) * step || step;
      if (dragAt()?.t === t) return;
      setDragAt({ h: d.h, t });
      void store.updateFeature(d.id, (f) => dragByHandle(f.kind, d.h, t));
      return;
    }
    if (e.buttons === 0) setHandleHot(!!handleOf() && !!viewer?.handleHit(e.clientX, e.clientY));
    // La caja aparece recién al arrastrar de verdad (un clic tiembla un par de píxeles)
    if (boxStart && (e.buttons & 1) === 1 && Math.hypot(e.clientX - boxStart.x, e.clientY - boxStart.y) >= 5) {
      const r = container.getBoundingClientRect();
      setBox({ x0: boxStart.x - r.left, y0: boxStart.y - r.top, x1: e.clientX - r.left, y1: e.clientY - r.top });
    }
    const s = ui.session();
    if (!s) return;
    const hit = snapped(e);
    setCursor(hit?.p);
    const drawing = !["select", "trim", "extend"].includes(ui.tool());
    if (hit && drawing && hit.kind !== "free") {
      const rect = container.getBoundingClientRect();
      setSnapView({
        kind: hit.kind,
        p: hit.p,
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
        guides: hit.guides ?? [],
        // Lo que origina el anclaje se resalta (la línea paralela, el arco tangente...)
        refs: [hit.direction?.entity, hit.align?.h, hit.align?.v].filter((x): x is number => x !== undefined),
      });
    } else setSnapView(undefined);
    if (dragging !== undefined && hit && (e.buttons & 1) === 1) {
      void ui.drag(dragging, viewer!.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p);
    }
  };

  const [overCube, setOverCube] = createSignal(false);
  // Selección por caja: desde dónde y el rectángulo que se ve mientras se arrastra
  let boxStart: { x: number; y: number; additive: boolean; before: Pick3d[] } | undefined;
  const [box, setBox] = createSignal<{ x0: number; y0: number; x1: number; y1: number }>();
  const finishBox = (x: number, y: number) => {
    const s = boxStart;
    boxStart = undefined;
    setBox(undefined);
    if (!s || !viewer || Math.hypot(x - s.x, y - s.y) < 5) return;
    if (ui.session()) return sketchBox(s.x, s.y, x, y, s.additive);
    const f = ui.pickFilter();
    const want = BOX_WANT[f];
    const got = viewer.boxSelect(s.x, s.y, x, y, want, x >= s.x);
    const found: Pick3d[] = [
      ...got.faces.map((face): Pick3d => ({ kind: "face", face })),
      ...got.edges.map((edge): Pick3d => ({ kind: "edge", edge })),
      ...got.vertices.map((at): Pick3d => ({ kind: "vertex", at })),
      ...got.regions.map((r): Pick3d => ({ kind: "region", ...r })),
    ];
    const key = (p: Pick3d) => JSON.stringify(p);
    // Lo que eligió el clic al empezar a arrastrar no cuenta
    ui.setPicks(() => {
      const base = s.additive ? s.before : [];
      const seen = new Set(base.map(key));
      return [...base, ...found.filter((p) => !seen.has(key(p)))];
    });
  };
  // Selección del sketch antes de empezar la caja (con Mayús se suma a ella)
  let sketchBoxBefore: number[] = [];
  /**
   * Caja dentro del sketch: de izquierda a derecha, lo que queda entero
   * adentro; de derecha a izquierda, lo que toca (como en el sólido)
   */
  const sketchBox = (x0: number, y0: number, x1: number, y1: number, additive: boolean) => {
    const s = ui.session();
    if (!s || !viewer) return;
    const [l, r, t, b] = [Math.min(x0, x1), Math.max(x0, x1), Math.min(y0, y1), Math.max(y0, y1)];
    const window = x1 >= x0;
    const scr = (p: P2) => viewer!.screenOf(planeToWorld(s.plane, p));
    const inside = ([x, y]: [number, number]) => x >= l && x <= r && y >= t && y <= b;
    // Un tramo que cruza la caja sin tener puntos adentro
    const crosses = (a: [number, number], c: [number, number]) => {
      const edges: [[number, number], [number, number]][] = [[[l, t], [r, t]], [[r, t], [r, b]], [[r, b], [l, b]], [[l, b], [l, t]]];
      const side = (p: [number, number], q: [number, number], o: [number, number]) => Math.sign((q[0] - p[0]) * (o[1] - p[1]) - (q[1] - p[1]) * (o[0] - p[0]));
      return edges.some(([p, q]) => side(a, c, p) !== side(a, c, q) && side(p, q, a) !== side(p, q, c));
    };
    const point = new Map(s.sketch.points.map((q) => [q.id, [q.x, q.y] as P2]));
    const found: number[] = [];
    for (const e of s.sketch.entities) {
      const poly = e.geometry.type === "point" ? [point.get(e.geometry.point)].filter((p): p is P2 => !!p) : entityPolyline(e.geometry, point);
      if (!poly?.length) continue;
      const sp = poly.map(scr);
      const hit = window ? sp.every(inside) : sp.some(inside) || sp.some((p, i) => i > 0 && crosses(sp[i - 1], p));
      if (hit) found.push(e.id);
    }
    for (const q of s.sketch.points) if (q.id !== s.sketch.origin && inside(scr([q.x, q.y]))) found.push(q.id);
    ui.setSelection(additive ? [...new Set([...sketchBoxBefore, ...found])] : found);
  };
  // Clic derecho sin arrastrar (el derecho también desplaza la vista): menú de la cara
  let rightDown: { x: number; y: number } | undefined;
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuEntry[] }>();
  const onPointerUp = (e: PointerEvent) => {
    dragging = undefined;
    if (handleDrag) {
      const d = handleDrag;
      handleDrag = undefined;
      setDragAt(undefined);
      if (!d.moved) void store.updateFeature(d.id, (f) => flipByHandle(f.kind, d.h));
      return;
    }
    if (e.button === 0 && boxStart) finishBox(e.clientX, e.clientY);
    const down = rightDown;
    rightDown = undefined;
    if (e.button !== 2 || !down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) return;
    if (ui.session() || ui.pick().kind !== "none" || !viewer) return;
    void openMenu(e.clientX, e.clientY);
  };

  /** F: acercar a lo elegido (o a todo si no hay nada) */
  const frameSelection = () => {
    const picks = ui.picks();
    viewer?.frameSelection(
      picks.flatMap((p) => (p.kind === "face" ? [p.face] : [])),
      picks.flatMap((p) => (p.kind === "edge" ? [p.edge] : [])),
      picks.flatMap((p) => (p.kind === "vertex" ? [p.at] : [])),
    );
  };

  /** Mirar de frente una cara plana */
  const normalToFace = async (face: number) => {
    const info = await invoke<{ surface: string; normal: P3 }>("cad_face_info", { face });
    if (info.surface !== "plane") return ui.setMessage("Esa cara no es plana");
    viewer?.lookFrom(info.normal);
  };

  /** Operaciones de las que salieron estas etiquetas (y el sketch de extrusiones y revoluciones), lo más reciente primero */
  const editEntries = (tags: { feature: number }[]): MenuEntry[] => {
    const features = store.doc()?.features ?? [];
    const byId = (id: number) => features.find((f) => f.id === id);
    const ids: number[] = [];
    for (const t of tags) {
      const f = byId(t.feature);
      if (!f || ids.includes(f.id)) continue;
      ids.push(f.id);
      if ((f.kind.type === "extrude" || f.kind.type === "revolve") && byId(f.kind.sketch) && !ids.includes(f.kind.sketch)) ids.push(f.kind.sketch);
    }
    ids.sort((a, b) => features.findIndex((f) => f.id === b) - features.findIndex((f) => f.id === a));
    return ids.map((id): MenuEntry => {
      const f = byId(id)!;
      const sketch = f.kind.type === "sketch";
      return {
        label: `${sketch ? "Editar sketch" : "Editar"} «${f.name}»`,
        onSelect: () => (sketch ? void store.settled().then(() => ui.editSketch(id)) : store.select(id)),
      };
    });
  };

  /**
   * Menú del clic derecho según lo que hay bajo el puntero (como Onshape): con
   * una cara o arista, editar lo que la creó y operaciones con ella (o con todo
   * lo elegido del mismo tipo, si ya estaba elegida); en vacío, las vistas.
   */
  const openMenu = async (x: number, y: number) => {
    const hit = viewer!.pick(x, y, { faces: true, edges: true });
    const act = ui.actions();
    const run = (name: keyof ReturnType<typeof ui.actions>) => () => act[name]?.();
    const sep: MenuEntry = { separator: true };
    if (hit?.kind === "face") {
      // La cara se suma a lo elegido si ya había caras elegidas y esta es una de ellas
      const picked = ui.picks().filter((p) => p.kind === "face");
      const inSel = picked.some((p) => p.kind === "face" && p.face === hit.face);
      const [ref, info] = await Promise.all([
        store.faceRef(hit.face).catch(() => undefined),
        invoke<{ surface: string }>("cad_face_info", { face: hit.face }).catch(() => undefined),
      ]);
      const use = (fn: () => void) => () => {
        if (!inSel) ui.setPicks([{ kind: "face", face: hit.face }]);
        fn();
      };
      const n = inSel ? picked.length : 1;
      const plane = info?.surface === "plane";
      const edit = editEntries(ref?.tags ?? []);
      setMenu({
        x,
        y,
        items: [
          { header: n > 1 ? `${n} caras` : "Cara" },
          ...edit,
          ...(edit.length ? [sep] : []),
          ...(plane && n === 1 ? [{ label: "Sketch en la cara", shortcut: "Mayús+S", onSelect: use(run("sketch")) }] : []),
          { label: n > 1 ? "Vaciar con estas caras abiertas" : "Vaciar con la cara abierta", onSelect: use(run("shell")) },
          { label: "Desmoldar", onSelect: use(run("draft")) },
          sep,
          ...(plane ? [{ label: "Mirar de frente", onSelect: () => void normalToFace(hit.face) }] : []),
          { label: "Acercar", shortcut: "F", onSelect: () => viewer?.frameSelection(inSel ? picked.map((p) => (p as { face: number }).face) : [hit.face], []) },
        ],
      });
    } else if (hit?.kind === "edge") {
      const picked = ui.picks().filter((p) => p.kind === "edge");
      const inSel = picked.some((p) => p.kind === "edge" && p.edge === hit.edge);
      const ref = await store.edgeRef(hit.edge).catch(() => undefined);
      const use = (fn: () => void) => () => {
        if (!inSel) ui.setPicks([{ kind: "edge", edge: hit.edge }]);
        fn();
      };
      const n = inSel ? picked.length : 1;
      const edit = editEntries((ref?.sides ?? []).flat());
      setMenu({
        x,
        y,
        items: [
          { header: n > 1 ? `${n} aristas` : "Arista" },
          { label: "Redondear", onSelect: use(run("fillet")) },
          { label: "Chaflán", onSelect: use(run("chamfer")) },
          ...(edit.length ? [sep, ...edit] : []),
          sep,
          { label: "Acercar", shortcut: "F", onSelect: () => viewer?.frameSelection([], inSel ? picked.map((p) => (p as { edge: number }).edge) : [hit.edge]) },
        ],
      });
    } else {
      const view = (label: string, dir: P3, shortcut: string): MenuEntry => ({ label, shortcut, onSelect: () => viewer?.lookFrom(dir) });
      setMenu({
        x,
        y,
        items: [
          { header: "Vista" },
          { label: "Encuadrar todo", shortcut: "Inicio", onSelect: () => viewer?.frameAll() },
          sep,
          view("Frente", [0, -1, 0], "1"),
          view("Derecha", [1, 0, 0], "3"),
          view("Arriba", [0, 0, 1], "7"),
          view("Isométrica", [1, -1, 1], "Mayús+7"),
        ],
      });
    }
  };

  const onKey = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
    // Vistas estándar, también dibujando: Mayús + 1…7 como Onshape, y 1/3/7
    // (con Ctrl la opuesta) como en el resto de la app
    const view = viewForKey(e);
    if (view) {
      viewer?.lookFrom(view);
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (!ui.session() && e.key === "Home" && !e.ctrlKey && !e.shiftKey && !e.altKey) {
      viewer?.frameAll();
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (!ui.session() && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey && e.code === "KeyS" && ui.pick().kind === "none") {
      // Sketch en la cara o el plano elegido (sin nada, se elige dónde)
      ui.actions().sketch?.();
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (!ui.session() && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && ui.pick().kind === "none" && (e.key.toLowerCase() === "f" || e.key === ".")) {
      // Acercar a lo elegido (sin nada, a todo)
      frameSelection();
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (ui.session()) {
      const key = e.key.toLowerCase();
      if (e.key === "Escape") {
        // La spline se termina con Esc (abierta)
        if (ui.tool() === "spline" && anchor().length >= 2) finishSpline(false);
        else if (chain() || anchor().length) resetTool();
        else ui.setTool("select");
      } else if (e.key === "Enter") void ui.finishSketch();
      else if (e.key === "Delete" || e.key === "Backspace") ui.deleteSelection();
      else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        // Q: construcción sí/no en lo elegido (como Onshape)
        if (key === "q" && ui.selection().length) {
          ui.toggleConstruction();
          e.preventDefault();
          e.stopPropagation();
          return;
        }
        const t = TOOLS.find((x) => x.key?.toLowerCase() === key);
        if (!t) return;
        ui.setTool(t.id);
        resetTool();
      } else return;
      e.preventDefault();
      e.stopPropagation();
    } else if (e.key === "Escape" && ui.pick().kind !== "none") {
      ui.cancelPick();
      e.stopPropagation();
    } else if (e.key === "Escape" && ui.picks().length) {
      ui.clearPicks();
      e.stopPropagation();
    } else if (store.draft() && (e.key === "Escape" || e.key === "Enter") && !target.closest("a, [aria-haspopup], [role=listbox], [role=option], [role=combobox], [role=dialog]")) {
      // Diálogo de la operación abierta: Enter acepta, Esc cancela (salvo en
      // una lista desplegable o un menú, que usan esas teclas). En un botón
      // también: el foco suele quedar en el que abrió la operación
      void (e.key === "Enter" ? store.acceptDraft() : store.cancelDraft());
      e.preventDefault();
      e.stopPropagation();
    }
  };
  onMount(() => window.addEventListener("keydown", onKey, true));
  onCleanup(() => window.removeEventListener("keydown", onKey, true));

  /** Cotas del sketch en edición: dónde dibujar cada valor */
  const dimensions = createMemo(() => {
    viewTick();
    const s = ui.session();
    if (!s || !viewer) return [];
    const pt = new Map(s.sketch.points.map((p) => [p.id, [p.x, p.y] as P2]));
    const ent = new Map(s.sketch.entities.map((e) => [e.id, e.geometry]));
    const mid = (a?: P2, b?: P2): P2 | undefined => (a && b ? [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] : undefined);
    const lineMid = (id: number) => {
      const g = ent.get(id);
      return g?.type === "line" ? mid(pt.get(g.start), pt.get(g.end)) : undefined;
    };
    const anchor = (c: SketchConstraint): P2 | undefined => {
      switch (c.type) {
        case "length":
          return lineMid(c.line);
        case "angle":
          return lineMid(c.a);
        case "distance":
        case "horizontal_distance":
        case "vertical_distance":
          return mid(pt.get(c.a), pt.get(c.b));
        case "radius":
        case "diameter": {
          const g = ent.get(c.entity);
          if (!g || (g.type !== "circle" && g.type !== "arc")) return undefined;
          const center = pt.get(g.center);
          if (!center) return undefined;
          const r = g.type === "circle" ? g.radius : dist(center, pt.get(g.start) ?? center);
          return [center[0] + r * Math.SQRT1_2, center[1] + r * Math.SQRT1_2];
        }
        default:
          return undefined;
      }
    };
    const rect = container.getBoundingClientRect();
    const out: { index: number; x: number; y: number; text: string; conflict: boolean; reference: boolean }[] = [];
    // Centro del sketch en pantalla: las etiquetas se corren hacia afuera de él
    const pts = s.sketch.points;
    const centroid: P2 = pts.length ? [pts.reduce((a, p) => a + p.x, 0) / pts.length, pts.reduce((a, p) => a + p.y, 0) / pts.length] : [0, 0];
    const [mx, my] = viewer.screenOf(planeToWorld(s.plane, centroid));
    /** Extremos de la línea o los puntos que acota (para correr la etiqueta a un costado) */
    const ends = (c: SketchConstraint): [P2, P2] | undefined => {
      if (c.type === "length" || c.type === "angle") {
        const g = ent.get(c.type === "length" ? c.line : c.a);
        const a = g?.type === "line" ? pt.get(g.start) : undefined;
        const b = g?.type === "line" ? pt.get(g.end) : undefined;
        return a && b ? [a, b] : undefined;
      }
      if (c.type === "distance" || c.type === "horizontal_distance" || c.type === "vertical_distance") {
        const [a, b] = [pt.get(c.a), pt.get(c.b)];
        return a && b ? [a, b] : undefined;
      }
      return undefined;
    };
    s.sketch.constraints.forEach((c, index) => {
      const v = constraintValue(c);
      const a = anchor(c);
      if (v === undefined || !a) return;
      let [x, y] = viewer!.screenOf(planeToWorld(s.plane, a));
      // Correr 16 px perpendicular a lo acotado, hacia afuera del sketch, para
      // no tapar la línea (y poder elegirla con un clic)
      const e2 = ends(c);
      if (e2) {
        const [p0, p1] = e2.map((p) => viewer!.screenOf(planeToWorld(s.plane, p)));
        let nx = -(p1[1] - p0[1]);
        let ny = p1[0] - p0[0];
        const l = Math.hypot(nx, ny) || 1;
        nx /= l;
        ny /= l;
        if (nx * (x - mx) + ny * (y - my) < 0) {
          nx = -nx;
          ny = -ny;
        }
        x += nx * 22;
        y += ny * 22;
      } else {
        const dx = x - mx;
        const dy = y - my;
        const l = Math.hypot(dx, dy) || 1;
        x += (dx / l) * 14;
        y += (dy / l) * 14;
      }
      const prefix = c.type === "radius" ? "R " : c.type === "diameter" ? "Ø " : "";
      const suffix = c.type === "angle" ? "°" : "";
      const expr = (c as { expr?: string }).expr;
      const reference = isReference(c);
      const shown = expr && !reference ? `${prefix}${expr} = ${+v.toFixed(3)}${suffix}` : `${prefix}${+v.toFixed(3)}${suffix}`;
      out.push({
        index,
        x: x - rect.left,
        y: y - rect.top,
        // Las de referencia entre paréntesis, como Onshape
        text: reference ? `(${shown})` : shown,
        conflict: s.report?.conflicting.includes(index) ?? false,
        reference,
      });
    });
    return out;
  });

  /** Operación con el diálogo abierto (o el sketch elegido) */
  const editedFeature = () => {
    const id = store.selected();
    return store.doc()?.features.some((f) => f.id === id) ? id : undefined;
  };
  // El diálogo se arrastra desde su borde de arriba
  const [dialogPos, setDialogPos] = createSignal<[number, number]>([0, 0]);
  const startDialogDrag = (e: PointerEvent) => {
    e.preventDefault();
    const [x0, y0] = dialogPos();
    const sx = e.clientX;
    const sy = e.clientY;
    const move = (ev: PointerEvent) => setDialogPos([x0 + ev.clientX - sx, Math.max(0, y0 + ev.clientY - sy)]);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const promptText = () => {
    const m = ui.pick();
    return m.kind === "none" ? undefined : m.prompt;
  };

  const statusText = () => {
    const r = ui.session()?.report;
    if (!r) return "";
    const names = {
      well_constrained: "Totalmente definido",
      under_constrained: `Faltan ${r.dof} restricciones`,
      over_constrained: "Restricciones en conflicto",
      failed: "No se pudo resolver",
    };
    return names[r.status];
  };

  return (
    <div class="absolute inset-0 z-[5] bg-viewport">
      <div
        ref={container}
        class={clsx(
          "absolute inset-0",
          overCube() || handleHot() ? "cursor-pointer" : (ui.session() && ui.tool() !== "select") || ui.pick().kind !== "none" ? "cursor-crosshair" : "cursor-default",
        )}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => setSnapView(undefined)}
        onContextMenu={(e) => e.preventDefault()}
      />

      <Show when={menu()}>{(m) => <ContextMenu x={m().x} y={m().y} items={m().items} onClose={() => setMenu(undefined)} />}</Show>

      {/* A qué se pega el cursor (anclaje) */}
      <Show when={snapView()}>
        {(v) => (
          <span
            data-snap={v().kind}
            class="absolute pointer-events-none px-1 rounded text-[11px] bg-bg-lighter/90 border border-border text-cyan"
            style={{ left: `${v().x + 14}px`, top: `${v().y + 14}px` }}
          >
            {SNAP_GLYPHS[v().kind].glyph} {SNAP_GLYPHS[v().kind].label}
          </span>
        )}
      </Show>

      {/* Cotas del sketch: clic para cambiar el valor */}
      <Index each={dimensions()}>
        {(d) => (
          <Show
            when={editingDim() === d().index}
            fallback={
              <button
                class={clsx(
                  "absolute -translate-x-1/2 -translate-y-1/2 px-1.5 py-0.5 rounded text-[11px] font-mono border",
                  d().conflict
                    ? "bg-error/20 border-error text-error"
                    : d().reference
                      ? "bg-bg-lighter/70 border-border/60 text-text-muted"
                      : "bg-bg-lighter/90 border-border text-text hover:border-accent",
                  // Dibujando, los clics son para el dibujo (las cotas se piden solas)
                  ui.tool() !== "select" && "pointer-events-none",
                )}
                style={{ left: `${d().x}px`, top: `${d().y}px` }}
                onPointerDown={(e) => e.stopPropagation()}
                // Una cota de referencia no se escribe: mide
                onClick={() => !d().reference && setEditingDim(d().index)}
              >
                {d().text}
              </button>
            }
          >
            <input
              ref={(el) =>
                setTimeout(() => {
                  el.focus();
                  el.select();
                })
              }
              type="text"
              // Solo al abrir: si el solver responde mientras se escribe, no pisa el texto
              value={untrack(() => {
                const c = ui.session()!.sketch.constraints[d().index]!;
                return (c as { expr?: string }).expr ?? constraintValue(c) ?? 0;
              })}
              class="absolute -translate-x-1/2 -translate-y-1/2 w-20 px-1.5 py-0.5 rounded text-[11px] font-mono bg-bg border border-accent text-text outline-none"
              style={{ left: `${d().x}px`, top: `${d().y}px` }}
              onPointerDown={(e) => e.stopPropagation()}
              onKeyDown={async (e) => {
                e.stopPropagation();
                if (e.key === "Escape") {
                  // Como en el visor: Esc corta las cotas pedidas y la herramienta en curso
                  setDimQueue([]);
                  setEditingDim(undefined);
                  resetTool();
                  return;
                }
                if (e.key !== "Enter" && e.key !== "Tab") return;
                e.preventDefault();
                // Enter/Tab: aplicar y pasar a la siguiente cota pedida
                const input = e.currentTarget;
                const text = input.value.trim();
                const before = String(input.defaultValue);
                if (text && text !== before) ui.setMessage(await ui.setConstraintText(d().index, text));
                input.dataset.done = "1";
                nextDim();
              }}
              onBlur={async (e) => {
                const input = e.currentTarget;
                if (input.dataset.done) return;
                // Clic en otro lado: lo escrito vale; si no se escribió, queda lo dibujado
                const text = input.value.trim();
                if (editingDim() === d().index && text && text !== String(input.defaultValue)) {
                  ui.setMessage(await ui.setConstraintText(d().index, text));
                }
                setDimQueue([]);
                if (editingDim() === d().index) setEditingDim(undefined);
              }}
            />
          </Show>
        )}
      </Index>

      {/* Barra superior: herramientas, vista y el diálogo de la operación abierta */}
      <div class="absolute top-2 left-2 right-36 flex flex-col items-start gap-1.5 pointer-events-none">
        <Show
          when={ui.session()}
          fallback={
            <>
            <DesignToolbar store={store} ui={ui} actions={props.actions} />
            <div class="flex items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto">
              <IconButton aria-label="Encuadrar todo" size="sm" onClick={() => viewer?.frameAll()}>
                <Icons.FrameCorners size={14} />
              </IconButton>
              <div class="w-px h-5 bg-border mx-0.5" />
              <div role="radiogroup" aria-label="Qué elegir" class="flex items-center gap-0.5">
                <For each={FILTERS}>
                  {(f) => (
                    <Tooltip content={f.tip}>
                      <button
                        role="radio"
                        aria-checked={ui.pickFilter() === f.id}
                        class={clsx(
                          "px-1.5 py-0.5 rounded text-[11px]",
                          ui.pickFilter() === f.id ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface",
                        )}
                        onClick={() => ui.setPickFilter(f.id)}
                      >
                        {f.label}
                      </button>
                    </Tooltip>
                  )}
                </For>
              </div>
              <div class="w-px h-5 bg-border mx-0.5" />
              <Tooltip content={ui.section() ? "Sacar la vista de corte" : "Vista de corte (no cambia el modelo)"}>
                <IconButton
                  aria-label="Vista de corte"
                  size="sm"
                  active={!!ui.section()}
                  onClick={() => ui.setSection(ui.section() ? null : { plane: "xz", at: 0.5, flip: false })}
                >
                  <Icons.Scissors size={14} />
                </IconButton>
              </Tooltip>
              <Show when={ui.section()}>
                {(s) => (
                  <>
                    <For each={["xy", "xz", "yz"] as const}>
                      {(p) => (
                        <button
                          aria-label={`Corte por ${p.toUpperCase()}`}
                          class={clsx("px-1.5 py-0.5 rounded text-[11px]", s().plane === p ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface")}
                          onClick={() => ui.setSection({ ...s(), plane: p })}
                        >
                          {p.toUpperCase()}
                        </button>
                      )}
                    </For>
                    <div class="w-24" aria-label="Posición del corte">
                      <Slider value={s().at} min={0} max={1} step={0.01} onChange={(v) => ui.setSection({ ...s(), at: v })} />
                    </div>
                    <Tooltip content="Ver el otro lado">
                      <IconButton aria-label="Invertir el corte" size="sm" onClick={() => ui.setSection({ ...s(), flip: !s().flip })}>
                        <Icons.ArrowsLeftRight size={14} />
                      </IconButton>
                    </Tooltip>
                  </>
                )}
              </Show>
              <div class="w-px h-5 bg-border mx-0.5" />
              <Tooltip content={ui.showPlanes() ? "Ocultar los planos base" : "Mostrar los planos base"}>
                <IconButton aria-label="Planos base" size="sm" active={ui.showPlanes()} onClick={() => ui.setShowPlanes(!ui.showPlanes())}>
                  <Icons.Square size={14} />
                </IconButton>
              </Tooltip>
              <Show when={props.scanMesh}>
                <Tooltip content={scanVisible() ? "Ocultar el modelo de referencia" : "Mostrar el modelo de referencia"}>
                  <IconButton aria-label="Modelo de referencia" size="sm" active={scanVisible()} onClick={() => setScanVisible(!scanVisible())}>
                    <Icons.Scan size={14} />
                  </IconButton>
                </Tooltip>
                <div class="w-24">
                  <Slider value={scanOpacity()} min={0.05} max={1} step={0.05} onChange={setScanOpacity} />
                </div>
              </Show>
            </div>
            {/* Diálogo de la operación elegida (por id: no se rehace con cada cambio del documento) */}
            <Show when={!ui.assemblyMode()}>
              <For each={editedFeature() !== undefined ? [editedFeature()!] : []}>
                {(id) => (
                  <div
                    data-feature-dialog
                    class="w-72 max-h-[60vh] flex flex-col rounded-md border border-border bg-bg-lighter/95 shadow-lg pointer-events-auto"
                    style={{ transform: `translate(${dialogPos()[0]}px, ${dialogPos()[1]}px)` }}
                  >
                    <div
                      class="h-3 shrink-0 cursor-move rounded-t-md flex items-center justify-center hover:bg-surface/60"
                      title="Arrastrar para mover"
                      onPointerDown={startDialogDrag}
                    >
                      <div class="w-8 h-0.5 rounded bg-border" />
                    </div>
                    <div class="overflow-y-auto px-2.5 pb-2.5">
                      <FeatureEditor featureId={id} store={store} ui={ui} sketches={props.actions.sketches()} tools={props.actions.toolFeatures()} />
                    </div>
                  </div>
                )}
              </For>
            </Show>
            </>
          }
        >
          {(s) => (
            <div class="flex flex-wrap items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto">
              <For each={TOOLS}>
                {(t, i) => (
                  <>
                    <Show when={i() > 0 && TOOLS[i() - 1].group !== t.group}>
                      <div class="w-px h-5 bg-border mx-0.5" />
                    </Show>
                    <Tooltip content={t.key ? `${t.label} (${t.key})` : t.label}>
                      <button
                        aria-label={t.short}
                        class={clsx(
                          "p-1 rounded",
                          ui.tool() === t.id ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface",
                        )}
                        onClick={() => {
                          ui.setTool(t.id);
                          resetTool();
                        }}
                      >
                        <t.icon size={18} />
                        {/* El nombre para lectores de pantalla (y las pruebas que buscan por texto) */}
                        <span class="sr-only">{t.short}</span>
                      </button>
                    </Tooltip>
                  </>
                )}
              </For>
              <Show when={ui.tool() === "text"}>
                <input
                  type="text"
                  value={textValue()}
                  aria-label="Texto"
                  class="w-32 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                  onInput={(e) => setTextValue(e.currentTarget.value)}
                />
                <label class="flex items-center gap-1 text-xs text-text-muted">
                  Tamaño
                  <input
                    type="number"
                    min="0.1"
                    step="1"
                    value={textSize()}
                    class="w-14 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => setTextSize(Math.max(0.1, parseFloat(e.currentTarget.value) || 10))}
                  />
                  mm
                </label>
                <label class="px-2 py-1 rounded text-xs text-text-muted hover:text-text hover:bg-surface cursor-pointer whitespace-nowrap" title="Fuente .ttf u .otf">
                  {fontName()}…
                  <input
                    type="file"
                    accept=".ttf,.otf"
                    class="hidden"
                    onChange={async (e) => {
                      const file = e.currentTarget.files?.[0];
                      if (!file) return;
                      try {
                        textFont = { name: file.name.replace(/\.[^.]+$/, ""), font: parseFont(await file.arrayBuffer()) };
                        setFontName(textFont.name);
                      } catch (err) {
                        ui.setMessage(`No se pudo leer la fuente: ${err}`);
                      }
                    }}
                  />
                </label>
              </Show>
              {/* Texto elegido: cambiar lo que dice o su tamaño */}
              <For each={selectedText() ? [selectedText()!.id] : []}>
                {(id) => {
                  const current = () => ui.session()?.sketch.texts?.find((t) => t.id === id);
                  const [text, setText] = createSignal(current()?.text ?? "");
                  const [size, setSize] = createSignal(current()?.size ?? 10);
                  return (
                    <>
                      <input
                        type="text"
                        value={text()}
                        aria-label="Texto elegido"
                        class="w-32 px-1.5 py-0.5 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                        onInput={(e) => setText(e.currentTarget.value)}
                        onKeyDown={(e) => e.key === "Enter" && void rewriteText(id, text(), size())}
                      />
                      <label class="flex items-center gap-1 text-xs text-text-muted">
                        Tamaño
                        <input
                          type="number"
                          min="0.1"
                          step="1"
                          value={size()}
                          aria-label="Tamaño del texto elegido"
                          class="w-14 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                          onChange={(e) => setSize(Math.max(0.1, parseFloat(e.currentTarget.value) || 10))}
                        />
                        mm
                      </label>
                      <Button size="sm" onClick={() => void rewriteText(id, text(), size())}>
                        Rehacer texto
                      </Button>
                    </>
                  );
                }}
              </For>
              <Show when={ui.tool() === "polygon"}>
                <label class="flex items-center gap-1 text-xs text-text-muted">
                  Lados
                  <input
                    type="number"
                    min="3"
                    max="64"
                    value={polygonSides()}
                    class="w-12 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => setPolygonSides(Math.max(3, Math.min(64, parseInt(e.currentTarget.value) || 6)))}
                  />
                </label>
              </Show>
              <div class="w-px h-5 bg-border mx-1" />
              <span
                class={clsx(
                  "text-xs px-1",
                  s().report?.status === "well_constrained" ? "text-success" : s().report?.status === "under_constrained" ? "text-text-muted" : "text-error",
                )}
              >
                {statusText()}
              </span>
              <div class="w-px h-5 bg-border mx-1" />
              <Button size="sm" variant="primary" onClick={() => void ui.finishSketch()}>
                Terminar sketch
              </Button>
              <Button size="sm" variant="ghost" onClick={() => ui.cancelSketch()}>
                Descartar
              </Button>
            </div>
          )}
        </Show>
      </div>

      {/* Cota que sobre-define el sketch: dejarla de referencia o quitarla */}
      <Show when={ui.extraDimension()}>
        {(i) => (
          <div class="absolute top-24 left-1/2 -translate-x-1/2 flex items-center gap-2 rounded-md border border-error/60 bg-bg-lighter/95 px-3 py-1.5 text-xs text-text">
            <Icons.Warning size={14} class="text-error" />
            Esta cota sobre-define el sketch
            <Button size="sm" variant="primary" onClick={() => ui.toggleReference(i())}>
              Dejarla de referencia
            </Button>
            <Button size="sm" variant="ghost" onClick={() => ui.removeConstraint(i())}>
              Quitarla
            </Button>
          </div>
        )}
      </Show>

      {/* Caja de selección: hacia la derecha, lo que queda adentro; hacia la izquierda, lo que toca */}
      <Show when={box()}>
        {(r) => (
          <div
            data-box-select
            class={clsx("absolute pointer-events-none border border-cyan bg-cyan/10", r().x1 < r().x0 && "border-dashed")}
            style={{
              left: `${Math.min(r().x0, r().x1)}px`,
              top: `${Math.min(r().y0, r().y1)}px`,
              width: `${Math.abs(r().x1 - r().x0)}px`,
              height: `${Math.abs(r().y1 - r().y0)}px`,
            }}
          />
        )}
      </Show>

      {/* Plano 2D encima del visor */}
      <Show when={ui.drawingOpen()}>
        <DrawingView store={store} ui={ui} />
      </Show>
      {/* Desarrollo de chapa encima del visor */}
      <Show when={ui.flatOpen()}>
        <FlatPatternView store={store} ui={ui} />
      </Show>

      {/* Medidas de lo elegido */}
      <Show when={measurement()}>{(m) => <MeasurePanel m={m()} />}</Show>

      {/* Mensajes y errores */}
      <div class="absolute bottom-2 left-2 right-2 flex flex-col gap-1 items-start pointer-events-none">
        {/* Versión abierta en solo lectura */}
        <Show when={store.viewing() !== undefined && store.committed()?.versions?.[store.viewing()!]}>
          {(v) => (
            <div data-viewing class="flex items-center gap-3 rounded-md border border-warning/60 bg-bg-lighter/95 px-3 py-1.5 text-xs text-text pointer-events-auto">
              <Icons.Eye size={14} class="text-warning" />
              Viendo «{v().name}» (solo lectura)
              <button class="text-accent hover:underline" onClick={() => void store.restoreVersion(store.viewing()!)}>
                Restaurar
              </button>
              <button class="text-text-muted hover:text-text" onClick={() => void store.viewVersion(undefined)}>
                Volver al diseño
              </button>
            </div>
          )}
        </Show>
        {/* Comparación con una versión */}
        <Show when={store.comparison()}>
          {(c) => (
            <div data-comparison class="flex items-center gap-3 rounded-md border border-border bg-bg-lighter/95 px-3 py-1.5 text-xs text-text pointer-events-auto">
              <span>Desde «{store.committed()?.versions?.[c().version]?.name}»:</span>
              <span class="text-green">+{formatVolume(c().added)}</span>
              <span class="text-red">−{formatVolume(c().removed)}</span>
              <button class="text-text-muted hover:text-text" onClick={() => void store.compareVersion(undefined)}>
                Quitar
              </button>
            </div>
          )}
        </Show>
        {/* Lo que se espera que el usuario elija (abajo: arriba tapaba la barra) */}
        <Show when={promptText()}>
          <div class="flex items-center gap-3 rounded-md border border-accent/60 bg-bg-lighter/95 px-3 py-1.5 text-xs text-text pointer-events-auto">
            <Icons.Cursor size={14} class="text-accent" />
            {promptText()}
            <button class="text-text-muted hover:text-text" onClick={() => ui.cancelPick()}>
              Cancelar (Esc)
            </button>
          </div>
        </Show>
        <Show when={store.busy()}>
          <span class="text-xs text-text-muted bg-bg-lighter/90 rounded px-2 py-0.5">Recalculando...</span>
        </Show>
        <Show when={ui.message()}>
          <span class="text-xs text-text bg-bg-lighter/90 rounded px-2 py-0.5">{ui.message()}</span>
        </Show>
        <Show when={store.error()}>
          <span class="text-xs text-error bg-bg-lighter/90 rounded px-2 py-0.5">{store.error()}</span>
        </Show>
        <Show when={props.actions.notice()}>
          <span class="text-xs text-warning bg-bg-lighter/90 rounded px-2 py-0.5">{props.actions.notice()}</span>
        </Show>
        <Show when={!ui.session() && ui.picks().length > 0}>
          <span class="flex items-center gap-2 text-xs text-text bg-bg-lighter/90 rounded px-2 py-0.5 pointer-events-auto">
            Elegido: {pickSummary(ui.picks())}
            <button class="text-text-muted hover:text-text" onClick={() => ui.clearPicks()}>
              Limpiar (Esc)
            </button>
          </span>
        </Show>
        {/* Los atajos del momento van en la barra de estado (lib/designHints.ts) */}
      </div>
    </div>
  );
};

/** Filtros de selección del visor (fuera de los sketches) */
const FILTERS: { id: PickFilter; label: string; tip: string }[] = [
  { id: "all", label: "Todo", tip: "Elegir caras, aristas, vértices, regiones y planos" },
  { id: "faces", label: "Caras", tip: "Elegir solo caras" },
  { id: "edges", label: "Aristas", tip: "Elegir solo aristas" },
  { id: "vertices", label: "Vértices", tip: "Elegir solo vértices" },
  { id: "sketches", label: "Sketches", tip: "Elegir solo regiones de sketches y planos" },
];
const FILTER_WANT: Record<PickFilter, Parameters<CadViewer["pick"]>[2]> = {
  all: { faces: true, edges: true, vertices: true, regions: true, planes: true, refPlanes: true },
  faces: { faces: true },
  edges: { edges: true },
  vertices: { vertices: true },
  sketches: { regions: true, planes: true, refPlanes: true },
};
/** Por caja, "Todo" elige caras, aristas y regiones (los vértices, con su filtro) */
const BOX_WANT: Record<PickFilter, Parameters<CadViewer["boxSelect"]>[4]> = {
  all: { faces: true, edges: true, regions: true },
  faces: { faces: true },
  edges: { edges: true },
  vertices: { vertices: true },
  sketches: { regions: true },
};

/** Vistas estándar de Onshape: Mayús + número, dirección desde donde se mira (Z arriba) */
const STANDARD_VIEWS: Record<string, P3> = {
  Digit1: [0, -1, 0], // frente
  Digit2: [0, 1, 0], // atrás
  Digit3: [-1, 0, 0], // izquierda
  Digit4: [1, 0, 0], // derecha
  Digit5: [0, 0, 1], // arriba
  Digit6: [0, 0, -1], // abajo
  Digit7: [1, -1, 1], // isométrica
};
/** Las del resto de la app (teclado numérico de Blender): 1 frente, 3 derecha, 7 arriba; Ctrl, la opuesta */
const BLENDER_VIEWS: Record<string, P3> = { "1": [0, -1, 0], "3": [1, 0, 0], "7": [0, 0, 1] };

function viewForKey(e: KeyboardEvent): P3 | undefined {
  if (e.metaKey || e.altKey) return undefined;
  if (e.shiftKey && !e.ctrlKey) return STANDARD_VIEWS[e.code];
  if (e.shiftKey) return undefined;
  const d = BLENDER_VIEWS[e.key];
  return d && (e.ctrlKey ? (d.map((c) => -c) as P3) : d);
}

// ─── Medidas ──────────────────────────────────────────────────────────────

const KIND_NAMES: Record<string, string> = {
  plane: "Cara plana",
  cylinder: "Cara cilíndrica",
  cone: "Cara cónica",
  sphere: "Cara esférica",
  torus: "Cara tórica",
  line: "Arista recta",
  circle: "Arista circular",
  ellipse: "Arista elíptica",
  vertex: "Vértice",
};
const mm = (v: number) => `${v.toFixed(3).replace(/\.?0+$/, "")} mm`;

/** Panel de medidas abajo a la derecha del visor (Z arriba, como lo ve el usuario) */
const MeasurePanel: Component<{ m: Measurement }> = (props) => {
  const rows = (): [string, string][] => {
    const m = props.m;
    const out: [string, string][] = [];
    if (m.items.length === 1) {
      const it = m.items[0];
      out.push(["", KIND_NAMES[it.kind] ?? (it.kind.startsWith("b") ? "Superficie libre" : it.kind)]);
      if (it.area != null) out.push(["Área", `${it.area.toFixed(3).replace(/\.?0+$/, "")} mm²`]);
      if (it.length != null) out.push(["Largo", mm(it.length)]);
      if (it.radius != null) out.push(["Radio", mm(it.radius)], ["Diámetro", mm(2 * it.radius)]);
      if (it.kind === "vertex") out.push(["X", mm(it.center[0])], ["Y", mm(it.center[1])], ["Z", mm(it.center[2])]);
      return out;
    }
    if (m.distance) {
      out.push(["Distancia mínima", mm(m.distance.value)]);
      const [dx, dy, dz] = m.distance.delta.map(Math.abs);
      out.push(["ΔX", mm(dx)], ["ΔY", mm(dy)], ["ΔZ", mm(dz)]);
    }
    if (m.center_distance != null) out.push(["Entre centros", mm(m.center_distance)]);
    if (m.angle != null) out.push(["Ángulo", `${m.angle.toFixed(2).replace(/\.?0+$/, "")}°`]);
    return out;
  };
  return (
    <div
      aria-label="Medidas"
      class="absolute bottom-2 right-2 min-w-44 rounded-md border border-border bg-bg-lighter/95 px-2.5 py-1.5 text-xs shadow"
    >
      <For each={rows()}>
        {([k, v]) => (
          <div class="flex justify-between gap-4">
            <span class="text-text-muted">{k}</span>
            <span class={clsx(k ? "font-mono text-text" : "text-text")} data-measure={k}>
              {v}
            </span>
          </div>
        )}
      </For>
    </div>
  );
};
