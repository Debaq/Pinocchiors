import { Component, For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import type { AnimationClip, BoneTrack } from "../../lib/animation";
import {
  channelData,
  editKeyChannel,
  groupKeys,
  keyRefId,
  resolveHandles,
  sampleChannels,
  type ChannelGroup,
  type KeyRef,
  type RotationCodecs,
} from "../../lib/curves";

/** Un canal que se dibuja: de qué pista y grupo, cuál componente */
export interface CurveChannelRow {
  id: string;
  track: { bone: string; kind?: BoneTrack["kind"] };
  group: ChannelGroup;
  channel: number;
  label: string;
  /** Nombre de la pista (encabezado) */
  owner: string;
  color: string;
}

/** Id de una key de un canal en la selección del editor de curvas */
export const curveKeyId = (row: CurveChannelRow, frame: number) =>
  `${keyRefId({ bone: row.track.bone, kind: row.track.kind, group: row.group, frame })}\u0001${row.channel}`;

export function parseCurveKeyId(id: string): { ref: KeyRef; channel: number } {
  const [ref, channel] = id.split("\u0001");
  const [kind, bone, group, frame] = ref.split("\u0000");
  return { ref: { kind: kind === "b" ? undefined : (kind as BoneTrack["kind"]), bone, group: group as ChannelGroup, frame: Number(frame) }, channel: Number(channel) };
}

export interface CurveEditorProps {
  clip: AnimationClip;
  frame: number;
  rows: CurveChannelRow[];
  rotation: RotationCodecs;
  selection: Set<string>;
  onSelection: (selection: Set<string>) => void;
  /** Cambio del clip: vista previa mientras se arrastra (`commit` false) o definitivo */
  onEdit: (clip: AnimationClip, commit: boolean, description: string) => void;
  /** Cada canal en −1…1 (para comparar formas) */
  normalize: boolean;
  /** Cambia para pedir encuadre (a la selección, o a todo) */
  fitRequest: number;
  /** Ancho de la columna de nombres (el mismo que la hoja de claves, para alinear con la regla) */
  namesWidth: number;
}

const ROW = 20;

interface Curve {
  row: CurveChannelRow;
  points: [number, number][];
  keys: { frame: number; value: number; handles?: { in: [number, number]; out: [number, number] } }[];
  lo: number;
  hi: number;
}

/**
 * Editor de curvas: un canal por fila a la izquierda y las curvas a la
 * derecha, con el tiempo alineado a la regla de la línea de tiempo. Clic
 * elige keys (Shift suma), arrastrar las mueve en tiempo y valor, las manijas
 * de las Bézier se arrastran (Alt: libres), arrastrar en vacío elige con un
 * recuadro y la rueda acerca o aleja el valor.
 */
export const CurveEditor: Component<CurveEditorProps> = (props) => {
  let area: HTMLDivElement | undefined;
  const [size, setSize] = createSignal({ w: 600, h: 200 });
  const [hidden, setHidden] = createSignal<Set<string>>(new Set());
  const [view, setView] = createSignal({ lo: -1, hi: 1 });
  const [box, setBox] = createSignal<{ x0: number; y0: number; x1: number; y1: number } | null>(null);

  onMount(() => {
    const ro = new ResizeObserver(() => setSize({ w: area?.clientWidth ?? 600, h: area?.clientHeight ?? 200 }));
    if (area) ro.observe(area);
    onCleanup(() => ro.disconnect());
  });

  const span = () => Math.max(1, props.clip.end - props.clip.start + 1);
  const scale = () => size().w / span();
  const xOf = (f: number) => (f - props.clip.start + 0.5) * scale();
  const frameAt = (x: number) => x / scale() - 0.5 + props.clip.start;
  const yOf = (v: number) => size().h - 8 - ((v - view().lo) * (size().h - 16)) / (view().hi - view().lo || 1);
  const valueAt = (y: number) => view().lo + ((size().h - 8 - y) * (view().hi - view().lo)) / (size().h - 16);

  const cycle = () => (props.clip.cyclic ? props.clip.end - props.clip.start : undefined);

  /** Curva de cada canal visible: muestras por cuadro, keys y manijas */
  const curves = createMemo<Curve[]>(() => {
    const out: Curve[] = [];
    for (const row of props.rows) {
      if (hidden().has(row.id)) continue;
      const track = props.clip.tracks.find((t) => t.bone === row.track.bone && t.kind === row.track.kind);
      if (!track) continue;
      const { keys, codec } = groupKeys(track, row.group, props.rotation);
      if (keys.length === 0) continue;
      const data = channelData(keys, codec);
      const values: number[] = [];
      const frames: number[] = [];
      for (let f = Math.floor(props.clip.start); f <= Math.ceil(props.clip.end); f++) {
        frames.push(f);
        values.push(sampleChannels(keys, f, codec, data, cycle())[row.channel]);
      }
      const keyPts = keys.map((k, i) => {
        const value = data.values[i][row.channel];
        const bezier = k.interpolation === "bezier" || keys[i - 1]?.interpolation === "bezier";
        if (!bezier) return { frame: k.frame, value };
        const h = resolveHandles(keys, data, i, cycle());
        return {
          frame: k.frame,
          value,
          handles: {
            in: [k.frame + h.in.dt[row.channel], value + h.in.dv[row.channel]] as [number, number],
            out: [k.frame + h.out.dt[row.channel], value + h.out.dv[row.channel]] as [number, number],
          },
        };
      });
      const all = [...values, ...keyPts.map((k) => k.value)];
      let lo = Math.min(...all);
      let hi = Math.max(...all);
      if (hi - lo < 1e-6) [lo, hi] = [lo - 1, hi + 1];
      out.push({ row, points: frames.map((f, i) => [f, values[i]]), keys: keyPts, lo, hi });
    }
    return out;
  });

  /** Valor como se dibuja (normalizado o no) y de vuelta */
  const show = (c: Curve, v: number) => (props.normalize ? -1 + (2 * (v - c.lo)) / (c.hi - c.lo) : v);
  const unshow = (c: Curve, v: number) => (props.normalize ? c.lo + ((v + 1) * (c.hi - c.lo)) / 2 : v);

  /** Encuadre vertical: lo elegido o todo lo visible */
  const fit = () => {
    const list = curves();
    const sel = props.selection;
    let values: number[] = [];
    for (const c of list) {
      const picked = c.keys.filter((k) => sel.has(curveKeyId(c.row, k.frame)));
      values.push(...(picked.length > 0 && sel.size > 0 ? picked.map((k) => show(c, k.value)) : sel.size > 0 ? [] : c.points.map(([, v]) => show(c, v))));
    }
    if (values.length === 0) values = list.flatMap((c) => c.points.map(([, v]) => show(c, v)));
    if (values.length === 0) return setView({ lo: -1, hi: 1 });
    let lo = Math.min(...values);
    let hi = Math.max(...values);
    const pad = Math.max(1e-3, (hi - lo) * 0.1);
    if (hi - lo < 1e-6) [lo, hi] = [lo - 1, hi + 1];
    setView({ lo: lo - pad, hi: hi + pad });
  };
  createEffect(on(() => [props.fitRequest, props.normalize, props.rows.map((r) => r.id).join()], fit));

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const r = area!.getBoundingClientRect();
    const at = valueAt(e.clientY - r.top);
    const k = Math.exp(e.deltaY * 0.002);
    setView((v) => ({ lo: at + (v.lo - at) * k, hi: at + (v.hi - at) * k }));
  };

  const local = (e: PointerEvent) => {
    const r = area!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top] as const;
  };

  /** Arrastre de keys elegidas: tiempo (redondeado) y valor, desde el clip de partida */
  const dragKeys = (e: PointerEvent, curve: Curve, frame: number) => {
    e.stopPropagation();
    const id = curveKeyId(curve.row, frame);
    let selection = props.selection;
    if (e.shiftKey) {
      selection = new Set(selection);
      if (selection.has(id)) selection.delete(id);
      else selection.add(id);
      props.onSelection(selection);
      return;
    }
    if (!selection.has(id)) {
      selection = new Set([id]);
      props.onSelection(selection);
    }
    const base = props.clip;
    const [x0, y0] = local(e);
    const picked = [...selection].map(parseCurveKeyId);
    const curveOf = new Map(curves().map((c) => [`${c.row.track.kind ?? "b"}|${c.row.track.bone}|${c.row.group}|${c.row.channel}`, c]));
    let moved = false;
    let last = base;
    let lastDf = 0;
    const move = (ev: PointerEvent) => {
      const [x, y] = local(ev);
      const df = Math.round(frameAt(x) - frameAt(x0));
      lastDf = df;
      const dy = valueAt(y) - valueAt(y0);
      if (!moved && Math.hypot(x - x0, y - y0) < 3) return;
      moved = true;
      let next = base;
      // Valor: cada canal en sus unidades
      for (const { ref, channel } of picked) {
        const c = curveOf.get(`${ref.kind ?? "b"}|${ref.bone}|${ref.group}|${channel}`);
        const k = c?.keys.find((kk) => kk.frame === ref.frame);
        if (!c || !k) continue;
        next = editKeyChannel(next, ref, channel, { value: unshow(c, show(c, k.value) + dy) }, props.rotation);
      }
      // Tiempo: una vez por key (sus canales van juntos), en el orden que no pisa
      if (df !== 0) {
        const refs = [...new Map(picked.map((p) => [keyRefId(p.ref), p.ref])).values()].sort((a, b) => (df > 0 ? b.frame - a.frame : a.frame - b.frame));
        for (const ref of refs) next = editKeyChannel(next, ref, 0, { frame: ref.frame + df }, props.rotation);
      }
      last = next;
      props.onEdit(next, false, "");
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (!moved) return;
      props.onEdit(last, true, "Mover keys");
      // La selección sigue a las keys movidas
      props.onSelection(new Set(picked.map(({ ref, channel }) => `${keyRefId({ ...ref, frame: ref.frame + lastDf })}\u0001${channel}`)));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Arrastre de una manija (Alt: libre, si no alineada con la otra) */
  const dragHandle = (e: PointerEvent, curve: Curve, frame: number, side: "in" | "out") => {
    e.stopPropagation();
    const base = props.clip;
    const k = curve.keys.find((kk) => kk.frame === frame)!;
    const ref: KeyRef = { bone: curve.row.track.bone, kind: curve.row.track.kind, group: curve.row.group, frame };
    let last = base;
    const move = (ev: PointerEvent) => {
      const [x, y] = local(ev);
      const dt = frameAt(x) - frame;
      const dv = unshow(curve, valueAt(y)) - k.value;
      last = editKeyChannel(base, ref, curve.row.channel, { handle: { side, dt, dv, free: ev.altKey } }, props.rotation);
      props.onEdit(last, false, "");
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (last !== base) props.onEdit(last, true, "Mover manija");
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Recuadro en el vacío: elige las keys adentro (Shift suma) */
  const startBox = (e: PointerEvent) => {
    if (e.button !== 0) return;
    const [x0, y0] = local(e);
    setBox({ x0, y0, x1: x0, y1: y0 });
    const move = (ev: PointerEvent) => {
      const [x, y] = local(ev);
      setBox({ x0, y0, x1: x, y1: y });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const b = box();
      setBox(null);
      if (!b) return;
      const [xa, xb] = [Math.min(b.x0, b.x1), Math.max(b.x0, b.x1)];
      const [ya, yb] = [Math.min(b.y0, b.y1), Math.max(b.y0, b.y1)];
      const selection = new Set(e.shiftKey ? props.selection : []);
      if (xb - xa > 2 || yb - ya > 2) {
        for (const c of curves()) {
          for (const k of c.keys) {
            const x = xOf(k.frame);
            const y = yOf(show(c, k.value));
            if (x >= xa && x <= xb && y >= ya && y <= yb) selection.add(curveKeyId(c.row, k.frame));
          }
        }
      }
      props.onSelection(selection);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Líneas de la grilla de valores: cada 1, 2, 5, 10… con al menos 24 px entre ellas */
  const gridLines = createMemo(() => {
    const { lo, hi } = view();
    const range = hi - lo;
    const target = (range * 24) / Math.max(1, size().h);
    const pow = Math.pow(10, Math.floor(Math.log10(target)));
    const step = [1, 2, 5, 10].map((m) => m * pow).find((s) => s >= target) ?? pow * 10;
    const out: number[] = [];
    for (let v = Math.ceil(lo / step) * step; v <= hi; v += step) out.push(Math.round(v / step) * step);
    return out;
  });

  const path = (c: Curve) =>
    c.points.map(([f, v], i) => `${i === 0 ? "M" : "L"} ${xOf(f).toFixed(1)} ${yOf(show(c, v)).toFixed(1)}`).join(" ");

  const owners = createMemo(() => {
    const out: { owner: string; rows: CurveChannelRow[] }[] = [];
    for (const r of props.rows) {
      const last = out[out.length - 1];
      if (last && last.owner === r.owner) last.rows.push(r);
      else out.push({ owner: r.owner, rows: [r] });
    }
    return out;
  });

  return (
    <div class="flex flex-1 min-h-0">
      {/* Canales */}
      <div class="shrink-0 border-r border-border overflow-y-auto" style={{ width: `${props.namesWidth}px` }}>
        <For each={owners()} fallback={<p class="px-2 py-2 text-xs text-text-dim">Elige articulaciones con keys (o ninguna, para ver todas).</p>}>
          {(o) => (
            <div>
              <div class="px-2 text-[11px] font-semibold text-text truncate" style={{ height: `${ROW}px`, "line-height": `${ROW}px` }}>
                {o.owner}
              </div>
              <For each={o.rows}>
                {(r) => (
                  <button
                    class={clsx("w-full flex items-center gap-1.5 pl-4 pr-2 text-[11px] text-left", hidden().has(r.id) ? "text-text-dim" : "text-text-muted hover:text-text")}
                    style={{ height: `${ROW}px` }}
                    title="Clic: mostrar u ocultar · doble clic: solo este"
                    onClick={() => {
                      const next = new Set(hidden());
                      if (next.has(r.id)) next.delete(r.id);
                      else next.add(r.id);
                      setHidden(next);
                    }}
                    onDblClick={() => setHidden(new Set(props.rows.filter((x) => x.id !== r.id).map((x) => x.id)))}
                  >
                    <span class="w-2 h-2 rounded-sm shrink-0" style={{ background: hidden().has(r.id) ? "transparent" : r.color, border: `1px solid ${r.color}` }} />
                    <span class="truncate">{r.label}</span>
                  </button>
                )}
              </For>
            </div>
          )}
        </For>
      </div>

      {/* Curvas */}
      <div ref={area} class="relative flex-1 min-w-0 overflow-hidden select-none" onWheel={onWheel} onPointerDown={startBox}>
        <svg class="absolute inset-0 w-full h-full">
          <For each={gridLines()}>
            {(v) => (
              <>
                <line x1="0" x2={size().w} y1={yOf(v)} y2={yOf(v)} stroke="currentColor" class={v === 0 ? "text-border-hover" : "text-border"} stroke-width="0.6" />
                <text x="3" y={yOf(v) - 2} font-size="9" class="fill-text-dim">
                  {Number(v.toPrecision(4))}
                </text>
              </>
            )}
          </For>
          <line x1={xOf(props.frame)} x2={xOf(props.frame)} y1="0" y2={size().h} stroke="var(--color-accent)" stroke-width="1" />
          <For each={curves()}>
            {(c) => (
              <g>
                <path d={path(c)} fill="none" stroke={c.row.color} stroke-width="1.5" stroke-opacity="0.9" />
                <For each={c.keys}>
                  {(k) => {
                    const selected = () => props.selection.has(curveKeyId(c.row, k.frame));
                    return (
                      <>
                        <Show when={selected() && k.handles}>
                          <For each={["in", "out"] as const}>
                            {(side) => {
                              const h = () => k.handles![side];
                              return (
                                <>
                                  <line x1={xOf(k.frame)} y1={yOf(show(c, k.value))} x2={xOf(h()[0])} y2={yOf(show(c, h()[1]))} stroke="var(--color-text-muted)" stroke-width="1" />
                                  <circle
                                    cx={xOf(h()[0])}
                                    cy={yOf(show(c, h()[1]))}
                                    r="3.5"
                                    fill="var(--color-bg)"
                                    stroke="var(--color-text)"
                                    class="cursor-pointer"
                                    onPointerDown={(e) => dragHandle(e, c, k.frame, side)}
                                  >
                                    <title>Manija (Alt: libre)</title>
                                  </circle>
                                </>
                              );
                            }}
                          </For>
                        </Show>
                        <rect
                          x={xOf(k.frame) - 3.5}
                          y={yOf(show(c, k.value)) - 3.5}
                          width="7"
                          height="7"
                          transform={`rotate(45 ${xOf(k.frame)} ${yOf(show(c, k.value))})`}
                          fill={selected() ? "var(--color-accent)" : c.row.color}
                          stroke="var(--color-bg-darker)"
                          class="cursor-move"
                          onPointerDown={(e) => dragKeys(e, c, k.frame)}
                        >
                          <title>{`${c.row.label} · cuadro ${k.frame} · ${Number(k.value.toFixed(3))}`}</title>
                        </rect>
                      </>
                    );
                  }}
                </For>
              </g>
            )}
          </For>
          <Show when={box()}>
            {(b) => (
              <rect
                x={Math.min(b().x0, b().x1)}
                y={Math.min(b().y0, b().y1)}
                width={Math.abs(b().x1 - b().x0)}
                height={Math.abs(b().y1 - b().y0)}
                fill="var(--color-accent)"
                fill-opacity="0.1"
                stroke="var(--color-accent)"
                stroke-dasharray="3 2"
              />
            )}
          </Show>
        </svg>
      </div>
    </div>
  );
};
