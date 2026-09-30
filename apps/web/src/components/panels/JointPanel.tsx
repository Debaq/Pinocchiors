import { Component, For, Index, Show, createMemo, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Panel } from "../ui";
import * as Icons from "../icons";
import { boundaryPoints, type DiskPoint, type JointLimits, type SwingShape } from "../../lib/jointLimits";

/** Un cuadro de la animación de la articulación activa */
export interface TrajectorySample {
  frame: number;
  swing: DiskPoint;
  twist: number;
  hinge: number;
  /** Grados fuera del límite */
  excess: number;
}

export interface ChainView {
  names: string[];
  /** Posiciones en el plano de doblez (posada y en reposo) */
  posed: [number, number][];
  rest: [number, number][];
  /** Rango de cada bisagra dibujado alrededor de su articulación: ángulos en el plano (grados) */
  arcs: { index: number; from: number; to: number }[];
  /** Adónde llega la punta con esos límites */
  reach: [number, number][];
}

export interface DiskSummary {
  name: string;
  limits?: JointLimits;
  swing: DiskPoint;
  hinge: number;
}

export interface RelationNode {
  id: string;
  label: string;
  kind: "bone" | "control" | "chain" | "constraint";
  role: "parent" | "active" | "child" | "control" | "chain";
}

export interface RelationEdge {
  from: string;
  to: string;
  kind: "hierarchy" | "ik" | "follows" | "constraint";
}

export type LimitsAuto = "anatomical" | "mesh" | "observed" | "clear";

export interface JointPanelProps {
  boneName?: string;
  limits?: JointLimits;
  /** Giro actual de la activa */
  current?: { swing: DiskPoint; twist: number; hinge: number };
  trajectory: TrajectorySample[];
  keyFrames: number[];
  limitsOff: boolean;
  posing: boolean;
  canMesh: boolean;
  hasSelection: boolean;
  onLimitsOff: (off: boolean) => void;
  onLimits: (description: string, limits: JointLimits | undefined) => void;
  /** Vista previa mientras se arrastra el límite (se dibuja el cono) */
  onLimitsPreview: (limits: JointLimits | undefined) => void;
  onKind: (kind: "none" | "hinge" | "ball") => void;
  /** Editar la key de `frame`: nuevo swing, twist o ángulo de bisagra */
  onKeyEdit: (frame: number, change: { swing?: DiskPoint; twist?: number; hinge?: number }, commit: boolean) => void;
  onProbe: () => void;
  onAuto: (mode: LimitsAuto) => void;
  chain?: ChainView;
  onChainDrag: (index: number, deltaDegrees: number, commit: boolean) => void;
  disks: DiskSummary[];
  onBatch: (op: "copyActive" | "mirror" | "scale", factor?: number) => void;
  relations: { nodes: RelationNode[]; edges: RelationEdge[] };
  onSelectNode: (node: RelationNode) => void;
  onReparent: (controlId: string, boneName: string | null) => void;
  /** Se soltó un hueso sobre otro: restricción nueva (el soltado sigue al arrastrado) */
  onConnect: (target: string, owner: string) => void;
  shared: { name: string; fraction: number }[];
}

// ─── Utilidades ─────────────────────────────────────────────────────────────

const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

/** Punto del puntero en coordenadas del SVG (viewBox cuadrado de `size`) */
function svgPoint(svg: SVGSVGElement, e: PointerEvent, width: number, height = width): [number, number] {
  const r = svg.getBoundingClientRect();
  return [((e.clientX - r.left) * width) / r.width, ((e.clientY - r.top) * height) / r.height];
}

/** Arrastre con el puntero: `move` con cada movimiento, `up` al soltar */
function drag(e: PointerEvent, move: (e: PointerEvent) => void, up: () => void): void {
  e.preventDefault();
  e.stopPropagation();
  const onMove = (ev: PointerEvent) => move(ev);
  const onUp = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    up();
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

/** Color por tiempo: azul al empezar, amarillo al terminar; rojo si se sale */
const timeColor = (t: number, out: boolean) => (out ? "var(--color-red)" : `hsl(${220 - 170 * t} 80% 65%)`);

const path = (pts: [number, number][], close = true) =>
  pts.length === 0 ? "" : `M ${pts.map((p) => `${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join(" L ")}${close ? " Z" : ""}`;

// ─── Disco de swing con anillo de twist ─────────────────────────────────────

const DISK = 240;
const C = DISK / 2;
const R = 96;

const BallEditor: Component<{
  limits: JointLimits;
  current?: { swing: DiskPoint; twist: number };
  trajectory: TrajectorySample[];
  keyFrames: number[];
  onChange: (limits: JointLimits, commit: boolean) => void;
  onKeyEdit: JointPanelProps["onKeyEdit"];
}> = (props) => {
  let svg: SVGSVGElement | undefined;
  const [draft, setDraft] = createSignal<JointLimits | null>(null);
  const limits = () => draft() ?? props.limits;
  const shape = (): SwingShape => limits().swing ?? { type: "ellipse", rx: 45, ry: 45 };
  // Escala del disco: fija mientras se arrastra, para que no salte
  const [frozen, setFrozen] = createSignal<number | null>(null);
  const range = createMemo(() => {
    if (frozen() !== null) return frozen()!;
    const border = boundaryPoints(props.limits.swing ?? { type: "ellipse", rx: 45, ry: 45 }, 48).map((p) => Math.hypot(...p));
    const traj = props.trajectory.map((s) => Math.hypot(...s.swing));
    return Math.min(180, Math.max(60, ...border.map((r) => r * 1.15), ...traj.map((r) => r * 1.1)));
  });
  const toSvg = (p: DiskPoint): [number, number] => [C + (p[0] * R) / range(), C - (p[1] * R) / range()];
  const fromSvg = (x: number, y: number): DiskPoint => [((x - C) * range()) / R, ((C - y) * range()) / R];

  const commit = (next: JointLimits) => {
    setDraft(null);
    setFrozen(null);
    props.onChange(next, true);
  };
  const edit = (e: PointerEvent, apply: (p: DiskPoint, base: JointLimits) => JointLimits) => {
    setFrozen(range());
    const base = limits();
    let last = base;
    drag(
      e,
      (ev) => {
        const [x, y] = svgPoint(svg!, ev, DISK);
        last = apply(fromSvg(x, y), base);
        setDraft(last);
        props.onChange(last, false);
      },
      () => commit(last)
    );
  };

  const soft = createMemo(() => {
    const s = limits().stiffness ?? 0;
    if (s <= 0) return "";
    const pts = boundaryPoints(shape(), 48).map((p) => toSvg([p[0] * (1 - s), p[1] * (1 - s)]));
    return path(pts);
  });

  /** Ángulo del anillo de twist: 0 arriba, positivo en sentido horario */
  const ringPoint = (t: number, r = R + 13): [number, number] => [C + r * Math.sin(rad(t)), C - r * Math.cos(rad(t))];
  const ringAngle = (x: number, y: number) => deg(Math.atan2(x - C, -(y - C)));
  const twistArc = createMemo(() => {
    const [a, b] = limits().twist ?? [-180, 180];
    const steps = Math.max(2, Math.ceil(Math.abs(b - a) / 5));
    return path(Array.from({ length: steps + 1 }, (_, i) => ringPoint(a + ((b - a) * i) / steps)), false);
  });

  const n = () => Math.max(1, props.trajectory.length - 1);
  const keyed = createMemo(() => new Set(props.keyFrames));

  return (
    <div class="space-y-1">
      <svg
        ref={svg}
        viewBox={`0 0 ${DISK} ${DISK}`}
        class="w-full max-w-[340px] mx-auto block select-none touch-none"
        onDblClick={(e) => {
          // Doble clic en un polígono: un punto nuevo en el lado más cercano
          const s = shape();
          if (s.type !== "polygon") return;
          const [x, y] = svgPoint(svg!, e as unknown as PointerEvent, DISK);
          const p = fromSvg(x, y);
          let best = 0;
          let bestD = Infinity;
          s.points.forEach((a, i) => {
            const b = s.points[(i + 1) % s.points.length];
            const mid: DiskPoint = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
            const d = Math.hypot(mid[0] - p[0], mid[1] - p[1]);
            if (d < bestD) [best, bestD] = [i, d];
          });
          const points = [...s.points];
          points.splice(best + 1, 0, p);
          commit({ ...limits(), swing: { type: "polygon", points } });
        }}
      >
        {/* Grilla: cada 30° */}
        <For each={[30, 60, 90, 120, 150, 180].filter((d) => d <= range())}>
          {(d) => <circle cx={C} cy={C} r={(d * R) / range()} fill="none" stroke="currentColor" class="text-border" stroke-width="0.6" />}
        </For>
        <line x1={C - R} y1={C} x2={C + R} y2={C} stroke="currentColor" class="text-border" stroke-width="0.6" />
        <line x1={C} y1={C - R} x2={C} y2={C + R} stroke="currentColor" class="text-border" stroke-width="0.6" />
        <text x={C + R - 8} y={C - 4} class="fill-text-dim" font-size="9">X</text>
        <text x={C + 4} y={C - R + 9} class="fill-text-dim" font-size="9">Z</text>

        {/* Límite y zona blanda */}
        <path d={path(boundaryPoints(shape(), 64).map(toSvg))} fill="var(--color-accent)" fill-opacity="0.12" stroke="var(--color-accent)" stroke-width="1.5" />
        <Show when={soft()}>
          <path d={soft()} fill="none" stroke="var(--color-accent)" stroke-dasharray="3 3" stroke-width="1" />
        </Show>

        {/* Anillo de twist */}
        <circle cx={C} cy={C} r={R + 13} fill="none" stroke="currentColor" class="text-border" stroke-width="5" />
        <path d={twistArc()} fill="none" stroke="var(--color-accent)" stroke-opacity="0.7" stroke-width="5" />
        <Show when={props.current}>
          {(cur) => {
            const [x1, y1] = ringPoint(cur().twist, R + 8);
            const [x2, y2] = ringPoint(cur().twist, R + 18);
            return <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="var(--color-text)" stroke-width="2" />;
          }}
        </Show>
        <Index each={limits().twist ?? []}>
          {(t, i) => {
            const p = () => ringPoint(t());
            return (
              <circle
                cx={p()[0]}
                cy={p()[1]}
                r="5"
                class="cursor-pointer"
                fill="var(--color-bg)"
                stroke="var(--color-accent)"
                stroke-width="2"
                onPointerDown={(e) =>
                  edit(e, (pt, base) => {
                    const [x, y] = toSvg(pt);
                    const next = [...(base.twist ?? [-30, 30])] as [number, number];
                    next[i] = Math.round(ringAngle(x, y));
                    return { ...base, twist: [Math.min(...next), Math.max(...next)] };
                  })
                }
              >
                <title>{i === 0 ? "Twist mínimo" : "Twist máximo"}</title>
              </circle>
            );
          }}
        </Index>

        {/* Trayectoria del clip, coloreada por tiempo */}
        <For each={props.trajectory.slice(1)}>
          {(s, i) => {
            const a = () => toSvg(props.trajectory[i()].swing);
            const b = () => toSvg(s.swing);
            return <line x1={a()[0]} y1={a()[1]} x2={b()[0]} y2={b()[1]} stroke={timeColor(i() / n(), s.excess > 0.5)} stroke-width="2" />;
          }}
        </For>
        <For each={props.trajectory.filter((s) => keyed().has(s.frame))}>
          {(s) => {
            const p = () => toSvg(s.swing);
            return (
              <circle
                cx={p()[0]}
                cy={p()[1]}
                r="4"
                class="cursor-move"
                fill={s.excess > 0.5 ? "var(--color-red)" : "var(--color-text)"}
                onPointerDown={(e) => {
                  let last = s.swing;
                  drag(
                    e,
                    (ev) => {
                      const [x, y] = svgPoint(svg!, ev, DISK);
                      last = fromSvg(x, y);
                      props.onKeyEdit(s.frame, { swing: last }, false);
                    },
                    () => props.onKeyEdit(s.frame, { swing: last }, true)
                  );
                }}
              >
                <title>Key del cuadro {s.frame}: arrastrar para cambiarla</title>
              </circle>
            );
          }}
        </For>

        {/* Asas del límite */}
        <Show when={shape().type === "ellipse"}>
          {(() => {
            const e = () => shape() as Extract<SwingShape, { type: "ellipse" }>;
            const handle = (pt: () => DiskPoint, apply: (p: DiskPoint, s: Extract<SwingShape, { type: "ellipse" }>) => SwingShape, title: string) => (
              <circle
                cx={toSvg(pt())[0]}
                cy={toSvg(pt())[1]}
                r="5"
                class="cursor-pointer"
                fill="var(--color-bg)"
                stroke="var(--color-accent)"
                stroke-width="2"
                onPointerDown={(ev) => edit(ev, (p, base) => ({ ...base, swing: apply(p, (base.swing ?? e()) as Extract<SwingShape, { type: "ellipse" }>) }))}
              >
                <title>{title}</title>
              </circle>
            );
            return (
              <>
                {handle(() => [(e().cx ?? 0) + e().rx, e().cy ?? 0], (p, s) => ({ ...s, rx: Math.max(2, Math.abs(p[0] - (s.cx ?? 0))) }), "Ancho (hacia X)")}
                {handle(() => [e().cx ?? 0, (e().cy ?? 0) + e().ry], (p, s) => ({ ...s, ry: Math.max(2, Math.abs(p[1] - (s.cy ?? 0))) }), "Alto (hacia Z)")}
                {handle(() => [e().cx ?? 0, e().cy ?? 0], (p, s) => ({ ...s, cx: p[0], cy: p[1] }), "Centro del límite")}
              </>
            );
          })()}
        </Show>
        <Show when={shape().type === "polygon"}>
          <Index each={(shape() as Extract<SwingShape, { type: "polygon" }>).points}>
            {(pt, i) => (
              <circle
                cx={toSvg(pt())[0]}
                cy={toSvg(pt())[1]}
                r="4.5"
                class="cursor-pointer"
                fill="var(--color-bg)"
                stroke="var(--color-accent)"
                stroke-width="2"
                onPointerDown={(e) => {
                  if (e.button === 2) return;
                  edit(e, (p, base) => {
                    const points = [...((base.swing as Extract<SwingShape, { type: "polygon" }>).points ?? [])];
                    points[i] = p;
                    return { ...base, swing: { type: "polygon", points } };
                  });
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  const points = (shape() as Extract<SwingShape, { type: "polygon" }>).points;
                  if (points.length > 3) commit({ ...limits(), swing: { type: "polygon", points: points.filter((_, k) => k !== i) } });
                }}
              >
                <title>Arrastrar · clic derecho: quitar el punto</title>
              </circle>
            )}
          </Index>
        </Show>

        {/* Pose actual */}
        <Show when={props.current}>
          {(cur) => <circle cx={toSvg(cur().swing)[0]} cy={toSvg(cur().swing)[1]} r="5" fill="var(--color-orange)" stroke="var(--color-bg)" stroke-width="1.5" />}
        </Show>
      </svg>
      <p class="text-[10px] text-text-dim text-center leading-snug">
        Centro = reposo · distancia = cuánto se inclina · anillo = twist
        {shape().type === "polygon" ? " · doble clic agrega un punto" : ""}
      </p>
    </div>
  );
};

// ─── Transportador de la bisagra ────────────────────────────────────────────

const HingeEditor: Component<{
  limits: JointLimits;
  current?: number;
  trajectory: TrajectorySample[];
  onChange: (limits: JointLimits, commit: boolean) => void;
}> = (props) => {
  let svg: SVGSVGElement | undefined;
  const [draft, setDraft] = createSignal<JointLimits | null>(null);
  const limits = () => draft() ?? props.limits;
  // El hueso en reposo apunta abajo; el ángulo positivo gira en sentido antihorario
  const point = (a: number, r = R): [number, number] => [C + r * Math.sin(rad(a)), C + r * Math.cos(rad(a))];
  const angleAt = (x: number, y: number) => Math.round(deg(Math.atan2(x - C, y - C)));
  const arc = createMemo(() => {
    const lo = limits().min ?? -90;
    const hi = limits().max ?? 90;
    const steps = Math.max(2, Math.ceil((hi - lo) / 4));
    return `M ${C} ${C} L ${Array.from({ length: steps + 1 }, (_, i) => point(lo + ((hi - lo) * i) / steps).join(" ")).join(" L ")} Z`;
  });
  const n = () => Math.max(1, props.trajectory.length - 1);
  const edit = (e: PointerEvent, which: "min" | "max") => {
    let last = limits();
    drag(
      e,
      (ev) => {
        const [x, y] = svgPoint(svg!, ev, DISK);
        const a = angleAt(x, y);
        last = which === "min" ? { ...limits(), min: Math.min(a, limits().max ?? 180) } : { ...limits(), max: Math.max(a, limits().min ?? -180) };
        setDraft(last);
        props.onChange(last, false);
      },
      () => {
        setDraft(null);
        props.onChange(last, true);
      }
    );
  };
  return (
    <div class="space-y-1">
      <svg ref={svg} viewBox={`0 0 ${DISK} ${DISK}`} class="w-full max-w-[340px] mx-auto block select-none touch-none">
        <circle cx={C} cy={C} r={R} fill="none" stroke="currentColor" class="text-border" stroke-width="0.8" />
        <path d={arc()} fill="var(--color-accent)" fill-opacity="0.15" stroke="var(--color-accent)" stroke-width="1.5" />
        {/* Reposo */}
        <line x1={C} y1={C} x2={point(0)[0]} y2={point(0)[1]} stroke="currentColor" class="text-text-dim" stroke-dasharray="3 3" />
        {/* Cuadros del clip, de azul a amarillo */}
        <For each={props.trajectory}>
          {(s, i) => {
            const [x1, y1] = point(s.hinge, R + 4);
            const [x2, y2] = point(s.hinge, R + 12);
            return <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={timeColor(i() / n(), s.excess > 0.5)} stroke-width="2" />;
          }}
        </For>
        <Show when={props.current !== undefined}>
          <line x1={C} y1={C} x2={point(props.current!)[0]} y2={point(props.current!)[1]} stroke="var(--color-orange)" stroke-width="3" stroke-linecap="round" />
        </Show>
        <For each={["min", "max"] as const}>
          {(which) => {
            const p = () => point(limits()[which] ?? (which === "min" ? -90 : 90));
            return (
              <circle cx={p()[0]} cy={p()[1]} r="6" class="cursor-pointer" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" onPointerDown={(e) => edit(e, which)}>
                <title>{which === "min" ? "Mínimo" : "Máximo"}</title>
              </circle>
            );
          }}
        </For>
      </svg>
      <p class="text-[10px] text-text-dim text-center">Punteado = reposo · naranja = ahora · marcas = cuadros del clip</p>
    </div>
  );
};

// ─── Curvas por eje ─────────────────────────────────────────────────────────

const CURVE_W = 260;
const CURVE_H = 58;

const MiniCurve: Component<{
  label: string;
  values: number[];
  frames: number[];
  keyFrames: number[];
  band?: [number, number];
  onEdit: (frame: number, value: number, commit: boolean) => void;
}> = (props) => {
  let svg: SVGSVGElement | undefined;
  const range = createMemo(() => {
    const all = [...props.values, ...(props.band ?? [])];
    const lo = Math.min(...all, 0) - 10;
    const hi = Math.max(...all, 0) + 10;
    return [lo, hi] as const;
  });
  const x = (i: number) => 4 + (i * (CURVE_W - 8)) / Math.max(1, props.values.length - 1);
  const y = (v: number) => CURVE_H - 4 - ((v - range()[0]) * (CURVE_H - 8)) / (range()[1] - range()[0]);
  const value = (py: number) => range()[0] + ((CURVE_H - 4 - py) * (range()[1] - range()[0])) / (CURVE_H - 8);
  const keyed = createMemo(() => new Set(props.keyFrames));
  return (
    <div>
      <div class="flex justify-between text-[10px] text-text-muted">
        <span>{props.label}</span>
        <span class="font-mono">{props.values.length > 0 ? `${Math.round(Math.min(...props.values))}° … ${Math.round(Math.max(...props.values))}°` : ""}</span>
      </div>
      <svg ref={svg} viewBox={`0 0 ${CURVE_W} ${CURVE_H}`} class="w-full block rounded bg-bg-darker/60 select-none touch-none">
        <Show when={props.band}>
          <rect x="0" width={CURVE_W} y={y(Math.max(...props.band!))} height={Math.abs(y(props.band![0]) - y(props.band![1]))} fill="var(--color-accent)" fill-opacity="0.1" />
        </Show>
        <line x1="0" x2={CURVE_W} y1={y(0)} y2={y(0)} stroke="currentColor" class="text-border" stroke-width="0.6" />
        <path d={path(props.values.map((v, i) => [x(i), y(v)]), false)} fill="none" stroke="var(--color-text-muted)" stroke-width="1.2" />
        <For each={props.frames.map((f, i) => ({ f, i })).filter(({ f }) => keyed().has(f))}>
          {({ f, i }) => (
            <circle
              cx={x(i)}
              cy={y(props.values[i])}
              r="3.5"
              class="cursor-ns-resize"
              fill="var(--color-text)"
              onPointerDown={(e) => {
                let last = props.values[i];
                drag(
                  e,
                  (ev) => {
                    last = Math.round(value(svgPoint(svg!, ev, CURVE_W, CURVE_H)[1]));
                    props.onEdit(f, last, false);
                  },
                  () => props.onEdit(f, last, true)
                );
              }}
            >
              <title>Cuadro {f}: arrastrar arriba o abajo</title>
            </circle>
          )}
        </For>
      </svg>
    </div>
  );
};

// ─── Cadena de perfil ───────────────────────────────────────────────────────

const CHAIN_W = 260;
const CHAIN_H = 190;

const ChainProfile: Component<{ chain: ChainView; onDrag: JointPanelProps["onChainDrag"] }> = (props) => {
  let svg: SVGSVGElement | undefined;
  // Encuadre fijo mientras se arrastra
  const [frozen, setFrozen] = createSignal<{ s: number; ox: number; oy: number } | null>(null);
  const fit = createMemo(() => {
    if (frozen()) return frozen()!;
    const pts = [...props.chain.posed, ...props.chain.rest, ...props.chain.reach];
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const w = Math.max(...xs) - Math.min(...xs) || 1;
    const h = Math.max(...ys) - Math.min(...ys) || 1;
    const s = Math.min((CHAIN_W - 24) / w, (CHAIN_H - 24) / h);
    return { s, ox: (Math.min(...xs) + Math.max(...xs)) / 2, oy: (Math.min(...ys) + Math.max(...ys)) / 2 };
  });
  const to = (p: [number, number]): [number, number] => [CHAIN_W / 2 + (p[0] - fit().ox) * fit().s, CHAIN_H / 2 - (p[1] - fit().oy) * fit().s];
  const planeAngle = (from: [number, number], x: number, y: number) => {
    const a = to(from);
    return deg(Math.atan2(-(y - a[1]), x - a[0]));
  };
  return (
    <svg ref={svg} viewBox={`0 0 ${CHAIN_W} ${CHAIN_H}`} class="w-full block rounded bg-bg-darker/60 select-none touch-none">
      <For each={props.chain.reach}>{(p) => <circle cx={to(p)[0]} cy={to(p)[1]} r="1.2" fill="var(--color-accent)" fill-opacity="0.25" />}</For>
      <path d={path(props.chain.rest.map(to), false)} fill="none" stroke="currentColor" class="text-text-dim" stroke-dasharray="3 3" />
      <For each={props.chain.arcs}>
        {(a) => {
          const c = () => to(props.chain.posed[a.index]);
          const r = 18;
          const steps = Math.max(2, Math.ceil(Math.abs(a.to - a.from) / 6));
          const pts = () =>
            Array.from({ length: steps + 1 }, (_, i) => {
              const t = rad(a.from + ((a.to - a.from) * i) / steps);
              return [c()[0] + r * Math.cos(t), c()[1] - r * Math.sin(t)] as [number, number];
            });
          return <path d={`M ${c()[0]} ${c()[1]} L ${pts().map((p) => p.join(" ")).join(" L ")} Z`} fill="var(--color-accent)" fill-opacity="0.18" stroke="var(--color-accent)" stroke-width="0.8" />;
        }}
      </For>
      <path d={path(props.chain.posed.map(to), false)} fill="none" stroke="var(--color-orange)" stroke-width="2.5" stroke-linecap="round" />
      <For each={props.chain.posed}>
        {(p, i) => (
          <circle
            cx={to(p)[0]}
            cy={to(p)[1]}
            r={i() === 0 ? 4 : 5.5}
            class={i() === 0 ? "" : "cursor-move"}
            fill={i() === 0 ? "var(--color-text-dim)" : "var(--color-bg)"}
            stroke="var(--color-orange)"
            stroke-width="2"
            onPointerDown={(e) => {
              if (i() === 0) return;
              const k = i() - 1;
              setFrozen(fit());
              const [x0, y0] = svgPoint(svg!, e, CHAIN_W, CHAIN_H);
              const start = planeAngle(props.chain.posed[k], x0, y0);
              const pivot = props.chain.posed[k];
              let delta = 0;
              drag(
                e,
                (ev) => {
                  const [x, y] = svgPoint(svg!, ev, CHAIN_W, CHAIN_H);
                  delta = planeAngle(pivot, x, y) - start;
                  props.onDrag(k, delta, false);
                },
                () => {
                  setFrozen(null);
                  props.onDrag(k, delta, true);
                }
              );
            }}
          >
            <title>{props.chain.names[i()]}</title>
          </circle>
        )}
      </For>
    </svg>
  );
};

// ─── Disco chico ────────────────────────────────────────────────────────────

const SmallDisk: Component<{ disk: DiskSummary }> = (props) => {
  const S = 56;
  const c = S / 2;
  const r = 24;
  const range = () => {
    const l = props.disk.limits;
    if (l?.kind !== "ball" || !l.swing) return 90;
    return Math.max(45, ...boundaryPoints(l.swing, 24).map((p) => Math.hypot(...p) * 1.15));
  };
  const to = (p: DiskPoint): [number, number] => [c + (p[0] * r) / range(), c - (p[1] * r) / range()];
  return (
    <div class="flex flex-col items-center w-16">
      <svg viewBox={`0 0 ${S} ${S}`} class="w-14 h-14">
        <circle cx={c} cy={c} r={r} fill="none" stroke="currentColor" class="text-border" stroke-width="0.8" />
        <Show when={props.disk.limits?.kind === "ball" && props.disk.limits.swing}>
          <path d={path(boundaryPoints(props.disk.limits!.swing!, 32).map(to))} fill="var(--color-accent)" fill-opacity="0.15" stroke="var(--color-accent)" />
          <circle cx={to(props.disk.swing)[0]} cy={to(props.disk.swing)[1]} r="2.5" fill="var(--color-orange)" />
        </Show>
        <Show when={props.disk.limits?.kind === "hinge"}>
          {(() => {
            const l = props.disk.limits!;
            const p = (a: number): [number, number] => [c + r * Math.sin(rad(a)), c + r * Math.cos(rad(a))];
            const lo = l.min ?? -90;
            const hi = l.max ?? 90;
            const pts = Array.from({ length: 13 }, (_, i) => p(lo + ((hi - lo) * i) / 12));
            return (
              <>
                <path d={`M ${c} ${c} L ${pts.map((q) => q.join(" ")).join(" L ")} Z`} fill="var(--color-accent)" fill-opacity="0.15" stroke="var(--color-accent)" />
                <line x1={c} y1={c} x2={p(props.disk.hinge)[0]} y2={p(props.disk.hinge)[1]} stroke="var(--color-orange)" stroke-width="2" />
              </>
            );
          })()}
        </Show>
        <Show when={!props.disk.limits}>
          <text x={c} y={c + 3} text-anchor="middle" font-size="8" class="fill-text-dim">libre</text>
        </Show>
      </svg>
      <span class="text-[9px] text-text-dim truncate w-full text-center">{props.disk.name}</span>
    </div>
  );
};

// ─── Grafo de relaciones ────────────────────────────────────────────────────

const GRAPH_W = 260;
const GRAPH_H = 170;
const EDGE_COLORS = { hierarchy: "var(--color-text-dim)", ik: "var(--color-accent)", follows: "var(--color-green)", constraint: "var(--color-warning)" };

const RelationsGraph: Component<{
  nodes: RelationNode[];
  edges: RelationEdge[];
  onSelect: (node: RelationNode) => void;
  onReparent: (controlId: string, boneName: string | null) => void;
  onConnect: (target: string, owner: string) => void;
}> = (props) => {
  let svg: SVGSVGElement | undefined;
  const [dragging, setDragging] = createSignal<{ id: string; x: number; y: number } | null>(null);
  const layout = createMemo(() => {
    const pos = new Map<string, [number, number]>();
    const row = (role: RelationNode["role"], x0: number, x1: number, y: number, vertical = false) => {
      const list = props.nodes.filter((n) => n.role === role);
      list.forEach((n, i) => {
        const t = list.length === 1 ? 0.5 : i / (list.length - 1);
        pos.set(n.id, vertical ? [x0, y + (x1 - y) * t] : [x0 + (x1 - x0) * t, y]);
      });
    };
    row("parent", GRAPH_W / 2, GRAPH_W / 2, 18);
    row("active", GRAPH_W / 2, GRAPH_W / 2, GRAPH_H / 2);
    row("child", 60, GRAPH_W - 60, GRAPH_H - 18);
    row("control", 34, 20, GRAPH_H - 30, true);
    row("chain", GRAPH_W - 34, 30, GRAPH_H - 30, true);
    return pos;
  });
  const at = (id: string) => layout().get(id) ?? [0, 0];
  const nodeAt = (x: number, y: number) =>
    props.nodes.find((n) => n.kind === "bone" && Math.hypot(at(n.id)[0] - x, at(n.id)[1] - y) < 22);
  return (
    <svg ref={svg} viewBox={`0 0 ${GRAPH_W} ${GRAPH_H}`} class="w-full block rounded bg-bg-darker/60 select-none touch-none">
      <For each={props.edges}>
        {(e) => (
          <line x1={at(e.from)[0]} y1={at(e.from)[1]} x2={at(e.to)[0]} y2={at(e.to)[1]} stroke={EDGE_COLORS[e.kind]} stroke-width="1.3" stroke-dasharray={e.kind === "follows" ? "3 2" : e.kind === "constraint" ? "5 2" : undefined} />
        )}
      </For>
      <Show when={dragging()}>
        {(d) => <line x1={at(d().id)[0]} y1={at(d().id)[1]} x2={d().x} y2={d().y} stroke="var(--color-green)" stroke-width="1.5" stroke-dasharray="3 2" />}
      </Show>
      <For each={props.nodes}>
        {(n) => {
          const [x, y] = at(n.id);
          const w = Math.min(96, 10 + n.label.length * 5.2);
          return (
            <g
              class="cursor-pointer"
              onPointerDown={(e) => {
                if (n.kind !== "control" && n.kind !== "bone") return props.onSelect(n);
                // Un control se arrastra hasta un hueso para que lo siga (o afuera: suelto);
                // un hueso, hasta otro para crear una restricción
                let moved = false;
                drag(
                  e,
                  (ev) => {
                    const [px, py] = svgPoint(svg!, ev, GRAPH_W, GRAPH_H);
                    moved = true;
                    setDragging({ id: n.id, x: px, y: py });
                  },
                  () => {
                    const d = dragging();
                    setDragging(null);
                    if (!moved || !d) return props.onSelect(n);
                    const target = nodeAt(d.x, d.y);
                    if (n.kind === "bone") {
                      if (target && target.id !== n.id) props.onConnect(n.label, target.label);
                      return;
                    }
                    props.onReparent(n.id, target ? target.label : null);
                  }
                );
              }}
            >
              <rect
                x={x - w / 2}
                y={y - 9}
                width={w}
                height="18"
                rx={n.kind === "bone" ? 4 : 9}
                fill={n.role === "active" ? "var(--color-accent)" : "var(--color-bg-lighter)"}
                fill-opacity={n.role === "active" ? 0.3 : 1}
                stroke={
                  n.kind === "chain"
                    ? "var(--color-accent)"
                    : n.kind === "control"
                      ? "var(--color-green)"
                      : n.kind === "constraint"
                        ? "var(--color-warning)"
                        : "var(--color-border)"
                }
              />
              <text x={x} y={y + 3} text-anchor="middle" font-size="8.5" class="fill-text">
                {n.label.length > 16 ? `${n.label.slice(0, 15)}…` : n.label}
              </text>
            </g>
          );
        }}
      </For>
    </svg>
  );
};

// ─── Panel ──────────────────────────────────────────────────────────────────

/**
 * Panel de articulación: límites (bisagra o rótula) editados en 2D, la
 * trayectoria del clip sobre ellos, curvas por eje, la cadena de perfil
 * con su alcance, discos de la selección, relaciones y límites automáticos
 */
export const JointPanel: Component<JointPanelProps> = (props) => {
  const [scale, setScale] = createSignal(100);
  const kind = () => props.limits?.kind ?? "none";
  const frames = () => props.trajectory.map((s) => s.frame);
  const change = (limits: JointLimits, commit: boolean) => (commit ? props.onLimits("Límite de giro", limits) : props.onLimitsPreview(limits));

  return (
    <div class="space-y-3">
    <Panel title="Límite de giro" icon={<Icons.Bone size={14} />} defaultOpen>
      <div class="space-y-3">
        <Show when={props.boneName} fallback={<p class="text-xs text-text-muted">Elige una articulación para ver y editar su rango de giro.</p>}>
          <div class="flex items-center justify-between">
            <span class="text-xs font-mono text-accent truncate">{props.boneName}</span>
            <Checkbox label="Aplicar límites" checked={!props.limitsOff} onChange={(v) => props.onLimitsOff(!v)} />
          </div>

          {/* Tipo */}
          <div class="grid grid-cols-3 gap-1">
            <For each={[["none", "Libre"], ["hinge", "Bisagra"], ["ball", "Rótula"]] as const}>
              {([value, label]) => (
                <button
                  class={clsx(
                    "h-7 rounded border text-xs",
                    kind() === value ? "bg-accent/20 border-accent text-accent" : "border-border text-text-muted hover:text-text"
                  )}
                  onClick={() => props.onKind(value)}
                >
                  {label}
                </button>
              )}
            </For>
          </div>

          {/* Editor 2D */}
          <Show when={props.limits?.kind === "ball" && props.limits}>
            {(l) => (
              <>
                <BallEditor limits={l()} current={props.current} trajectory={props.trajectory} keyFrames={props.keyFrames} onChange={change} onKeyEdit={props.onKeyEdit} />
                <div class="grid grid-cols-2 gap-1">
                  <Button
                    size="sm"
                    variant={l().swing?.type !== "polygon" ? "primary" : "default"}
                    onClick={() => props.onLimits("Límite en elipse", { ...l(), swing: { type: "ellipse", rx: 45, ry: 45 } })}
                  >
                    Elipse
                  </Button>
                  <Button
                    size="sm"
                    variant={l().swing?.type === "polygon" ? "primary" : "default"}
                    onClick={() =>
                      props.onLimits("Límite libre", {
                        ...l(),
                        swing: { type: "polygon", points: boundaryPoints(l().swing ?? { type: "ellipse", rx: 45, ry: 45 }, 10) },
                      })
                    }
                  >
                    Polígono
                  </Button>
                </div>
              </>
            )}
          </Show>
          <Show when={props.limits?.kind === "hinge" && props.limits}>
            {(l) => (
              <>
                <HingeEditor limits={l()} current={props.current?.hinge} trajectory={props.trajectory} onChange={change} />
                <div class="grid grid-cols-2 gap-1">
                  <NumberInput label="Mín" suffix="°" step={1} value={Math.round(l().min ?? -90)} onChange={(v) => props.onLimits("Límite de giro", { ...l(), min: Math.min(v, l().max ?? 180) })} />
                  <NumberInput label="Máx" suffix="°" step={1} value={Math.round(l().max ?? 90)} onChange={(v) => props.onLimits("Límite de giro", { ...l(), max: Math.max(v, l().min ?? -180) })} />
                </div>
              </>
            )}
          </Show>
          <Show when={props.limits}>
            {(l) => (
              <div class="space-y-2">
                <Show when={l().kind === "ball"}>
                  <div class="grid grid-cols-2 gap-1">
                    <NumberInput label="Twist" suffix="°" step={1} value={Math.round(l().twist?.[0] ?? -30)} onChange={(v) => props.onLimits("Twist", { ...l(), twist: [Math.min(v, l().twist?.[1] ?? 180), l().twist?.[1] ?? 30] })} />
                    <NumberInput label="a" suffix="°" step={1} value={Math.round(l().twist?.[1] ?? 30)} onChange={(v) => props.onLimits("Twist", { ...l(), twist: [l().twist?.[0] ?? -30, Math.max(v, l().twist?.[0] ?? -180)] })} />
                  </div>
                  <NumberInput
                    label="Rigidez"
                    suffix="%"
                    min={0}
                    max={90}
                    step={5}
                    value={Math.round((l().stiffness ?? 0) * 100)}
                    onChange={(v) => props.onLimits("Rigidez del límite", { ...l(), stiffness: v > 0 ? v / 100 : undefined })}
                  />
                </Show>
                <Button size="sm" fullWidth onClick={props.onProbe} disabled={!props.posing} title="Recorre todo el rango en el visor para ver cómo deforma la piel">
                  <Icons.Play size={12} /> Probar rango
                </Button>
              </div>
            )}
          </Show>

        </Show>
      </div>
    </Panel>

          {/* Curvas por eje */}
          <Show when={props.boneName && props.trajectory.length > 1 && props.limits}>
            <Panel title="En el tiempo" icon={<Icons.Clock size={14} />} defaultOpen>
            {(() => {
              const l = props.limits!;
              return (
                <div class="space-y-1.5">
                  <span class="text-xs text-text-muted">En el tiempo (las keys se arrastran)</span>
                  <Show when={l.kind === "hinge"}>
                    <MiniCurve
                      label="Ángulo"
                      values={props.trajectory.map((s) => s.hinge)}
                      frames={frames()}
                      keyFrames={props.keyFrames}
                      band={[l.min ?? -180, l.max ?? 180]}
                      onEdit={(f, v, commit) => props.onKeyEdit(f, { hinge: v }, commit)}
                    />
                  </Show>
                  <Show when={l.kind === "ball"}>
                    <MiniCurve label="Inclinación hacia X" values={props.trajectory.map((s) => s.swing[0])} frames={frames()} keyFrames={props.keyFrames} onEdit={(f, v, commit) => {
                      const s = props.trajectory.find((t) => t.frame === f)!;
                      props.onKeyEdit(f, { swing: [v, s.swing[1]] }, commit);
                    }} />
                    <MiniCurve label="Inclinación hacia Z" values={props.trajectory.map((s) => s.swing[1])} frames={frames()} keyFrames={props.keyFrames} onEdit={(f, v, commit) => {
                      const s = props.trajectory.find((t) => t.frame === f)!;
                      props.onKeyEdit(f, { swing: [s.swing[0], v] }, commit);
                    }} />
                    <MiniCurve label="Twist" values={props.trajectory.map((s) => s.twist)} frames={frames()} keyFrames={props.keyFrames} band={l.twist} onEdit={(f, v, commit) => props.onKeyEdit(f, { twist: v }, commit)} />
                  </Show>
                </div>
              );
            })()}
            </Panel>
          </Show>

        {/* Límites automáticos */}
        <Panel title="Límites automáticos" icon={<Icons.MagicWand size={14} />} defaultOpen>
        <div class="space-y-1">
          <span class="text-xs text-text-muted">Límites automáticos {props.hasSelection ? "(la selección)" : "(todo el esqueleto)"}</span>
          <div class="grid grid-cols-2 gap-1">
            <Button size="sm" onClick={() => props.onAuto("anatomical")} title="Según el tipo de cadena: rodillas y codos bisagras, caderas y hombros rótulas…">
              Anatómicos
            </Button>
            <Button size="sm" onClick={() => props.onAuto("mesh")} disabled={!props.canMesh} title="Gira cada articulación hasta que la piel choca consigo misma">
              Por la malla
            </Button>
            <Button size="sm" onClick={() => props.onAuto("observed")} title="El rango que usan las animaciones, con un margen">
              Por la animación
            </Button>
            <Button size="sm" variant="ghost" onClick={() => props.onAuto("clear")}>
              Quitar límites
            </Button>
          </div>
        </div>
        <Show when={props.disks.length === 1}>
          <Button size="sm" fullWidth variant="ghost" onClick={() => props.onBatch("mirror")} title="Copia los límites al hueso del otro lado">
            Copiar al lado espejo
          </Button>
        </Show>
        </Panel>

        {/* Cadena o grupo */}
        <Show when={props.disks.length > 1}>
          <Panel title={`Selección (${props.disks.length})`} icon={<Icons.TreeStructure size={14} />} defaultOpen>
          <div class="space-y-2">
            <Show when={props.chain}>
              {(chain) => (
                <>
                  <ChainProfile chain={chain()} onDrag={props.onChainDrag} />
                  <p class="text-[10px] text-text-dim leading-snug">
                    Perfil en el plano de doblez: arrastra una articulación para posar · punteado = reposo · puntos = alcance de la punta
                  </p>
                </>
              )}
            </Show>
            <div class="flex flex-wrap gap-1">
              <For each={props.disks}>{(d) => <SmallDisk disk={d} />}</For>
            </div>
            <div class="grid grid-cols-2 gap-1">
              <Button size="sm" onClick={() => props.onBatch("copyActive")} title="Los límites de la activa en todas las elegidas">
                Igual a la activa
              </Button>
              <Button size="sm" onClick={() => props.onBatch("mirror")} title="Copia los límites al hueso del otro lado">
                Al lado espejo
              </Button>
            </div>
            <div class="flex gap-1 items-end">
              <NumberInput class="flex-1" label="Escalar rangos" suffix="%" min={10} max={300} step={5} value={scale()} onChange={setScale} />
              <Button size="sm" onClick={() => props.onBatch("scale", scale() / 100)}>
                Aplicar
              </Button>
            </div>
          </div>
          </Panel>
        </Show>

        {/* Relaciones */}
        <Show when={props.boneName && (props.relations.nodes.length > 1 || props.shared.length > 0)}>
          <Panel title="Relaciones" icon={<Icons.TreeStructure size={14} />} defaultOpen>
          <Show when={props.relations.nodes.length > 1}>
          <div class="space-y-1">
            <RelationsGraph
              nodes={props.relations.nodes}
              edges={props.relations.edges}
              onSelect={props.onSelectNode}
              onReparent={props.onReparent}
              onConnect={props.onConnect}
            />
            <p class="text-[10px] text-text-dim leading-snug">
              Gris: jerarquía · violeta: IK · verde: sigue a · arrastra un control hasta un hueso para que lo siga (afuera: suelto)
            </p>
          </div>
          </Show>
        <Show when={props.shared.length > 0}>
          <div class="space-y-1">
            <span class="text-xs text-text-muted">Comparte piel con</span>
            <For each={props.shared}>
              {(s) => (
                <div class="flex items-center gap-2 text-xs">
                  <span class="w-24 truncate text-text-muted">{s.name}</span>
                  <div class="flex-1 h-1.5 rounded bg-bg-darker overflow-hidden">
                    <div class="h-full bg-accent/70" style={{ width: `${Math.round(s.fraction * 100)}%` }} />
                  </div>
                  <span class="w-9 text-right font-mono text-text-dim">{Math.round(s.fraction * 100)}%</span>
                </div>
              )}
            </For>
          </div>
        </Show>
          </Panel>
        </Show>
    </div>
  );
};

