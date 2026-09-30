import { Component, For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import { IK_SOLVERS, type IkChain, type IkSolver, type RigControl } from "../../lib/rig";
import type { ScalarChannel } from "../../lib/animation";

export interface IkPanelProps {
  chains: IkChain[];
  controls: RigControl[];
  selectedChain?: string;
  /** Hay rig en el visor */
  posing: boolean;
  hasSelection: boolean;
  /** Valores en el cuadro actual de la cadena elegida */
  blend: number;
  pinned: boolean;
  roll: number;
  /** Tiene keys de cada canal (el valor sale de la animación) */
  keyed: Record<ScalarChannel, boolean>;
  autoIk: { enabled: boolean; toRoot: boolean };
  onAutoIk: (settings: { enabled: boolean; toRoot: boolean }) => void;
  onAutoRig: () => void;
  onNewChain: (solver: IkSolver) => void;
  onSelectChain: (id: string) => void;
  onChangeChain: (description: string, chain: IkChain) => void;
  onDeleteChain: (id: string) => void;
  onScalar: (channel: ScalarChannel, value: number, commit: boolean) => void;
  onMatch: (to: "fk" | "ik") => void;
}

const optionsOf = (controls: RigControl[]) => [
  { value: "", label: "Ninguno" },
  ...controls.map((c) => ({ value: c.id, label: c.name })),
];

/** Cadenas de IK: rig automático, creación, propiedades, mezcla, fijado y balanceo */
export const IkPanel: Component<IkPanelProps> = (props) => {
  const [newSolver, setNewSolver] = createSignal<IkSolver>("twoBone");
  const chain = () => props.chains.find((c) => c.id === props.selectedChain);
  const set = (description: string, change: Partial<IkChain>) => {
    const c = chain();
    if (c) props.onChangeChain(description, { ...c, ...change });
  };

  return (
    <Panel
      title="IK"
      icon={<Icons.MagicWand size={14} />}
      defaultOpen
      headerActions={
        <Button size="sm" variant="ghost" onClick={props.onAutoRig} title="Crea (o rehace) cadenas y controles desde el cuerpo: patas, brazos, cola, cabeza y centro de masa">
          Rig automático
        </Button>
      }
    >
      <div class="space-y-3">
        {/* IK automático al arrastrar */}
        <div class="space-y-1">
          <Checkbox
            label="IK automático al arrastrar (G)"
            checked={props.autoIk.enabled}
            onChange={(enabled) => props.onAutoIk({ ...props.autoIk, enabled })}
          />
          <Show when={props.autoIk.enabled}>
            <Checkbox
              class="pl-5"
              label="Hasta la raíz (si no, hasta la primera ramificación)"
              checked={props.autoIk.toRoot}
              onChange={(toRoot) => props.onAutoIk({ ...props.autoIk, toRoot })}
            />
          </Show>
        </div>

        {/* Cadenas */}
        <div class="rounded border border-border divide-y divide-border/60 max-h-56 overflow-y-auto">
          <For each={props.chains} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin cadenas. Usa el rig automático o crea una con la selección.</p>}>
            {(c) => (
              <button
                class={clsx(
                  "w-full flex items-center justify-between gap-2 px-2 py-1.5 text-xs text-left",
                  c.id === props.selectedChain ? "bg-accent/15 text-accent" : c.disabled ? "text-text-dim" : "text-text-muted hover:text-text"
                )}
                onClick={() => props.onSelectChain(c.id)}
              >
                <span class="truncate">{c.name}</span>
                <span class="text-text-dim shrink-0">{IK_SOLVERS.find((s) => s.value === c.solver)?.label}</span>
              </button>
            )}
          </For>
        </div>
        <div class="flex gap-1">
          <div class="flex-1">
            <Select options={IK_SOLVERS} value={newSolver()} onChange={(v) => setNewSolver(v as IkSolver)} />
          </div>
          <Button size="sm" onClick={() => props.onNewChain(newSolver())} disabled={!props.hasSelection} title="Cadena nueva con la selección (o de la activa hacia arriba)">
            <Icons.Plus size={12} /> Cadena
          </Button>
        </div>

        {/* Cadena elegida */}
        <Show when={chain()}>
          {(c) => (
            <div class="space-y-2 pt-1 border-t border-border/60">
              <input
                class="w-full px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent"
                value={c().name}
                onKeyDown={(e) => e.stopPropagation()}
                onChange={(e) => set("Renombrar cadena", { name: e.currentTarget.value.trim() || c().name })}
              />
              <p class="text-[11px] font-mono text-text-dim leading-relaxed break-words">{c().joints.join(" → ")}</p>
              <Select label="Tipo" options={IK_SOLVERS} value={c().solver} onChange={(v) => set("Tipo de IK", { solver: v as IkSolver })} />

              <Show when={c().solver !== "spline"}>
                <Select label="Objetivo" options={optionsOf(props.controls)} value={c().target ?? ""} onChange={(v) => set("Objetivo del IK", { target: v || undefined })} />
              </Show>
              <Show when={c().solver === "twoBone"}>
                <Select label="Pole" options={optionsOf(props.controls)} value={c().pole ?? ""} onChange={(v) => set("Pole del IK", { pole: v || undefined })} />
              </Show>
              <Show when={c().solver === "spline"}>
                <p class="text-xs text-text-muted">Curva: {(c().curve ?? []).map((id) => props.controls.find((x) => x.id === id)?.name ?? "?").join(", ") || "sin controles"}</p>
              </Show>

              <div class="grid grid-cols-2 gap-1">
                <Show when={c().solver === "twoBone" || c().solver === "spline"}>
                  <Checkbox label={c().solver === "spline" ? "Ajustar al largo" : "Estirar"} checked={!!c().stretch} onChange={(v) => set("Estirar", { stretch: v || undefined })} />
                </Show>
                <Show when={c().solver === "twoBone" || c().solver === "fabrik" || c().solver === "ccd"}>
                  <Checkbox label="Suelo" checked={!!c().ground} onChange={(v) => set("Suelo", { ground: v || undefined })} />
                  <Checkbox label="Gira con el objetivo" checked={!!c().alignEffector} onChange={(v) => set("Giro del efector", { alignEffector: v || undefined })} />
                </Show>
                <Show when={c().solver === "twoBone"}>
                  <Checkbox label="Pie invertido" checked={!!c().footRoll} onChange={(v) => set("Pie invertido", { footRoll: v || undefined })} />
                </Show>
                <Checkbox label="Activa" checked={!c().disabled} onChange={(v) => set(v ? "Activar cadena" : "Desactivar cadena", { disabled: v ? undefined : true })} />
              </div>
              <Show when={c().solver === "twoBone"}>
                <NumberInput
                  label="Suavizado"
                  suffix="%"
                  min={0}
                  max={50}
                  step={1}
                  value={Math.round((c().softness ?? 0) * 100)}
                  onChange={(v) => set("Suavizado del IK", { softness: v > 0 ? v / 100 : undefined })}
                />
              </Show>
              <Show when={c().solver === "fabrik" || c().solver === "ccd"}>
                <NumberInput label="Iteraciones" min={1} max={100} step={1} value={c().iterations ?? 12} onChange={(v) => set("Iteraciones del IK", { iterations: Math.round(v) })} />
              </Show>

              {/* Canales animables */}
              <Show when={props.posing}>
                <label class="block space-y-1">
                  <div class="flex justify-between text-xs">
                    <span
                      class="text-text-muted"
                      title="Sin key de mezcla, la cadena sigue las keys FK de sus articulaciones mientras la animación no mueva sus controles"
                    >
                      Mezcla FK ↔ IK {props.keyed.blend ? "◆" : c().blend === undefined ? "(auto)" : ""}
                    </span>
                    <span class="font-mono text-text">{Math.round(props.blend * 100)} %</span>
                  </div>
                  <input
                    type="range"
                    class="w-full accent-[var(--color-accent)]"
                    min={0}
                    max={100}
                    value={Math.round(props.blend * 100)}
                    onInput={(e) => props.onScalar("blend", Number(e.currentTarget.value) / 100, false)}
                    onChange={(e) => props.onScalar("blend", Number(e.currentTarget.value) / 100, true)}
                  />
                </label>
                <Show when={c().solver === "twoBone" && c().footRoll}>
                  <label class="block space-y-1">
                    <div class="flex justify-between text-xs">
                      <span class="text-text-muted">Balanceo del pie {props.keyed.roll ? "◆" : ""}</span>
                      <span class="font-mono text-text">{Math.round(props.roll)}°</span>
                    </div>
                    <input
                      type="range"
                      class="w-full accent-[var(--color-accent)]"
                      min={-45}
                      max={90}
                      value={Math.round(props.roll)}
                      onInput={(e) => props.onScalar("roll", Number(e.currentTarget.value), false)}
                      onChange={(e) => props.onScalar("roll", Number(e.currentTarget.value), true)}
                    />
                  </label>
                </Show>
                <Show when={c().solver === "twoBone" || c().solver === "fabrik" || c().solver === "ccd"}>
                  <Button
                    size="sm"
                    fullWidth
                    variant={props.pinned ? "primary" : "default"}
                    onClick={() => props.onScalar("pin", props.pinned ? 0 : 1, true)}
                    title="Desde este cuadro el efector queda quieto en el mundo (key escalonada)"
                  >
                    <Icons.Lock size={12} /> {props.pinned ? "Fijado desde aquí: soltar" : "Fijar desde este cuadro"}
                  </Button>
                </Show>
                <div class="grid grid-cols-2 gap-1">
                  <Button size="sm" onClick={() => props.onMatch("fk")} title="Keys de FK con la pose actual y mezcla en FK, sin salto">
                    IK → FK
                  </Button>
                  <Button size="sm" onClick={() => props.onMatch("ik")} title="Los controles van adonde está la pose FK y la mezcla pasa a IK, sin salto">
                    FK → IK
                  </Button>
                </div>
              </Show>
              <Button size="sm" variant="danger" fullWidth onClick={() => props.onDeleteChain(c().id)}>
                Borrar cadena
              </Button>
            </div>
          )}
        </Show>
      </div>
    </Panel>
  );
};
