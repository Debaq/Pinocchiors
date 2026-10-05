import { Component, For, Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { clsx } from "clsx";
import { CadViewer } from "../../lib/CadViewer";
import type { CadStore, P2, Sketch } from "../../lib/cad";
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
function hitTest(s: Sketch, p: P2, tol: number): { point?: number; entity?: number } {
  const pt = new Map(s.points.map((q) => [q.id, [q.x, q.y] as P2]));
  let best: { id: number; d: number } | undefined;
  for (const q of s.points) {
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

export const CadView: Component<CadViewProps> = (props) => {
  let container!: HTMLDivElement;
  let viewer: CadViewer | undefined;
  const [cursor, setCursor] = createSignal<P2>();
  // Estado de la herramienta en curso (clics ya dados)
  const [chain, setChain] = createSignal<{ first: number; last: number }>();
  const [anchor, setAnchor] = createSignal<P2[]>([]);
  const [scanVisible, setScanVisible] = createSignal(true);
  const [scanOpacity, setScanOpacity] = createSignal(0.35);
  let dragging: number | undefined;

  const ui = props.ui;
  const store = props.store;

  const resetTool = () => {
    setChain(undefined);
    setAnchor([]);
  };

  onMount(async () => {
    viewer = new CadViewer(container);
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
  createEffect(() => viewer?.setBody(store.mesh()));
  createEffect(() => {
    const h = ui.highlight();
    viewer?.setHighlight(h.faces, h.edges);
  });
  createEffect(() => viewer?.setScanHighlight(ui.scanHighlight()));
  createEffect(() => viewer?.setScanVisible(scanVisible(), scanOpacity()));

  // Sketch: vista de frente al entrar, normal al salir
  createEffect(
    on(
      () => ui.session()?.feature,
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
      if (t === "line" && ch) preview.push([pt(ch.last)!, c]);
      if (t === "rect" && an.length === 1) {
        const [a] = an;
        preview.push([a, [c[0], a[1]], c, [a[0], c[1]], a]);
      }
      if (t === "circle" && an.length === 1) {
        const r = dist(an[0], c);
        preview.push(Array.from({ length: 49 }, (_, i) => [an[0][0] + r * Math.cos((i / 48) * 2 * Math.PI), an[0][1] + r * Math.sin((i / 48) * 2 * Math.PI)] as P2));
      }
      if (t === "arc" && an.length >= 1) preview.push([an[0], an.length === 2 ? an[1] : c]);
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
  const snapped = (e: PointerEvent): { p: P2; id?: number } | undefined => {
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
    return { p };
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
        const id = hit.id ?? ui.addPoint(sk, hit.p);
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
