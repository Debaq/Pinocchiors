import { Component, For, Show } from "solid-js";
import { Panel } from "../ui";
import * as Icons from "../icons";
import type { TextureSelection, TextureSlot } from "../layout/TextureEditor";

// Tipos de `SceneStructure` (apps/desktop/src/structure.rs)

export interface NodeInfo {
  name: string;
  mesh: number | null;
  skin: number | null;
  children: number[];
  translation: [number, number, number];
  rotation: [number, number, number, number];
  scale: [number, number, number];
}

export interface PrimitiveInfo {
  vertices: number;
  triangles: number;
  material: number | null;
  attributes: string[];
}

export interface MaterialInfo {
  name: string;
  base_color: [number, number, number, number];
  metallic: number;
  roughness: number;
  emissive: [number, number, number];
  normal_scale: number;
  occlusion_strength: number;
  alpha_mode: "opaque" | "mask" | "blend";
  alpha_cutoff: number;
  double_sided: boolean;
  unlit: boolean;
  base_color_texture: number | null;
  metallic_roughness_texture: number | null;
  normal_texture: number | null;
  occlusion_texture: number | null;
  emissive_texture: number | null;
}

export interface SceneStructure {
  meters_per_unit: number;
  y_up: boolean;
  roots: number[];
  nodes: NodeInfo[];
  meshes: { name: string; primitives: PrimitiveInfo[] }[];
  materials: MaterialInfo[];
  textures: { name: string; format: string; width: number; height: number; bytes: number }[];
  skeletons: { name: string; joints: string[]; roots: number[] }[];
  animations: {
    name: string;
    channels: number;
    nodes: number;
    duration: number;
    translation: number;
    rotation: number;
    scale: number;
    weights: number;
  }[];
}

export interface StructureStepProps {
  structure?: SceneStructure;
  fileName?: string;
  format?: string;
  /** URL (blob) de cada textura, para las miniaturas */
  textureUrls?: string[];
  /** Clic en una textura: abrirla en el editor de texturas */
  onOpenTexture?: (selection: TextureSelection) => void;
}

/** Canales de un material con su índice de textura y su mapa en el visor */
const materialChannels = (m: MaterialInfo): [string, number | null, TextureSlot][] => [
  ["Color", m.base_color_texture, "base"],
  ["Metal / rugosidad", m.metallic_roughness_texture, "metallicRoughness"],
  ["Normal", m.normal_texture, "normal"],
  ["Oclusión", m.occlusion_texture, "occlusion"],
  ["Emisión", m.emissive_texture, "emissive"],
];

/** Formatos que se leen con Z arriba (STL; se pasan a la escena con Y arriba) */
const Z_UP_FORMATS = ["STL", "PLY", "3MF"];

const formatBytes = (n: number) =>
  n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`;

/** Color lineal de glTF a CSS (sRGB) */
const cssColor = (c: [number, number, number, number] | [number, number, number]) => {
  const srgb = (x: number) => {
    const v = Math.min(1, Math.max(0, x));
    return Math.round(255 * (v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055));
  };
  return `rgb(${srgb(c[0])}, ${srgb(c[1])}, ${srgb(c[2])})`;
};

const Row: Component<{ label: string; value: string | number }> = (props) => (
  <div class="flex justify-between gap-3 text-xs">
    <span class="text-text-muted shrink-0">{props.label}</span>
    <span class="text-text text-right truncate">{props.value}</span>
  </div>
);

const Chip: Component<{ text: string }> = (props) => (
  <span class="px-1.5 py-0.5 rounded bg-bg-lighter text-[10px] font-mono text-text-muted">{props.text}</span>
);

export const StructureStep: Component<StructureStepProps> = (props) => {
  const s = () => props.structure;
  const thumb = (index: number | null) => (index === null ? undefined : props.textureUrls?.[index]);

  /** Nodo del árbol con sus hijos, indentado */
  const NodeTree: Component<{ index: number; depth: number }> = (p) => {
    const node = () => s()!.nodes[p.index];
    return (
      <>
        <div class="flex items-center gap-1.5 text-xs py-0.5" style={{ "padding-left": `${p.depth * 12}px` }}>
          <span class="text-text-muted">{node().children.length > 0 ? "▾" : "•"}</span>
          <span class="text-text truncate">{node().name || `nodo ${p.index}`}</span>
          <Show when={node().mesh !== null}>
            <Chip text={`malla ${node().mesh}`} />
          </Show>
          <Show when={node().skin !== null}>
            <Chip text={`skin ${node().skin}`} />
          </Show>
        </div>
        <For each={node().children}>{(child) => <NodeTree index={child} depth={p.depth + 1} />}</For>
      </>
    );
  };

  return (
    <div class="space-y-4">
      <h3 class="text-sm font-semibold text-text">Estructura del archivo</h3>
      <Show
        when={s()}
        fallback={<p class="text-xs text-text-muted">Importa un modelo para ver lo que trae el archivo.</p>}
      >
        {(structure) => {
          const totals = () => {
            const prims = structure().meshes.flatMap((m) => m.primitives);
            return {
              primitives: prims.length,
              vertices: prims.reduce((a, p) => a + p.vertices, 0),
              triangles: prims.reduce((a, p) => a + p.triangles, 0),
              textureBytes: structure().textures.reduce((a, t) => a + t.bytes, 0),
            };
          };
          return (
            <>
              <Panel title="Resumen" icon={<Icons.Info size={14} />} defaultOpen>
                <div class="space-y-1.5 pt-1">
                  <Row label="Archivo" value={props.fileName ?? "--"} />
                  <Row label="Formato" value={props.format ?? "--"} />
                  <Row label="Unidades" value={`1 unidad = ${structure().meters_per_unit} m`} />
                  <Row label="Eje arriba del archivo" value={Z_UP_FORMATS.includes(props.format ?? "") || !structure().y_up ? "Z" : "Y"} />
                  <Row label="Nodos" value={structure().nodes.length} />
                  <Row label="Mallas / primitivas" value={`${structure().meshes.length} / ${totals().primitives}`} />
                  <Row label="Vértices" value={totals().vertices.toLocaleString()} />
                  <Row label="Triángulos" value={totals().triangles.toLocaleString()} />
                  <Row label="Materiales" value={structure().materials.length} />
                  <Row
                    label="Texturas"
                    value={`${structure().textures.length} (${formatBytes(totals().textureBytes)})`}
                  />
                  <Row label="Esqueletos" value={structure().skeletons.length} />
                  <Row label="Animaciones" value={structure().animations.length} />
                </div>
              </Panel>

              <Show when={structure().nodes.length > 0}>
                <Panel title={`Nodos (${structure().nodes.length})`} icon={<Icons.TreeStructure size={14} />} defaultOpen>
                  <div class="pt-1 max-h-64 overflow-y-auto">
                    <For each={structure().roots}>{(root) => <NodeTree index={root} depth={0} />}</For>
                  </div>
                </Panel>
              </Show>

              <Panel title={`Mallas (${structure().meshes.length})`} icon={<Icons.Cube size={14} />} defaultOpen={false}>
                <div class="space-y-3 pt-1">
                  <For each={structure().meshes}>
                    {(mesh, i) => (
                      <div class="space-y-1">
                        <div class="text-xs text-text font-medium">{mesh.name || `malla ${i()}`}</div>
                        <For each={mesh.primitives}>
                          {(prim, j) => (
                            <div class="pl-2 border-l border-border space-y-1">
                              <Row
                                label={`Primitiva ${j()}`}
                                value={`${prim.vertices.toLocaleString()} v · ${prim.triangles.toLocaleString()} tri`}
                              />
                              <Row
                                label="Material"
                                value={prim.material === null ? "ninguno" : structure().materials[prim.material]?.name || `#${prim.material}`}
                              />
                              <div class="flex flex-wrap gap-1">
                                <For each={prim.attributes}>{(a) => <Chip text={a} />}</For>
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                    )}
                  </For>
                </div>
              </Panel>

              <Show when={structure().materials.length > 0}>
                <Panel title={`Materiales (${structure().materials.length})`} icon={<Icons.Sliders size={14} />} defaultOpen>
                  <div class="space-y-3 pt-1">
                    <For each={structure().materials}>
                      {(m, i) => {
                        const channels = materialChannels(m);
                        return (
                          <div class="space-y-1.5">
                            <div class="flex items-center gap-2">
                              <span
                                class="w-4 h-4 rounded border border-border shrink-0"
                                style={{ background: cssColor(m.base_color) }}
                              />
                              <span class="text-xs text-text font-medium truncate">{m.name || `material ${i()}`}</span>
                            </div>
                            <Row
                              label="Metal / rugosidad"
                              value={`${m.metallic.toFixed(2)} / ${m.roughness.toFixed(2)}`}
                            />
                            <Row
                              label="Transparencia"
                              value={m.alpha_mode === "mask" ? `recorte ${m.alpha_cutoff}` : m.alpha_mode === "blend" ? "mezcla" : "opaco"}
                            />
                            <Show when={m.double_sided || m.unlit}>
                              <div class="flex gap-1">
                                <Show when={m.double_sided}>
                                  <Chip text="doble cara" />
                                </Show>
                                <Show when={m.unlit}>
                                  <Chip text="sin iluminación" />
                                </Show>
                              </div>
                            </Show>
                            <div class="grid grid-cols-3 gap-1.5">
                              <For each={channels.filter(([, t]) => t !== null)}>
                                {([label, t, slot]) => (
                                  <button
                                    class="space-y-0.5 text-left group"
                                    title="Abrir en el editor de texturas"
                                    onClick={() => props.onOpenTexture?.({ material: i(), slot })}
                                  >
                                    <Show
                                      when={thumb(t)}
                                      fallback={<div class="aspect-square rounded bg-bg-lighter" />}
                                    >
                                      <img
                                        src={thumb(t)}
                                        class="aspect-square w-full object-cover rounded border border-border group-hover:border-accent transition-colors"
                                      />
                                    </Show>
                                    <div class="text-[10px] text-text-muted truncate group-hover:text-text">{label}</div>
                                  </button>
                                )}
                              </For>
                            </div>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Panel>
              </Show>

              <Show when={structure().textures.length > 0}>
                <Panel title={`Texturas (${structure().textures.length})`} icon={<Icons.Checkerboard size={14} />} defaultOpen={false}>
                  <div class="space-y-2 pt-1">
                    <For each={structure().textures}>
                      {(t, i) => (
                        <button
                          class="flex gap-2 items-center w-full text-left rounded hover:bg-surface/40 transition-colors"
                          title="Abrir en el editor de texturas"
                          onClick={() => {
                            // Primer material y mapa que usan esta textura
                            for (const [material, m] of structure().materials.entries()) {
                              const channel = materialChannels(m).find(([, index]) => index === i());
                              if (channel) return props.onOpenTexture?.({ material, slot: channel[2] });
                            }
                          }}
                        >
                          <Show when={thumb(i())} fallback={<div class="w-10 h-10 rounded bg-bg-lighter shrink-0" />}>
                            <img src={thumb(i())} class="w-10 h-10 object-cover rounded border border-border shrink-0" />
                          </Show>
                          <div class="min-w-0 flex-1">
                            <div class="text-xs text-text truncate">{t.name || `textura ${i()}`}</div>
                            <div class="text-[10px] text-text-muted">
                              {t.format} · {t.width}×{t.height} · {formatBytes(t.bytes)}
                            </div>
                          </div>
                        </button>
                      )}
                    </For>
                  </div>
                </Panel>
              </Show>

              <Show when={structure().skeletons.length > 0}>
                <Panel title={`Esqueletos (${structure().skeletons.length})`} icon={<Icons.Bone size={14} />} defaultOpen={false}>
                  <div class="space-y-2 pt-1">
                    <For each={structure().skeletons}>
                      {(sk, i) => (
                        <div class="space-y-1">
                          <Row label={sk.name || `esqueleto ${i()}`} value={`${sk.joints.length} articulaciones`} />
                          <div class="flex flex-wrap gap-1">
                            <For each={sk.joints.slice(0, 40)}>{(j) => <Chip text={j} />}</For>
                            <Show when={sk.joints.length > 40}>
                              <Chip text={`+${sk.joints.length - 40}`} />
                            </Show>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Panel>
              </Show>

              <Show when={structure().animations.length > 0}>
                <Panel title={`Animaciones (${structure().animations.length})`} icon={<Icons.Play size={14} />} defaultOpen={false}>
                  <div class="space-y-2 pt-1">
                    <For each={structure().animations}>
                      {(a, i) => (
                        <div class="space-y-1">
                          <Row label={a.name || `animación ${i()}`} value={`${a.duration.toFixed(2)} s`} />
                          <div class="text-[10px] text-text-muted">
                            {a.channels} canales en {a.nodes} nodos · T {a.translation} · R {a.rotation} · E {a.scale}
                            {a.weights > 0 ? ` · morph ${a.weights}` : ""}
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Panel>
              </Show>
            </>
          );
        }}
      </Show>
    </div>
  );
};
