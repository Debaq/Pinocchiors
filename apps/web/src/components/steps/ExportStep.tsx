import { Component, For, Show } from "solid-js";
import { Button, Checkbox, Select, Slider } from "../ui";
import * as Icons from "../icons";

export type ExportFormat = "step" | "glb" | "gltf" | "obj" | "stl" | "3mf" | "ply" | "usdz" | "bvh" | "json";

/** Formatos que llevan el esqueleto sin modelo */
export const SKELETON_FORMATS: ExportFormat[] = ["glb", "gltf", "bvh"];
/** Formatos del sólido de Diseñar */
export const DESIGN_FORMATS: ExportFormat[] = ["step", "stl", "3mf", "obj", "ply", "glb"];
export type WebPreset = "none" | "balanced" | "light" | "custom";

export interface ExportOptions {
  format: ExportFormat;
  preset: WebPreset;
  draco: boolean;
  /** Nivel de compresión Draco 0-10 */
  dracoLevel: number;
  /** 16 bits de posición en vez de 14: evita manchas en superficies muy juntas */
  highPrecision: boolean;
  /** Porcentaje de triángulos a conservar; 100 = sin reducir */
  simplifyPercent: number;
  /** Lado máximo de las texturas en píxeles; 0 = sin límite */
  maxTextureSize: number;
  /** Calidad JPEG de las texturas opacas; 0 = mantener el original */
  textureQuality: number;
  /** Soldar vértices duplicados y quitar materiales sin uso */
  cleanGeometry: boolean;
}

type WebSettings = Omit<ExportOptions, "format" | "preset">;

const PRESETS: Record<Exclude<WebPreset, "custom">, WebSettings> = {
  none: {
    draco: false,
    dracoLevel: 7,
    highPrecision: true,
    simplifyPercent: 100,
    maxTextureSize: 0,
    textureQuality: 0,
    cleanGeometry: false,
  },
  balanced: {
    draco: true,
    dracoLevel: 7,
    highPrecision: true,
    simplifyPercent: 100,
    maxTextureSize: 2048,
    textureQuality: 85,
    cleanGeometry: true,
  },
  light: {
    draco: true,
    dracoLevel: 10,
    highPrecision: false,
    simplifyPercent: 50,
    maxTextureSize: 1024,
    textureQuality: 75,
    cleanGeometry: true,
  },
};

export const defaultExportOptions: ExportOptions = { format: "glb", preset: "none", ...PRESETS.none };

interface FormatEntry {
  id: ExportFormat;
  name: string;
  hint: string;
}

const FORMATS: FormatEntry[] = [
  { id: "step", name: "STEP", hint: "Sólido exacto para otros programas de CAD (FreeCAD, Onshape, Fusion, SolidWorks)." },
  { id: "glb", name: "GLB", hint: "Web, juegos y AR. Un solo archivo con texturas, rig y animaciones." },
  { id: "gltf", name: "glTF", hint: "Como GLB, pero en JSON + .bin separados: fácil de inspeccionar." },
  { id: "usdz", name: "USDZ", hint: "AR en iPhone/iPad (Quick Look). Conserva el rig." },
  { id: "obj", name: "OBJ", hint: "Geometría con materiales y texturas (.mtl). Sin rig." },
  { id: "3mf", name: "3MF", hint: "Impresión 3D moderna: milímetros, malla cerrada y colores." },
  { id: "stl", name: "STL", hint: "Impresión 3D clásica. Solo geometría, en milímetros." },
  { id: "ply", name: "PLY", hint: "Geometría con color por vértice (escaneo, MeshLab)." },
  { id: "bvh", name: "BVH", hint: "Esqueleto y la animación activa (captura de movimiento): Blender, MotionBuilder, three.js." },
  { id: "json", name: "Pesos", hint: "Pesos de skinning del autorig en JSON." },
];

const TEXTURE_SIZES = [
  { value: "0", label: "Sin límite" },
  { value: "4096", label: "4096 px" },
  { value: "2048", label: "2048 px" },
  { value: "1024", label: "1024 px" },
  { value: "512", label: "512 px" },
];

const PRESET_OPTIONS = [
  { value: "none", label: "Sin optimizar", description: "Tal cual, máxima fidelidad" },
  { value: "balanced", label: "Web equilibrado", description: "Draco + texturas 2048 px, sin perder forma" },
  { value: "light", label: "Web ligero", description: "Para móviles: mitad de triángulos, texturas 1024 px" },
  { value: "custom", label: "Personalizado" },
];

/** Formatos que pasan por el exportador glTF (Draco, reducción, texturas) */
const isGltf = (format: ExportFormat) => format === "glb" || format === "gltf";

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export interface ExportStepProps {
  onExport?: () => void;
  canExport?: boolean;
  /** No hay modelo: se exporta el esqueleto solo con sus animaciones */
  skeletonOnly?: boolean;
  /** Se exporta el sólido de Diseñar */
  design?: boolean;
  /** Piezas del diseño (clave "operación:índice") y la elegida; sin elegir, todo el diseño */
  designParts?: { key: string; name: string }[];
  designPart?: string;
  onDesignPartChange?: (key: string | undefined) => void;
  hasSkeleton?: boolean;
  /** Esqueleto solo: un octaedro por hueso para verlo en cualquier visor */
  boneShapes?: boolean;
  onBoneShapesChange?: (value: boolean) => void;
  autorigComplete?: boolean;
  hasQuadMesh?: boolean;
  includeRig?: boolean;
  onIncludeRigChange?: (value: boolean) => void;
  useRetopology?: boolean;
  onUseRetopologyChange?: (value: boolean) => void;
  /** Reempaquetar el mapa UV apretado y re-hornear sus texturas */
  compactUv?: boolean;
  onCompactUvChange?: (value: boolean) => void;
  options: ExportOptions;
  onOptionsChange?: (options: ExportOptions) => void;
  /** Tamaño y archivos de la última exportación */
  lastExport?: { bytes: number; files: string[] };
}

export const ExportStep: Component<ExportStepProps> = (props) => {
  const opts = () => props.options;
  const set = (patch: Partial<ExportOptions>) => props.onOptionsChange?.({ ...opts(), ...patch });
  /** Cambiar un control a mano deja el perfil en "Personalizado" */
  const tweak = (patch: Partial<WebSettings>) => set({ ...patch, preset: "custom" });
  const applyPreset = (preset: string) => {
    if (preset === "custom") set({ preset: "custom" });
    else set({ preset: preset as WebPreset, ...PRESETS[preset as Exclude<WebPreset, "custom">] });
  };
  const formatDisabled = (id: ExportFormat) =>
    props.design
      ? !DESIGN_FORMATS.includes(id)
      : id === "step" ||
        (id === "json" && !props.autorigComplete) ||
        (id === "bvh" && !props.hasSkeleton) ||
        (props.skeletonOnly === true && !SKELETON_FORMATS.includes(id));
  // Del modelo de la app (no del diseño)
  const model = () => !props.design;
  const current = () => FORMATS.find((f) => f.id === opts().format) ?? FORMATS[0];
  const keepsRig = () => ["glb", "gltf", "usdz"].includes(opts().format);

  return (
    <div class="space-y-5">
      <h3 class="text-sm font-semibold text-text">{props.design ? "Exportar diseño" : props.skeletonOnly ? "Exportar esqueleto" : "Exportar Modelo"}</h3>

      {/* Formato */}
      <div class="space-y-2">
        <span class="text-xs font-medium text-text-muted">Formato</span>
        <div class="grid grid-cols-4 gap-1.5">
          <For each={FORMATS}>
            {(f) => (
              <button
                type="button"
                disabled={formatDisabled(f.id)}
                onClick={() => set({ format: f.id })}
                class={`px-2 py-1.5 rounded-md text-xs font-medium border transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
                  opts().format === f.id
                    ? "bg-accent/20 border-accent text-text"
                    : "bg-surface/30 border-border text-text-muted hover:text-text"
                }`}
              >
                {f.name}
              </button>
            )}
          </For>
        </div>
        <p class="text-[10px] text-text-dim leading-relaxed">{current().hint}</p>
      </div>

      <Show when={props.design}>
        <div class="space-y-3 p-3 rounded-md bg-surface/30 border border-border">
          <Show when={(props.designParts?.length ?? 0) > 1}>
            <Select
              label="Qué exportar"
              options={[{ value: "all", label: "Todo el diseño" }, ...(props.designParts ?? []).map((p) => ({ value: p.key, label: p.name }))]}
              value={props.designPart ?? "all"}
              onChange={(v) => props.onDesignPartChange?.(v === "all" ? undefined : v)}
            />
          </Show>
          <p class="text-[10px] text-text-dim leading-relaxed">
            Se exporta el sólido de Diseñar en milímetros. STEP lo guarda exacto (caras y aristas reales), con cada pieza
            con su nombre y color; 3MF lleva un objeto por pieza; los demás formatos, una malla fina.
          </p>
        </div>
      </Show>

      {/* Esqueleto solo */}
      <Show when={model() && props.skeletonOnly && opts().format !== "bvh"}>
        <div class="space-y-3 p-3 rounded-md bg-surface/30 border border-border">
          <Checkbox
            label="Figura visible de los huesos"
            checked={props.boneShapes}
            onChange={props.onBoneShapesChange}
          />
          <p class="text-[10px] text-text-dim leading-relaxed">
            Sin modelo se exportan el esqueleto y todas sus animaciones. La figura agrega un octaedro por hueso para
            verlo en visores que no dibujan esqueletos; para usarlo en otro programa conviene sin figura.
          </p>
        </div>
      </Show>
      <Show when={opts().format === "bvh"}>
        <p class="text-[10px] text-text-dim leading-relaxed p-3 rounded-md bg-surface/30 border border-border">
          BVH guarda una sola animación: se exporta la activa, en su rango de cuadros. Sin animaciones queda la pose de
          reposo.
        </p>
      </Show>

      {/* Contenido */}
      <Show when={model() && !props.skeletonOnly && opts().format !== "json" && opts().format !== "bvh"}>
        <div class="space-y-3 p-3 rounded-md bg-surface/30 border border-border">
          <Checkbox
            label="Incluir esqueleto y pesos"
            checked={props.includeRig && props.autorigComplete && keepsRig()}
            disabled={!props.autorigComplete || !keepsRig()}
            onChange={props.onIncludeRigChange}
          />
          <Checkbox
            label="Usar malla retopologizada"
            checked={props.useRetopology && props.hasQuadMesh}
            disabled={!props.hasQuadMesh}
            onChange={props.onUseRetopologyChange}
          />
          <Checkbox
            label="Compactar el mapa UV"
            checked={props.compactUv === true}
            onChange={props.onCompactUvChange}
          />
          <p class="text-[10px] text-text-dim leading-relaxed">
            Reacomoda las islas lo más juntas posible y vuelve a hornear las texturas (lo pintado
            incluido): se pinta sobre el mapa legible y se exporta el que aprovecha la textura.
          </p>
          <p class="text-[10px] text-text-dim leading-relaxed">
            {props.autorigComplete ? "" : "Ejecuta el autorig para exportar el rig. "}
            {props.hasQuadMesh ? "" : "Ejecuta la retopología para exportar la malla de quads."}
          </p>
        </div>
      </Show>

      {/* Optimización web */}
      <Show when={model() && isGltf(opts().format) && !props.skeletonOnly}>
        <div class="space-y-3 p-3 rounded-md bg-surface/30 border border-border">
          <Select
            label="Optimización web"
            options={PRESET_OPTIONS}
            value={opts().preset}
            onChange={applyPreset}
          />

          <Checkbox
            label="Comprimir geometría (Draco)"
            checked={opts().draco}
            onChange={(v) => tweak({ draco: v })}
          />
          <Show when={opts().draco}>
            <div class="pl-6 space-y-3">
              <Slider
                label="Nivel de compresión"
                min={0}
                max={10}
                step={1}
                value={opts().dracoLevel}
                onChange={(v) => tweak({ dracoLevel: v })}
                showValue
              />
              <Checkbox
                label="Alta precisión (16 bits)"
                checked={opts().highPrecision}
                onChange={(v) => tweak({ highPrecision: v })}
              />
              <p class="text-[10px] text-text-dim leading-relaxed">
                Draco reduce la geometría ~10 veces. Con 14 bits pueden verse manchas donde hay superficies muy
                juntas; el visor necesita el decodificador Draco (three.js, Babylon y model-viewer lo traen).
              </p>
            </div>
          </Show>

          <Slider
            label="Triángulos a conservar"
            min={10}
            max={100}
            step={5}
            value={opts().simplifyPercent}
            onChange={(v) => tweak({ simplifyPercent: v })}
            showValue
            formatValue={(v) => (v >= 100 ? "Todos" : `${v}%`)}
          />

          <Select
            label="Tamaño máximo de texturas"
            options={TEXTURE_SIZES}
            value={String(opts().maxTextureSize)}
            onChange={(v) => tweak({ maxTextureSize: Number(v) })}
          />
          <Slider
            label="Calidad JPEG de texturas"
            min={0}
            max={100}
            step={5}
            value={opts().textureQuality}
            onChange={(v) => tweak({ textureQuality: v })}
            showValue
            formatValue={(v) => (v === 0 ? "Original" : `${v}`)}
          />
          <Checkbox
            label="Limpiar geometría y materiales sin uso"
            checked={opts().cleanGeometry}
            onChange={(v) => tweak({ cleanGeometry: v })}
          />
          <p class="text-[10px] text-text-dim leading-relaxed">
            Las texturas con transparencia quedan en PNG. Las texturas suelen pesar más que la geometría: bajar
            su tamaño es lo que más reduce el archivo.
          </p>
        </div>
      </Show>

      <Button
        onClick={props.onExport}
        disabled={!props.canExport || formatDisabled(opts().format)}
        variant="primary"
        fullWidth
        icon={<Icons.Export size={16} />}
      >
        Exportar {current().name}
      </Button>

      <Show when={props.lastExport}>
        {(last) => (
          <div class="p-3 rounded-md bg-surface/30 border border-border space-y-1">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">Última exportación</span>
              <span class="font-medium text-text">{formatBytes(last().bytes)}</span>
            </div>
            <For each={last().files}>
              {(file) => <p class="text-[10px] text-text-dim truncate">{file.split("/").pop()}</p>}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
};
