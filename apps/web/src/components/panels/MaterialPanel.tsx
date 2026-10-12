import { Component, For, Show, createMemo, createSignal } from "solid-js";
import { Checkbox, Select, Slider } from "../ui";
import type { MaterialInfo } from "../steps/StructureStep";
import type { TextureSlot } from "../layout/TextureEditor";
import { MATERIAL_PRESETS, PRESET_GROUPS, deleteUserPreset, presetFromMaterial, presetPatch, presetPreview, saveUserPreset, userPresets, type MaterialPreset } from "../../lib/materialPresets.ts";
import { hexToLinear, linearToHex } from "../../lib/materialColor.ts";
import { renderMaterialPreview, type PreviewMaterial } from "../../lib/materialPreview.ts";
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

/** Imágenes de los materiales: sus miniaturas y cómo cargarlas o quitarlas */
export interface MaterialMaps {
  urls: string[];
  onLoad: (slot: TextureSlot) => void;
  onClear: (slot: TextureSlot) => void;
}

export interface MaterialActions {
  /** Miniaturas de las imágenes del archivo, por índice */
  textureUrls?: string[];
  onLoadMap?: (index: number, slot: TextureSlot) => void;
  onClearMap?: (index: number, slot: TextureSlot) => void;
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

const pct = (v: number) => `${Math.round(v * 100)}%`;

const Bubble: Component<{ preview: PreviewMaterial; label: string; onClick: () => void; onDelete?: () => void }> = (props) => (
  <div class="relative group">
    <button class="flex flex-col items-center gap-0.5 w-full" title={props.label} onClick={props.onClick}>
      <img
        src={renderMaterialPreview(props.preview, 56)}
        width={44}
        height={44}
        class="rounded-full border border-border group-hover:border-accent transition-colors"
        alt=""
      />
      <span class="text-[10px] text-text-muted group-hover:text-text truncate w-full text-center">{props.label}</span>
    </button>
    <Show when={props.onDelete}>
      <button
        class="absolute -top-1 right-0 hidden group-hover:block w-4 h-4 rounded-full bg-bg border border-border text-[10px] leading-none text-text-muted hover:text-red"
        title={t("Quitar de mis materiales")}
        onClick={props.onDelete}
      >
        ×
      </button>
    </Show>
  </div>
);

/** Galería de presets: los propios y los de fábrica por grupo, con su bolita */
const PresetGallery: Component<{ onPick: (preset: MaterialPreset) => void }> = (props) => (
  <div class="space-y-2 p-2 rounded-md bg-bg-lighter/40 border border-border max-h-80 overflow-y-auto">
    <Show when={userPresets().length > 0}>
      <span class="text-[11px] text-text-dim">{t("Mis materiales")}</span>
      <div class="grid grid-cols-4 gap-1.5">
        <For each={userPresets()}>
          {(p) => <Bubble preview={presetPreview(p)} label={p.label} onClick={() => props.onPick(p)} onDelete={() => deleteUserPreset(p.id)} />}
        </For>
      </div>
    </Show>
    <For each={PRESET_GROUPS}>
      {(group) => (
        <>
          <span class="block text-[11px] text-text-dim pt-1">{group.label()}</span>
          <div class="grid grid-cols-4 gap-1.5">
            <For each={MATERIAL_PRESETS.filter((p) => p.group === group.id)}>
              {(p) => <Bubble preview={presetPreview(p)} label={p.label} onClick={() => props.onPick(p)} />}
            </For>
          </div>
        </>
      )}
    </For>
  </div>
);

const MAP_SLOTS: { slot: TextureSlot; label: () => string; index: (m: MaterialInfo) => number | null }[] = [
  { slot: "base", label: () => t("Color"), index: (m) => m.base_color_texture },
  { slot: "metallicRoughness", label: () => t("Metal / rugosidad"), index: (m) => m.metallic_roughness_texture },
  { slot: "normal", label: () => t("Relieve (normal)"), index: (m) => m.normal_texture },
  { slot: "occlusion", label: () => t("Oclusión"), index: (m) => m.occlusion_texture },
  { slot: "emissive", label: () => t("Emisión"), index: (m) => m.emissive_texture },
];

/** Imágenes de un material: cargar una desde un archivo o quitarla */
const MapsSection: Component<{ material: MaterialInfo; maps: MaterialMaps }> = (props) => (
  <div class="space-y-1.5">
    <For each={MAP_SLOTS}>
      {(row) => {
        const index = () => row.index(props.material);
        const url = () => (index() === null ? undefined : props.maps.urls[index()!]);
        return (
          <div class="flex items-center gap-2 text-xs">
            <Show when={url()} fallback={<div class="w-6 h-6 rounded bg-bg-lighter border border-border shrink-0" />}>
              <img src={url()} class="w-6 h-6 rounded object-cover border border-border shrink-0" alt="" />
            </Show>
            <span class="text-text-muted flex-1 truncate">{row.label()}</span>
            <button class="text-accent hover:underline" onClick={() => props.maps.onLoad(row.slot)}>
              {index() === null ? t("Cargar") : t("Cambiar")}
            </button>
            <Show when={index() !== null}>
              <button class="text-text-muted hover:text-red" title={t("Quitar imagen")} onClick={() => props.maps.onClear(row.slot)}>
                ×
              </button>
            </Show>
          </div>
        );
      }}
    </For>
  </div>
);

/** Color, brillo, brillo propio (glow) y translucidez de un material */
export const MaterialEditor: Component<{
  material: MaterialInfo;
  onChange: (patch: MaterialPatch) => void;
  /** Imágenes del material (solo los del archivo, no los de la piel) */
  maps?: MaterialMaps;
}> = (props) => {
  const m = () => props.material;
  const glow = () => Math.max(...m().emissive);
  // Color del glow sin su intensidad (con intensidad 0 se guarda blanco)
  const glowColor = () => (glow() > 0 ? m().emissive.map((c) => c / glow()) : [1, 1, 1]);
  const opacity = () => (m().alpha_mode === "blend" ? m().base_color[3] : 1);
  const hasMap = () => m().base_color_texture !== null;

  const [galleryOpen, setGalleryOpen] = createSignal(false);
  const [saving, setSaving] = createSignal<string | undefined>();
  const preview = createMemo(() =>
    renderMaterialPreview(
      {
        color: [m().base_color[0], m().base_color[1], m().base_color[2]],
        opacity: opacity(),
        metallic: m().metallic,
        roughness: m().roughness,
        emissive: m().emissive,
        flat: m().unlit,
      },
      112
    )
  );
  const apply = (preset: MaterialPreset) => props.onChange(presetPatch(preset, hasMap()));
  const save = () => {
    const name = saving()?.trim();
    if (!name) return;
    saveUserPreset(presetFromMaterial(`user-${Date.now()}`, name, m()));
    setSaving(undefined);
    setGalleryOpen(true);
  };

  return (
    <div class="space-y-2">
      <div class="flex items-center gap-3">
        <img src={preview()} width={56} height={56} class="rounded-full border border-border shrink-0" alt="" />
        <div class="flex flex-col gap-1 text-xs">
          <button class="text-accent hover:underline text-left" onClick={() => setGalleryOpen(!galleryOpen())}>
            {galleryOpen() ? t("Ocultar presets") : t("Elegir un preset")}
          </button>
          <button class="text-text-muted hover:text-text text-left" onClick={() => setSaving(saving() === undefined ? m().name : undefined)}>
            {t("Guardar como preset")}
          </button>
        </div>
      </div>
      <Show when={saving() !== undefined}>
        <div class="flex gap-1">
          <input
            class="flex-1 min-w-0 px-2 py-1 text-xs rounded bg-bg-lighter border border-border text-text"
            value={saving()}
            placeholder={t("Nombre del preset")}
            onInput={(e) => setSaving(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") save();
              if (e.key === "Escape") setSaving(undefined);
            }}
          />
          <button class="px-2 text-xs text-accent hover:underline" onClick={save}>
            {t("Guardar")}
          </button>
        </div>
      </Show>
      <Show when={galleryOpen()}>
        <PresetGallery onPick={apply} />
      </Show>
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
      <Slider
        label={t("Vidrio (transmisión)")}
        title={t("Cuánta luz atraviesa el material: líquidos, vidrio, hielo")}
        value={m().transmission}
        onChange={(v) => props.onChange({ transmission: v })}
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <Show when={m().transmission > 0}>
        <Slider
          label={t("Refracción (IOR)")}
          title={t("Agua 1.33 · vidrio 1.5 · diamante 2.4")}
          value={m().ior}
          onChange={(v) => props.onChange({ ior: v })}
          min={1}
          max={2.5}
          step={0.01}
          formatValue={(v) => v.toFixed(2)}
        />
      </Show>
      <Slider
        label={t("Barniz")}
        title={t("Capa brillante encima: lo húmedo, lacado o encerado")}
        value={m().clearcoat}
        onChange={(v) => props.onChange({ clearcoat: v })}
        min={0}
        max={1}
        step={0.01}
        formatValue={pct}
      />
      <Show when={m().clearcoat > 0}>
        <Slider
          label={t("Rugosidad del barniz")}
          value={m().clearcoat_roughness}
          onChange={(v) => props.onChange({ clearcoat_roughness: v })}
          min={0}
          max={1}
          step={0.01}
          formatValue={pct}
        />
      </Show>
      <div class="flex gap-4">
        <Checkbox small label={t("Plano (sin luz)")} checked={m().unlit} onChange={(v) => props.onChange({ unlit: v })} />
        <Checkbox small label={t("Doble cara")} checked={m().double_sided} onChange={(v) => props.onChange({ double_sided: v })} />
      </div>
      <Show when={props.maps}>
        {(maps) => (
          <div class="space-y-2 pt-1">
            <MapsSection material={m()} maps={maps()} />
            <Show when={MAP_SLOTS.some((row) => row.index(m()) !== null)}>
              <Slider
                label={t("Repetición")}
                title={t("Cuántas veces se repiten las imágenes sobre la superficie")}
                value={m().uv_scale[0]}
                onChange={(v) => props.onChange({ uv_scale: [v, v] })}
                min={0.1}
                max={20}
                step={0.1}
                formatValue={(v) => `×${v.toFixed(1)}`}
              />
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
};

const NONE = "none";

/** Las imágenes de un material, si quien llama sabe cargarlas */
export const mapsOf = (actions: MaterialActions, index: number): MaterialMaps | undefined =>
  actions.textureUrls && actions.onLoadMap && actions.onClearMap
    ? { urls: actions.textureUrls, onLoad: (slot) => actions.onLoadMap!(index, slot), onClear: (slot) => actions.onClearMap!(index, slot) }
    : undefined;

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
                    <MaterialEditor material={mat()} onChange={(patch) => props.onEdit(p.material!, patch)} maps={mapsOf(props, p.material!)} />
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
