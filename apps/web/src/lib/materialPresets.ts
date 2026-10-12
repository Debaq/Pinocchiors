import { t } from "./i18n.ts";
import { createPersisted } from "./ui-state.ts";
import { hexToLinear, linearToHex } from "./materialColor.ts";
import type { PreviewMaterial } from "./materialPreview.ts";

export type PresetGroup = "organic" | "clear" | "metal" | "nature" | "things";

export const PRESET_GROUPS: { id: PresetGroup; label: () => string }[] = [
  { id: "organic", label: () => t("Orgánico") },
  { id: "clear", label: () => t("Transparente y luz") },
  { id: "metal", label: () => t("Metal") },
  { id: "nature", label: () => t("Naturaleza") },
  { id: "things", label: () => t("Objetos") },
];

/** Material de partida: colores en sRGB (#rrggbb), el resto de 0 a 1 */
export interface MaterialPreset {
  id: string;
  label: string;
  group?: PresetGroup;
  color: string;
  /** Opacidad: menos de 1 lo vuelve translúcido */
  opacity?: number;
  metallic: number;
  /** Brillo: 0 mate, 1 espejo */
  shine: number;
  /** Color y fuerza del glow (luz propia) */
  glow?: { color: string; strength: number };
  /** Sin iluminación: color plano */
  flat?: boolean;
  /** Vidrio: cuánta luz atraviesa (0 a 1) y refracción */
  transmission?: number;
  ior?: number;
  /** Grosor del vidrio: 0 a 1 de lo máximo (un quinto del tamaño del modelo) */
  thickness?: number;
  /** Barniz húmedo o lacado */
  clearcoat?: number;
  clearcoatRoughness?: number;
}

export const MATERIAL_PRESETS: MaterialPreset[] = [
  // Orgánico
  { id: "skin", group: "organic", get label() { return t("Piel"); }, color: "#d19a7a", metallic: 0, shine: 0.35, clearcoat: 0.15, clearcoatRoughness: 0.5 },
  { id: "skin-grey", group: "organic", get label() { return t("Piel rugosa"); }, color: "#8c8a86", metallic: 0, shine: 0.1 },
  { id: "scales-green", group: "organic", get label() { return t("Escamas"); }, color: "#3f7a3a", metallic: 0.1, shine: 0.6, clearcoat: 0.4, clearcoatRoughness: 0.3 },
  { id: "fur-brown", group: "organic", get label() { return t("Pelaje"); }, color: "#6a4528", metallic: 0, shine: 0 },
  { id: "feathers", group: "organic", get label() { return t("Plumas"); }, color: "#c9d6e3", metallic: 0, shine: 0.4, clearcoat: 0.2, clearcoatRoughness: 0.4 },
  { id: "flesh", group: "organic", get label() { return t("Carne"); }, color: "#b8473f", metallic: 0, shine: 0.45, clearcoat: 0.6, clearcoatRoughness: 0.25 },
  { id: "blood", group: "organic", get label() { return t("Sangre"); }, color: "#6e0a0f", metallic: 0, shine: 0.9, clearcoat: 1, clearcoatRoughness: 0.05 },
  { id: "bone", group: "organic", get label() { return t("Hueso"); }, color: "#e3d9c0", metallic: 0, shine: 0.3 },
  { id: "horn", group: "organic", get label() { return t("Cuerno"); }, color: "#4a3a2b", metallic: 0, shine: 0.5, clearcoat: 0.4, clearcoatRoughness: 0.3 },
  { id: "claw", group: "organic", get label() { return t("Garra"); }, color: "#d8d0bc", metallic: 0, shine: 0.55, clearcoat: 0.6, clearcoatRoughness: 0.2 },
  { id: "pearl", group: "organic", get label() { return t("Concha nácar"); }, color: "#efe6e8", metallic: 0.3, shine: 0.85, clearcoat: 1, clearcoatRoughness: 0.1 },
  { id: "eye", group: "organic", get label() { return t("Ojo húmedo"); }, color: "#f2efe8", metallic: 0, shine: 0.95, clearcoat: 1, clearcoatRoughness: 0 },
  { id: "slime", group: "organic", get label() { return t("Baba"); }, color: "#7bd34a", opacity: 0.85, metallic: 0, shine: 0.95, transmission: 0.5, ior: 1.35, thickness: 0.3, clearcoat: 1, clearcoatRoughness: 0 },
  { id: "cloth", group: "organic", get label() { return t("Tela"); }, color: "#4a5f8a", metallic: 0, shine: 0 },
  { id: "leather", group: "organic", get label() { return t("Cuero"); }, color: "#6b3f26", metallic: 0, shine: 0.4, clearcoat: 0.2, clearcoatRoughness: 0.4 },
  // Transparente y luz
  { id: "ghost", group: "clear", get label() { return t("Transparente"); }, color: "#ffffff", opacity: 0.2, metallic: 0, shine: 0.5 },
  { id: "ghost-faint", group: "clear", get label() { return t("Casi invisible"); }, color: "#ffffff", opacity: 0.06, metallic: 0, shine: 0.3 },
  { id: "liquid", group: "clear", get label() { return t("Líquido transparente"); }, color: "#bfe4ff", metallic: 0, shine: 1, transmission: 1, ior: 1.33, thickness: 0.6 },
  { id: "glass", group: "clear", get label() { return t("Vidrio"); }, color: "#f2f8fa", metallic: 0, shine: 1, transmission: 1, ior: 1.5 },
  { id: "glass-thick", group: "clear", get label() { return t("Vidrio grueso"); }, color: "#d6efe6", metallic: 0, shine: 1, transmission: 1, ior: 1.52, thickness: 0.8 },
  { id: "ice", group: "clear", get label() { return t("Hielo"); }, color: "#d3ecf7", metallic: 0, shine: 0.8, transmission: 0.8, ior: 1.31, thickness: 0.7 },
  { id: "amber", group: "clear", get label() { return t("Ámbar y resina"); }, color: "#f0a030", metallic: 0, shine: 0.9, transmission: 0.85, ior: 1.55, thickness: 0.6 },
  { id: "jade", group: "clear", get label() { return t("Jade"); }, color: "#6fb58c", metallic: 0, shine: 0.8, transmission: 0.3, ior: 1.6, thickness: 0.4, clearcoat: 0.5, clearcoatRoughness: 0.1 },
  { id: "neon-green", group: "clear", get label() { return t("Luz neón"); }, color: "#0b1a10", metallic: 0, shine: 0.6, glow: { color: "#39ff7a", strength: 1 } },
  { id: "lava", group: "clear", get label() { return t("Lava"); }, color: "#2a0c04", metallic: 0, shine: 0.3, glow: { color: "#ff5a14", strength: 0.8 } },
  // Metal
  { id: "chrome", group: "metal", get label() { return t("Metal pulido"); }, color: "#d9dce0", metallic: 1, shine: 0.92 },
  { id: "brushed", group: "metal", get label() { return t("Metal cepillado"); }, color: "#b5b9bf", metallic: 1, shine: 0.55 },
  { id: "gold", group: "metal", get label() { return t("Oro"); }, color: "#ffc857", metallic: 1, shine: 0.85 },
  { id: "copper", group: "metal", get label() { return t("Cobre"); }, color: "#c8754a", metallic: 1, shine: 0.75 },
  { id: "iron", group: "metal", get label() { return t("Hierro oxidado"); }, color: "#6b4a3a", metallic: 0.7, shine: 0.2 },
  // Naturaleza
  { id: "dirt", group: "nature", get label() { return t("Tierra"); }, color: "#5a4632", metallic: 0, shine: 0.05 },
  { id: "stone", group: "nature", get label() { return t("Piedra"); }, color: "#8a8a86", metallic: 0, shine: 0.15 },
  { id: "wood", group: "nature", get label() { return t("Madera"); }, color: "#9a6b3c", metallic: 0, shine: 0.3 },
  { id: "bark", group: "nature", get label() { return t("Corteza"); }, color: "#4b3a2c", metallic: 0, shine: 0.05 },
  { id: "leaf", group: "nature", get label() { return t("Hoja"); }, color: "#4f8a2f", metallic: 0, shine: 0.35, clearcoat: 0.2, clearcoatRoughness: 0.4 },
  { id: "moss", group: "nature", get label() { return t("Musgo"); }, color: "#4d6b2a", metallic: 0, shine: 0 },
  { id: "mushroom", group: "nature", get label() { return t("Hongo"); }, color: "#d9c7a6", metallic: 0, shine: 0.2 },
  { id: "coral", group: "nature", get label() { return t("Coral"); }, color: "#f0745c", metallic: 0, shine: 0.25 },
  { id: "sand", group: "nature", get label() { return t("Arena"); }, color: "#d6bf8f", metallic: 0, shine: 0 },
  { id: "snow", group: "nature", get label() { return t("Nieve"); }, color: "#f4f7fb", metallic: 0, shine: 0.15 },
  // Objetos
  { id: "plastic", group: "things", get label() { return t("Plástico brillante"); }, color: "#d6453d", metallic: 0, shine: 0.85, clearcoat: 0.5, clearcoatRoughness: 0.1 },
  { id: "rubber", group: "things", get label() { return t("Goma"); }, color: "#2b2b2e", metallic: 0, shine: 0.1 },
  { id: "wax", group: "things", get label() { return t("Cera"); }, color: "#efe3b8", metallic: 0, shine: 0.45, transmission: 0.2, ior: 1.45 },
  { id: "ceramic", group: "things", get label() { return t("Cerámica"); }, color: "#f1efe9", metallic: 0, shine: 0.85, clearcoat: 1, clearcoatRoughness: 0.05 },
  { id: "car-paint", group: "things", get label() { return t("Pintura de auto"); }, color: "#1f4fa8", metallic: 0.5, shine: 0.8, clearcoat: 1, clearcoatRoughness: 0.03 },
  { id: "flat", group: "things", get label() { return t("Color plano"); }, color: "#7aa2f7", metallic: 0, shine: 0, flat: true },
];

/** Un quinto del tamaño del modelo: el grosor de vidrio máximo */
export const MAX_THICKNESS = 0.2;

/**
 * Lo que cambia un preset en un material (formato de `MaterialInfo`).
 * `keepColor`: el color del material (lineal) pasa por encima del del preset, y el
 * del glow, si el preset brilla, también.
 */
export function presetPatch(
  preset: MaterialPreset,
  options: { keepTexture: boolean; keepColor?: [number, number, number]; modelSize: number }
) {
  const own = options.keepColor;
  const [r, g, b] = options.keepTexture ? [1, 1, 1] : own ?? hexToLinear(preset.color);
  const glow = preset.glow ? (own ?? hexToLinear(preset.glow.color)) : [0, 0, 0];
  const k = preset.glow?.strength ?? 0;
  const opacity = preset.opacity ?? 1;
  return {
    base_color: [r, g, b, opacity] as [number, number, number, number],
    metallic: preset.metallic,
    roughness: 1 - preset.shine,
    emissive: [glow[0] * k, glow[1] * k, glow[2] * k] as [number, number, number],
    alpha_mode: (opacity < 1 ? "blend" : "opaque") as "blend" | "opaque",
    unlit: !!preset.flat,
    transmission: preset.transmission ?? 0,
    ior: preset.ior ?? 1.5,
    thickness: (preset.thickness ?? 0) * MAX_THICKNESS * options.modelSize,
    clearcoat: preset.clearcoat ?? 0,
    clearcoat_roughness: preset.clearcoatRoughness ?? 0.1,
  };
}

/** Bolita de un preset */
export const presetPreview = (preset: MaterialPreset): PreviewMaterial => {
  const p = presetPatch(preset, { keepTexture: false, modelSize: 1 });
  return { color: [p.base_color[0], p.base_color[1], p.base_color[2]], opacity: p.base_color[3], metallic: p.metallic, roughness: p.roughness, emissive: p.emissive, flat: p.unlit, transmission: p.transmission, ior: p.ior, thickness: p.thickness / MAX_THICKNESS, clearcoat: p.clearcoat, clearcoatRoughness: p.clearcoat_roughness };
};

/** Material de la escena a preset (para guardarlo en la biblioteca propia) */
export function presetFromMaterial(
  id: string,
  label: string,
  modelSize: number,
  m: { base_color: number[]; metallic: number; roughness: number; emissive: number[]; alpha_mode: string; unlit: boolean; transmission: number; ior: number; thickness: number; clearcoat: number; clearcoat_roughness: number }
): MaterialPreset {
  const strength = Math.max(...m.emissive);
  return {
    id,
    label,
    color: linearToHex(m.base_color),
    opacity: m.alpha_mode === "blend" ? m.base_color[3] : undefined,
    metallic: m.metallic,
    shine: 1 - m.roughness,
    glow: strength > 0 ? { color: linearToHex(m.emissive.map((c) => c / strength)), strength } : undefined,
    flat: m.unlit || undefined,
    transmission: m.transmission || undefined,
    ior: m.ior !== 1.5 ? m.ior : undefined,
    thickness: m.thickness > 0 ? Math.min(1, m.thickness / (MAX_THICKNESS * modelSize)) : undefined,
    clearcoat: m.clearcoat || undefined,
    clearcoatRoughness: m.clearcoat > 0 ? m.clearcoat_roughness : undefined,
  };
}

/** Biblioteca propia: se guarda en la app y sirve en todos los proyectos */
const [userPresetList, setUserPresetList] = createPersisted<MaterialPreset[]>("materials.userPresets", []);
export const userPresets = userPresetList;
export const saveUserPreset = (preset: MaterialPreset) => setUserPresetList([...userPresetList(), preset]);
export const deleteUserPreset = (id: string) => setUserPresetList(userPresetList().filter((p) => p.id !== id));
