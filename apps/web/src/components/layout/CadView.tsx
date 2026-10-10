import { Component, For, type JSX, Index, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { CadViewer, arc3Polyline, entityPolyline, planeToWorld } from "../../lib/CadViewer";
import type { LightSettings } from "../../lib/lightRig";
import type { CameraPose } from "../../lib/cameraRig";
import { parse as parseFont, type Font } from "opentype.js";
import { layoutText } from "../../lib/sketchText";
import { CONSTRAINT_LABELS, editsAsSketch, geometryPoints, type Report3d, type Sketch3d, type Plane, addPoint, addText, removeText, textOf, constraintIds, ellipsePolyline, splineOf, splinePolyline, curvePolyline, constraintValue, isReference, extendLine, isSolidPoint, leavingDirection, placeSnap, tangentArc, trimAt, type CadStore, designMass, dragByHandle, flipByHandle, handleField, partColor, partHidden, samePart, type FeatureHandle, type MeasureItem, type Measurement, type P2, type P3, type Sketch, type SketchConstraint } from "../../lib/cad";
import { infer, solidRefs, SNAP_GLYPHS, type Snap, type SnapKind } from "../../lib/sketchSnap";
import { clipCenter, measureConstraint, rotation, scaling, selectedEntities, splitEntityAt, transformSelection, translation, type Xform } from "../../lib/sketchTransform";
import { trimByStroke } from "../../lib/sketchEdit";
import { gridSegments, parseCoords, snapToGrid } from "../../lib/sketchInput";
import { checkSketch, connectedChain, problemsText, selectByKind } from "../../lib/sketchCheck";
import { constraintGlyphs } from "../../lib/sketchGlyphs";
import { ellipseArcPolyline, ellipseParam, makeBSpline, rhoWeight, sample as sampleCurve } from "../../lib/sketchCurves";
import { addParallelogram, insertSplinePoint, splineCurvature } from "../../lib/sketchSplines";
import { addArcSlot, addCircumscribedPolygon, addRect3, addSlot, arcSlotOutline, circumcircle, circumscribedVertices, nearestOnEntity, rectFrom3 } from "../../lib/sketchShapes";
import type { CadUi, Hit3d, Pick3d, PickFilter, SketchTool } from "../../lib/cadUi";
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
type ToolDef = { id: SketchTool; short: string; label: string; key?: string; icon: (p: { size?: number }) => JSX.Element; group: number; family?: string };
const TOOLS: ToolDef[] = [
  { id: "select", short: "Elegir", label: "Elegir y arrastrar", key: "S", icon: SketchIcons.Select, group: 0 },
  { id: "line", short: "Línea", label: "Línea", key: "L", icon: SketchIcons.Line, group: 1, family: "line" },
  { id: "line_inf", short: "Línea infinita", label: "Línea infinita de construcción (dos puntos por donde pasa)", icon: SketchIcons.LineInfinite, group: 1, family: "line" },
  { id: "centerline", short: "Línea central", label: "Línea central (eje de revolución y de los diámetros, que se toman sin elegirlo; dos clics)", icon: SketchIcons.Centerline, group: 1, family: "line" },
  { id: "line_mid", short: "Línea centro", label: "Línea desde el centro (centro, extremo: crece igual a los dos lados)", key: "M", icon: SketchIcons.LineMid, group: 1, family: "line" },
  { id: "rect", short: "Rectángulo", label: "Rectángulo", key: "R", icon: SketchIcons.Rect, group: 1, family: "rect" },
  { id: "rect_center", short: "Rect. centro", label: "Rectángulo por el centro (centro, esquina)", icon: SketchIcons.RectCenter, group: 1, family: "rect" },
  { id: "rect3", short: "Rect. 3 p.", label: "Rectángulo por 3 puntos (un lado y el ancho: queda inclinado)", icon: SketchIcons.Rect3, group: 1, family: "rect" },
  { id: "parallelogram", short: "Paralelogramo", label: "Paralelogramo (tres esquinas seguidas; la cuarta sale sola)", icon: SketchIcons.Parallelogram, group: 1, family: "rect" },
  { id: "circle", short: "Círculo", label: "Círculo", key: "C", icon: SketchIcons.Circle, group: 1, family: "circle" },
  { id: "circle2", short: "Círculo 2 p.", label: "Círculo por 2 puntos (los extremos de un diámetro)", icon: SketchIcons.Circle2, group: 1, family: "circle" },
  { id: "circle3", short: "Círculo 3 p.", label: "Círculo por 3 puntos", icon: SketchIcons.Circle3, group: 1, family: "circle" },
  { id: "circle_tan", short: "Círculo tangente", label: "Círculo tangente a 3 líneas o curvas (clic en cada una, cerca de donde toca)", icon: SketchIcons.CircleTan, group: 1, family: "circle" },
  { id: "arc", short: "Arco", label: "Arco (centro, inicio, fin)", key: "A", icon: SketchIcons.ArcCenter, group: 1, family: "arc" },
  { id: "arc3", short: "Arco 3 p.", label: "Arco por 3 puntos (inicio, fin, uno por donde pasa)", key: "3", icon: SketchIcons.Arc3, group: 1, family: "arc" },
  { id: "tangent", short: "Tangente", label: "Arco tangente (desde el extremo de una línea o arco)", key: "G", icon: SketchIcons.TangentArc, group: 1, family: "arc" },
  { id: "ellipse", short: "Elipse", label: "Elipse (centro, extremo del eje mayor, ancho)", key: "I", icon: SketchIcons.Ellipse, group: 1, family: "ellipse" },
  { id: "ellipse_arc", short: "Arco elíptico", label: "Arco elíptico (centro, eje mayor, ancho, comienzo y fin antihorario)", icon: SketchIcons.EllipseArc, group: 1, family: "ellipse" },
  { id: "polygon", short: "Polígono", label: "Polígono regular (centro, vértice)", key: "P", icon: SketchIcons.Polygon, group: 1 },
  { id: "slot", short: "Ranura", label: "Ranura (centro, centro, ancho)", key: "U", icon: SketchIcons.Slot, group: 1, family: "slot" },
  { id: "slot_center", short: "Ranura centro", label: "Ranura por el centro (centro, extremo, ancho)", icon: SketchIcons.SlotCenter, group: 1, family: "slot" },
  { id: "slot_arc", short: "Ranura arco", label: "Ranura en arco (centro del arco, comienzo, fin antihorario, ancho)", icon: SketchIcons.SlotArc, group: 1, family: "slot" },
  { id: "slot_arc3", short: "Ranura arco 3 p.", label: "Ranura en arco por 3 puntos (comienzo, uno por donde pasa, fin, ancho)", icon: SketchIcons.SlotArc3, group: 1, family: "slot" },
  { id: "spline", short: "Spline", label: "Spline (clics por donde pasa; clic en el primero la cierra, Esc la termina)", key: "N", icon: SketchIcons.Spline, group: 1, family: "spline" },
  { id: "bspline", short: "Spline por polos", label: "Spline por polos de control (clics en los polos; clic en el primero la cierra, Esc la termina)", icon: SketchIcons.BSpline, group: 1, family: "spline" },
  { id: "conic", short: "Cónica", label: "Cónica (comienzo, fin y vértice del triángulo; rho da la forma: 0,5 parábola, menos elipse, más hipérbola)", icon: SketchIcons.Conic, group: 1, family: "spline" },
  { id: "parabola", short: "Parábola", label: "Parábola (comienzo, fin y vértice del triángulo de control)", icon: SketchIcons.Parabola, group: 1, family: "spline" },
  { id: "spline_point", short: "Punto en spline", label: "Agregar un punto a una spline (o un polo a una spline por polos, sin cambiar su forma): clic sobre la curva", icon: SketchIcons.SplinePoint, group: 1, family: "spline" },
  { id: "point", short: "Punto", label: "Punto suelto (para agujeros y referencias)", key: "O", icon: SketchIcons.Point, group: 1 },
  { id: "text", short: "Texto", label: "Texto (clic donde empieza la línea base)", key: "X", icon: SketchIcons.Text, group: 1 },
  { id: "trim", short: "Recortar", label: "Recortar (clic en el tramo a quitar, o arrastrar: quita todo lo que toca el trazo)", key: "T", icon: SketchIcons.Trim, group: 2 },
  { id: "extend", short: "Extender", label: "Extender (clic cerca del extremo)", key: "E", icon: SketchIcons.Extend, group: 2 },
  { id: "use", short: "Usar", label: "Usar arista o cara del sólido (una cara trae todo su contorno; queda ligado: si el sólido cambia, se mueve con él)", key: "J", icon: SketchIcons.Use, group: 2 },
  { id: "split", short: "Partir", label: "Partir una línea, arco o círculo (clic donde se parte)", key: "D", icon: SketchIcons.Split, group: 2 },
  { id: "stretch", short: "Estirar", label: "Estirar (caja sobre los puntos que se corren, después punto base y destino: lo demás se estira)", key: "W", icon: SketchIcons.Stretch, group: 3 },
  { id: "move", short: "Mover", label: "Mover lo elegido (punto base, destino)", key: "V", icon: SketchIcons.Move, group: 3 },
  { id: "copy", short: "Copiar", label: "Copiar lo elegido (punto base, destino; Ctrl+C y Ctrl+V también)", key: "K", icon: SketchIcons.Copy, group: 3 },
  { id: "rotate", short: "Girar", label: "Girar lo elegido (centro, desde, hasta)", key: "H", icon: SketchIcons.Rotate, group: 3 },
  { id: "scale", short: "Escalar", label: "Escalar lo elegido (punto base, desde, hasta)", key: "Y", icon: SketchIcons.Scale, group: 3 },
];

/** Nombre de cada familia de variantes (para el botón que abre la lista) */
const FAMILY_MENU: Record<string, string> = {
  line: "Más líneas",
  rect: "Más rectángulos",
  circle: "Más círculos",
  arc: "Más arcos",
  slot: "Más ranuras",
  ellipse: "Más elipses",
  spline: "Más splines y cónicas",
};

/** Herramientas en las que un clic elige algo del sketch: se resalta lo que está bajo el mouse */
const PRESELECT = new Set<SketchTool>(["select", "trim", "extend", "split", "circle_tan", "spline_point"]);
/** Tope de íconos de restricciones a la vista */
const MAX_GLYPHS = 500;

/** Herramientas que transforman lo elegido */
const TRANSFORMS: SketchTool[] = ["move", "copy", "rotate", "scale"];

/** Ángulo con signo (grados, antihorario) de `a` a `b` alrededor de `c` */
const angleAt = (c: P2, a: P2, b: P2) => {
  const [u, v] = [[a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]];
  return (Math.atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1]) * 180) / Math.PI;
};

/** Lo que hace una herramienta de transformar con sus clics (`pts`) y el último (`to`) */
function transformOf(tool: SketchTool, pts: P2[], to: P2): { f: Xform; scale?: number } | undefined {
  const [a, b] = pts;
  if (tool === "move" || tool === "copy") return a && { f: translation([to[0] - a[0], to[1] - a[1]]) };
  if (!a || !b) return undefined;
  if (tool === "rotate") return { f: rotation(a, angleAt(a, b, to)) };
  const r0 = dist(a, b);
  const k = r0 > 1e-9 ? dist(a, to) / r0 : 0;
  return k > 1e-9 ? { f: scaling(a, k), scale: k } : undefined;
}

const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Etiqueta de una cota en pantalla */
interface DimView {
  index: number;
  x: number;
  y: number;
  text: string;
  conflict: boolean;
  reference: boolean;
  locked: boolean;
  /** Desde dónde sale la guía (texto movido a mano), en px del visor */
  leader?: [number, number];
  /** Punto acotado (coordenadas del sketch) y corrimiento del texto, si se movió */
  anchor: P2;
  offset?: [number, number];
}

/** Lo que va antes del valor de cada tipo de cota */
const DIM_PREFIX: Partial<Record<SketchConstraint["type"], (c: SketchConstraint) => string>> = {
  radius: () => "R ",
  diameter: () => "Ø ",
  axis_diameter: () => "Ø ",
  arc_length: () => "⌒ ",
  curve_length: () => "Σ ",
  circle_distance: (c) => (c.type === "circle_distance" && c.max ? "máx " : "mín "),
};

// Fuente del texto: la incluida (Liberation Sans, OFL) o la que se elija
let textFont: { name: string; font: Font } | undefined;
async function loadTextFont(): Promise<Font> {
  if (!textFont) {
    const res = await fetch(`${import.meta.env.BASE_URL}fonts/LiberationSans-Regular.ttf`);
    textFont = { name: "Liberation Sans", font: parseFont(await res.arrayBuffer()) };
  }
  return textFont.font;
}
/** Negrita y cursiva de la fuente incluida (una elegida se usa tal cual) */
const styledFonts = new Map<string, Font>();
async function loadStyledFont(bold: boolean, italic: boolean): Promise<Font> {
  const regular = await loadTextFont();
  if ((!bold && !italic) || textFont?.name !== "Liberation Sans") return regular;
  const name = bold && italic ? "BoldItalic" : bold ? "Bold" : "Italic";
  if (!styledFonts.has(name)) {
    const res = await fetch(`${import.meta.env.BASE_URL}fonts/LiberationSans-${name}.ttf`);
    styledFonts.set(name, parseFont(await res.arrayBuffer()));
  }
  return styledFonts.get(name)!;
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
/** Punto de la elipse (centro `c`, extremo del eje mayor `a`, del menor `b`) en la dirección paramétrica de `p` */
function onEllipse(c: P2, a: P2, b: P2, p: P2): P2 {
  const u: P2 = [a[0] - c[0], a[1] - c[1]];
  const v: P2 = [b[0] - c[0], b[1] - c[1]];
  const t = ellipseParam(c, u, v, p);
  return [c[0] + u[0] * Math.cos(t) + v[0] * Math.sin(t), c[1] + u[1] * Math.cos(t) + v[1] * Math.sin(t)];
}

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
    if (g.type === "line" && e.infinite) {
      // Infinita: la distancia a la recta entera
      const [a, b] = [pt.get(g.start)!, pt.get(g.end)!];
      d = Math.abs((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / (dist(a, b) || 1);
    } else if (g.type === "line") d = segDist(p, pt.get(g.start)!, pt.get(g.end)!);
    else if (g.type === "circle") d = Math.abs(dist(p, pt.get(g.center)!) - g.radius);
    else if (g.type === "arc") d = Math.abs(dist(p, pt.get(g.center)!) - dist(pt.get(g.start)!, pt.get(g.center)!));
    else if (g.type === "point") d = dist(p, pt.get(g.point)!);
    else if (g.type === "ellipse") {
      const pts = ellipsePolyline(pt.get(g.center)!, pt.get(g.major)!, pt.get(g.minor)!);
      for (let i = 0; i + 1 < pts.length; i++) d = Math.min(d, segDist(p, pts[i], pts[i + 1]));
    }
    else {
      const pts = (g.type === "spline" ? splineOf(g, (x) => pt.get(x)) : curvePolyline(g, (x) => pt.get(x))) ?? [];
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
  const [chain, setChain] = createSignal<{ first: number; last: number; lastSnapped: boolean; lastEntity?: number }>();
  // Polilínea línea-arco: con A el próximo tramo de la línea sale como arco tangente
  const [chainArc, setChainArc] = createSignal(false);
  // Último punto puesto (las coordenadas relativas parten de ahí)
  let lastPlaced: P2 | undefined;
  // Arco tangente: desde qué punto, en qué dirección y de qué entidad viene
  const [tangentFrom, setTangentFrom] = createSignal<{ point: number; dir: P2; entity: number }>();
  const [anchor, setAnchor] = createSignal<Snap[]>([]);
  // Recortar arrastrando: el trazo en el plano y hasta dónde se alejó en pantalla del comienzo
  const [trimStroke, setTrimStroke] = createSignal<P2[]>();
  let trimFrom: { x: number; y: number; far: number } | undefined;
  // Estirar: los puntos tomados con la caja
  const [stretchPts, setStretchPts] = createSignal<number[]>();
  // Anclaje bajo el cursor: punto resaltado y su glifo junto al puntero
  const [snapView, setSnapView] = createSignal<{ kind: SnapKind; p: P2; x: number; y: number; guides: [P2, P2][]; refs: number[] }>();
  const [polygonSides, setPolygonSides] = createSignal(6);
  // Cónica: factor rho (0,5 = parábola); spline por polos: grado
  const [conicRho, setConicRho] = createSignal(0.5);
  const [bsplineDegree, setBsplineDegree] = createSignal(3);
  // Variante a la vista de cada familia de herramientas, y la lista abierta
  const [familyPick, setFamilyPick] = createSignal<Record<string, SketchTool>>({});
  const [openFamily, setOpenFamily] = createSignal<string>();
  createEffect(() => {
    const t = TOOLS.find((x) => x.id === ui.tool());
    if (t?.family) setFamilyPick((m) => (m[t.family!] === t.id ? m : { ...m, [t.family!]: t.id }));
  });
  // Un clic fuera cierra la lista de variantes
  const closeFamily = (e: PointerEvent) => {
    if (openFamily() && !(e.target as HTMLElement).closest("[data-family-list], [data-toolbar-menu]")) setOpenFamily(undefined);
  };
  onMount(() => window.addEventListener("pointerdown", closeFamily, true));
  onCleanup(() => window.removeEventListener("pointerdown", closeFamily, true));
  /** Botones de la barra: una entrada por familia (con su lista) o por herramienta suelta */
  const toolSlots = () => {
    const out: { family?: string; group: number; tools: ToolDef[] }[] = [];
    for (const t of TOOLS) {
      const prev = t.family && out.find((x) => x.family === t.family);
      if (prev) prev.tools.push(t);
      else out.push({ family: t.family, group: t.group, tools: [t] });
    }
    return out;
  };
  // Polígono inscrito (el clic es un vértice) o circunscrito (el clic es el medio de un lado)
  const [polygonCircumscribed, setPolygonCircumscribed] = createSignal(false);
  // Círculo tangente: las entidades elegidas (y dónde se hizo clic en cada una)
  const [tanPicks, setTanPicks] = createSignal<{ entity: number; p: P2 }[]>([]);
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
      await loadTextFont();
      const s = ui.session();
      const t = s?.sketch.texts?.find((x) => x.id === id);
      const a = t && s!.sketch.points.find((p) => p.id === t.anchor);
      if (!t || !a || !text.trim()) return;
      const at: P2 = [a.x, a.y];
      const style = t.style ?? {};
      const styled = await loadStyledFont(!!style.bold, !!style.italic);
      const contours = layoutText(styled, text, size, at, style.align, textPath(s!.sketch, style.path));
      ui.change((sk) => {
        const cur = sk.texts?.find((x) => x.id === id);
        if (!cur) return;
        removeText(sk, cur, true);
        addText(sk, contours, at, { text, size, font: fontName(), ...(Object.keys(style).length ? { style } : {}) }, cur.anchor, cur.id);
      });
      ui.setSelection([t.anchor]);
      ui.setMessage(undefined);
    } catch (err) {
      ui.setMessage(`No se pudo leer la fuente: ${err}`);
    }
  };
  const [textValue, setTextValue] = createSignal("Texto");
  const [textBold, setTextBold] = createSignal(false);
  const [textItalic, setTextItalic] = createSignal(false);
  const [textAlign, setTextAlign] = createSignal<"left" | "center" | "right">("left");
  /** Polilínea de la curva que siguen las letras de un texto */
  const textPath = (sk: Sketch, id?: number): P2[] | undefined => {
    const e = id !== undefined ? sk.entities.find((x) => x.id === id) : undefined;
    if (!e) return undefined;
    return entityPolyline(e.geometry, new Map(sk.points.map((p) => [p.id, [p.x, p.y] as P2])));
  };
  /** Curva elegida que seguiría un texto nuevo (una sola, que no sea un punto ni parte de otro texto) */
  const textCurve = () => {
    const s = ui.session();
    const sel = ui.selection();
    if (!s || sel.length !== 1) return undefined;
    const e = s.sketch.entities.find((x) => x.id === sel[0]);
    return e && e.geometry.type !== "point" && !textOf(s.sketch, e.id) ? e.id : undefined;
  };
  const [textSize, setTextSize] = createSignal(10);
  const [fontName, setFontName] = createSignal("Liberation Sans");
  const [scanVisible, setScanVisible] = createSignal(true);
  const [scanOpacity, setScanOpacity] = createSignal(0.35);
  let dragging: number | undefined;
  // Lazo libre (Ctrl+arrastrar en vacío con Elegir): sus puntos en pantalla
  const [lasso, setLasso] = createSignal<[number, number][]>();
  let lassoAdditive = false;
  // Entidad o punto bajo el mouse (preselección)
  const [preHover, setPreHover] = createSignal<number>();
  // Para el doble clic sobre una entidad (elige la cadena)
  let lastEntityClick: { id: number; t: number } | undefined;
  // Cambia con cada cuadro dibujado: las cotas HTML siguen a la cámara
  const [viewTick, setViewTick] = createSignal(0);
  const [editingDim, setEditingDim] = createSignal<number>();
  /** Texto de cota que se está arrastrando: dónde va (corrimiento en mm del plano) */
  const [dimDrag, setDimDrag] = createSignal<{ index: number; offset: [number, number] }>();
  /** Para no abrir la edición al soltar un arrastre de texto */
  let dimDragged = false;
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

  /** Lo que impide cerrar regiones en el sketch en edición */
  const problems = createMemo(() => {
    const s = ui.session();
    return s ? checkSketch(s.sketch) : undefined;
  });

  const resetTool = () => {
    setChain(undefined);
    setChainArc(false);
    setAnchor([]);
    setTrimStroke(undefined);
    setStretchPts(undefined);
    setTangentFrom(undefined);
    setTanPicks([]);
  };
  // Deshacer o rehacer en el sketch: lo que la herramienta tenía a medias (y las
  // cotas pedidas, por índice) puede ya no existir
  createEffect(
    on(
      ui.sketchRestored,
      () => {
        resetTool();
        setDimQueue([]);
        setEditingDim(undefined);
      },
      { defer: true },
    ),
  );

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
  // Dibujando con «ocultar el modelo»
  createEffect(() => viewer?.setBodyHidden(!!ui.session() && ui.assist().hideModel));
  // Vista de corte: el plano recorre la caja envolvente del sólido
  createEffect(() => {
    store.mesh();
    const s = ui.section();
    const body = store.result()?.body;
    // Dibujando con el corte en el plano: se ve lo que queda detrás del sketch
    const sk = ui.session();
    if (sk && ui.assist().section) {
      const n = sk.plane.normal;
      return viewer?.setSection({ origin: sk.plane.origin, normal: [-n[0], -n[1], -n[2]] });
    }
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
    const stroke = trimStroke();
    if (stroke && stroke.length > 1) preview.push(stroke);
    const pt = (id: number): P2 | undefined => {
      const p = s.sketch.points.find((q) => q.id === id);
      return p ? [p.x, p.y] : undefined;
    };
    const ch = chain();
    const an = anchor().map((a) => a.p);
    if (c) {
      const t = ui.tool();
      const from = ch && pt(ch.last);
      if (t === "line" && from && chainArc() && ch?.lastEntity !== undefined) {
        const dir = leavingDirection(s.sketch, ch.lastEntity, ch.last);
        const arc = dir && tangentArc(from, dir, c);
        if (arc) {
          const r = dist(arc.center, from);
          const a0 = Math.atan2(from[1] - arc.center[1], from[0] - arc.center[0]);
          let sweep = Math.atan2(c[1] - arc.center[1], c[0] - arc.center[0]) - a0;
          if (arc.ccw) while (sweep <= 0) sweep += 2 * Math.PI;
          else while (sweep >= 0) sweep -= 2 * Math.PI;
          preview.push(Array.from({ length: 49 }, (_, i) => [arc.center[0] + r * Math.cos(a0 + (sweep * i) / 48), arc.center[1] + r * Math.sin(a0 + (sweep * i) / 48)] as P2));
        } else preview.push([from, c]);
      } else if (t === "line" && from) preview.push([from, c]);
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
      if (t === "polygon" && an.length === 1) {
        if (polygonCircumscribed()) {
          const v = circumscribedVertices(an[0], c, Math.max(3, Math.round(polygonSides())));
          preview.push([...v, v[0]]);
        } else preview.push(polygonPoints(an[0], c, polygonSides(), true));
      }
      if ((t === "circle2" || t === "circle3") && an.length) {
        const cc = t === "circle2" ? { c: [(an[0][0] + c[0]) / 2, (an[0][1] + c[1]) / 2] as P2, r: dist(an[0], c) / 2 } : an.length === 2 ? circumcircle(an[0], an[1], c) : undefined;
        if (cc) preview.push(Array.from({ length: 49 }, (_, i) => [cc.c[0] + cc.r * Math.cos((i / 48) * 2 * Math.PI), cc.c[1] + cc.r * Math.sin((i / 48) * 2 * Math.PI)] as P2));
        else preview.push([an[0], c]);
      }
      if (t === "rect3" && an.length === 1) preview.push([an[0], c]);
      if (t === "rect3" && an.length === 2) {
        const r = rectFrom3(an[0], an[1], c);
        preview.push(r ? [...r, r[0]] : [an[0], an[1]]);
      }
      if (t === "slot_center" && an.length === 1) preview.push([[2 * an[0][0] - c[0], 2 * an[0][1] - c[1]], c]);
      if (t === "slot_center" && an.length === 2) {
        const a: P2 = [2 * an[0][0] - an[1][0], 2 * an[0][1] - an[1][1]];
        preview.push(slotOutline(a, an[1], slotRadius(a, an[1], c)));
      }
      if (t === "slot_arc" && an.length >= 1) {
        const [k] = an;
        if (an.length === 1) preview.push([k, c]);
        else {
          const R = dist(k, an[1]);
          const a0 = Math.atan2(an[1][1] - k[1], an[1][0] - k[0]);
          const to = an.length === 3 ? an[2] : c;
          let sweep = Math.atan2(to[1] - k[1], to[0] - k[0]) - a0;
          while (sweep <= 0) sweep += 2 * Math.PI;
          if (an.length === 2) preview.push(Array.from({ length: 33 }, (_, i) => [k[0] + R * Math.cos(a0 + (sweep * i) / 32), k[1] + R * Math.sin(a0 + (sweep * i) / 32)] as P2));
          else preview.push(arcSlotOutline(k, an[1], sweep, Math.max(1e-6, Math.abs(dist(k, c) - R))));
        }
      }
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
      if (t === "line_inf" && an.length === 1) {
        const l = dist(an[0], c) || 1;
        const k = Math.max(50 * l, 1000) / l;
        preview.push([
          [an[0][0] - (c[0] - an[0][0]) * k, an[0][1] - (c[1] - an[0][1]) * k],
          [an[0][0] + (c[0] - an[0][0]) * k, an[0][1] + (c[1] - an[0][1]) * k],
        ]);
      }
      if (t === "parallelogram" && an.length === 1) preview.push([an[0], c]);
      if (t === "parallelogram" && an.length === 2) preview.push([an[0], an[1], c, [an[0][0] + c[0] - an[1][0], an[0][1] + c[1] - an[1][1]], an[0]]);
      if (t === "ellipse_arc" && an.length === 1) preview.push([an[0], c]);
      if (t === "ellipse_arc" && an.length >= 2) {
        const minor = an.length === 2 ? ellipseMinor(an[0], an[1], c) : ellipseMinor(an[0], an[1], an[2]);
        if (minor && an.length <= 3) preview.push(ellipsePolyline(an[0], an[1], minor));
        if (minor && an.length === 3) preview.push([an[0], onEllipse(an[0], an[1], minor, c)]);
        if (minor && an.length === 4) preview.push(ellipseArcPolyline(an[0], an[1], minor, onEllipse(an[0], an[1], minor, an[3]), onEllipse(an[0], an[1], minor, c)));
      }
      if ((t === "conic" || t === "parabola") && an.length === 1) preview.push([an[0], c]);
      if ((t === "conic" || t === "parabola") && an.length === 2) {
        const w = rhoWeight(t === "parabola" ? 0.5 : conicRho());
        const curve = makeBSpline({ poles: [an[0], c, an[1]], weights: [1, w, 1], degree: 2 });
        preview.push([an[0], c, an[1]]);
        if (curve) preview.push(sampleCurve(curve, 48));
      }
      if (t === "bspline" && an.length >= 1) {
        const poles = [...an, c];
        preview.push(poles);
        const curve = makeBSpline({ poles, degree: bsplineDegree() });
        if (curve) preview.push(sampleCurve(curve, 16));
      }
      if (t === "slot_arc3" && an.length === 1) preview.push([an[0], c]);
      if (t === "slot_arc3" && an.length >= 2) {
        // Comienzo, por donde pasa, fin: el arco del medio; después el ancho
        const arc = an.length === 2 ? arcThrough(an[0], c, an[1]) : arcThrough(an[0], an[2], an[1]);
        if (arc && an.length === 2) preview.push(Array.from({ length: 33 }, (_, i) => [arc.c[0] + arc.r * Math.cos(arc.a0 + (arc.sweep * i) / 32), arc.c[1] + arc.r * Math.sin(arc.a0 + (arc.sweep * i) / 32)] as P2));
        if (arc && an.length === 3) {
          const start: P2 = arc.sweep > 0 ? an[0] : an[2];
          preview.push(arcSlotOutline(arc.c, start, Math.abs(arc.sweep), Math.max(1e-6, Math.abs(dist(arc.c, c) - arc.r))));
        }
      }
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
      if (TRANSFORMS.includes(t) && an.length) {
        // Lo elegido donde quedaría, y las guías desde el primer clic
        const xf = transformOf(t, an, c);
        const pts = new Map(s.sketch.points.map((q) => [q.id, [q.x, q.y] as P2]));
        if (xf) for (const e of selectedEntities(s.sketch, ui.selection())) {
          const poly = entityPolyline(e.geometry, pts);
          if (poly) preview.push(poly.map(xf.f));
        }
        if (t === "rotate" || t === "scale") preview.push(an.length === 2 ? [an[1], an[0], c] : [an[0], c]);
        else preview.push([an[0], c]);
      }
      const st = stretchPts();
      if (t === "stretch" && st?.length && an.length) {
        // Lo que toca los puntos tomados, estirado hasta el cursor
        const d: P2 = [c[0] - an[0][0], c[1] - an[0][1]];
        const moved = new Set(st);
        const pts = new Map(s.sketch.points.map((q) => [q.id, (moved.has(q.id) ? [q.x + d[0], q.y + d[1]] : [q.x, q.y]) as P2]));
        for (const e of s.sketch.entities)
          if (geometryPoints(e.geometry).some((p) => moved.has(p))) {
            const poly = entityPolyline(e.geometry, pts);
            if (poly) preview.push(poly);
          }
        preview.push([an[0], c]);
      }
      const clip = ui.clipboard();
      if (t === "paste" && clip) {
        const o = clipCenter(clip);
        const pts = new Map(clip.points.map((q) => [q.id, [q.x - o[0] + c[0], q.y - o[1] + c[1]] as P2]));
        for (const e of clip.entities) {
          const poly = entityPolyline(e.geometry, pts);
          if (poly) preview.push(poly);
        }
      }
      if (t === "slot" && an.length === 1) preview.push([an[0], c]);
      if (t === "centerline" && an.length === 1) preview.push([an[0], c]);
      if (t === "slot" && an.length === 2) preview.push(slotOutline(an[0], an[1], slotRadius(an[0], an[1], c)));
    }
    const as = ui.assist();
    let grid: ReturnType<typeof gridSegments> | undefined;
    if (as.grid) {
      const xs = [0, ...s.sketch.points.map((q) => q.x)];
      const ys = [0, ...s.sketch.points.map((q) => q.y)];
      const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
      const pad = Math.max(x1 - x0, y1 - y0, as.spacing * 20);
      grid = gridSegments([x0 - pad, y0 - pad, x1 + pad, y1 + pad], as.spacing);
    }
    viewer.setSketch({
      plane: s.plane,
      sketch: s.sketch,
      regions: s.regions,
      grid,
      selected: ui.selection(),
      hover: [
        ...ui.hoverIds(),
        ...(snapView()?.refs ?? []),
        ...tanPicks().map((x) => x.entity),
        ...(preHover() !== undefined ? [preHover()!] : []),
        // Lo que atan las restricciones elegidas
        ...ui.selectedConstraints().flatMap((i) => (s.sketch.constraints[i] ? constraintIds(s.sketch.constraints[i]) : [])),
      ],
      freePoints: s.report?.free_points,
      freeEntities: s.report?.free_entities,
      // Flechas de lo que se mueve (los puntos de un texto van con su ancla)
      freedom:
        s.report && ui.sketchShow().freedom && ui.sketchShow().points
          ? (() => {
              const inText = new Set((s.sketch.texts ?? []).flatMap((t) => t.points.filter((p) => p !== t.anchor)));
              return {
                dirs: (s.report.free_dirs ?? []).filter(([p]) => !inText.has(p)),
                free: s.report.free_points.filter((p) => !inText.has(p) && p !== s.sketch.origin),
                radius: s.report.free_radius ?? [],
              };
            })()
          : undefined,
      // Peine de curvatura: dientes de hasta ~60 px
      curvature: (() => {
        const id = ui.combEntity();
        if (id === undefined || !s.sketch.entities.some((x) => x.id === id)) return undefined;
        return splineCurvature(s.sketch, id, viewer!.pixelSizeMm() * 60);
      })(),
      linkedEntities: s.sketch.uses?.map((u) => u.entity),
      hideConstruction: !ui.sketchShow().construction,
      hidePoints: !ui.sketchShow().points,
      problems: [...(problems()?.looseEnds.map((x) => x.p) ?? []), ...(problems()?.crossings.map((x) => x.p) ?? [])],
      warnEntities: problems()?.overlaps.flat(),
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
    const as = ui.assist();
    const free = (): Snap => ({ p: as.gridSnap ? snapToGrid(p, as.spacing) : p, kind: "free" });
    if (e.shiftKey || !as.infer) return free();
    // Dibujando una línea: puede salir paralela, perpendicular o tangente
    // (la línea desde el centro sale del centro con la misma dirección)
    const from = ui.tool() === "line" || ui.tool() === "line_mid" ? chain()?.last : undefined;
    const r = infer(s.sketch, p, viewer.pixelSizeMm() * 8, {
      exclude: dragging !== undefined ? [dragging] : [],
      from,
      solid: solid(),
      // Un rectángulo alineado con su primera esquina (o su centro) tendría ancho o alto cero
      noAlign: ui.tool() === "rect" || ui.tool() === "rect_center" ? anchor().flatMap((a) => (a.id !== undefined ? [a.id] : [])) : [],
    });
    // Sin nada a qué pegarse: al nudo de la rejilla
    return r.kind === "free" && as.gridSnap ? free() : r;
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

  /** Crea la spline por polos con los puntos marcados (abierta o cerrada) */
  const finishBSpline = (closed: boolean) => {
    const an = anchor();
    setAnchor([]);
    if (an.length < (closed ? 3 : 2)) return;
    ui.change((sk) => {
      const poles = an.map((a) => placeSnap(sk, a));
      const degree = Math.max(1, Math.min(Math.round(bsplineDegree()), poles.length - 1));
      ui.addEntity(sk, { type: "bspline", poles, degree, ...(closed ? { closed: true } : {}) });
    });
  };

  /**
   * Coordenadas escritas: el próximo clic de la herramienta cae en ese punto
   * (absoluto, relativo al último o polar). Devuelve un mensaje si no se entendió.
   */
  const typedPoint = (text: string): string | undefined => {
    const s = ui.session();
    if (!s || !viewer) return;
    const p = parseCoords(text, lastPlaced);
    if (typeof p === "string") return p;
    // Un punto que ya está ahí se usa (cierra contornos)
    const tol = 1e-9 * Math.max(1, Math.hypot(p[0], p[1]));
    const same = s.sketch.points.find((q) => Math.hypot(q.x - p[0], q.y - p[1]) <= tol);
    const [x, y] = viewer.screenOf(planeToWorld(s.plane, p));
    const fake = { clientX: x, clientY: y, shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, timeStamp: performance.now(), button: 0 } as PointerEvent;
    sketchClick(fake, { p, kind: same ? "point" : "free", ...(same ? { id: same.id } : {}) } as Snap);
    return undefined;
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

  const sketchClick = (e: PointerEvent, forced?: Snap) => {
    const s = ui.session();
    const hit = forced ?? snapped(e);
    if (!s || !hit || !viewer) return;
    const t = ui.tool();
    if (!["select", "trim", "extend", "split", "use", "stretch"].includes(t)) lastPlaced = hit.p;
    if (t === "select") {
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const h = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8);
      if (!e.shiftKey) ui.setSelectedConstraints([]);
      if (h.point !== undefined) {
        dragging = h.point;
        if (e.shiftKey) ui.setSelection((sel) => (sel.includes(h.point!) ? sel.filter((x) => x !== h.point) : [...sel, h.point!]));
        else if (!ui.selection().includes(h.point)) ui.setSelection([h.point]);
        return;
      }
      if (h.entity !== undefined) {
        const id = h.entity;
        // Doble clic: toda la cadena unida por los extremos (con la hora del evento: si la
        // página está ocupada, el segundo clic se procesa tarde pero llegó a tiempo)
        const now = e.timeStamp;
        if (lastEntityClick && lastEntityClick.id === id && now - lastEntityClick.t < 500) {
          lastEntityClick = undefined;
          const chain = connectedChain(s.sketch, id);
          ui.setSelection((sel) => (e.shiftKey ? [...new Set([...sel, ...chain])] : chain));
          return;
        }
        lastEntityClick = { id, t: now };
        ui.setSelection((sel) => (e.shiftKey ? (sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id]) : [id]));
        return;
      }
      // En vacío con Ctrl: lazo libre
      if (e.ctrlKey || e.metaKey) {
        lassoAdditive = e.shiftKey;
        setLasso([[e.clientX, e.clientY]]);
        return;
      }
      // En vacío: arrastrar elige por caja (Mayús suma)
      boxStart = { x: e.clientX, y: e.clientY, additive: e.shiftKey, before: [] };
      sketchBoxBefore = ui.selection();
      if (!e.shiftKey) ui.setSelection([]);
      return;
    }
    if (t === "use") {
      // La arista o la cara del sólido bajo el cursor, proyectada al plano y ligada a él
      const pick = viewer.pick(e.clientX, e.clientY, { edges: true, faces: true });
      const plane = s.plane;
      const linked =
        pick?.kind === "edge"
          ? store.projectEdge(pick.edge, plane).then((pe) => [pe])
          : pick?.kind === "face"
            ? store.projectFace(pick.face, plane)
            : undefined;
      if (!linked) return ui.setMessage("Clic sobre una arista o una cara del sólido");
      linked
        .then((pes) => ui.setMessage(ui.addLinked(pes, (i) => ({ edge: pes[i].ref }), pes.length === 1 ? "de la arista" : "del contorno de la cara")))
        .catch((err) => ui.setMessage(String(err)));
      return;
    }
    // Cotas de la forma recién dibujada (se piden enseguida)
    const dims: number[] = [];
    if (t === "line" && chainArc() && chain()?.lastEntity !== undefined) {
      // Tramo en arco: tangente al anterior, del último punto al clic
      const ch = chain()!;
      const dir = leavingDirection(s.sketch, ch.lastEntity!, ch.last);
      const startPos = s.sketch.points.find((q) => q.id === ch.last);
      const arc = dir && startPos && tangentArc([startPos.x, startPos.y], dir, hit.p);
      if (!arc) return ui.setMessage("En línea recta no hay arco: mover el punto hacia un costado");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const end = placeSnap(sk, hit);
        if (end === ch.last) return;
        const center = ui.addPoint(sk, arc.center);
        const id = ui.addEntity(sk, arc.ccw ? { type: "arc", center, start: ch.last, end } : { type: "arc", center, start: end, end: ch.last });
        sk.constraints.push({ type: "tangent", a: ch.lastEntity!, b: id });
        dims.push(dim(sk, { type: "radius", entity: id, value: round(dist(arc.center, hit.p)) }));
        const closed = end === ch.first;
        setChain(closed ? undefined : { first: ch.first, last: end, lastSnapped: hit.id !== undefined, lastEntity: id });
      });
      // Después del arco vuelve a línea (A otra vez para otro arco)
      setChainArc(false);
      return askDims(dims);
    }
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
        setChain(closed ? undefined : { first: ch.first, last: id, lastSnapped: snappedToPoint, lastEntity: line });
      });
      askDims(dims);
      return;
    }
    if (t === "centerline") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      if (dist(an[0].p, hit.p) > 1e-9)
        ui.change((sk) => {
          const a = placeSnap(sk, an[0]);
          const b = placeSnap(sk, hit);
          if (a === b) return;
          // Una sola línea central por sketch: la anterior queda como construcción común
          for (const e of sk.entities) delete e.axis;
          const line = ui.addEntity(sk, { type: "line", start: a, end: b });
          const ent = sk.entities.find((e) => e.id === line)!;
          ent.construction = true;
          ent.axis = true;
          if (hit.axis) sk.constraints.push({ type: hit.axis, line });
        });
      setAnchor([]);
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
      const path = textCurve();
      const style = {
        ...(textBold() ? { bold: true } : {}),
        ...(textItalic() ? { italic: true } : {}),
        ...(textAlign() !== "left" ? { align: textAlign() } : {}),
        ...(path !== undefined ? { path } : {}),
      };
      loadStyledFont(textBold(), textItalic())
        .then((font) => {
          const contours = layoutText(font, text, textSize(), at, textAlign(), textPath(s.sketch, path));
          ui.change((sk) => addText(sk, contours, at, { text, size: textSize(), font: fontName(), ...(Object.keys(style).length ? { style } : {}) }));
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
    if (t === "bspline") {
      const an = anchor();
      if (an.length >= 3 && dist(an[0].p, hit.p) <= viewer.pixelSizeMm() * 8) return finishBSpline(true);
      return setAnchor([...an, hit]);
    }
    if (t === "line_inf") {
      const an = anchor();
      if (!an.length) return setAnchor([hit]);
      setAnchor([]);
      if (dist(an[0].p, hit.p) < 1e-9) return;
      ui.change((sk) => {
        const id = ui.addEntity(sk, { type: "line", start: placeSnap(sk, an[0]), end: placeSnap(sk, hit) });
        const e = sk.entities.find((x) => x.id === id)!;
        e.construction = true;
        e.infinite = true;
      });
      return;
    }
    if (t === "parallelogram") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [A, B] = an;
      setAnchor([]);
      if (Math.abs((B.p[0] - A.p[0]) * (hit.p[1] - B.p[1]) - (B.p[1] - A.p[1]) * (hit.p[0] - B.p[0])) < 1e-9) return ui.setMessage("Las tres esquinas están en línea");
      ui.change((sk) => {
        const [a, b, c] = [placeSnap(sk, A), placeSnap(sk, B), placeSnap(sk, hit)];
        addParallelogram(sk, a, b, c);
      });
      return;
    }
    if (t === "ellipse_arc") {
      const an = anchor();
      if (an.length < 4) {
        if (an.length === 2 && !ellipseMinor(an[0].p, an[1].p, hit.p)) return ui.setMessage("La elipse necesita ancho: mover el punto hacia un costado del eje");
        return setAnchor([...an, hit]);
      }
      const [C, M, W, S] = an;
      const minor = ellipseMinor(C.p, M.p, W.p)!;
      setAnchor([]);
      ui.setMessage(undefined);
      ui.change((sk) => {
        const center = placeSnap(sk, C);
        const major = placeSnap(sk, M);
        const mi = addPoint(sk, minor);
        const start = addPoint(sk, onEllipse(C.p, M.p, minor, S.p));
        const end = addPoint(sk, onEllipse(C.p, M.p, minor, hit.p));
        ui.addEntity(sk, { type: "ellipse_arc", center, major, minor: mi, start, end });
        if (C.id === undefined || M.id === undefined) dims.push(dim(sk, { type: "distance", a: center, b: major, value: round(dist(C.p, M.p)) }));
        dims.push(dim(sk, { type: "distance", a: center, b: mi, value: round(dist(C.p, minor)) }));
      });
      return askDims(dims);
    }
    if (t === "conic" || t === "parabola") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [A, B] = an;
      setAnchor([]);
      const rho = t === "parabola" ? 0.5 : Math.min(0.99, Math.max(0.01, conicRho()));
      ui.change((sk) => {
        const [a, b, apex] = [placeSnap(sk, A), placeSnap(sk, B), placeSnap(sk, hit)];
        ui.addEntity(sk, { type: "bspline", poles: [a, apex, b], degree: 2, weights: [1, rhoWeight(rho), 1] });
      });
      return;
    }
    if (t === "slot_arc3") {
      const an = anchor();
      if (an.length < 3) return setAnchor([...an, hit]);
      const [S, M, E] = an;
      setAnchor([]);
      const arc = arcThrough(S.p, E.p, M.p);
      if (!arc) return ui.setMessage("Los tres puntos están en línea: mover el del medio hacia un costado");
      const w = Math.abs(dist(arc.c, hit.p) - arc.r);
      let msg: string | undefined;
      ui.change((sk) => {
        const center = ui.addPoint(sk, arc.c);
        // Los arcos van antihorarios: si pasa por el otro lado, del fin al comienzo
        const [first, last] = arc.sweep > 0 ? [S, E] : [E, S];
        const start = placeSnap(sk, first);
        const end = placeSnap(sk, last);
        const r = addArcSlot(sk, center, start, end, w);
        if (typeof r === "string") return void (msg = r);
        dims.push(dim(sk, { type: "radius", entity: r.capStart, value: round(w) }));
      });
      ui.setMessage(msg);
      return askDims(dims);
    }
    if (t === "spline_point") {
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      let msg: string | undefined = "Clic sobre una spline";
      if (target !== undefined) ui.change((sk) => void (msg = insertSplinePoint(sk, target, raw)));
      ui.setMessage(msg);
      return;
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
    if (t === "circle2" || t === "circle3") {
      const an = anchor();
      const need = t === "circle2" ? 1 : 2;
      if (an.length < need) return setAnchor([...an, hit]);
      const hits = [...an, hit];
      const cc =
        t === "circle2" ? { c: [(hits[0].p[0] + hit.p[0]) / 2, (hits[0].p[1] + hit.p[1]) / 2] as P2, r: dist(hits[0].p, hit.p) / 2 } : circumcircle(hits[0].p, hits[1].p, hit.p);
      if (!cc || cc.r < 1e-9) return ui.setMessage(t === "circle2" ? "Los dos puntos coinciden" : "Los tres puntos están en línea: mover el tercero hacia un costado");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const center = ui.addPoint(sk, cc.c);
        const c = ui.addEntity(sk, { type: "circle", center, radius: cc.r });
        // Los clics sobre puntos que ya estaban: el círculo pasa por ellos
        const on = [...new Set(hits.flatMap((h) => (h.id !== undefined && sk.points.some((q) => q.id === h.id) ? [h.id] : [])))];
        for (const p of on) sk.constraints.push({ type: "point_on_circle", point: p, circle: c });
        if (t === "circle2" && on.length === 2) {
          // Por dos puntos: el centro en el medio (diámetro de construcción)
          const d = ui.addEntity(sk, { type: "line", start: on[0], end: on[1] });
          sk.entities.find((x) => x.id === d)!.construction = true;
          sk.constraints.push({ type: "midpoint", point: center, line: d });
        } else if (on.length < (t === "circle2" ? 2 : 3)) dims.push(dim(sk, { type: "diameter", entity: c, value: round(2 * cc.r) }));
      });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "circle_tan") {
      // Las líneas o curvas bajo el cursor (no los puntos)
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      const g = target !== undefined ? s.sketch.entities.find((x) => x.id === target)?.geometry : undefined;
      if (target === undefined || !g || !["line", "circle", "arc"].includes(g.type)) return ui.setMessage("Clic sobre una línea, círculo o arco");
      const picks = [...tanPicks().filter((x) => x.entity !== target), { entity: target, p: raw }];
      if (picks.length < 3) {
        setTanPicks(picks);
        return ui.setMessage(`Elegidas ${picks.length} de 3`);
      }
      // Arranque: el círculo por los puntos de cada una más cercanos a los clics (cerca de donde toca)
      const q = picks.map((x) => nearestOnEntity(s.sketch, x.entity, x.p)!);
      const cc = circumcircle(q[0], q[1], q[2]);
      setTanPicks([]);
      if (!cc) return ui.setMessage("No se puede arrancar con esos clics: hacer clic cerca de donde el círculo toca cada una");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const center = ui.addPoint(sk, cc.c);
        const c = ui.addEntity(sk, { type: "circle", center, radius: cc.r });
        for (const x of picks) sk.constraints.push({ type: "tangent", a: x.entity, b: c });
      });
      return;
    }
    if (t === "rect3") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [A, B] = an;
      const r = rectFrom3(A.p, B.p, hit.p);
      if (!r) return ui.setMessage("El tercer punto da el ancho: moverlo hacia un costado del primer lado");
      ui.setMessage(undefined);
      ui.change((sk) => {
        const pa = placeSnap(sk, A);
        const pb = placeSnap(sk, B);
        const l = addRect3(sk, pa, pb, r[2], r[3]);
        if (A.id === undefined || B.id === undefined) dims.push(dim(sk, { type: "length", line: l[0], value: round(dist(A.p, B.p)) }));
        dims.push(dim(sk, { type: "length", line: l[1], value: round(dist(r[1], r[2])) }));
      });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "slot_center") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit]);
      const [M, B] = an;
      const a: P2 = [2 * M.p[0] - B.p[0], 2 * M.p[1] - B.p[1]];
      const r = slotRadius(a, B.p, hit.p);
      const len = dist(a, B.p);
      if (r > 1e-9 && len > 1e-9)
        ui.change((sk) => {
          const cm = placeSnap(sk, M);
          const cb = placeSnap(sk, B);
          const ca = ui.addPoint(sk, a);
          // Eje de construcción con el centro en el medio
          const axis = ui.addEntity(sk, { type: "line", start: ca, end: cb });
          sk.entities.find((x) => x.id === axis)!.construction = true;
          sk.constraints.push({ type: "midpoint", point: cm, line: axis });
          const { arcA } = addSlot(sk, ca, cb, r);
          if (M.id === undefined || B.id === undefined) dims.push(dim(sk, { type: "length", line: axis, value: round(len) }));
          dims.push(dim(sk, { type: "radius", entity: arcA, value: round(r) }));
        });
      setAnchor([]);
      return askDims(dims);
    }
    if (t === "slot_arc") {
      const an = anchor();
      if (an.length < 3) return setAnchor([...an, hit]);
      const [C, S, E] = an;
      const R = dist(C.p, S.p);
      const w = Math.abs(dist(C.p, hit.p) - R);
      const ang = Math.atan2(E.p[1] - C.p[1], E.p[0] - C.p[0]);
      const b: P2 = [C.p[0] + R * Math.cos(ang), C.p[1] + R * Math.sin(ang)];
      if (R < 1e-9 || w < 1e-9) return setAnchor([]);
      let msg: string | undefined;
      ui.change((sk) => {
        const center = placeSnap(sk, C);
        const start = placeSnap(sk, S);
        // El fin solo se pega a un punto que ya estaba (el resto queda en el radio)
        const end = E.id !== undefined && E.id !== start && E.id !== center && sk.points.some((q) => q.id === E.id) ? E.id : ui.addPoint(sk, b);
        const r = addArcSlot(sk, center, start, end, w);
        if (typeof r === "string") return void (msg = r);
        if (C.id === undefined || S.id === undefined) dims.push(dim(sk, { type: "radius", entity: r.axis, value: round(R) }));
        dims.push(dim(sk, { type: "radius", entity: r.capStart, value: round(w) }));
      });
      ui.setMessage(msg);
      setAnchor([]);
      return askDims(dims);
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
    if (t === "split") {
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      if (target === undefined) return ui.setMessage("Clic sobre la línea, arco o círculo que se parte");
      let msg: string | undefined;
      // En un anclaje (un cruce, el punto medio) se parte justo ahí
      ui.change((sk) => {
        const r = splitEntityAt(sk, target, hit.kind !== "free" ? hit.p : raw);
        if (typeof r === "string") msg = r;
      });
      ui.setMessage(msg);
      return;
    }
    if (t === "paste") {
      ui.paste(hit.p);
      ui.setTool("select");
      return;
    }
    if (TRANSFORMS.includes(t)) {
      if (!ui.selection().length) return ui.setMessage("Elegir primero (con «Elegir») lo que se transforma");
      const pts = anchor().map((a) => a.p);
      const xf = transformOf(t, pts, hit.p);
      if (!xf || pts.length < (t === "move" || t === "copy" ? 1 : 2)) {
        // Rotar o escalar con el segundo clic en el primero: no hay "desde"
        if (pts.length === 1 && t !== "move" && t !== "copy" && dist(pts[0], hit.p) < 1e-9) return;
        return setAnchor([...anchor(), hit]);
      }
      ui.setMessage(ui.transformSelected(xf.f, { scale: xf.scale, copy: t === "copy" }));
      setAnchor([]);
      return;
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
      // El enganche a puntos no sirve acá: el trazo sigue al cursor (un clic es un trazo de un punto)
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      trimFrom = { x: e.clientX, y: e.clientY, far: 0 };
      setTrimStroke([raw]);
      return;
    }
    if (t === "stretch") {
      const pts = stretchPts();
      // Primero la caja sobre los puntos que se corren
      if (!pts?.length) {
        boxStart = { x: e.clientX, y: e.clientY, additive: false, before: [] };
        return;
      }
      const an = anchor();
      if (!an.length) return setAnchor([hit]);
      let msg: string | undefined;
      ui.change((sk) => (msg = transformSelection(sk, pts, translation([hit.p[0] - an[0].p[0], hit.p[1] - an[0].p[1]]))));
      ui.setMessage(msg);
      resetTool();
      ui.setSelection([]);
      return;
    }
    if (t === "polygon") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit]);
      const n = Math.max(3, Math.round(polygonSides()));
      const r = dist(an[0].p, hit.p);
      if (r > 1e-9 && polygonCircumscribed())
        ui.change((sk) => {
          const center = placeSnap(sk, an[0]);
          const { circle, lines } = addCircumscribedPolygon(sk, center, hit.p, n);
          // Sobre un punto que ya estaba: el medio del primer lado queda ahí
          if (hit.id !== undefined && hit.id !== center) sk.constraints.push({ type: "midpoint", point: hit.id, line: lines[0] });
          if (an[0].id === undefined || hit.id === undefined) dims.push(dim(sk, { type: "radius", entity: circle, value: round(r) }));
        });
      else if (r > 1e-9)
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
          const ca = placeSnap(sk, an[0]);
          const cb = placeSnap(sk, an[1]);
          const { arcA } = addSlot(sk, ca, cb, r);
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

  /** Sketch 3D en edición: el resuelto de la vista previa (o el del borrador) */
  const sketch3dOf = (feature: number): { sketch: Sketch3d; report?: Report3d } | undefined => {
    const view = store.result()?.sketches3d?.find((v) => v.id === feature);
    if (view) return view;
    const f = store.doc()?.features.find((x) => x.id === feature);
    return f?.kind.type === "sketch3d" ? { sketch: f.kind.sketch } : undefined;
  };
  /** Qué hay bajo el puntero en el sketch 3D: punto, curva, vértice del sólido o el plano activo */
  const hit3d = (e: PointerEvent, feature: number, plane: Plane): Hit3d | undefined => {
    if (!viewer) return undefined;
    const sk = sketch3dOf(feature)?.sketch;
    const near = (p: P3) => {
      const [x, y] = viewer!.screenOf(p);
      return Math.hypot(x - e.clientX, y - e.clientY);
    };
    if (sk) {
      let best: { d: number; id: number; at: P3 } | undefined;
      for (const p of sk.points) {
        const at: P3 = [p.x, p.y, p.z];
        const d = near(at);
        if (d <= 8 && (!best || d < best.d)) best = { d, id: p.id, at };
      }
      if (best) return { at: best.at, point: best.id };
      const at = new Map(sk.points.map((p) => [p.id, [p.x, p.y, p.z] as P3]));
      for (const ent of sk.entities) {
        const g = ent.geometry;
        const pts: P3[] =
          g.type === "line"
            ? [at.get(g.start)!, at.get(g.end)!]
            : g.type === "arc"
              ? arc3Polyline(at.get(g.start)!, at.get(g.mid)!, at.get(g.end)!)
              : g.type === "spline"
                ? g.points.map((q) => at.get(q)!)
                : [];
        if (pts.some((q) => !q)) continue;
        for (let i = 1; i < pts.length; i++) {
          const [ax, ay] = viewer.screenOf(pts[i - 1]);
          const [bx, by] = viewer.screenOf(pts[i]);
          const [dx, dy] = [bx - ax, by - ay];
          const t = Math.max(0, Math.min(1, ((e.clientX - ax) * dx + (e.clientY - ay) * dy) / (dx * dx + dy * dy || 1)));
          if (Math.hypot(ax + t * dx - e.clientX, ay + t * dy - e.clientY) <= 6) {
            const q = pts[i - 1].map((v, k) => v + t * (pts[i][k] - v)) as P3;
            return { at: q, entity: ent.id };
          }
        }
      }
    }
    const v = viewer.pick(e.clientX, e.clientY, { vertices: true });
    if (v?.kind === "vertex") return { at: v.at, vertex: true };
    const p = viewer.planePoint3(e.clientX, e.clientY, plane);
    return p ? { at: p.map((x) => Math.round(x * 1e4) / 1e4) as P3 } : undefined;
  };
  // Sketch 3D en el visor mientras su diálogo está abierto
  createEffect(() => {
    const d = store.draft();
    const f = d ? store.doc()?.features.find((x) => x.id === d.feature) : undefined;
    if (!viewer) return;
    if (!d || f?.kind.type !== "sketch3d") {
      viewer.setSketch3d(null);
      return;
    }
    const view = sketch3dOf(f.id);
    if (!view) return viewer.setSketch3d(null);
    const mode = ui.pick();
    const cursor = mode.kind === "sketch3d" ? ui.cursor3d() : undefined;
    const conflict = (view.report?.conflicting ?? []).flatMap((i) => {
      const c = view.sketch.constraints[i];
      return c && "line" in c ? [c.line] : c && "a" in c && c.type !== "coincident" && c.type !== "distance" ? [c.a, c.b] : [];
    });
    viewer.setSketch3d({
      sketch: view.sketch,
      selected: ui.sel3d(),
      free: view.report?.free_points ?? [],
      conflict,
      plane: mode.kind === "sketch3d" ? mode.plane : undefined,
      preview: mode.kind === "sketch3d" && mode.from && cursor ? [mode.from, cursor] : undefined,
      cursor,
    });
  });

  const pickClick = async (e: PointerEvent) => {
    if (!viewer) return;
    const mode = ui.pick();
    try {
      if (mode.kind === "sketch3d") {
        const hit = hit3d(e, mode.feature, mode.plane);
        if (hit) mode.click(hit, e);
      } else if (mode.kind === "face") {
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
    const mode3 = ui.pick();
    if (mode3.kind === "sketch3d" && e.buttons === 0) ui.setCursor3d(hit3d(e, mode3.feature, mode3.plane)?.at);
    const ts = trimStroke();
    if (ts && trimFrom && (e.buttons & 1) === 1) {
      const ss = ui.session();
      const p = ss && viewer?.planePoint(e.clientX, e.clientY, ss.plane);
      trimFrom.far = Math.max(trimFrom.far, Math.hypot(e.clientX - trimFrom.x, e.clientY - trimFrom.y));
      const last = ts[ts.length - 1];
      if (p && viewer && Math.hypot(p[0] - last[0], p[1] - last[1]) >= viewer.pixelSizeMm() * 3) setTrimStroke([...ts, p]);
      return;
    }
    const ls = lasso();
    if (ls && (e.buttons & 1) === 1) {
      const last = ls[ls.length - 1];
      if (Math.hypot(e.clientX - last[0], e.clientY - last[1]) >= 3) setLasso([...ls, [e.clientX, e.clientY]]);
      return;
    }
    // Preselección: lo que elegiría un clic, resaltado
    const ss = ui.session();
    if (ss && viewer && e.buttons === 0 && PRESELECT.has(ui.tool())) {
      const raw = viewer.planePoint(e.clientX, e.clientY, ss.plane);
      const h = raw ? hitTest(ss.sketch, raw, viewer.pixelSizeMm() * 8, ui.tool() === "select") : {};
      setPreHover(h.point ?? h.entity);
    } else if (preHover() !== undefined) setPreHover(undefined);
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
    if (ui.session() && ui.tool() === "stretch") return stretchBox(s.x, s.y, x, y);
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
  /** Lazo: lo que queda entero adentro (entidades y puntos) */
  const finishLasso = () => {
    const poly = lasso()!;
    setLasso(undefined);
    const s = ui.session();
    if (!s || !viewer || poly.length < 3) return;
    const inside = ([x, y]: [number, number]) => {
      let ins = false;
      for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
        const [a, b] = [poly[i], poly[j]];
        if (a[1] > y !== b[1] > y && x < ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1]) + a[0]) ins = !ins;
      }
      return ins;
    };
    const scr = (p: P2) => viewer!.screenOf(planeToWorld(s.plane, p));
    const point = new Map(s.sketch.points.map((q) => [q.id, [q.x, q.y] as P2]));
    const found: number[] = [];
    for (const e of s.sketch.entities) {
      const pl = e.geometry.type === "point" ? [point.get(e.geometry.point)].filter((p): p is P2 => !!p) : entityPolyline(e.geometry, point);
      if (pl?.length && pl.map(scr).every(inside)) found.push(e.id);
    }
    for (const q of s.sketch.points) if (q.id !== s.sketch.origin && inside(scr([q.x, q.y]))) found.push(q.id);
    ui.setSelection((cur) => (lassoAdditive ? [...new Set([...cur, ...found])] : found));
  };
  /** Estirar: los puntos dentro de la caja (en cualquier sentido) son los que se corren */
  const stretchBox = (x0: number, y0: number, x1: number, y1: number) => {
    const s = ui.session();
    if (!s || !viewer) return;
    const [l, r, t, b] = [Math.min(x0, x1), Math.max(x0, x1), Math.min(y0, y1), Math.max(y0, y1)];
    const found = s.sketch.points
      .filter((q) => q.id !== s.sketch.origin)
      .filter((q) => {
        const [x, y] = viewer!.screenOf(planeToWorld(s.plane, [q.x, q.y]));
        return x >= l && x <= r && y >= t && y <= b;
      })
      .map((q) => q.id);
    if (!found.length) return ui.setMessage("La caja no tomó ningún punto: arrastrarla sobre las puntas que se corren");
    setStretchPts(found);
    ui.setSelection(found);
    ui.setMessage(`${found.length} ${found.length === 1 ? "punto tomado" : "puntos tomados"}: clic en el punto base y después en el destino`);
  };
  /** Recortar: un clic quita el tramo bajo el cursor; un trazo, todo lo que cruza */
  const finishTrim = () => {
    const stroke = trimStroke();
    const from = trimFrom;
    setTrimStroke(undefined);
    trimFrom = undefined;
    const s = ui.session();
    if (!stroke?.length || !s || !viewer) return;
    let msg: string | undefined;
    if (!from || from.far < 5 || stroke.length < 2) {
      const target = hitTest(s.sketch, stroke[0], viewer.pixelSizeMm() * 8, false).entity;
      if (target === undefined) return;
      ui.change((sk) => (msg = trimAt(sk, target, stroke[0])));
    } else
      ui.change((sk) => {
        const r = trimByStroke(sk, stroke);
        msg = r.message ?? (r.trimmed ? undefined : "El trazo no cruzó nada que recortar");
      });
    ui.setMessage(msg);
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
    ui.endDrag();
    if (trimStroke()) return finishTrim();
    if (lasso()) return finishLasso();
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
      const sketch = editsAsSketch(f.kind);
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
        else if (ui.tool() === "bspline" && anchor().length >= 2) finishBSpline(false);
        else if (chain() || anchor().length) resetTool();
        else ui.setTool("select");
      } else if (e.key === "Enter") void ui.finishSketch();
      else if (e.key === "Delete" || e.key === "Backspace") ui.deleteSelection();
      else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && key === "c") ui.setMessage(ui.copySelection());
      else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && key === "a") {
        ui.setTool("select");
        resetTool();
        ui.setSelection(selectByKind(ui.session()!.sketch, "all"));
      }
      else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && key === "v") {
        // Pegar: lo copiado sigue al cursor hasta el clic
        if (!ui.clipboard()) return ui.setMessage("No hay nada copiado (Ctrl+C con algo elegido)");
        ui.setTool("paste");
        resetTool();
        ui.setMessage("Clic donde va lo pegado (Esc cancela)");
      }
      else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        // Q: construcción sí/no en lo elegido, o el modo construcción sin nada elegido (como Onshape)
        if (key === "q") {
          ui.construction();
          e.preventDefault();
          e.stopPropagation();
          return;
        }
        // Dibujando líneas encadenadas, A alterna el próximo tramo entre línea y arco tangente
        if (key === "a" && ui.tool() === "line" && chain()?.lastEntity !== undefined) {
          const on = !chainArc();
          setChainArc(on);
          ui.setMessage(on ? "Próximo tramo: arco tangente (A vuelve a línea)" : undefined);
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

  /** Íconos de las restricciones: en fila junto a lo que atan */
  const glyphs = createMemo(() => {
    viewTick();
    const s = ui.session();
    if (!s || !viewer || !ui.sketchShow().constraints) return [];
    const rect = container.getBoundingClientRect();
    const count = new Map<string, number>();
    const sel = new Set(ui.selectedConstraints());
    const conflict = new Set(s.report?.conflicting ?? []);
    return constraintGlyphs(s.sketch)
      .slice(0, MAX_GLYPHS)
      .map((g) => {
        const k = count.get(g.host) ?? 0;
        count.set(g.host, k + 1);
        const [x, y] = viewer!.screenOf(planeToWorld(s.plane, g.at));
        return { ...g, x: x - rect.left + 10 + k * 15, y: y - rect.top + 12, selected: sel.has(g.index), conflict: conflict.has(g.index) };
      });
  });

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
    /** Pie de la perpendicular desde un punto a la recta de una línea */
    const foot = (point: number, line: number): P2 | undefined => {
      const g = ent.get(line);
      const [p, a, b] = [pt.get(point), g?.type === "line" ? pt.get(g.start) : undefined, g?.type === "line" ? pt.get(g.end) : undefined];
      if (!p || !a || !b) return undefined;
      const d: P2 = [b[0] - a[0], b[1] - a[1]];
      const t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1] || 1);
      return [a[0] + t * d[0], a[1] + t * d[1]];
    };
    /** Medio de un arco */
    const arcMid = (id: number): P2 | undefined => {
      const g = ent.get(id);
      if (g?.type !== "arc") return undefined;
      const [k, a, b] = [pt.get(g.center), pt.get(g.start), pt.get(g.end)];
      if (!k || !a || !b) return undefined;
      const a0 = Math.atan2(a[1] - k[1], a[0] - k[0]);
      let sweep = Math.atan2(b[1] - k[1], b[0] - k[0]) - a0;
      if (sweep <= 0) sweep += 2 * Math.PI;
      const r = dist(k, a);
      return [k[0] + r * Math.cos(a0 + sweep / 2), k[1] + r * Math.sin(a0 + sweep / 2)];
    };
    /** Un lugar sobre la curva (línea, arco o círculo) */
    const curveMid = (id: number): P2 | undefined => {
      const g = ent.get(id);
      if (g?.type === "line") return lineMid(id);
      if (g?.type === "arc") return arcMid(id);
      if (g?.type === "circle") {
        const k = pt.get(g.center);
        return k && [k[0] + g.radius * Math.SQRT1_2, k[1] + g.radius * Math.SQRT1_2];
      }
      return undefined;
    };
    /** Punto, medio de una línea o centro de un círculo (para las cotas con círculos) */
    const targetAt = (id: number): P2 | undefined => {
      const g = ent.get(id);
      if (!g) return pt.get(id);
      if (g.type === "line") return lineMid(id);
      if (g.type === "point") return pt.get(g.point);
      return "center" in g ? pt.get(g.center) : undefined;
    };
    const anchor = (c: SketchConstraint): P2 | undefined => {
      switch (c.type) {
        case "length":
          return lineMid(c.line);
        case "angle":
          return lineMid(c.a);
        case "horizontal_distance":
        case "vertical_distance":
          // Ordenadas: junto al punto medido
          if (c.opts?.ordinate) return pt.get(c.b);
          return mid(pt.get(c.a), pt.get(c.b));
        case "distance":
          return mid(pt.get(c.a), pt.get(c.b));
        case "curve_length":
          return curveMid(c.entities[0]);
        case "circle_distance":
          return mid(targetAt(c.a), targetAt(c.b));
        case "point_line_distance":
        case "axis_diameter":
          return mid(pt.get(c.point), foot(c.point, c.line));
        case "arc_length":
          return arcMid(c.arc);
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
    const out: DimView[] = [];
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
      if (c.type === "point_line_distance" || c.type === "axis_diameter") {
        const [a, b] = [pt.get(c.point), foot(c.point, c.line)];
        return a && b && dist(a, b) > 1e-9 ? [a, b] : undefined;
      }
      return undefined;
    };
    const showDims = ui.sketchShow().dims;
    s.sketch.constraints.forEach((c, index) => {
      const v = constraintValue(c);
      const a = anchor(c);
      // Ocultas: menos la que se está escribiendo
      if (v === undefined || !a || (!showDims && editingDim() !== index)) return;
      let [x, y] = viewer!.screenOf(planeToWorld(s.plane, a));
      const opts = "opts" in c ? c.opts : undefined;
      // Texto movido a mano (o mientras se arrastra): donde se dejó, con una guía hasta lo acotado
      const drag = dimDrag();
      const offset = drag?.index === index ? drag.offset : opts?.offset;
      let leader: [number, number] | undefined;
      const e2 = ends(c);
      if (offset) {
        leader = [x - rect.left, y - rect.top];
        [x, y] = viewer!.screenOf(planeToWorld(s.plane, [a[0] + offset[0], a[1] + offset[1]]));
      } else if (e2) {
        // Correr 22 px perpendicular a lo acotado, hacia afuera del sketch, para
        // no tapar la línea (y poder elegirla con un clic)
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
      const prefix = DIM_PREFIX[c.type]?.(c) ?? "";
      const suffix = c.type === "angle" ? "°" : "";
      const expr = (c as { expr?: string }).expr?.trim();
      const reference = isReference(c);
      const value = `${prefix}${+v.toFixed(3)}${suffix}`;
      // Nombre: el parámetro si la fórmula es solo eso; si no, d1, d2… por orden
      const name = expr && /^[\p{L}_][\p{L}\p{N}_]*$/u.test(expr) ? expr : `d${index + 1}`;
      const mode = ui.dimLabel();
      const shown = mode === "name" ? `${prefix}${name}` : mode === "expr" && expr && !reference ? `${prefix}${expr} = ${+v.toFixed(3)}${suffix}` : value;
      out.push({
        index,
        x: x - rect.left,
        y: y - rect.top,
        // Las de referencia entre paréntesis, como Onshape
        text: reference ? `(${shown})` : shown,
        conflict: s.report?.conflicting.includes(index) ?? false,
        reference,
        locked: !!opts?.locked,
        leader,
        anchor: a,
        offset,
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

  /**
   * Arrastrar el texto de una cota: el corrimiento se guarda en mm del plano
   * (sigue a lo acotado si la geometría se mueve). Un clic sin moverse edita.
   */
  const startDimDrag = (e: PointerEvent, d: DimView) => {
    e.stopPropagation();
    dimDragged = false;
    const s = ui.session();
    if (e.button !== 0 || !s || !viewer || ui.tool() !== "select") return;
    const rect = container.getBoundingClientRect();
    // Paso de px a mm del plano cerca del punto acotado (inversa de la jacobiana)
    const at = (p: P2) => viewer!.screenOf(planeToWorld(s.plane, p));
    const o = at(d.anchor);
    const ux = at([d.anchor[0] + 1, d.anchor[1]]);
    const uy = at([d.anchor[0], d.anchor[1] + 1]);
    const [a, b, c, k] = [ux[0] - o[0], uy[0] - o[0], ux[1] - o[1], uy[1] - o[1]];
    const det = a * k - b * c;
    if (Math.abs(det) < 1e-12) return;
    const toPlane = (dx: number, dy: number): [number, number] => [(k * dx - b * dy) / det, (-c * dx + a * dy) / det];
    // Donde está ahora el texto (sin mover, va corrido de lo acotado)
    const start = d.offset ?? toPlane(d.x + rect.left - o[0], d.y + rect.top - o[1]);
    const [sx, sy] = [e.clientX, e.clientY];
    const move = (ev: PointerEvent) => {
      const [dx, dy] = [ev.clientX - sx, ev.clientY - sy];
      if (!dimDragged && Math.hypot(dx, dy) < 4) return;
      dimDragged = true;
      const m = toPlane(dx, dy);
      setDimDrag({ index: d.index, offset: [+(start[0] + m[0]).toFixed(4), +(start[1] + m[1]).toFixed(4)] });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const moved = dimDrag();
      setDimDrag(undefined);
      if (dimDragged && moved) ui.setDimOpts(moved.index, { offset: moved.offset });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Clic derecho en una cota: bloquear, devolver el texto, impulsora o de referencia, quitar */
  const dimMenu = (x: number, y: number, d: DimView) => {
    const c = ui.session()?.sketch.constraints[d.index];
    if (!c) return;
    const sep: MenuEntry = { separator: true };
    const flippable = c.type === "angle";
    setMenu({
      x,
      y,
      items: [
        { header: CONSTRAINT_LABELS[c.type] },
        { label: d.locked ? "Desbloquear" : "Bloquear", onSelect: () => ui.setDimOpts(d.index, { locked: !d.locked }) },
        ...(d.offset ? [{ label: "Devolver el texto a su lugar", onSelect: () => ui.setDimOpts(d.index, { offset: undefined }) }] : []),
        ...(flippable
          ? [
              {
                label: c.supplementary ? "Ángulo (no suplementario)" : "Ángulo suplementario",
                onSelect: () =>
                  ui.change((sk) => {
                    const k = sk.constraints[d.index];
                    if (k?.type !== "angle") return;
                    // El mismo dibujo: el ángulo hasta la otra punta de la segunda línea
                    if (k.supplementary) delete k.supplementary;
                    else k.supplementary = true;
                    const v = measureConstraint(sk, k);
                    if (v !== undefined && !(k as { expr?: string }).expr) k.degrees = +v.toFixed(6);
                  }),
              },
            ]
          : []),
        sep,
        { label: "Quitar", onSelect: () => ui.removeConstraint(d.index) },
      ],
    });
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

      {/* Íconos de restricciones: clic elige (Mayús suma), Supr borra */}
      <Index each={glyphs()}>
        {(g) => (
          <button
            data-constraint-glyph={g().index}
            class={clsx(
              "absolute -translate-x-1/2 -translate-y-1/2 min-w-4 h-4 px-0.5 rounded text-[10px] leading-none border flex items-center justify-center",
              g().selected
                ? "bg-cyan text-bg border-cyan"
                : g().conflict
                  ? "bg-bg/90 text-error border-error"
                  : "bg-bg/80 text-text-muted border-border hover:text-text hover:border-cyan",
              ui.tool() !== "select" && "pointer-events-none",
            )}
            style={{ left: `${g().x}px`, top: `${g().y}px` }}
            title={CONSTRAINT_LABELS[ui.session()?.sketch.constraints[g().index]?.type ?? "coincident"]}
            onPointerDown={(e) => e.stopPropagation()}
            onMouseEnter={() => {
              const c = ui.session()?.sketch.constraints[g().index];
              ui.setHoverIds(c ? constraintIds(c) : []);
            }}
            onMouseLeave={() => ui.setHoverIds([])}
            onClick={(e) => {
              const i = g().index;
              if (e.shiftKey) ui.setSelectedConstraints((cur) => (cur.includes(i) ? cur.filter((x) => x !== i) : [...cur, i]));
              else {
                ui.setSelectedConstraints([i]);
                ui.setSelection([]);
              }
            }}
          >
            {g().glyph}
          </button>
        )}
      </Index>

      {/* Lazo libre mientras se arrastra */}
      <Show when={lasso()}>
        {(ls) => {
          const r = () => container.getBoundingClientRect();
          return (
            <svg class="absolute inset-0 w-full h-full pointer-events-none" data-lasso>
              <polygon points={ls().map(([x, y]) => `${x - r().left},${y - r().top}`).join(" ")} class="fill-cyan/10 stroke-cyan" stroke-dasharray="4 3" />
            </svg>
          );
        }}
      </Show>

      {/* Guías de las cotas con el texto movido a mano */}
      <svg class="absolute inset-0 w-full h-full pointer-events-none" data-dim-leaders>
        <For each={dimensions().filter((d) => d.leader)}>
          {(d) => <line x1={d.leader![0]} y1={d.leader![1]} x2={d.x} y2={d.y} class="stroke-text-muted" stroke-width="1" stroke-dasharray="3 2" />}
        </For>
      </svg>

      {/* Cotas del sketch: clic para cambiar el valor, arrastrar para mover el texto */}
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
                data-dim={d().index}
                onPointerDown={(e) => startDimDrag(e, d())}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  dimMenu(e.clientX, e.clientY, d());
                }}
                onClick={() => {
                  if (dimDragged) return void (dimDragged = false);
                  // Una cota de referencia no se escribe: mide; una bloqueada, tampoco
                  if (d().locked) ui.setMessage("Cota bloqueada: clic derecho para desbloquearla");
                  else if (!d().reference) setEditingDim(d().index);
                }}
              >
                <Show when={d().locked}>
                  <Icons.Lock size={10} class="inline -mt-0.5 mr-0.5" />
                </Show>
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
              <For each={toolSlots()}>
                {(slot, i) => {
                  const shown = () => TOOLS.find((t) => t.id === (slot.family ? (familyPick()[slot.family] ?? slot.tools[0].id) : slot.tools[0].id))!;
                  const variants = slot.tools;
                  return (
                    <>
                      <Show when={i() > 0 && toolSlots()[i() - 1].group !== slot.group}>
                        <div class="w-px h-5 bg-border mx-0.5" />
                      </Show>
                      <div class="relative flex items-center">
                        <Tooltip content={shown().key ? `${shown().label} (${shown().key})` : shown().label}>
                          <button
                            aria-label={shown().short}
                            class={clsx(
                              "p-1 rounded",
                              variants.some((t) => t.id === ui.tool()) ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface",
                            )}
                            onClick={() => {
                              ui.setTool(shown().id);
                              resetTool();
                            }}
                          >
                            {(() => {
                              const Icon = shown().icon;
                              return <Icon size={18} />;
                            })()}
                            {/* El nombre para lectores de pantalla (y las pruebas que buscan por texto) */}
                            <span class="sr-only">{shown().short}</span>
                          </button>
                        </Tooltip>
                        <Show when={variants.length > 1}>
                          <div data-toolbar-menu={FAMILY_MENU[slot.family!]} data-items={JSON.stringify(variants.map((t) => t.short))}>
                            <button
                              class="px-0.5 py-1 rounded text-text-dim hover:text-text hover:bg-surface"
                              onClick={() => setOpenFamily(openFamily() === slot.family ? undefined : slot.family)}
                            >
                              <Icons.CaretDown size={10} />
                              <span class="sr-only">{FAMILY_MENU[slot.family!]}</span>
                            </button>
                          </div>
                          <Show when={openFamily() === slot.family}>
                            <div class="absolute top-full left-0 mt-1 z-20 min-w-44 rounded-md border border-border bg-bg-lighter shadow-lg py-1" data-family-list>
                              <For each={variants}>
                                {(t) => (
                                  <button
                                    class={clsx(
                                      "w-full flex items-center gap-2 px-2 py-1 text-xs text-left",
                                      ui.tool() === t.id ? "text-accent" : "text-text-muted hover:text-text hover:bg-surface",
                                    )}
                                    title={t.label}
                                    onClick={() => {
                                      ui.setTool(t.id);
                                      resetTool();
                                      setOpenFamily(undefined);
                                    }}
                                  >
                                    <t.icon size={16} />
                                    {t.short}
                                  </button>
                                )}
                              </For>
                            </div>
                          </Show>
                        </Show>
                      </div>
                    </>
                  );
                }}
              </For>
              <div class="w-px h-5 bg-border mx-0.5" />
              <Tooltip content="Construcción (Q): con algo elegido lo pasa a construcción o de vuelta; sin nada elegido, lo que se dibuja sale de construcción (no cuenta para las regiones)">
                <button
                  aria-label="Construcción"
                  class={clsx("p-1 rounded", ui.constructionMode() ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface")}
                  onClick={() => ui.construction()}
                >
                  <SketchIcons.Construction size={18} />
                  <span class="sr-only">Construcción</span>
                </button>
              </Tooltip>
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
                <div class="flex items-center gap-0.5" role="group" aria-label="Estilo del texto">
                  <button
                    aria-pressed={textBold()}
                    title="Negrita (con la fuente incluida)"
                    class={clsx("px-1.5 py-0.5 rounded text-xs font-bold", textBold() ? "bg-accent text-bg" : "text-text-muted hover:bg-surface")}
                    onClick={() => setTextBold(!textBold())}
                  >
                    N
                  </button>
                  <button
                    aria-pressed={textItalic()}
                    title="Cursiva (con la fuente incluida)"
                    class={clsx("px-1.5 py-0.5 rounded text-xs italic", textItalic() ? "bg-accent text-bg" : "text-text-muted hover:bg-surface")}
                    onClick={() => setTextItalic(!textItalic())}
                  >
                    K
                  </button>
                  <For each={[["left", "Izq."], ["center", "Centro"], ["right", "Der."]] as const}>
                    {([k, label]) => (
                      <button
                        aria-pressed={textAlign() === k}
                        title="Alineación respecto del clic"
                        class={clsx("px-1.5 py-0.5 rounded text-[11px]", textAlign() === k ? "bg-accent text-bg" : "text-text-muted hover:bg-surface")}
                        onClick={() => setTextAlign(k)}
                      >
                        {label}
                      </button>
                    )}
                  </For>
                </div>
                <span class="text-[11px] text-text-dim whitespace-nowrap" title="Con una curva elegida antes de elegir Texto, las letras la siguen desde el punto más cercano al clic">
                  {textCurve() !== undefined ? "Sobre la curva elegida" : "En línea recta"}
                </span>
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
              <Show when={ui.tool() === "conic"}>
                <label class="flex items-center gap-1 text-xs text-text-muted" title="0,5 = parábola; menos, arco de elipse; más, hipérbola">
                  rho
                  <input
                    type="number"
                    min="0.01"
                    max="0.99"
                    step="0.05"
                    value={conicRho()}
                    aria-label="Factor rho"
                    class="w-14 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => setConicRho(Math.min(0.99, Math.max(0.01, parseFloat(e.currentTarget.value) || 0.5)))}
                  />
                </label>
              </Show>
              <Show when={ui.tool() === "bspline"}>
                <label class="flex items-center gap-1 text-xs text-text-muted" title="Grado de la curva: 2 cuadrática, 3 cúbica…">
                  Grado
                  <input
                    type="number"
                    min="1"
                    max="7"
                    value={bsplineDegree()}
                    aria-label="Grado"
                    class="w-12 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
                    onChange={(e) => setBsplineDegree(Math.max(1, Math.min(7, parseInt(e.currentTarget.value) || 3)))}
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
                <button
                  class="px-2 py-0.5 rounded text-xs text-text-muted hover:text-text hover:bg-surface border border-border"
                  title="Inscrito: el clic es un vértice. Circunscrito: el clic es el medio de un lado (el círculo toca los lados)"
                  onClick={() => setPolygonCircumscribed(!polygonCircumscribed())}
                >
                  {polygonCircumscribed() ? "Circunscrito" : "Inscrito"}
                </button>
              </Show>
              <div class="w-px h-5 bg-border mx-1" />
              <Tooltip content="Deshacer en el sketch (Ctrl+Z)">
                <IconButton aria-label="Deshacer en el sketch" size="sm" disabled={!ui.sketchHistory().undo} onClick={() => ui.undoSketch()}>
                  <Icons.ArrowCounterClockwise size={15} />
                </IconButton>
              </Tooltip>
              <Tooltip content="Rehacer en el sketch (Ctrl+Shift+Z)">
                <IconButton aria-label="Rehacer en el sketch" size="sm" disabled={!ui.sketchHistory().redo} onClick={() => ui.redoSketch()}>
                  <Icons.ArrowClockwise size={15} />
                </IconButton>
              </Tooltip>
              <div class="w-px h-5 bg-border mx-1" />
              <span
                class={clsx(
                  "text-xs px-1",
                  s().report?.status === "well_constrained" ? "text-success" : s().report?.status === "under_constrained" ? "text-text-muted" : "text-error",
                )}
              >
                {statusText()}
              </span>
              <Show when={problems() && problemsText(problems()!)}>
                {(t) => (
                  <span class="text-xs px-1 text-error" data-sketch-problems title="Marcados en rojo en el visor: así no se cierran las regiones (no se puede extruir esa parte)">
                    {t()}
                  </span>
                )}
              </Show>
              <div class="w-px h-5 bg-border mx-1" />
              <For each={[["dims", "Cotas"], ["constraints", "Restric."], ["construction", "Constr."], ["points", "Puntos"], ["freedom", "Libres"]] as const}>
                {([k, label]) => (
                  <button
                    aria-label={`Mostrar ${label.toLowerCase()}`}
                    aria-pressed={ui.sketchShow()[k]}
                    class={clsx("px-1.5 py-0.5 rounded text-[11px]", ui.sketchShow()[k] ? "text-text hover:bg-surface" : "text-text-dim line-through hover:bg-surface")}
                    title={ui.sketchShow()[k] ? `Ocultar ${label.toLowerCase()}` : `Mostrar ${label.toLowerCase()}`}
                    onClick={() => ui.toggleShow(k)}
                  >
                    {label}
                  </button>
                )}
              </For>
              <div class="w-px h-5 bg-border mx-1" />
              <button
                aria-label="Inferencias"
                aria-pressed={ui.assist().infer}
                class={clsx("px-1.5 py-0.5 rounded text-[11px]", ui.assist().infer ? "text-text hover:bg-surface" : "text-text-dim line-through hover:bg-surface")}
                title={ui.assist().infer ? "Apagar anclajes e inferencias (Mayús los apaga para un clic)" : "Prender anclajes e inferencias"}
                onClick={() => ui.setAssist({ infer: !ui.assist().infer })}
              >
                Inferir
              </button>
              <button
                aria-label="Rejilla del sketch"
                aria-pressed={ui.assist().grid}
                class={clsx("px-1.5 py-0.5 rounded text-[11px]", ui.assist().grid ? "text-text hover:bg-surface" : "text-text-dim line-through hover:bg-surface")}
                title="Rejilla del sketch (paso y anclaje en el panel)"
                onClick={() => ui.setAssist({ grid: !ui.assist().grid })}
              >
                Rejilla
              </button>
              <input
                aria-label="Coordenadas del próximo punto"
                class="w-28 px-1.5 py-0.5 rounded bg-surface border border-border text-[11px] font-mono"
                placeholder="x, y · @dx, dy · @d<á"
                title="Próximo punto escrito: x, y (absoluto) · @dx, dy (desde el último) · @d<ángulo (polar). Enter lo pone."
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key !== "Enter") return;
                  const msg = typedPoint(e.currentTarget.value);
                  ui.setMessage(msg);
                  if (!msg) e.currentTarget.value = "";
                }}
              />
              <div role="radiogroup" aria-label="Qué dicen las cotas" class="flex items-center gap-0.5 ml-0.5">
                <For each={[["value", "Valor", "Las cotas muestran su valor"], ["name", "Nombre", "Las cotas muestran su nombre (d1, d2… o el parámetro)"], ["expr", "Fórmula", "Las cotas con fórmula la muestran junto al valor"]] as const}>
                  {([k, label, tip]) => (
                    <button
                      role="radio"
                      aria-checked={ui.dimLabel() === k}
                      title={tip}
                      class={clsx("px-1.5 py-0.5 rounded text-[11px]", ui.dimLabel() === k ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface")}
                      onClick={() => ui.setDimLabel(k)}
                    >
                      {label}
                    </button>
                  )}
                </For>
              </div>
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
