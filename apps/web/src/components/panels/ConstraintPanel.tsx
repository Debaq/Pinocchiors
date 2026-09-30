import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import { CONSTRAINT_TYPES, controlTarget, type ConstraintType, type RigConstraint } from "../../lib/constraints";
import type { RigControl } from "../../lib/rig";

export interface ConstraintPanelProps {
  constraints: RigConstraint[];
  /** Nombres de las articulaciones animables */
  joints: string[];
  controls: RigControl[];
  selected?: string;
  /** Hay rig en el visor */
  posing: boolean;
  /** Articulación activa (la dueña de una restricción nueva) */
  activeJoint?: string;
  /** Otras articulaciones elegidas (objetivo o cadena de una nueva) */
  otherJoints: string[];
  /** Influencia y valor del driver en el cuadro actual */
  influence: number;
  value: number;
  keyed: { influence: boolean; value: boolean };
  onNew: (type: ConstraintType) => void;
  onSelect: (id: string) => void;
  onChange: (description: string, constraint: RigConstraint) => void;
  onDelete: (id: string) => void;
  onMove: (id: string, direction: -1 | 1) => void;
  /** Influencia (`blend`) o valor del driver (`roll`): provisorio al arrastrar, key al soltar */
  onScalar: (channel: "blend" | "roll", value: number, commit: boolean) => void;
}

const AXES = ["X", "Y", "Z"] as const;
const axisOptions = AXES.map((a, i) => ({ value: String(i), label: a }));

/** Restricciones del rig: lista ordenada, creación y propiedades de la elegida */
export const ConstraintPanel: Component<ConstraintPanelProps> = (props) => {
  const [newType, setNewType] = createSignal<ConstraintType>("copyRotation");
  const current = () => props.constraints.find((c) => c.id === props.selected);
  const set = (description: string, change: Partial<RigConstraint>) => {
    const c = current();
    if (c) props.onChange(description, { ...c, ...change });
  };
  const targetOptions = () => [
    { value: "", label: "Ninguno" },
    ...props.joints.map((n) => ({ value: n, label: n })),
    ...props.controls.map((c) => ({ value: controlTarget(c.id), label: `◯ ${c.name}` })),
  ];
  const jointOptions = () => props.joints.map((n) => ({ value: n, label: n }));
  const typeOf = (t: ConstraintType) => CONSTRAINT_TYPES.find((x) => x.value === t)!;

  const scalarRow = (label: string, channel: "blend" | "roll", value: number, keyed: boolean) => (
    <label class="block space-y-1">
      <div class="flex justify-between text-xs">
        <span class="text-text-muted">
          {label} {keyed ? "◆" : ""}
        </span>
        <span class="font-mono text-text">{Math.round(value * 100)} %</span>
      </div>
      <input
        type="range"
        class="w-full accent-[var(--color-accent)]"
        min={0}
        max={100}
        value={Math.round(value * 100)}
        onInput={(e) => props.onScalar(channel, Number(e.currentTarget.value) / 100, false)}
        onChange={(e) => props.onScalar(channel, Number(e.currentTarget.value) / 100, true)}
      />
    </label>
  );

  const axesRow = (c: RigConstraint) => (
    <div class="flex items-center gap-2 text-xs">
      <span class="text-text-muted">Ejes</span>
      <For each={AXES}>
        {(a, i) => (
          <Checkbox
            label={a}
            checked={(c.axes ?? [true, true, true])[i()]}
            onChange={(on) => {
              const axes = [...(c.axes ?? [true, true, true])] as [boolean, boolean, boolean];
              axes[i()] = on;
              set("Ejes de la restricción", { axes: axes.every(Boolean) ? undefined : axes });
            }}
          />
        )}
      </For>
    </div>
  );

  /** Lista de articulaciones extra (driver y reparto) */
  const jointsRow = (c: RigConstraint) => (
    <div class="space-y-1">
      <p class="text-[11px] font-mono text-text-dim leading-relaxed break-words">{[c.owner, ...(c.joints ?? [])].join(" · ")}</p>
      <div class="grid grid-cols-2 gap-1">
        <Button
          size="sm"
          disabled={props.otherJoints.length === 0}
          onClick={() => set("Articulaciones de la restricción", { joints: [...new Set([...(c.joints ?? []), ...props.otherJoints])].filter((n) => n !== c.owner) })}
          title="Suma las articulaciones elegidas en el visor"
        >
          <Icons.Plus size={12} /> Elegidas
        </Button>
        <Button size="sm" disabled={(c.joints ?? []).length === 0} onClick={() => set("Articulaciones de la restricción", { joints: undefined })}>
          Vaciar
        </Button>
      </div>
    </div>
  );

  return (
    <Panel title="Restricciones" icon={<Icons.TreeStructure size={14} />} defaultOpen>
      <div class="space-y-3">
        <div class="rounded border border-border divide-y divide-border/60 max-h-48 overflow-y-auto">
          <For
            each={props.constraints}
            fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin restricciones. Elige una articulación (y el objetivo con Mayús) y crea una.</p>}
          >
            {(c) => (
              <button
                class={clsx(
                  "w-full flex items-center justify-between gap-2 px-2 py-1.5 text-xs text-left",
                  c.id === props.selected ? "bg-accent/15 text-accent" : c.disabled ? "text-text-dim" : "text-text-muted hover:text-text"
                )}
                onClick={() => props.onSelect(c.id)}
              >
                <span class="truncate">{c.name}</span>
                <span class="text-text-dim shrink-0">{typeOf(c.type).label}</span>
              </button>
            )}
          </For>
        </div>
        <div class="flex gap-1">
          <div class="flex-1">
            <Select options={CONSTRAINT_TYPES} value={newType()} onChange={(v) => setNewType(v as ConstraintType)} />
          </div>
          <Button size="sm" onClick={() => props.onNew(newType())} disabled={!props.activeJoint} title={typeOf(newType()).hint}>
            <Icons.Plus size={12} /> Nueva
          </Button>
        </div>

        <Show when={current()}>
          {(c) => (
            <div class="space-y-2 pt-1 border-t border-border/60">
              <input
                class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                value={c().name}
                onKeyDown={(e) => e.stopPropagation()}
                onChange={(e) => set("Renombrar restricción", { name: e.currentTarget.value.trim() || c().name })}
              />
              <Select label="Tipo" options={CONSTRAINT_TYPES} value={c().type} onChange={(v) => set("Tipo de restricción", { type: v as ConstraintType })} />
              <p class="text-[11px] text-text-dim leading-relaxed">{typeOf(c().type).hint}</p>
              <Select label="Articulación" options={jointOptions()} value={c().owner} onChange={(v) => set("Articulación de la restricción", { owner: v })} />

              <Show when={c().type !== "driver" && c().type !== "distribute"}>
                <Select
                  label={c().type === "map" ? "Hueso de origen" : "Objetivo"}
                  options={c().type === "map" ? [{ value: "", label: "Ninguno" }, ...jointOptions()] : targetOptions()}
                  value={c().target ?? ""}
                  onChange={(v) => set("Objetivo de la restricción", { target: v || undefined })}
                />
              </Show>

              <Show when={c().type === "copyRotation"}>
                <Select
                  label="Espacio"
                  options={[
                    { value: "world", label: "Mundo (misma orientación)" },
                    { value: "local", label: "Local (mismo giro en sus ejes)" },
                  ]}
                  value={c().space ?? "world"}
                  onChange={(v) => set("Espacio de la restricción", { space: v === "local" ? "local" : undefined })}
                />
                <div class="grid grid-cols-2 gap-1">
                  <Checkbox label="Sumar al propio" checked={!!c().offset} onChange={(v) => set("Sumar giro", { offset: v || undefined })} />
                  <Show when={c().space === "local"}>
                    <Checkbox label="Al revés" checked={!!c().invert} onChange={(v) => set("Invertir giro", { invert: v || undefined })} />
                  </Show>
                </div>
              </Show>
              <Show when={c().type === "copyRotation" || c().type === "copyLocation"}>{axesRow(c())}</Show>

              <Show when={c().type === "map"}>
                <div class="grid grid-cols-3 gap-1 items-end">
                  <Select label="Eje origen" options={axisOptions} value={String(c().fromAxis ?? 0)} onChange={(v) => set("Mapeo", { fromAxis: Number(v) as 0 | 1 | 2 })} />
                  <NumberInput label="Desde" suffix="°" value={(c().fromRange ?? [0, 90])[0]} onChange={(v) => set("Mapeo", { fromRange: [v, (c().fromRange ?? [0, 90])[1]] })} />
                  <NumberInput label="Hasta" suffix="°" value={(c().fromRange ?? [0, 90])[1]} onChange={(v) => set("Mapeo", { fromRange: [(c().fromRange ?? [0, 90])[0], v] })} />
                  <Select label="Eje destino" options={axisOptions} value={String(c().toAxis ?? 0)} onChange={(v) => set("Mapeo", { toAxis: Number(v) as 0 | 1 | 2 })} />
                  <NumberInput label="Desde" suffix="°" value={(c().toRange ?? [0, 45])[0]} onChange={(v) => set("Mapeo", { toRange: [v, (c().toRange ?? [0, 45])[1]] })} />
                  <NumberInput label="Hasta" suffix="°" value={(c().toRange ?? [0, 45])[1]} onChange={(v) => set("Mapeo", { toRange: [(c().toRange ?? [0, 45])[0], v] })} />
                </div>
              </Show>

              <Show when={c().type === "driver"}>
                <div class="grid grid-cols-2 gap-1 items-end">
                  <Select label="Eje" options={axisOptions} value={String(c().axis ?? 0)} onChange={(v) => set("Eje del driver", { axis: Number(v) as 0 | 1 | 2 })} />
                  <NumberInput label="Giro en 100 %" suffix="°" min={-180} max={180} value={c().angle ?? 90} onChange={(v) => set("Giro del driver", { angle: v })} />
                </div>
              </Show>
              <Show when={c().type === "driver" || c().type === "distribute"}>{jointsRow(c())}</Show>

              <Show when={props.posing}>
                <Show when={c().type === "driver"}>{scalarRow("Valor", "roll", props.value, props.keyed.value)}</Show>
                {scalarRow("Influencia", "blend", props.influence, props.keyed.influence)}
              </Show>

              <div class="grid grid-cols-3 gap-1">
                <Button size="sm" onClick={() => props.onMove(c().id, -1)} title="Se evalúa antes">
                  ↑ Antes
                </Button>
                <Button size="sm" onClick={() => props.onMove(c().id, 1)} title="Se evalúa después">
                  ↓ Después
                </Button>
                <Checkbox label="Activa" checked={!c().disabled} onChange={(v) => set(v ? "Activar restricción" : "Desactivar restricción", { disabled: v ? undefined : true })} />
              </div>
              <Button size="sm" variant="danger" fullWidth onClick={() => props.onDelete(c().id)}>
                Borrar restricción
              </Button>
            </div>
          )}
        </Show>
      </div>
    </Panel>
  );
};
