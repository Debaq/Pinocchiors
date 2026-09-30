import { Component, For, Show, createMemo, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Button, Checkbox, NumberInput, Panel, Select } from "../ui";
import * as Icons from "../icons";
import {
  BONE_SHAPES,
  CONTROL_SHAPES,
  ROTATION_MODES,
  boneProps,
  type BoneProps,
  type BoneShape,
  type ControlShape,
  type RigControl,
  type RigSettings,
  type RotationMode,
} from "../../lib/rig";
import type { Vec3 } from "../../lib/animation";
import { SPRING_PRESETS } from "../../lib/secondary";

export type RollMode = "auto" | "view" | "normal" | "mirror";

export interface RigPanelProps {
  boneNames: string[];
  settings: RigSettings;
  /** Articulaciones elegidas y la activa (−1: ninguna) */
  selection: number[];
  active: number;
  /** La activa tiene par del otro lado */
  hasMirror: boolean;
  /** Hay malla (para alinear el roll a su normal) */
  hasMesh: boolean;
  /** Giro de la activa en sus ejes (grados, orden del modo) y su desplazamiento */
  rotation?: Vec3;
  translation?: Vec3;
  /** Se puede posar (hay rig en el visor) */
  posing: boolean;
  selectedControl?: string;
  /** Cambio de los datos del rig, con deshacer */
  onChange: (description: string, next: RigSettings) => void;
  /** Cambio de propiedades de los huesos elegidos */
  onBoneProps: (description: string, change: Partial<BoneProps>) => void;
  onRoll: (mode: RollMode) => void;
  onRotation: (degrees: Vec3) => void;
  onTranslation: (offset: Vec3) => void;
  onAutoGroups: () => void;
  onSelectGroup: (groupId: string) => void;
  onNewGroup: () => void;
  onAddControl: () => void;
  onSelectControl: (id: string) => void;
  onApplyRest: () => void;
  onClearPose: () => void;
  /** Resortes en orejas, colas, antenas, trompas y tentáculos sin IK */
  onAutoSprings: () => void;
}

const inputClass =
  "w-full min-w-0 px-2 py-1 rounded bg-surface/40 border border-border text-xs text-text outline-none focus:border-accent";

const AXES = ["X", "Y", "Z"] as const;

/**
 * Propiedades del rig: el hueso activo (y los demás elegidos), sus grupos,
 * los controles y la pose de reposo
 */
export const RigPanel: Component<RigPanelProps> = (props) => {
  const activeName = () => props.boneNames[props.active];
  const selectedNames = () => props.selection.map((j) => props.boneNames[j]).filter(Boolean);
  const active = createMemo(() => (activeName() ? boneProps(props.settings, activeName()!) : undefined));
  const control = () => props.settings.controls.find((c) => c.id === props.selectedControl);
  const [renaming, setRenaming] = createSignal<string | null>(null);

  const groupOptions = () => [
    { value: "", label: "Sin grupo" },
    ...props.settings.groups.map((g) => ({ value: g.id, label: g.name })),
  ];
  const plural = () => (selectedNames().length > 1 ? ` (${selectedNames().length} huesos)` : "");

  const setControl = (description: string, change: Partial<RigControl>) => {
    const c = control();
    if (!c) return;
    props.onChange(description, {
      ...props.settings,
      controls: props.settings.controls.map((x) => (x.id === c.id ? { ...x, ...change } : x)),
    });
  };

  const setGroup = (id: string, change: Partial<RigSettings["groups"][number]>, description: string) =>
    props.onChange(description, {
      ...props.settings,
      groups: props.settings.groups.map((g) => (g.id === id ? { ...g, ...change } : g)),
    });

  const isolate = (id: string) => {
    const others = props.settings.groups.filter((g) => g.id !== id);
    const alreadyIsolated = others.every((g) => g.hidden) && !props.settings.groups.find((g) => g.id === id)?.hidden;
    props.onChange(alreadyIsolated ? "Mostrar todos los grupos" : "Aislar grupo", {
      ...props.settings,
      groups: props.settings.groups.map((g) => ({ ...g, hidden: alreadyIsolated ? false : g.id !== id })),
    });
  };

  const deleteGroup = (id: string) => {
    const bones = { ...props.settings.bones };
    for (const [name, p] of Object.entries(bones)) {
      if (p.group === id) {
        const { group: _, ...rest } = p;
        if (Object.keys(rest).length === 0) delete bones[name];
        else bones[name] = rest;
      }
    }
    props.onChange("Borrar grupo", { ...props.settings, bones, groups: props.settings.groups.filter((g) => g.id !== id) });
  };

  return (
    <div class="space-y-3">
      {/* Hueso activo */}
      <Panel title={`Hueso${plural()}`} icon={<Icons.Bone size={14} />} defaultOpen>
        <Show
          when={active()}
          fallback={<p class="text-xs text-text-muted">Elige una articulación en el visor o en la línea de tiempo.</p>}
        >
          {(p) => (
            <div class="space-y-3">
              <div class="text-xs font-mono text-accent truncate">{activeName()}</div>

              {/* Pose actual en los ejes de la articulación */}
              <Show when={props.posing && props.rotation}>
                <div class="space-y-1">
                  <span class="text-xs text-text-muted">
                    Giro en grados ({p().rotationMode === "quaternion" ? "Euler XYZ" : `Euler ${p().rotationMode}`}, ejes locales)
                  </span>
                  <div class="grid grid-cols-3 gap-1">
                    <For each={AXES}>
                      {(axis, i) => (
                        <NumberInput
                          label={axis}
                          step={1}
                          value={Math.round(props.rotation![i()] * 10) / 10}
                          disabled={p().lockRotation[i()]}
                          onChange={(v) => {
                            const next = [...props.rotation!] as Vec3;
                            next[i()] = v;
                            props.onRotation(next);
                          }}
                        />
                      )}
                    </For>
                  </div>
                </div>
              </Show>
              <Show when={props.posing && props.translation}>
                <div class="space-y-1">
                  <span class="text-xs text-text-muted">Desplazamiento (marco del padre)</span>
                  <div class="grid grid-cols-3 gap-1">
                    <For each={AXES}>
                      {(axis, i) => (
                        <NumberInput
                          label={axis}
                          step={0.01}
                          value={Math.round(props.translation![i()] * 1e3) / 1e3}
                          disabled={p().lockTranslation}
                          onChange={(v) => {
                            const next = [...props.translation!] as Vec3;
                            next[i()] = v;
                            props.onTranslation(next);
                          }}
                        />
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              <Select
                label="Grupo"
                options={groupOptions()}
                value={p().group ?? ""}
                onChange={(v) => props.onBoneProps("Grupo del hueso", { group: v || undefined })}
              />

              <div class="flex items-center gap-2">
                <span class="text-xs text-text-muted w-12">Color</span>
                <input
                  type="color"
                  class="h-6 w-10 rounded border border-border bg-transparent cursor-pointer"
                  value={p().color ?? "#ffb86c"}
                  onChange={(e) => props.onBoneProps("Color del hueso", { color: e.currentTarget.value })}
                />
                <Show when={p().color}>
                  <Button size="sm" variant="ghost" onClick={() => props.onBoneProps("Color del hueso", { color: undefined })}>
                    Del grupo
                  </Button>
                </Show>
              </div>

              <div class="grid grid-cols-2 gap-1">
                <Checkbox label="Visible" checked={!p().hidden} onChange={(v) => props.onBoneProps(v ? "Mostrar hueso" : "Ocultar hueso", { hidden: v ? undefined : true })} />
                <Checkbox label="Bloqueado" checked={p().locked} onChange={(v) => props.onBoneProps(v ? "Bloquear hueso" : "Desbloquear hueso", { locked: v || undefined })} />
                <Checkbox label="Deforma" checked={p().deform} onChange={(v) => props.onBoneProps(v ? "El hueso deforma" : "El hueso no deforma", { deform: v ? undefined : false })} />
                <Checkbox
                  label="Fijo en su lugar"
                  checked={p().lockTranslation}
                  onChange={(v) => props.onBoneProps(v ? "Bloquear desplazamiento" : "Permitir desplazamiento", { lockTranslation: v || undefined })}
                />
              </div>

              {/* Resorte (F8) */}
              <div class="space-y-1">
                <div class="flex items-center gap-1">
                  <div class="flex-1">
                    <Select
                      label="Movimiento secundario"
                      options={[
                        { value: "", label: "Sin resorte (sigue sus keys)" },
                        ...SPRING_PRESETS.map((x) => ({ value: x.id, label: x.label })),
                        ...(p().spring && !SPRING_PRESETS.some((x) => JSON.stringify(x.settings) === JSON.stringify(p().spring)) ? [{ value: "custom", label: "Personalizado" }] : []),
                      ]}
                      value={p().spring ? (SPRING_PRESETS.find((x) => JSON.stringify(x.settings) === JSON.stringify(p().spring))?.id ?? "custom") : ""}
                      onChange={(v) => {
                        if (v === "custom") return;
                        const preset = SPRING_PRESETS.find((x) => x.id === v);
                        props.onBoneProps(preset ? `Resorte: ${preset.label}` : "Quitar resorte", { spring: preset ? { ...preset.settings } : undefined });
                      }}
                    />
                  </div>
                </div>
                <Show when={p().spring}>
                  {(spring) => (
                    <div class="grid grid-cols-3 gap-1">
                      <NumberInput
                        label="Rigidez"
                        suffix="%"
                        min={0}
                        max={100}
                        value={Math.round(spring().stiffness * 100)}
                        onChange={(v) => props.onBoneProps("Rigidez del resorte", { spring: { ...spring(), stiffness: Math.max(0, Math.min(1, v / 100)) } })}
                      />
                      <NumberInput
                        label="Freno"
                        suffix="%"
                        min={0}
                        max={100}
                        value={Math.round(spring().damping * 100)}
                        onChange={(v) => props.onBoneProps("Freno del resorte", { spring: { ...spring(), damping: Math.max(0, Math.min(1, v / 100)) } })}
                      />
                      <NumberInput
                        label="Gravedad"
                        suffix="%"
                        min={0}
                        max={100}
                        value={Math.round(spring().gravity * 100)}
                        onChange={(v) => props.onBoneProps("Gravedad del resorte", { spring: { ...spring(), gravity: Math.max(0, Math.min(1, v / 100)) } })}
                      />
                      <Checkbox
                        class="col-span-3"
                        label="No atraviesa el cuerpo"
                        checked={spring().collide !== false}
                        onChange={(v) => props.onBoneProps("Choque del resorte", { spring: { ...spring(), collide: v ? undefined : false } })}
                      />
                    </div>
                  )}
                </Show>
                <Button size="sm" variant="ghost" fullWidth onClick={props.onAutoSprings} title="Orejas, colas, antenas, trompas y tentáculos que no tengan IK">
                  Resortes automáticos
                </Button>
              </div>

              <Checkbox
                label="Piel con cuaterniones duales (no se estrangula al girar)"
                checked={!!props.settings.dualQuaternion}
                onChange={(v) => props.onChange(v ? "Cuaterniones duales" : "Piel lineal", { ...props.settings, dualQuaternion: v || undefined })}
              />
              <Show when={props.settings.dualQuaternion}>
                <p class="text-[11px] text-text-dim leading-relaxed">
                  Solo en el visor: glTF no guarda este tipo de piel, así que lo exportado se ve con piel lineal en otras herramientas.
                </p>
              </Show>

              <div class="space-y-1">
                <span class="text-xs text-text-muted">Ejes de giro bloqueados</span>
                <div class="flex gap-1">
                  <For each={AXES}>
                    {(axis, i) => (
                      <button
                        class={clsx(
                          "flex-1 h-7 rounded border text-xs font-mono flex items-center justify-center gap-1",
                          p().lockRotation[i()] ? "bg-accent/20 border-accent text-accent" : "border-border text-text-muted hover:text-text"
                        )}
                        title={p().lockRotation[i()] ? `No gira en ${axis}` : `Gira en ${axis}`}
                        onClick={() => {
                          const next = [...p().lockRotation] as [boolean, boolean, boolean];
                          next[i()] = !next[i()];
                          props.onBoneProps("Ejes de giro", { lockRotation: next.some(Boolean) ? next : undefined });
                        }}
                      >
                        <Show when={p().lockRotation[i()]}>
                          <Icons.Lock size={11} />
                        </Show>
                        {axis}
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <Select
                label="Rotación"
                options={ROTATION_MODES}
                value={p().rotationMode}
                onChange={(v) => props.onBoneProps("Modo de rotación", { rotationMode: v === "quaternion" ? undefined : (v as RotationMode) })}
              />
              <Select
                label="Forma"
                options={BONE_SHAPES}
                value={p().shape ?? "default"}
                onChange={(v) => props.onBoneProps("Forma del hueso", { shape: v === "default" ? undefined : (v as BoneShape) })}
              />

              {/* Roll: giro de los ejes alrededor del hueso */}
              <div class="space-y-1">
                <NumberInput
                  label="Roll"
                  suffix="°"
                  step={5}
                  value={Math.round((p().roll * 180) / Math.PI * 100) / 100}
                  onChange={(v) => props.onBoneProps("Roll", { roll: v === 0 ? undefined : (v * Math.PI) / 180 })}
                />
                <div class="grid grid-cols-2 gap-1">
                  <Button size="sm" onClick={() => props.onRoll("auto")} title="Ejes calculados del esqueleto (roll 0)">
                    Automático
                  </Button>
                  <Button size="sm" onClick={() => props.onRoll("view")} title="El eje Z apunta hacia la cámara">
                    A la vista
                  </Button>
                  <Button size="sm" onClick={() => props.onRoll("normal")} disabled={!props.hasMesh} title="El eje Z sigue la superficie más cercana">
                    A la normal
                  </Button>
                  <Button size="sm" onClick={() => props.onRoll("mirror")} disabled={!props.hasMirror} title="El reflejo del hueso del otro lado">
                    Como el espejo
                  </Button>
                </div>
              </div>
            </div>
          )}
        </Show>
      </Panel>

      {/* Grupos */}
      <Panel
        title="Grupos"
        icon={<Icons.TreeStructure size={14} />}
        defaultOpen
        headerActions={
          <div class="flex gap-1">
            <Button size="sm" variant="ghost" onClick={props.onAutoGroups} title="Rehace los grupos desde el cuerpo (patas, brazos, cola…)">
              Automáticos
            </Button>
            <Button size="sm" variant="ghost" onClick={props.onNewGroup} disabled={props.selection.length === 0} title="Grupo nuevo con los huesos elegidos">
              <Icons.Plus size={12} />
            </Button>
          </div>
        }
      >
        <div class="rounded border border-border divide-y divide-border/60 max-h-64 overflow-y-auto">
          <For each={props.settings.groups} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin grupos.</p>}>
            {(group) => (
              <div class="flex items-center gap-1.5 px-2 py-1 text-xs">
                <input
                  type="color"
                  class="h-4 w-4 shrink-0 rounded-sm border-0 bg-transparent cursor-pointer p-0"
                  value={group.color}
                  onChange={(e) => setGroup(group.id, { color: e.currentTarget.value }, "Color del grupo")}
                />
                <Show
                  when={renaming() === group.id}
                  fallback={
                    <button
                      class={clsx("flex-1 min-w-0 text-left truncate", group.hidden ? "text-text-dim" : "text-text-muted hover:text-text")}
                      title="Clic: elegir sus huesos · doble clic: renombrar"
                      onClick={() => props.onSelectGroup(group.id)}
                      onDblClick={() => setRenaming(group.id)}
                    >
                      {group.name}
                    </button>
                  }
                >
                  <input
                    class={inputClass}
                    value={group.name}
                    autofocus
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") e.currentTarget.blur();
                      if (e.key === "Escape") setRenaming(null);
                    }}
                    onBlur={(e) => {
                      const name = e.currentTarget.value.trim();
                      setRenaming(null);
                      if (name && name !== group.name) setGroup(group.id, { name, auto: undefined }, "Renombrar grupo");
                    }}
                  />
                </Show>
                <button
                  class="text-text-muted hover:text-text"
                  title={group.hidden ? "Mostrar" : "Ocultar"}
                  onClick={() => setGroup(group.id, { hidden: !group.hidden || undefined }, group.hidden ? "Mostrar grupo" : "Ocultar grupo")}
                >
                  <Show when={group.hidden} fallback={<Icons.Eye size={13} />}>
                    <Icons.EyeSlash size={13} />
                  </Show>
                </button>
                <button class="text-text-muted hover:text-text" title="Aislar (ocultar los demás)" onClick={() => isolate(group.id)}>
                  <Icons.CubeFocus size={13} />
                </button>
                <button class="text-text-muted hover:text-red" title="Borrar grupo (los huesos quedan)" onClick={() => deleteGroup(group.id)}>
                  <Icons.Trash size={13} />
                </button>
              </div>
            )}
          </For>
        </div>
      </Panel>

      {/* Controles */}
      <Panel
        title="Controles"
        icon={<Icons.ArrowsOutCardinal size={14} />}
        defaultOpen={props.settings.controls.length > 0}
        headerActions={
          <Button size="sm" variant="ghost" onClick={props.onAddControl} title="Control nuevo en la articulación activa (o en el centro)">
            <Icons.Plus size={12} />
          </Button>
        }
      >
        <div class="space-y-2">
          <p class="text-xs text-text-dim leading-relaxed">
            Objetos que no deforman la malla, con keys propias. Mueven las cadenas de IK y sirven de objetivo a las restricciones.
          </p>
          <div class="rounded border border-border divide-y divide-border/60">
            <For each={props.settings.controls} fallback={<p class="px-2 py-2 text-xs text-text-dim">Sin controles.</p>}>
              {(c) => (
                <button
                  class={clsx(
                    "w-full flex items-center justify-between px-2 py-1.5 text-xs text-left",
                    c.id === props.selectedControl ? "bg-accent/15 text-accent" : "text-text-muted hover:text-text"
                  )}
                  onClick={() => props.onSelectControl(c.id)}
                >
                  <span class="truncate">{c.name}</span>
                  <span class="text-text-dim">{c.parent ?? "suelto"}</span>
                </button>
              )}
            </For>
          </div>
          <Show when={control()}>
            {(c) => (
              <div class="space-y-2">
                <input
                  class={inputClass}
                  value={c().name}
                  onKeyDown={(e) => e.stopPropagation()}
                  onChange={(e) => setControl("Renombrar control", { name: e.currentTarget.value.trim() || c().name })}
                />
                <Select
                  label="Forma"
                  options={CONTROL_SHAPES}
                  value={c().shape}
                  onChange={(v) => setControl("Forma del control", { shape: v as ControlShape })}
                />
                <Select
                  label="Sigue a"
                  options={[{ value: "", label: "Nada (suelto)" }, ...props.boneNames.map((n) => ({ value: n, label: n }))]}
                  value={c().parent ?? ""}
                  onChange={(v) => setControl("Padre del control", { parent: v || null })}
                />
                <NumberInput
                  label="Tamaño"
                  step={0.01}
                  min={0.0001}
                  value={Math.round(c().size * 1e4) / 1e4}
                  onChange={(v) => v > 0 && setControl("Tamaño del control", { size: v })}
                />
                <div class="flex items-center gap-2">
                  <span class="text-xs text-text-muted w-12">Color</span>
                  <input
                    type="color"
                    class="h-6 w-10 rounded border border-border bg-transparent cursor-pointer"
                    value={c().color ?? "#8be9fd"}
                    onChange={(e) => setControl("Color del control", { color: e.currentTarget.value })}
                  />
                  <div class="flex-1" />
                  <Button
                    size="sm"
                    variant="danger"
                    onClick={() =>
                      props.onChange("Borrar control", {
                        ...props.settings,
                        controls: props.settings.controls.filter((x) => x.id !== c().id),
                      })
                    }
                  >
                    Borrar
                  </Button>
                </div>
              </div>
            )}
          </Show>
        </div>
      </Panel>

      {/* Reposo */}
      <Panel title="Pose de reposo" icon={<Icons.Person size={14} />} defaultOpen={false}>
        <div class="space-y-2">
          <p class="text-xs text-text-dim leading-relaxed">
            El reposo es la pose sin giros, aparte del cuadro 0. Aplicar la pose como reposo mueve las articulaciones
            adonde las deja la pose y vuelve a ligar la piel con los mismos pesos.
          </p>
          <div class="grid grid-cols-2 gap-1">
            <Button size="sm" onClick={props.onClearPose} disabled={!props.posing}>
              Volver a reposo
            </Button>
            <Button size="sm" onClick={props.onApplyRest} disabled={!props.posing}>
              Aplicar como reposo
            </Button>
          </div>
        </div>
      </Panel>
    </div>
  );
};
