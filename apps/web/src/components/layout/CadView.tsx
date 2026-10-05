import { Component, For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { CadViewer, planeToWorld } from "../../lib/CadViewer";
import { constraintValue, splitLineAt, trimLine, type CadStore, type P2, type Sketch, type SketchConstraint } from "../../lib/cad";
import type { CadUi, SketchTool } from "../../lib/cadUi";
import type { MeshData } from "../../lib/Viewer3D";
import { Button, IconButton, Slider, Tooltip } from "../ui";
import * as Icons from "../icons";

export interface CadViewProps {
  store: CadStore;
  ui: CadUi;
  /** Modelo cargado, de referencia (escaneo a calcar) */
  scanMesh?: MeshData | null;
}

const TOOLS: { id: SketchTool; label: string; key: string }[] = [
  { id: "select", label: "Elegir y arrastrar", key: "S" },
  { id: "line", label: "Línea", key: "L" },
  { id: "rect", label: "Rectángulo", key: "R" },
  { id: "circle", label: "Círculo", key: "C" },
  { id: "arc", label: "Arco (centro, inicio, fin)", key: "A" },
  { id: "polygon", label: "Polígono regular (centro, vértice)", key: "P" },
  { id: "slot", label: "Ranura (centro, centro, ancho)", key: "U" },
  { id: "trim", label: "Recortar (clic en el tramo a quitar)", key: "T" },
];

const dist = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]);

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
    else {
      const pts = g.points.map((x) => pt.get(x)!);
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
  const [chain, setChain] = createSignal<{ first: number; last: number }>();
  const [anchor, setAnchor] = createSignal<P2[]>([]);
  const [polygonSides, setPolygonSides] = createSignal(6);
  const [scanVisible, setScanVisible] = createSignal(true);
  const [scanOpacity, setScanOpacity] = createSignal(0.35);
  let dragging: number | undefined;
  // Cambia con cada cuadro dibujado: las cotas HTML siguen a la cámara
  const [viewTick, setViewTick] = createSignal(0);
  const [editingDim, setEditingDim] = createSignal<number>();

  const ui = props.ui;
  const store = props.store;

  const resetTool = () => {
    setChain(undefined);
    setAnchor([]);
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
    viewer?.setHighlight(h.faces, h.edges);
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
        if (feature !== undefined && s) viewer.lookAtPlane(s.plane);
        else viewer.resetUp();
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
      // Sketch seleccionado (sin editar): se muestra igual
      const sel = store.selected();
      const view = sel !== undefined ? store.sketchView(sel) : undefined;
      viewer.setSketch(view ? { plane: view.plane, sketch: view.sketch, regions: view.regions } : null);
      return;
    }
    const c = cursor();
    const preview: P2[][] = [];
    const pt = (id: number): P2 | undefined => {
      const p = s.sketch.points.find((q) => q.id === id);
      return p ? [p.x, p.y] : undefined;
    };
    const ch = chain();
    const an = anchor();
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
      if (t === "slot" && an.length === 1) preview.push([an[0], c]);
      if (t === "slot" && an.length === 2) preview.push(slotOutline(an[0], an[1], slotRadius(an[0], an[1], c)));
    }
    viewer.setSketch({
      plane: s.plane,
      sketch: s.sketch,
      regions: s.regions,
      selected: ui.selection(),
      freePoints: s.report?.free_points,
      preview,
    });
  });

  /** Posición del cursor sobre el plano, enganchada a un punto cercano */
  const snapped = (e: PointerEvent): { p: P2; id?: number; on?: { entity: number; kind: "line" | "circle" } } | undefined => {
    const s = ui.session();
    if (!s || !viewer) return undefined;
    const p = viewer.planePoint(e.clientX, e.clientY, s.plane);
    if (!p) return undefined;
    const tol = viewer.pixelSizeMm() * 8;
    const hit = hitTest(s.sketch, p, tol);
    if (hit.point !== undefined) {
      const q = s.sketch.points.find((x) => x.id === hit.point)!;
      return { p: [q.x, q.y], id: q.id };
    }
    // Sobre una línea o un círculo: el punto queda pegado a la curva
    if (hit.entity !== undefined) {
      const g = s.sketch.entities.find((x) => x.id === hit.entity)!.geometry;
      const pt = (id: number): P2 => {
        const q = s.sketch.points.find((x) => x.id === id)!;
        return [q.x, q.y];
      };
      if (g.type === "line") {
        const [a, b] = [pt(g.start), pt(g.end)];
        const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
        const t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy || 1);
        return { p: [a[0] + t * dx, a[1] + t * dy], on: { entity: hit.entity, kind: "line" } };
      }
      if (g.type === "circle" || g.type === "arc") {
        const c = pt(g.center);
        const r = g.type === "circle" ? g.radius : dist(c, pt(g.start));
        const d = dist(p, c) || 1;
        return { p: [c[0] + ((p[0] - c[0]) * r) / d, c[1] + ((p[1] - c[1]) * r) / d], on: { entity: hit.entity, kind: "circle" } };
      }
    }
    return { p };
  };

  /** Punto nuevo (o el existente enganchado), pegado a la curva si cayó sobre una */
  const placePoint = (sk: Sketch, hit: { p: P2; id?: number; on?: { entity: number; kind: "line" | "circle" } }): number => {
    if (hit.id !== undefined) return hit.id;
    const id = ui.addPoint(sk, hit.p);
    // Sobre una línea: se parte ahí (el cruce cierra regiones)
    if (hit.on?.kind === "line") splitLineAt(sk, hit.on.entity, id);
    if (hit.on?.kind === "circle") sk.constraints.push({ type: "point_on_circle", point: id, circle: hit.on.entity });
    return id;
  };

  /** Restricción horizontal/vertical automática para líneas casi alineadas */
  const autoAxis = (sk: Sketch, line: number, a: P2, b: P2) => {
    const ang = Math.abs(Math.atan2(b[1] - a[1], b[0] - a[0]));
    const lim = (3 * Math.PI) / 180;
    if (Math.min(ang, Math.PI - ang) < lim) sk.constraints.push({ type: "horizontal", line });
    else if (Math.abs(ang - Math.PI / 2) < lim) sk.constraints.push({ type: "vertical", line });
  };

  const sketchClick = (e: PointerEvent) => {
    const s = ui.session();
    const hit = snapped(e);
    if (!s || !hit || !viewer) return;
    const t = ui.tool();
    if (t === "select") {
      const h = hitTest(s.sketch, hit.p, viewer.pixelSizeMm() * 8);
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
    if (t === "line") {
      const ch = chain();
      let closed = false;
      ui.change((sk) => {
        const id = placePoint(sk, hit);
        if (!ch) {
          setChain({ first: id, last: id });
          return;
        }
        if (id === ch.last) return;
        const a = sk.points.find((q) => q.id === ch.last)!;
        const line = ui.addEntity(sk, { type: "line", start: ch.last, end: id });
        autoAxis(sk, line, [a.x, a.y], hit.p);
        closed = id === ch.first;
        setChain(closed ? undefined : { first: ch.first, last: id });
      });
      return;
    }
    if (t === "rect") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit.p]);
      if (dist(an[0], hit.p) > 1e-9) ui.change((sk) => ui.addRectangle(sk, an[0], hit.p));
      return setAnchor([]);
    }
    if (t === "circle") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit.p]);
      const r = dist(an[0], hit.p);
      if (r > 1e-9) ui.change((sk) => ui.addEntity(sk, { type: "circle", center: ui.addPoint(sk, an[0]), radius: r }));
      return setAnchor([]);
    }
    if (t === "trim") {
      // El enganche a puntos no sirve acá: la línea bajo el cursor
      const raw = viewer.planePoint(e.clientX, e.clientY, s.plane) ?? hit.p;
      const target = hitTest(s.sketch, raw, viewer.pixelSizeMm() * 8, false).entity;
      if (target === undefined) return;
      let msg: string | undefined;
      ui.change((sk) => (msg = trimLine(sk, target, raw)));
      ui.setMessage(msg);
      return;
    }
    if (t === "polygon") {
      const an = anchor();
      if (an.length === 0) return setAnchor([hit.p]);
      const n = Math.max(3, Math.round(polygonSides()));
      const r = dist(an[0], hit.p);
      if (r > 1e-9)
        ui.change((sk) => {
          const center = ui.addPoint(sk, an[0]);
          const circle = ui.addEntity(sk, { type: "circle", center, radius: r });
          sk.entities.find((e) => e.id === circle)!.construction = true;
          const pts = polygonPoints(an[0], hit.p, n).map((p) => ui.addPoint(sk, p));
          const lines = pts.map((p, i) => ui.addEntity(sk, { type: "line", start: p, end: pts[(i + 1) % n] }));
          pts.forEach((p) => sk.constraints.push({ type: "point_on_circle", point: p, circle }));
          for (let i = 1; i < n; i++) sk.constraints.push({ type: "equal", a: lines[0], b: lines[i] });
        });
      return setAnchor([]);
    }
    if (t === "slot") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit.p]);
      const [a, b] = an;
      const r = slotRadius(a, b, hit.p);
      const len = dist(a, b);
      if (r > 1e-9 && len > 1e-9)
        ui.change((sk) => {
          const n: P2 = [-(b[1] - a[1]) / len, (b[0] - a[0]) / len];
          const off = (p: P2, s: number): P2 => [p[0] + n[0] * r * s, p[1] + n[1] * r * s];
          const ca = ui.addPoint(sk, a);
          const cb = ui.addPoint(sk, b);
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
        });
      return setAnchor([]);
    }
    if (t === "arc") {
      const an = anchor();
      if (an.length < 2) return setAnchor([...an, hit.p]);
      const [c, a] = an;
      const r = dist(c, a);
      const ang = Math.atan2(hit.p[1] - c[1], hit.p[0] - c[0]);
      const b: P2 = [c[0] + r * Math.cos(ang), c[1] + r * Math.sin(ang)];
      ui.change((sk) => {
        ui.addEntity(sk, { type: "arc", center: ui.addPoint(sk, c), start: ui.addPoint(sk, a), end: ui.addPoint(sk, b) });
      });
      setAnchor([]);
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
      } else {
        // Sin herramienta: describir lo que se clicó
        const hit = viewer.pick(e.clientX, e.clientY, { faces: true, edges: true });
        if (!hit) {
          ui.setHighlight({ faces: [], edges: [] });
          return ui.setMessage(undefined);
        }
        if (hit.kind === "face") {
          ui.setHighlight({ faces: [hit.face], edges: [] });
          const info = await invoke<{ surface: string; area: number; radius: number | null }>("cad_face_info", { face: hit.face });
          const names: Record<string, string> = { plane: "plana", cylinder: "cilíndrica", cone: "cónica", sphere: "esférica", torus: "tórica" };
          const radius = info.radius != null ? ` · radio ${info.radius.toFixed(3)} mm` : "";
          ui.setMessage(`Cara ${names[info.surface] ?? info.surface} · área ${info.area.toFixed(2)} mm²${radius}`);
        } else if (hit.kind === "edge") {
          ui.setHighlight({ faces: [], edges: [hit.edge] });
          ui.setMessage(`Arista ${hit.edge + 1}`);
        }
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
        if (chain() || anchor().length) resetTool();
        else ui.setTool("select");
      } else if (e.key === "Enter") void ui.finishSketch();
      else if (e.key === "Delete" || e.key === "Backspace") ui.deleteSelection();
      else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        const t = TOOLS.find((x) => x.key.toLowerCase() === key);
        if (!t) return;
        ui.setTool(t.id);
        resetTool();
      } else return;
      e.preventDefault();
      e.stopPropagation();
    } else if (e.key === "Escape" && ui.pick().kind !== "none") {
      ui.cancelPick();
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
    const out: { index: number; x: number; y: number; text: string; conflict: boolean }[] = [];
    s.sketch.constraints.forEach((c, index) => {
      const v = constraintValue(c);
      const a = anchor(c);
      if (v === undefined || !a) return;
      const [x, y] = viewer!.screenOf(planeToWorld(s.plane, a));
      const prefix = c.type === "radius" ? "R " : c.type === "diameter" ? "Ø " : "";
      const suffix = c.type === "angle" ? "°" : "";
      out.push({
        index,
        x: x - rect.left,
        y: y - rect.top,
        text: `${prefix}${+v.toFixed(3)}${suffix}`,
        conflict: s.report?.conflicting.includes(index) ?? false,
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
        onContextMenu={(e) => e.preventDefault()}
      />

      {/* Cotas del sketch: clic para cambiar el valor */}
      <For each={dimensions()}>
        {(d) => (
          <Show
            when={editingDim() === d.index}
            fallback={
              <button
                class={clsx(
                  "absolute -translate-x-1/2 -translate-y-1/2 px-1.5 py-0.5 rounded text-[11px] font-mono border",
                  d.conflict ? "bg-error/20 border-error text-error" : "bg-bg-lighter/90 border-border text-text hover:border-accent",
                )}
                style={{ left: `${d.x}px`, top: `${d.y}px` }}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={() => setEditingDim(d.index)}
              >
                {d.text}
              </button>
            }
          >
            <input
              ref={(el) => setTimeout(() => el.select())}
              type="number"
              step="0.1"
              value={constraintValue(ui.session()!.sketch.constraints[d.index]!) ?? 0}
              class="absolute -translate-x-1/2 -translate-y-1/2 w-20 px-1.5 py-0.5 rounded text-[11px] font-mono bg-bg border border-accent text-text outline-none"
              style={{ left: `${d.x}px`, top: `${d.y}px` }}
              onPointerDown={(e) => e.stopPropagation()}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") setEditingDim(undefined);
                e.stopPropagation();
              }}
              onBlur={(e) => {
                const v = parseFloat(e.currentTarget.value);
                if (editingDim() === d.index && !Number.isNaN(v)) ui.setConstraintValue(d.index, v);
                setEditingDim(undefined);
              }}
            />
          </Show>
        )}
      </For>

      {/* Barra superior */}
      <div class="absolute top-2 left-2 right-2 flex items-start gap-2 pointer-events-none">
        <Show
          when={ui.session()}
          fallback={
            <div class="flex items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto">
              <IconButton aria-label="Encuadrar todo" size="sm" onClick={() => viewer?.frameAll()}>
                <Icons.FrameCorners size={14} />
              </IconButton>
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
            <div class="flex items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto">
              <For each={TOOLS}>
                {(t) => (
                  <Tooltip content={`${t.label} (${t.key})`}>
                    <button
                      class={clsx(
                        "px-2 py-1 rounded text-xs",
                        ui.tool() === t.id ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface",
                      )}
                      onClick={() => {
                        ui.setTool(t.id);
                        resetTool();
                      }}
                    >
                      {t.label.split(" ")[0]}
                    </button>
                  </Tooltip>
                )}
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
            Clic: dibujar · Esc: cortar la herramienta · Supr: borrar lo elegido · Enter: terminar · Alt+arrastrar o botón del medio: girar la
            vista
          </span>
        </Show>
      </div>
    </div>
  );
};
