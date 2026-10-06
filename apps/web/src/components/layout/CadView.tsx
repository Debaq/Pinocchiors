import { Component, For, type JSX, Index, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { CadViewer, planeToWorld } from "../../lib/CadViewer";
import { parse as parseFont, type Font } from "opentype.js";
import { outlineContours } from "../../lib/sketchText";
import { addPoint, addTextContours, constraintIds, ellipsePolyline, splineOf, splinePolyline, constraintValue, isReference, extendLine, isSolidPoint, leavingDirection, placeSnap, tangentArc, trimAt, type CadStore, type P2, type P3, type Sketch, type SketchConstraint } from "../../lib/cad";
import { infer, solidRefs, SNAP_GLYPHS, type Snap, type SnapKind } from "../../lib/sketchSnap";
import type { CadUi, SketchTool } from "../../lib/cadUi";
import type { MeshData } from "../../lib/Viewer3D";
import { Button, IconButton, Slider, Tooltip } from "../ui";
import * as Icons from "../icons";
import * as SketchIcons from "../icons/sketch";

export interface CadViewProps {
  store: CadStore;
  ui: CadUi;
  /** Modelo cargado, de referencia (escaneo a calcar) */
  scanMesh?: MeshData | null;
}

// Grupos de la barra: elegir · dibujar · modificar
const TOOLS: { id: SketchTool; short: string; label: string; key?: string; icon: (p: { size?: number }) => JSX.Element; group: number }[] = [
  { id: "select", short: "Elegir", label: "Elegir y arrastrar", key: "S", icon: SketchIcons.Select, group: 0 },
  { id: "line", short: "Línea", label: "Línea", key: "L", icon: SketchIcons.Line, group: 1 },
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

  onMount(async () => {
    viewer = new CadViewer(container);
    viewer.onRender = () => setViewTick((t) => t + 1);
    // Para las pruebas en navegador (scratch de desarrollo)
    if (import.meta.env.DEV) (window as unknown as { __cadViewer?: CadViewer }).__cadViewer = viewer;
    try {
      viewer.mmPerUnit = await invoke<number>("cad_mm_per_unit");
    } catch {
      viewer.mmPerUnit = 1;
    }
    viewer.setScan(props.scanMesh);
    viewer.setBody(store.mesh());
    viewer.frameAll();
    setPlanesReady((n) => n + 1);
  });
  onCleanup(() => viewer?.dispose());

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
  // Encuadrar cuando aparece el primer sólido (no en cada recálculo)
  let hadBody = false;
  createEffect(() => {
    const m = store.mesh();
    viewer?.setBody(m);
    if (m && !hadBody && !ui.session()) viewer?.frameAll();
    hadBody = !!m;
  });
  createEffect(() => {
    const h = ui.highlight();
    const picks = ui.picks();
    const faces = [...h.faces, ...picks.flatMap((p) => (p.kind === "face" ? [p.face] : []))];
    const edges = [...h.edges, ...picks.flatMap((p) => (p.kind === "edge" ? [p.edge] : []))];
    viewer?.setHighlight(faces, edges);
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
  // Planos base: visibles fuera de la edición (y al elegir dónde va un sketch)
  const [planesReady, setPlanesReady] = createSignal(0);
  createEffect(() => {
    planesReady();
    const show = ui.showPlanes() && !ui.session();
    const selected = ui.picks().flatMap((p) => (p.kind === "plane" ? [p.plane] : []));
    viewer?.setPlanes(show || ui.pick().kind === "place", selected);
  });
  createEffect(() => viewer?.setScanHighlight(ui.scanHighlight()));
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
          viewer.lookAtPlane(s.plane);
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
      if (mode.kind === "region") {
        const view = store.sketchView(mode.sketch);
        viewer.setSketch(view ? { plane: view.plane, sketch: view.sketch, regions: view.regions, chosen: mode.chosen() } : null);
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
    const from = ui.tool() === "line" ? chain()?.last : undefined;
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
      if (!e.shiftKey) ui.setSelection([]);
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
          ui.change((sk) => addTextContours(sk, contours));
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
        ui.setHighlight({ faces: [hit.face], edges: [] });
        ui.setPick({ kind: "none" });
        mode.done(ref, hit.face);
      } else if (mode.kind === "edges") {
        const hit = viewer.pick(e.clientX, e.clientY, { edges: true });
        if (hit?.kind !== "edge") return;
        const ref = await store.edgeRef(hit.edge);
        const h = ui.highlight();
        const edges = h.edges.includes(hit.edge) ? h.edges.filter((x) => x !== hit.edge) : [...h.edges, hit.edge];
        ui.setHighlight({ faces: [], edges });
        mode.toggle(ref, hit.edge);
      } else if (mode.kind === "region") {
        const view = store.sketchView(mode.sketch);
        const p = view && viewer.planePoint(e.clientX, e.clientY, view.plane);
        if (p) mode.toggle(p);
      } else if (mode.kind === "scan") {
        const hit = viewer.pick(e.clientX, e.clientY, { scan: true });
        if (hit?.kind !== "scan") return;
        ui.setMessage("Buscando la zona...");
        const result = await store.scanPick(mode.shape, hit.triangle);
        ui.setScanHighlight(result.faces);
        ui.setMessage(undefined);
        mode.done(result, hit.triangle);
      } else if (mode.kind === "place") {
        // Dónde va el sketch: un plano base o una cara plana
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true, planes: true });
        if (hit?.kind === "plane") {
          ui.setPick({ kind: "none" });
          mode.done({ type: hit.plane });
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
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true, edges: true, regions: true, planes: true });
        const additive = e.shiftKey || e.ctrlKey || e.metaKey;
        if (!hit || hit.kind === "scan") {
          if (!additive) ui.clearPicks();
          return ui.setMessage(undefined);
        }
        if (hit.kind === "face") ui.pickToggle({ kind: "face", face: hit.face }, additive);
        else if (hit.kind === "edge") ui.pickToggle({ kind: "edge", edge: hit.edge }, additive);
        else if (hit.kind === "region") ui.pickToggle({ kind: "region", sketch: hit.sketch, region: hit.region }, additive);
        else ui.pickToggle({ kind: "plane", plane: hit.plane }, additive);
        if (hit.kind === "face" && ui.picks().length === 1) {
          const info = await invoke<{ surface: string; area: number; radius: number | null }>("cad_face_info", { face: hit.face });
          const names: Record<string, string> = { plane: "plana", cylinder: "cilíndrica", cone: "cónica", sphere: "esférica", torus: "tórica" };
          const radius = info.radius != null ? ` · radio ${info.radius.toFixed(3)} mm` : "";
          ui.setMessage(`Cara ${names[info.surface] ?? info.surface} · área ${info.area.toFixed(2)} mm²${radius}`);
        } else ui.setMessage(undefined);
      }
    } catch (err) {
      ui.setMessage(String(err));
    }
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0 || e.altKey) return;
    if (ui.session()) sketchClick(e);
    else void pickClick(e);
  };

  const onPointerMove = (e: PointerEvent) => {
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

  const onPointerUp = () => {
    dragging = undefined;
  };

  const onKey = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
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
    } else if (store.draft() && (e.key === "Escape" || e.key === "Enter") && !target.closest("button, a, [role=listbox], [role=option], [role=combobox], [role=dialog]")) {
      // Diálogo de la operación abierta: Enter acepta, Esc cancela (salvo en
      // un botón o una lista desplegada, que usan esas teclas)
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
        class={clsx("absolute inset-0", (ui.session() && ui.tool() !== "select") || ui.pick().kind !== "none" ? "cursor-crosshair" : "cursor-default")}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => setSnapView(undefined)}
        onContextMenu={(e) => e.preventDefault()}
      />

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

      {/* Barra superior */}
      <div class="absolute top-2 left-2 right-2 flex items-start gap-2 pointer-events-none">
        <Show
          when={ui.session()}
          fallback={
            <div class="flex items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto">
              <IconButton aria-label="Encuadrar todo" size="sm" onClick={() => viewer?.frameAll()}>
                <Icons.FrameCorners size={14} />
              </IconButton>
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

      {/* Lo que se espera que el usuario elija */}
      <Show when={promptText()}>
        <div class="absolute top-14 left-1/2 -translate-x-1/2 flex items-center gap-3 rounded-md border border-accent/60 bg-bg-lighter/95 px-3 py-1.5 text-xs text-text">
          <Icons.Cursor size={14} class="text-accent" />
          {promptText()}
          <button class="text-text-muted hover:text-text" onClick={() => ui.cancelPick()}>
            Cancelar (Esc)
          </button>
        </div>
      </Show>

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

      {/* Mensajes y errores */}
      <div class="absolute bottom-2 left-2 right-2 flex flex-col gap-1 items-start pointer-events-none">
        <Show when={store.busy()}>
          <span class="text-xs text-text-muted bg-bg-lighter/90 rounded px-2 py-0.5">Recalculando...</span>
        </Show>
        <Show when={ui.message()}>
          <span class="text-xs text-text bg-bg-lighter/90 rounded px-2 py-0.5">{ui.message()}</span>
        </Show>
        <Show when={store.error()}>
          <span class="text-xs text-error bg-bg-lighter/90 rounded px-2 py-0.5">{store.error()}</span>
        </Show>
        <Show when={ui.session()}>
          <span class="text-[11px] text-text-dim">
            Clic: dibujar · Mayús: sin anclajes · Esc: cortar la herramienta · Supr: borrar lo elegido · Enter: terminar · Alt+arrastrar o botón del medio: girar la
            vista
          </span>
        </Show>
      </div>
    </div>
  );
};
