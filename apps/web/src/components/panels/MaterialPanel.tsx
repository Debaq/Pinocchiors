import { Component, For, Show } from "solid-js";
import { Checkbox, Select, Slider } from "../ui";
import type { MaterialInfo } from "../steps/StructureStep";
import { MATERIAL_PRESETS, type MaterialPreset } from "../../lib/materialPresets.ts";
import { t } from "../../lib/i18n";

/** Cambio parcial de un material */
export type MaterialPatch = Partial<Omit<MaterialInfo, "base_color_texture" | "metallic_roughness_texture" | "normal_texture" | "occlusion_texture" | "emissive_texture">>;

/** Primitiva de la malla del nodo elegido y el material que usa */
export interface NodePrimitive {
  mesh: number;
  primitive: number;
  material: number | null;
  /** Cuántas primitivas del archivo usan ese material */
  shared: number;
}

export interface MaterialActions {
  materials: MaterialInfo[];
  onEdit: (index: number, patch: MaterialPatch) => void;
  /** Material nuevo (copia de `copyOf` si se dice) para una primitiva, o suelto si no hay `target` */
  onAdd: (target?: { mesh: number; primitive: number }, copyOf?: number) => void;
  /** Asigna un material (o ninguno) a una primitiva */
  onAssign: (mesh: number, primitive: number, material: number | null) => void;
  onRemove: (index: number) => void;
  /** Lo mismo para varias primitivas a la vez (selección de varias partes) */
  onAssignMany: (targets: { mesh: number; primitive: number }[], material: number | null) => void;
  onAddMany: (targets: { mesh: number; primitive: number }[]) => void;
}

// Los factores de glTF son lineales; el selector de color del navegador es sRGB
const toSrgb = (x: number) => {
  const v = Math.min(1, Math.max(0, x));
  return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
};
const toLinear = (x: number) => (x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4));

export const linearToHex = (c: number[]) =>
  "#" + [0, 1, 2].map((i) => Math.round(255 * toSrgb(c[i])).toString(16).padStart(2, "0")).join("");

export const hexToLinear = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => toLinear(v / 255)) as [number, number, number];
};

const pct = (v: number) => `${Math.round(v * 100)}%`;

/** Color, brillo, brillo propio (glow) y translucidez de un material */
export const MaterialEditor: Component<{
  material: MaterialInfo;
  onChange: (patch: MaterialPatch) => void;
}> = (props) => {
  const m = () => props.material;
  const glow = () => Math.max(...m().emissive);
  // Color del glow sin su intensidad (con intensidad 0 se guarda blanco)
  const glowColor = () => (glow() > 0 ? m().emissive.map((c) => c / glow()) : [1, 1, 1]);
  const opacity = () => (m().alpha_mode === "blend" ? m().base_color[3] : 1);
  const hasMap = () => m().base_color_texture !== null;

  const applyPreset = (preset: MaterialPreset) => {
    const [r, g, b] = hasMap() ? [1, 1, 1] : hexToLinear(preset.color);
    const glowColor = preset.glow ? hexToLinear(preset.glow.color) : [0, 0, 0];
    const k = preset.glow?.strength ?? 0;
    const opacity = preset.opacity ?? 1;
    props.onChange({
      base_color: [r, g, b, opacity],
      metallic: preset.metallic,
      roughness: 1 - preset.shine,
      emissive: [glowColor[0] * k, glowColor[1] * k, glowColor[2] * k],
      alpha_mode: opacity < 1 ? "blend" : "opaque",
      unlit: !!preset.flat,
    });
  };

  return (
    <div class="space-y-2">
      <Select
        options={MATERIAL_PRESETS.map((p) => ({ value: p.id, label: p.label }))}
        placeholder={t("Preset...")}
        onChange={(id) => {
          const preset = MATERIAL_PRESETS.find((p) => p.id === id);
          if (preset) applyPreset(preset);
        }}
      />
      <input
        class="w-full px-2 py-1 text-xs rounded bg-bg-lighter border border-border text-text"
        value={m().name}
        placeholder={t("Nombre del material")}
        onChange={(e) => props.onChange({ name: e.currentTarget.value })}
      />
      <label class="flex items-center justify-between text-xs text-text-muted">
        {hasMap() ? t("Tinte del color") : t("Color")}
        <input
          type="color"
          value={linearToHex(m().base_color)}
          onInput={(e) => {
            const [r, g, b] = hexToLinear(e.currentTarget.value);
            props.onChange({ base_color: [r, g, b, m().base_color[3]] });
          }}
          class="w-10 h-6 rounded border border-border bg-transparent cursor-pointer"
        />
      </label>
      <Slider
        label={t("Brillo")}
        value={1 - m().roughness}
        onChange={(v) => props.onChange({ roughness: 1 - v })}
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <Slider
        label={t("Metal")}
        value={m().metallic}
        onChange={(v) => props.onChange({ metallic: v })}
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <Slider
        label={t("Opacidad")}
        value={opacity()}
        onChange={(v) =>
          props.onChange({
            alpha_mode: v < 1 ? "blend" : m().alpha_mode === "blend" ? "opaque" : m().alpha_mode,
            base_color: [m().base_color[0], m().base_color[1], m().base_color[2], v],
          })
        }
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <label class="flex items-center justify-between text-xs text-text-muted">
        {t("Color del glow")}
        <input
          type="color"
          value={linearToHex(glowColor())}
          onInput={(e) => {
            const [r, g, b] = hexToLinear(e.currentTarget.value);
            const k = glow() > 0 ? glow() : 0.5;
            props.onChange({ emissive: [r * k, g * k, b * k] });
          }}
          class="w-10 h-6 rounded border border-border bg-transparent cursor-pointer"
        />
      </label>
      <Slider
        label={t("Glow")}
        value={glow()}
        onChange={(v) => {
          const c = glowColor();
          props.onChange({ emissive: [c[0] * v, c[1] * v, c[2] * v] });
        }}
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <div class="flex gap-4">
        <Checkbox small label={t("Plano (sin luz)")} checked={m().unlit} onChange={(v) => props.onChange({ unlit: v })} />
        <Checkbox small label={t("Doble cara")} checked={m().double_sided} onChange={(v) => props.onChange({ double_sided: v })} />
      </div>
    </div>
  );
};

const NONE = "none";

/** Materiales de las primitivas del nodo elegido: cambiar, editar, añadir */
export const NodeMaterials: Component<{ primitives: NodePrimitive[] } & MaterialActions> = (props) => {
  const options = () => [
    { value: NONE, label: t("Sin material") },
    ...props.materials.map((m, i) => ({ value: String(i), label: m.name || t("material {i}", { i }) })),
  ];
  return (
    <div class="space-y-3 text-xs px-3 py-2 rounded-md bg-surface/30 border border-border">
      <span class="text-text font-medium">{t("Material del objeto")}</span>
      <Show when={props.primitives.length > 1}>
        <div class="space-y-2 pb-2 border-b border-border">
          <span class="text-text-dim">{t("Toda la selección ({n} partes)", { n: props.primitives.length })}</span>
          <Select
            options={options()}
            placeholder={t("Asignar un material a todas...")}
            onChange={(v) => props.onAssignMany(props.primitives, v === NONE ? null : Number(v))}
          />
          <button class="text-accent hover:underline" onClick={() => props.onAddMany(props.primitives)}>
            {t("Material nuevo para todas")}
          </button>
        </div>
      </Show>
      <For each={props.primitives}>
        {(p) => {
          const target = { mesh: p.mesh, primitive: p.primitive };
          const material = () => (p.material === null ? undefined : props.materials[p.material]);
          return (
            <div class="space-y-2 pt-2 border-t border-border first:border-t-0 first:pt-0">
              <Show when={props.primitives.length > 1}>
                <span class="text-text-dim">{t("Primitiva {j}", { j: p.primitive })}</span>
              </Show>
              <Select
                options={options()}
                value={p.material === null ? NONE : String(p.material)}
                onChange={(v) => props.onAssign(p.mesh, p.primitive, v === NONE ? null : Number(v))}
              />
              <Show when={material()}>
                {(mat) => (
                  <>
                    <Show when={p.shared > 1}>
                      <p class="text-[11px] text-text-dim leading-relaxed">
                        {t("Este material lo usan {n} partes del archivo: cambiarlo las cambia a todas.", { n: p.shared })}
                      </p>
                      <button class="text-accent hover:underline" onClick={() => props.onAdd(target, p.material!)}>
                        {t("Duplicar solo para este objeto")}
                      </button>
                    </Show>
                    <MaterialEditor material={mat()} onChange={(patch) => props.onEdit(p.material!, patch)} />
                  </>
                )}
              </Show>
              <button class="text-accent hover:underline" onClick={() => props.onAdd(target)}>
                {t("Material nuevo")}
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
};
