// Partes del panel del ensamble (PLAN_ENSAMBLE EN2–EN11): elegir y editar uniones con vista
// previa, estado de las uniones, piezas (ocultar, suprimir, color, espejo, repetir, reemplazar,
// grupos), tornillos que se ponen solos, animación por pasos y explosión por pasos.

import { Component, For, Show, createEffect, createMemo, createSignal, on, onCleanup } from "solid-js";
import { clsx } from "clsx";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, IconButton, Select, Slider, Tooltip } from "../ui";
import * as Icons from "../icons";
import { t as tl } from "../../lib/i18n";
import { tBackend } from "../../lib/i18nBackend";
import type { CadUi } from "../../lib/cadUi";
import { MATE_LABELS, resolvedAssembly, samePart, type AsmInstance, type Assembly, type CadStore, type Connector, type Mate, type MateKind, type MotionStep, type P3, type PartView } from "../../lib/cad";
import { adjustConnector, freshId, patternCopies, rotateInstance } from "../../lib/assemblyTools";

// ─── Controles chicos ─────────────────────────────────────────────────────

const Num: Component<{ value: number; onCommit: (v: number) => void; label?: string; suffix?: string; step?: number; min?: number; title?: string }> = (props) => (
  <label class="flex min-w-0 flex-1 items-center gap-1 text-xs" title={props.title}>
    <Show when={props.label}>
      <span class="shrink-0 text-text-muted">{props.label}</span>
    </Show>
    <input
      type="number"
      value={Number.isFinite(props.value) ? +props.value.toFixed(6) : 0}
      step={props.step ?? 1}
      min={props.min}
      class="w-0 min-w-0 flex-1 rounded border border-border bg-surface/40 px-1.5 py-0.5 font-mono text-xs text-text outline-none focus:border-accent [appearance:textfield]"
      onChange={(e) => {
        const v = parseFloat(e.currentTarget.value);
        if (!Number.isNaN(v) && v !== props.value) props.onCommit(v);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        e.stopPropagation();
      }}
    />
    <Show when={props.suffix}>
      <span class="shrink-0 text-text-dim">{props.suffix}</span>
    </Show>
  </label>
);

const fmt = (v: number, d = 2) => (Math.abs(v) < 0.5 * 10 ** -d ? 0 : v).toFixed(d);

const turns = (k: MateKind) => k === "revolute" || k === "cylindrical" || k === "pin_slot";
const slides = (k: MateKind) => k === "slider" || k === "cylindrical" || k === "pin_slot";

// ─── Elegir el punto de unión ─────────────────────────────────────────────

/**
 * Pide en el visor un punto de unión (cara, arista o vértice) y devuelve su conector ligado a
 * la pieza; `null` si se canceló o falló
 */
export function pickConnector(store: CadStore, ui: CadUi, prompt: string): Promise<Connector | null> {
  return new Promise((resolve) => {
    let done = false;
    ui.setPick({
      kind: "asm_point",
      prompt,
      done: async (at) => {
        done = true;
        try {
          resolve(await store.assemblyConnectorAt(at));
        } catch (e) {
          ui.setMessage(tBackend(String(e)));
          resolve(null);
        }
      },
    });
    // Si se cancela (Esc u otra herramienta), se avisa
    const stop = setInterval(() => {
      if (done) return clearInterval(stop);
      if (ui.pick().kind !== "asm_point") {
        clearInterval(stop);
        resolve(null);
      }
    }, 200);
  });
}

const POINT_PROMPT = () => tl("Clic en una cara, una arista (su centro si es redonda; Alt: su punto medio) o un vértice");

/**
 * Arma una unión nueva: el punto de la primera pieza, el de la segunda (y la segunda cara de la
 * primera si es centrada). La unión se agrega enseguida (la pieza se mueve) y queda en edición
 */
export async function newMate(store: CadStore, ui: CadUi, kind: MateKind, setStep: (s: string) => void): Promise<number | null> {
  try {
    setStep("a");
    const a = await pickConnector(store, ui, tl("Pieza 1: ") + POINT_PROMPT());
    if (!a) return null;
    ui.setAsmConnectors([a]);
    let c: Connector | null = null;
    if (kind === "width") {
      setStep("c");
      c = await pickConnector(store, ui, tl("La otra cara de la pieza 1 (la unión queda centrada entre las dos)"));
      if (!c) return null;
      ui.setAsmConnectors([a, c]);
    }
    setStep("b");
    const b = await pickConnector(store, ui, tl("Pieza 2: ") + POINT_PROMPT());
    if (!b) return null;
    if (b.instance === a.instance) {
      ui.setMessage(tl("Las dos caras tienen que ser de instancias distintas"));
      return null;
    }
    let id = 0;
    await store.editAssembly((x) => {
      id = freshId(x);
      // Caras planas enfrentadas: normales opuestas
      const facing = a.z[0] * b.z[0] + a.z[1] * b.z[1] + a.z[2] * b.z[2] < 0;
      const flip = kind === "planar" || kind === "fastened" ? facing : false;
      x.mates.push({ id, name: `${MATE_LABELS[kind]} ${x.mates.length + 1}`, kind, a, b, ...(c ? { c } : {}), flip, ...(kind === "angle" ? { angle: 90 } : {}) });
      x.next_id = id + 1;
    });
    return id;
  } finally {
    setStep("");
    ui.setAsmConnectors([]);
  }
}

// ─── Editar una unión (EN2, EN3, EN4) ─────────────────────────────────────

const ConnectorRow: Component<{ label: string; c: Connector; name: string; lost?: string; onPick: () => void; onChange: (c: Connector) => void }> = (props) => (
  <div class="space-y-1 rounded border border-border/70 p-1" data-connector={props.label}>
    <div class="flex items-center gap-1 text-[11px]">
      <span class="flex-1 truncate text-text-muted">
        {props.label}: <span class="text-text">{props.name}</span>
      </span>
      <Tooltip content={tl("Volver a elegir la cara, arista o vértice")}>
        <Button size="sm" variant="ghost" onClick={props.onPick}>
          {tl("Elegir otra vez")}
        </Button>
      </Tooltip>
    </div>
    <Show when={props.lost}>
      <p class="text-[11px] text-error">{tBackend(props.lost!)}</p>
    </Show>
    <div class="flex items-center gap-1">
      <IconButton aria-label={tl("Girar −90°")} size="sm" variant="ghost" onClick={() => props.onChange(adjustConnector(props.c, { spin: ((props.c.spin ?? 0) - 90 + 360) % 360 }))}>
        <Icons.ArrowCounterClockwise size={11} />
      </IconButton>
      <IconButton aria-label={tl("Girar +90°")} size="sm" variant="ghost" onClick={() => props.onChange(adjustConnector(props.c, { spin: ((props.c.spin ?? 0) + 90) % 360 }))}>
        <Icons.ArrowClockwise size={11} />
      </IconButton>
      <Num label="∠" title={tl("Giro alrededor de su eje")} value={props.c.spin ?? 0} suffix="°" onCommit={(v) => props.onChange(adjustConnector(props.c, { spin: v }))} />
      <Num label="↕" title={tl("Separación a lo largo de su eje")} value={props.c.offset ?? 0} suffix="mm" onCommit={(v) => props.onChange(adjustConnector(props.c, { offset: v }))} />
      <Checkbox small label={tl("Voltear")} checked={!!props.c.reverse} onChange={(v) => props.onChange(adjustConnector(props.c, { reverse: v }))} />
    </div>
  </div>
);

/**
 * Edición de una unión con vista previa: cada cambio se aplica (la pieza se mueve) y todo junto
 * es un solo paso de deshacer; «Cancelar» la saca (nueva) o la deja como estaba
 */
export const MateEditor: Component<{ store: CadStore; ui: CadUi; mateId: number; isNew: boolean; onClose: () => void }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly;
  const mate = () => asm()?.mates.find((m) => m.id === props.mateId);
  const snapshot = structuredClone(mate());
  // La primera edición de una unión vieja abre un paso de deshacer; las demás (y las de una nueva) se suman
  let joined = props.isNew;
  const set = (f: (x: Mate) => void) => {
    const join = joined;
    joined = true;
    return store.editAssembly((a) => {
      const x = a.mates.find((m) => m.id === props.mateId);
      if (x) f(x);
    }, join);
  };
  props.ui.setAsmHoverMate(props.mateId);
  onCleanup(() => props.ui.setAsmHoverMate(undefined));
  // Si la unión desaparece (deshacer, borrar la pieza) se cierra
  createEffect(() => {
    if (store.doc() && !mate()) props.onClose();
  });
  const instName = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
  const lostOf = (instance: number) => store.result()?.assembly?.lost?.find((l) => l.mate === props.mateId && l.instance === instance)?.message;
  const error = () => store.result()?.assembly?.solution.mate_errors?.find(([id]) => id === props.mateId)?.[1];
  const repick = async (side: "a" | "b" | "c") => {
    const c = await pickConnector(store, props.ui, POINT_PROMPT());
    if (!c) return;
    const m = mate();
    if (!m) return;
    const other = side === "b" ? m.a.instance : m.b.instance;
    if (c.instance === other) return props.ui.setMessage(tl("Las dos caras tienen que ser de instancias distintas"));
    void set((x) => {
      if (side === "c") x.c = c;
      else x[side] = c;
    });
  };
  const setKind = async (k: MateKind) => {
    const m = mate();
    if (!m) return;
    if (k === "width" && !m.c) {
      const c = await pickConnector(store, props.ui, tl("La otra cara de la pieza 1 (la unión queda centrada entre las dos)"));
      if (!c) return;
      return void set((x) => {
        x.kind = k;
        x.c = c;
      });
    }
    void set((x) => {
      x.kind = k;
      if (k === "angle" && x.angle == null) x.angle = 90;
    });
  };
  const cancel = () => {
    if (props.isNew) void store.editAssembly((a) => (a.mates = a.mates.filter((m) => m.id !== props.mateId)), true);
    else if (snapshot && joined) void set((x) => Object.assign(x, structuredClone(snapshot)));
    props.onClose();
  };
  const limits = (which: "angle_limits" | "distance_limits", unit: string, span: [number, number]) => (
    <Show when={mate()}>
      {(m) => (
        <div class="flex items-center gap-1">
          <Checkbox small label={tl("Topes")} checked={m()[which] != null} onChange={(v) => void set((x) => (x[which] = v ? span : null))} />
          <Show when={m()[which]}>
            {(l) => (
              <>
                <Num label={tl("mín")} value={l()[0]} suffix={unit} onCommit={(v) => void set((x) => (x[which] = [v, l()[1]]))} />
                <Num label={tl("máx")} value={l()[1]} suffix={unit} onCommit={(v) => void set((x) => (x[which] = [l()[0], v]))} />
              </>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
  return (
    <Show when={mate()}>
      {(m) => (
        <div class="space-y-1.5 rounded border border-accent/70 bg-accent/5 p-1.5 text-xs" data-mate-editor={props.mateId}>
          <input
            class="w-full rounded border border-border bg-surface/40 px-1.5 py-0.5 text-xs text-text outline-none focus:border-accent"
            value={m().name}
            aria-label={tl("Nombre de la unión")}
            onChange={(e) => {
              const v = e.currentTarget.value.trim();
              if (v && v !== m().name) void set((x) => (x.name = v));
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              e.stopPropagation();
            }}
          />
          <Select options={(Object.keys(MATE_LABELS) as MateKind[]).map((k) => ({ value: k, label: MATE_LABELS[k] }))} value={m().kind} onChange={(v) => void setKind(v as MateKind)} />
          <ConnectorRow label={tl("Pieza 1")} c={m().a} name={instName(m().a.instance)} lost={lostOf(m().a.instance)} onPick={() => void repick("a")} onChange={(c) => void set((x) => (x.a = c))} />
          <Show when={m().kind === "width" && m().c}>
            {(c) => <ConnectorRow label={tl("Otra cara")} c={c()} name={instName(c().instance)} onPick={() => void repick("c")} onChange={(v) => void set((x) => (x.c = v))} />}
          </Show>
          <ConnectorRow label={tl("Pieza 2")} c={m().b} name={instName(m().b.instance)} lost={lostOf(m().b.instance)} onPick={() => void repick("b")} onChange={(c) => void set((x) => (x.b = c))} />
          <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
            <Checkbox small label={tl("Invertir")} checked={m().flip} onChange={(v) => void set((x) => (x.flip = v))} />
            <Checkbox small label={tl("Suprimir")} checked={!!m().suppressed} onChange={(v) => void set((x) => (x.suppressed = v || undefined))} />
          </div>
          {/* Separación (planas y pegadas) o distancia impuesta (deslizantes) */}
          <Show when={m().kind === "planar" || m().kind === "fastened" || slides(m().kind)}>
            <div class="flex items-center gap-1">
              <Checkbox
                small
                label={m().kind === "planar" || m().kind === "fastened" ? tl("Separación") : tl("Distancia")}
                checked={m().distance != null}
                onChange={(v) => void set((x) => (x.distance = v ? 0 : null))}
              />
              <Show when={m().distance != null}>
                <Num value={m().distance ?? 0} suffix="mm" onCommit={(v) => void set((x) => (x.distance = v))} />
              </Show>
            </div>
          </Show>
          <Show when={turns(m().kind) || m().kind === "planar" || m().kind === "angle"}>
            <div class="flex items-center gap-1">
              <Show
                when={m().kind !== "angle"}
                fallback={<span class="text-text-muted">{tl("Ángulo entre caras")}</span>}
              >
                <Checkbox small label={tl("Ángulo")} checked={m().angle != null} onChange={(v) => void set((x) => (x.angle = v ? 0 : null))} />
              </Show>
              <Show when={m().angle != null}>
                <Num value={m().angle ?? 0} suffix="°" onCommit={(v) => void set((x) => (x.angle = v))} />
              </Show>
            </div>
          </Show>
          <Show when={turns(m().kind)}>{limits("angle_limits", "°", [-90, 90])}</Show>
          <Show when={slides(m().kind)}>{limits("distance_limits", "mm", [0, 50])}</Show>
          <Show when={m().kind === "tangent" && !(m().a.radius || m().b.radius)}>
            <p class="text-[11px] text-warning">{tl("Para tangente, una de las dos tiene que ser una cara cilíndrica o un borde redondo")}</p>
          </Show>
          <Show when={error() !== undefined}>
            <p class="text-[11px] text-error">{tl("No se cumple (desvío {v})", { v: fmt(error()!, 3) })}</p>
          </Show>
          <div class="flex gap-1.5">
            <Button size="sm" onClick={props.onClose}>
              {tl("Aceptar")}
            </Button>
            <Button size="sm" variant="ghost" onClick={cancel}>
              {tl("Cancelar")}
            </Button>
          </div>
        </div>
      )}
    </Show>
  );
};

/** Marcas de una unión en la lista: no se cumple, sobra, perdió su cara, suprimida */
export function mateBadges(store: CadStore, m: Mate): { text: string; tone: "error" | "warning" | "dim" }[] {
  const view = store.result()?.assembly;
  const out: { text: string; tone: "error" | "warning" | "dim" }[] = [];
  if (m.suppressed) out.push({ text: tl("suprimida"), tone: "dim" });
  if (view?.lost?.some((l) => l.mate === m.id)) out.push({ text: tl("perdió su cara"), tone: "error" });
  const err = view?.solution.mate_errors?.find(([id]) => id === m.id);
  if (err) out.push({ text: tl("no se cumple"), tone: "error" });
  else if (view?.solution.redundant?.includes(m.id)) out.push({ text: tl("sobra"), tone: "warning" });
  return out;
}

// ─── Estado (EN5) ─────────────────────────────────────────────────────────

export const HealthSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly;
  const view = () => store.result()?.assembly;
  const name = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
  const mateName = (id: number) => asm()?.mates.find((m) => m.id === id)?.name ?? `#${id}`;
  const lost = () => view()?.lost ?? [];
  const moving = () => view()?.solution.instance_dof ?? [];
  return (
    <div class="space-y-1" data-assembly-health>
      <Show when={lost().length > 0}>
        <div class="space-y-0.5 rounded border border-error/60 bg-error/5 p-1.5">
          <For each={lost()}>
            {(l) => (
              <p class="text-[11px] text-error">
                {tl("«{mate}» perdió su cara en {inst}", { mate: mateName(l.mate), inst: name(l.instance) })}
              </p>
            )}
          </For>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              const ids = new Set(lost().map((l) => l.mate));
              void store.editAssembly((a) => (a.mates = a.mates.filter((m) => !ids.has(m.id))));
            }}
          >
            {tl("Quitar las uniones perdidas")}
          </Button>
        </div>
      </Show>
      <Show when={moving().length > 0}>
        <div class="flex items-center gap-1.5">
          <Checkbox small label={tl("Resaltar las que se mueven")} checked={props.ui.asmShowMoving()} onChange={props.ui.setAsmShowMoving} />
        </div>
        <p class="text-[11px] text-text-dim">{moving().map(([id, n]) => `${name(id)} (${n})`).join(", ")}</p>
      </Show>
    </div>
  );
};

// ─── Piezas (EN8) ─────────────────────────────────────────────────────────

const MIRRORS: { value: string; label: () => string; n: P3 | null }[] = [
  { value: "", label: () => tl("Sin espejo"), n: null },
  { value: "x", label: () => tl("Espejo en YZ (invierte X)"), n: [1, 0, 0] },
  { value: "y", label: () => tl("Espejo en XZ (invierte Y)"), n: [0, 1, 0] },
  { value: "z", label: () => tl("Espejo en XY (invierte Z)"), n: [0, 0, 1] },
];
const mirrorKey = (n: P3 | null | undefined) => (!n ? "" : Math.abs(n[0]) > 0.5 ? "x" : Math.abs(n[1]) > 0.5 ? "y" : "z");
const AXES: Record<string, P3> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

/** Botones y opciones de una instancia: ocultar, suprimir, color, espejo, repetir, reemplazar */
export const InstanceExtras: Component<{ store: CadStore; ui: CadUi; inst: AsmInstance; parts: PartView[]; isSub: boolean }> = (props) => {
  const store = props.store;
  const [open, setOpen] = createSignal(false);
  const [repeat, setRepeat] = createSignal<"linear" | "circular" | "points">("linear");
  const [count, setCount] = createSignal(3);
  const [step, setStep] = createSignal(20);
  const [axis, setAxis] = createSignal("x");
  const edit = (f: (i: AsmInstance, a: Assembly) => void) =>
    void store.editAssembly((a) => {
      const i = a.instances.find((x) => x.id === props.inst.id);
      if (i) f(i, a);
    });
  const pattern = () => store.doc()?.assembly?.patterns?.find((p) => p.source === props.inst.id);
  const copyOf = () => store.doc()?.assembly?.patterns?.find((p) => p.copies.includes(props.inst.id));
  const makeRepeat = async () => {
    const n = Math.max(1, Math.round(count()) - 1);
    if (repeat() === "points") {
      try {
        const r = await store.assemblyHolePattern(props.inst.id);
        return void store.editAssembly((a) => {
          const copies = patternCopies(a, props.inst.id, r.offsets.length);
          for (const c of copies) a.instances.push({ ...structuredClone(props.inst), id: c.id, name: c.name, fixed: false });
          const id = freshId(a);
          (a.patterns ??= []).push({ id, source: props.inst.id, kind: { type: "points", reference: r.reference, offsets: r.offsets }, copies: copies.map((c) => c.id) });
          a.next_id = id + 1;
        });
      } catch (e) {
        return props.ui.setMessage(tBackend(String(e)));
      }
    }
    void store.editAssembly((a) => {
      const copies = patternCopies(a, props.inst.id, n);
      for (const c of copies) a.instances.push({ ...structuredClone(props.inst), id: c.id, name: c.name, fixed: false });
      const id = freshId(a);
      const kind = repeat() === "linear" ? { type: "linear" as const, dir: AXES[axis()], step: step() } : { type: "circular" as const, origin: [0, 0, 0] as P3, axis: AXES[axis()], angle: step() };
      (a.patterns ??= []).push({ id, source: props.inst.id, kind, copies: copies.map((c) => c.id) });
      a.next_id = id + 1;
    });
  };
  const mirrorCopy = (n: P3) =>
    void store.editAssembly((a) => {
      const id = freshId(a);
      a.instances.push({ ...structuredClone(props.inst), id, name: `${props.inst.name} (${tl("espejo")})`, mirror: n, fixed: false, position: [props.inst.position[0] + 10, props.inst.position[1], props.inst.position[2]] });
      a.next_id = id + 1;
    });
  const others = () => props.parts.filter((p) => !samePart(p.id, props.inst.part));
  return (
    <div class="space-y-1">
      <div class="flex items-center gap-1">
        <IconButton
          aria-label={props.inst.hidden ? tl("Mostrar") : tl("Ocultar")}
          size="sm"
          variant="ghost"
          onClick={() => edit((i) => (i.hidden = !i.hidden || undefined))}
        >
          {props.inst.hidden ? <Icons.EyeSlash size={11} /> : <Icons.Eye size={11} />}
        </IconButton>
        <IconButton
          aria-label={props.inst.ghost ? tl("Volver a verla sólida") : tl("Verla translúcida (se ve y se toca lo de atrás)")}
          aria-pressed={!!props.inst.ghost}
          size="sm"
          variant="ghost"
          active={!!props.inst.ghost}
          onClick={() => edit((i) => (i.ghost = !i.ghost || undefined))}
        >
          <Icons.XRay size={11} />
        </IconButton>
        <Checkbox small label={tl("Suprimir")} checked={!!props.inst.suppressed} onChange={(v) => edit((i) => (i.suppressed = v || undefined))} />
        <Tooltip content={tl("Color propio en el ensamble")}>
          <input
            type="color"
            class="h-5 w-6 cursor-pointer rounded border border-border bg-transparent"
            value={props.inst.color ?? "#9aa4b8"}
            onChange={(e) => {
              const v = e.currentTarget.value;
              edit((i) => (i.color = v));
            }}
          />
        </Tooltip>
        <Show when={props.inst.color}>
          <Button size="sm" variant="ghost" onClick={() => edit((i) => (i.color = null))}>
            {tl("Color de la pieza")}
          </Button>
        </Show>
        <span class="flex-1" />
        <Button size="sm" variant="ghost" onClick={() => setOpen(!open())}>
          {open() ? tl("Menos") : tl("Más…")}
        </Button>
      </div>
      <Show when={copyOf()}>
        <p class="text-[11px] text-text-dim">{tl("Copia de un patrón: sigue a su original")}</p>
      </Show>
      <Show when={!props.inst.fixed && !copyOf()}>
        {/* Girar en pasos de 90° alrededor de los ejes del mundo (por su origen) */}
        <div class="flex items-center gap-1 text-[11px]">
          <span class="text-text-muted">{tl("Girar 90°")}</span>
          <For each={["x", "y", "z"]}>
            {(k) => (
              <Button size="sm" variant="ghost" onClick={() => edit((i) => (i.rotation = rotateInstance(i.position, i.rotation, AXES[k], 90)))}>
                {k.toUpperCase()}
              </Button>
            )}
          </For>
        </div>
      </Show>
      <Show when={open()}>
        <div class="space-y-1.5 rounded border border-border/70 p-1.5">
          <div class="flex items-center gap-1">
            <span class="w-20 shrink-0 text-[11px] text-text-muted">{tl("Espejo")}</span>
            <div class="flex-1">
              <Select options={MIRRORS.map((m) => ({ value: m.value, label: m.label() }))} value={mirrorKey(props.inst.mirror)} onChange={(v) => edit((i) => (i.mirror = MIRRORS.find((m) => m.value === v)?.n ?? null))} />
            </div>
          </div>
          <div class="flex flex-wrap gap-1">
            <For each={MIRRORS.slice(1)}>
              {(m) => (
                <Button size="sm" variant="ghost" onClick={() => mirrorCopy(m.n!)}>
                  {tl("Copia en espejo {axis}", { axis: m.value.toUpperCase() })}
                </Button>
              )}
            </For>
          </div>
          <Show
            when={!pattern()}
            fallback={
              <div class="flex items-center gap-1">
                <span class="flex-1 text-[11px] text-text-dim">{tl("Repetida {n} veces", { n: (pattern()?.copies.length ?? 0) + 1 })}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    void store.editAssembly((a) => {
                      const p = a.patterns?.find((x) => x.source === props.inst.id);
                      if (!p) return;
                      a.instances = a.instances.filter((i) => !p.copies.includes(i.id));
                      a.patterns = a.patterns?.filter((x) => x !== p);
                    })
                  }
                >
                  {tl("Quitar las copias")}
                </Button>
              </div>
            }
          >
            <div class="space-y-1">
              <Select
                options={[
                  { value: "linear", label: tl("Repetir en fila") },
                  { value: "circular", label: tl("Repetir en círculo") },
                  { value: "points", label: tl("Repetir en los otros agujeros iguales") },
                ]}
                value={repeat()}
                onChange={(v) => setRepeat(v as "linear" | "circular" | "points")}
              />
              <Show when={repeat() !== "points"}>
                <div class="flex items-center gap-1">
                  <Num label={tl("Total")} value={count()} step={1} min={2} onCommit={(v) => setCount(Math.max(2, Math.round(v)))} />
                  <Num label={repeat() === "linear" ? tl("Cada") : tl("Cada")} value={step()} suffix={repeat() === "linear" ? "mm" : "°"} onCommit={setStep} />
                  <div class="w-16">
                    <Select options={["x", "y", "z"].map((k) => ({ value: k, label: k.toUpperCase() }))} value={axis()} onChange={setAxis} />
                  </div>
                </div>
                <p class="text-[11px] text-text-dim">
                  {repeat() === "linear" ? tl("A lo largo del eje de la pieza original") : tl("Alrededor del eje de la pieza original, por su origen")}
                </p>
              </Show>
              <Button size="sm" onClick={() => void makeRepeat()}>
                {tl("Repetir")}
              </Button>
            </div>
          </Show>
          <Show when={others().length > 0}>
            <div class="flex items-center gap-1">
              <span class="w-20 shrink-0 text-[11px] text-text-muted">{tl("Reemplazar por")}</span>
              <div class="flex-1">
                <Select
                  options={[{ value: "", label: tl("Otra pieza…") }, ...others().map((p) => ({ value: `${p.id.feature}:${p.id.index}`, label: p.name }))]}
                  value=""
                  onChange={(v) => {
                    const p = others().find((x) => `${x.id.feature}:${x.id.index}` === v);
                    if (!p) return;
                    edit((i) => {
                      i.part = p.id;
                      i.name = p.name;
                    });
                    props.ui.setMessage(tl("Reemplazada: las uniones cuyas caras están en la pieza nueva se conservan; las otras quedan marcadas como perdidas"));
                  }}
                />
              </div>
            </div>
          </Show>
          <Show when={!props.isSub && props.store.activeTab()?.kind === "assembly"}>
            <Tooltip content={tl("Abre el diseño de esta pieza con las otras del ensamble en su lugar, solo de referencia: se dibuja sobre sus caras y las sigue si cambian")}>
              <Button
                size="sm"
                onClick={() =>
                  void props.store.editInContext(props.inst.id).then(
                    (r) => props.ui.setMessage(tl("En contexto: {n} piezas de referencia", { n: r.added + r.updated })),
                    (e) => props.ui.setMessage(tBackend(String(e))),
                  )
                }
              >
                {tl("Editar en contexto")}
              </Button>
            </Tooltip>
          </Show>
          <Show when={props.isSub}>
            <Checkbox small label={tl("Subensamble flexible (sus piezas se mueven aquí)")} checked={!!props.inst.flexible} onChange={(v) => edit((i) => (i.flexible = v || undefined))} />
          </Show>
        </div>
      </Show>
    </div>
  );
};

/** Grupos: piezas que se mueven juntas sin uniones */
export const GroupsSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly;
  const name = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
  return (
    <div class="space-y-1" data-groups>
      <Tooltip content={tl("Las elegidas (clic en el visor o la lista; Mayús suma) se mueven juntas, guardando su lugar")}>
        <Button
          size="sm"
          disabled={props.ui.asmPicked().length < 2}
          onClick={() =>
            void store.editAssembly((a) => {
              const id = freshId(a);
              const members = props.ui.asmPicked().filter((m) => a.instances.some((i) => i.id === m));
              // Una pieza va en un solo grupo
              a.groups = (a.groups ?? []).map((g) => ({ ...g, members: g.members.filter((m) => !members.includes(m)) }));
              a.groups.push({ id, name: tl("Grupo {n}", { n: a.groups.length + 1 }), members });
              a.next_id = id + 1;
            })
          }
        >
          {tl("Agrupar las elegidas")}
        </Button>
      </Tooltip>
      <For each={asm()?.groups ?? []}>
        {(g) => (
          <div class="flex items-center gap-1 rounded border border-border px-1.5 py-1 text-xs" data-group={g.id}>
            <button type="button" class="flex-1 truncate text-left" onClick={() => props.ui.setAsmPicked([...g.members])} title={g.members.map(name).join(", ")}>
              {g.name}: {g.members.map(name).join(", ")}
            </button>
            <Button size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.groups = (a.groups ?? []).filter((x) => x.id !== g.id)))}>
              {tl("Desagrupar")}
            </Button>
          </div>
        )}
      </For>
    </div>
  );
};

// ─── Tornillos (EN9) ──────────────────────────────────────────────────────

export const FastenSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const [faces, setFaces] = createSignal<number[]>([]);
  const [nut, setNut] = createSignal(false);
  const [washer, setWasher] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const picking = () => props.ui.pick().kind === "asm_faces";
  // Los agujeros elegidos, resaltados (solo mientras se eligen: no se pisa otro resaltado)
  createEffect(on(faces, (f) => props.ui.setHighlight({ faces: f, edges: [] }), { defer: true }));
  // Al cambiar el ensamble (se pusieron los tornillos), la numeración de caras ya no vale
  createEffect(on(() => props.store.result()?.assembly?.version, () => setFaces([]), { defer: true }));
  onCleanup(() => {
    if (picking()) props.ui.setPick({ kind: "none" });
  });
  const choose = () =>
    props.ui.setPick({
      kind: "asm_faces",
      prompt: tl("Clic en los agujeros (sus caras cilíndricas); otra vez lo quita"),
      toggle: (f) => setFaces(faces().includes(f) ? faces().filter((x) => x !== f) : [...faces(), f]),
    });
  const put = async () => {
    setBusy(true);
    try {
      const r = await props.store.fasten(faces(), nut(), washer(), props.ui.viewerEye());
      props.ui.setPick({ kind: "none" });
      setFaces([]);
      props.ui.setMessage([tl("{n} piezas agregadas", { n: r.added }), ...r.notes.map(tBackend)].join(" · "));
    } catch (e) {
      props.ui.setMessage(tBackend(String(e)));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="space-y-1" data-fasten>
      <div class="flex items-center gap-3">
        <Checkbox small label={tl("Arandela")} checked={washer()} onChange={setWasher} />
        <Checkbox small label={tl("Tuerca")} checked={nut()} onChange={setNut} />
      </div>
      <div class="flex gap-1.5">
        <Button size="sm" variant={picking() ? "primary" : undefined} onClick={() => (picking() ? props.ui.setPick({ kind: "none" }) : choose())}>
          {picking() ? tl("Listo") : tl("Elegir agujeros")}
        </Button>
        <Button size="sm" disabled={!faces().length || busy()} onClick={() => void put()}>
          {tl("Poner {n} tornillos", { n: faces().length })}
        </Button>
      </div>
      <p class="text-[11px] text-text-dim">{tl("Tornillo de la medida y el largo justos, con la cabeza del lado de la cámara, ya unido al agujero. Sale en la lista de materiales.")}</p>
    </div>
  );
};

// ─── Animación por pasos (EN10) ───────────────────────────────────────────

export const MotionSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly;
  const steps = () => asm()?.motion ?? [];
  const movable = () => (asm()?.mates ?? []).filter((m) => turns(m.kind) || slides(m.kind));
  const [frames, setFrames] = createSignal<[number, P3, P3][][] | null>(null);
  const [at, setAt] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [clash, setClash] = createSignal<string>();
  const FPS = 30;
  let raf = 0;
  let started = 0;
  // Si el ensamble cambia, los cuadros calculados ya no valen
  createEffect(on(() => store.result()?.assembly?.version, () => stop(true), { defer: true }));
  onCleanup(() => stop(true));
  const stop = (reset = false) => {
    cancelAnimationFrame(raf);
    setPlaying(false);
    if (reset) {
      setFrames(null);
      setAt(0);
      props.ui.setAsmFrame(null);
    }
  };
  const show = (k: number) => {
    const f = frames();
    if (!f?.length) return;
    const i = Math.max(0, Math.min(f.length - 1, Math.round(k)));
    setAt(i);
    props.ui.setAsmFrame(f[i]);
  };
  const ensure = async () => {
    if (frames()) return frames();
    setBusy(true);
    try {
      const f = await store.assemblySweep(steps(), FPS);
      setFrames(f);
      return f;
    } catch (e) {
      props.ui.setMessage(tBackend(String(e)));
      return null;
    } finally {
      setBusy(false);
    }
  };
  const play = async () => {
    const f = await ensure();
    if (!f?.length) return;
    if (at() >= f.length - 1) setAt(0);
    setPlaying(true);
    started = performance.now() - (at() / FPS) * 1000;
    const tick = () => {
      const k = ((performance.now() - started) / 1000) * FPS;
      if (k >= f.length - 1) {
        show(f.length - 1);
        return setPlaying(false);
      }
      show(k);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  };
  const editStep = (k: number, f: (s: MotionStep) => void) =>
    void store.editAssembly((a) => {
      const s = a.motion?.[k];
      if (s) f(s);
    });
  const addStep = () => {
    const m = movable()[0];
    if (!m) return;
    const v: MotionStep["var"] = turns(m.kind) ? "angle" : "distance";
    void store.editAssembly((a) => (a.motion = [...(a.motion ?? []), { mate: m.id, var: v, from: 0, to: v === "angle" ? 90 : 20, seconds: 1 }]));
  };
  const firstClash = async (s: MotionStep) => {
    setClash(undefined);
    try {
      const r = await store.assemblyFirstClash(s.mate, s.var, s.from, s.to, 24);
      const name = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
      setClash(r ? tl("Chocan {a} y {b} en {v}{u}", { a: name(r[2]), b: name(r[3]), v: fmt(r[1], 1), u: s.var === "angle" ? "°" : " mm" }) : tl("Ningún choque en todo el recorrido"));
    } catch (e) {
      setClash(tBackend(String(e)));
    }
  };
  const exportGlb = async () => {
    const path = await save({ filters: [{ name: "GLB", extensions: ["glb"] }], defaultPath: "ensamble_animado.glb" });
    if (!path) return;
    try {
      const bytes = await store.exportAssemblyAnimation(path, 0, "angle", 0, 0, 30, FPS, steps());
      props.ui.setMessage(tl("Exportado ({kb} KB)", { kb: Math.max(1, Math.round(bytes / 1024)) }));
    } catch (e) {
      props.ui.setMessage(tBackend(String(e)));
    }
  };
  const total = createMemo(() => frames()?.length ?? 0);
  return (
    <Show when={movable().length > 0} fallback={<p class="text-[11px] text-text-dim">{tl("Hace falta una bisagra, deslizante, cilíndrica o pasador en ranura para animar.")}</p>}>
      <div class="space-y-1.5" data-motion>
        <For each={steps()}>
          {(s, k) => {
            const m = () => asm()?.mates.find((x) => x.id === s.mate);
            return (
              <div class="space-y-1 rounded border border-border p-1.5 text-xs" data-motion-step={k()}>
                <div class="flex items-center gap-1">
                  <span class="w-5 shrink-0 text-text-dim">{k() + 1}.</span>
                  <div class="flex-1">
                    <Select
                      options={movable().map((x) => ({ value: String(x.id), label: x.name || MATE_LABELS[x.kind] }))}
                      value={String(s.mate)}
                      onChange={(v) =>
                        editStep(k(), (x) => {
                          x.mate = +v;
                          const kind = asm()?.mates.find((y) => y.id === +v)?.kind;
                          if (kind && !turns(kind)) x.var = "distance";
                          if (kind && !slides(kind)) x.var = "angle";
                        })
                      }
                    />
                  </div>
                  <IconButton aria-label={tl("Quitar paso")} size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.motion = (a.motion ?? []).filter((_, i) => i !== k())))}>
                    <Icons.Trash size={11} />
                  </IconButton>
                </div>
                <Show when={m() && turns(m()!.kind) && slides(m()!.kind)}>
                  <Select
                    options={[
                      { value: "angle", label: tl("Gira") },
                      { value: "distance", label: tl("Desliza") },
                    ]}
                    value={s.var}
                    onChange={(v) => editStep(k(), (x) => (x.var = v as MotionStep["var"]))}
                  />
                </Show>
                <div class="flex items-center gap-1">
                  <Num label={tl("Desde")} value={s.from} suffix={s.var === "angle" ? "°" : "mm"} onCommit={(v) => editStep(k(), (x) => (x.from = v))} />
                  <Num label={tl("Hasta")} value={s.to} suffix={s.var === "angle" ? "°" : "mm"} onCommit={(v) => editStep(k(), (x) => (x.to = v))} />
                  <Num label={tl("Dura")} value={s.seconds} step={0.1} min={0.1} suffix="s" onCommit={(v) => editStep(k(), (x) => (x.seconds = Math.max(0.1, v)))} />
                </div>
                <Button size="sm" variant="ghost" onClick={() => void firstClash(s)}>
                  {tl("¿Choca en el recorrido?")}
                </Button>
              </div>
            );
          }}
        </For>
        <Show when={clash()}>
          <p class="text-[11px] text-warning">{clash()}</p>
        </Show>
        <Button size="sm" variant="ghost" onClick={addStep}>
          <Icons.Plus size={11} /> {tl("Agregar paso")}
        </Button>
        <Show when={steps().length > 0}>
          <div class="flex items-center gap-1.5">
            <IconButton aria-label={playing() ? tl("Pausa") : tl("Reproducir")} size="sm" disabled={busy()} onClick={() => (playing() ? stop() : void play())}>
              {playing() ? <Icons.Pause size={12} /> : <Icons.Play size={12} />}
            </IconButton>
            <div class="flex-1">
              <Slider
                value={at()}
                min={0}
                max={Math.max(1, total() - 1)}
                step={1}
                onChange={async (v) => {
                  stop();
                  if (await ensure()) show(v);
                }}
              />
            </div>
            <span class="w-12 text-right font-mono text-[11px]">{fmt(at() / FPS, 1)} s</span>
            <IconButton aria-label={tl("Volver al ensamble")} size="sm" variant="ghost" onClick={() => stop(true)}>
              <Icons.Stop size={11} />
            </IconButton>
          </div>
          <Button size="sm" fullWidth onClick={() => void exportGlb()}>
            {tl("Exportar GLB animado")}
          </Button>
        </Show>
        <p class="text-[11px] text-text-dim">{tl("Cada paso mueve una unión y la deja donde terminó: primero abre la tapa, después gira la manija.")}</p>
      </div>
    </Show>
  );
};

// ─── Explosión por pasos (EN11) ───────────────────────────────────────────

const DIRS: { value: string; label: string; v: P3 }[] = [
  { value: "+z", label: "+Z", v: [0, 0, 1] },
  { value: "-z", label: "−Z", v: [0, 0, -1] },
  { value: "+x", label: "+X", v: [1, 0, 0] },
  { value: "-x", label: "−X", v: [-1, 0, 0] },
  { value: "+y", label: "+Y", v: [0, 1, 0] },
  { value: "-y", label: "−Y", v: [0, -1, 0] },
];
const dirKey = (v: P3) => DIRS.find((d) => d.v.every((x, k) => Math.abs(x - v[k]) < 1e-6))?.value ?? "+z";

export const ExplodeSection: Component<{ store: CadStore; ui: CadUi }> = (props) => {
  const store = props.store;
  const asm = () => store.doc()?.assembly;
  const steps = () => asm()?.explode ?? [];
  const name = (id: number) => asm()?.instances.find((i) => i.id === id)?.name ?? `#${id}`;
  let raf = 0;
  onCleanup(() => cancelAnimationFrame(raf));
  const play = () => {
    cancelAnimationFrame(raf);
    const n = steps().length;
    const t0 = performance.now();
    const tick = () => {
      const v = Math.min(n, (performance.now() - t0) / 1000);
      props.ui.setAsmExplodeAt(v);
      if (v < n) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  };
  const edit = (k: number, f: (s: { instances: number[]; dir: P3; distance: number }) => void) =>
    void store.editAssembly((a) => {
      const s = a.explode?.[k];
      if (s) f(s);
    });
  return (
    <div class="space-y-1.5" data-explode>
      <Show
        when={steps().length > 0}
        fallback={
          <>
            <div class="flex items-center gap-2 text-[11px]">
              <Slider value={props.ui.assemblyExplode()} min={0} max={2} step={0.05} onChange={(v) => props.ui.setAssemblyExplode(v)} />
              <span class="w-10 text-right font-mono">{Math.round(props.ui.assemblyExplode() * 100)}%</span>
            </div>
            <p class="text-[11px] text-text-dim">{tl("Aleja las piezas del centro para ver cómo se arma. Solo en pantalla: no cambia el ensamble ni lo que se exporta.")}</p>
          </>
        }
      >
        <For each={steps()}>
          {(s, k) => (
            <div class="space-y-1 rounded border border-border p-1.5 text-xs" data-explode-step={k()}>
              <div class="flex items-center gap-1">
                <span class="w-5 shrink-0 text-text-dim">{k() + 1}.</span>
                <button type="button" class="flex-1 truncate text-left" title={s.instances.map(name).join(", ")} onClick={() => props.ui.setAsmPicked([...s.instances])}>
                  {s.instances.map(name).join(", ")}
                </button>
                <IconButton aria-label={tl("Quitar paso")} size="sm" variant="ghost" onClick={() => void store.editAssembly((a) => (a.explode = (a.explode ?? []).filter((_, i) => i !== k())))}>
                  <Icons.Trash size={11} />
                </IconButton>
              </div>
              <div class="flex items-center gap-1">
                <div class="w-16">
                  <Select options={DIRS.map((d) => ({ value: d.value, label: d.label }))} value={dirKey(s.dir)} onChange={(v) => edit(k(), (x) => (x.dir = DIRS.find((d) => d.value === v)!.v))} />
                </div>
                <Num value={s.distance} suffix="mm" onCommit={(v) => edit(k(), (x) => (x.distance = v))} />
              </div>
            </div>
          )}
        </For>
        <div class="flex items-center gap-1.5">
          <IconButton aria-label={tl("Reproducir")} size="sm" onClick={play}>
            <Icons.Play size={12} />
          </IconButton>
          <div class="flex-1">
            <Slider value={props.ui.asmExplodeAt()} min={0} max={steps().length} step={0.01} onChange={(v) => props.ui.setAsmExplodeAt(v)} />
          </div>
          <span class="w-14 text-right font-mono text-[11px]">
            {fmt(props.ui.asmExplodeAt(), 1)}/{steps().length}
          </span>
        </div>
      </Show>
      <Tooltip content={tl("Las piezas elegidas (clic en el visor o la lista; Mayús suma) se corren juntas en un paso")}>
        <Button
          size="sm"
          disabled={!props.ui.asmPicked().length}
          onClick={() => {
            void store.editAssembly((a) => (a.explode = [...(a.explode ?? []), { instances: [...props.ui.asmPicked()], dir: [0, 0, 1], distance: 30 }]));
            props.ui.setAsmExplodeAt(steps().length + 1);
          }}
        >
          {tl("Agregar paso con las elegidas")}
        </Button>
      </Tooltip>
      <Show when={steps().length > 0}>
        <p class="text-[11px] text-text-dim">{tl("Para llevarla al plano 2D, marcar «Explosionada» en la hoja.")}</p>
      </Show>
    </div>
  );
};

/** Las uniones con los conectores rehechos (para el arrastre y la vista) */
export const resolvedOf = (store: CadStore): Assembly | undefined => {
  const a = store.doc()?.assembly;
  return a ? resolvedAssembly(a, store.result()?.assembly) : undefined;
};

/** Clase del estado de una unión en la lista */
export const badgeClass = (tone: "error" | "warning" | "dim") => clsx("rounded px-1 text-[10px]", tone === "error" ? "bg-error/15 text-error" : tone === "warning" ? "bg-warning/15 text-warning" : "bg-surface text-text-dim");
