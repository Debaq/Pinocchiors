import { t } from "./i18n.ts";

/** Material de partida: colores en sRGB (#rrggbb), el resto de 0 a 1 */
export interface MaterialPreset {
  id: string;
  label: string;
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
}

export const MATERIAL_PRESETS: MaterialPreset[] = [
  { id: "liquid", get label() { return t("Líquido transparente"); }, color: "#9fd8ff", opacity: 0.35, metallic: 0, shine: 0.95 },
  { id: "glass", get label() { return t("Vidrio"); }, color: "#e8f4f8", opacity: 0.2, metallic: 0, shine: 1 },
  { id: "neon-green", get label() { return t("Luz neón verde"); }, color: "#0b1a10", metallic: 0, shine: 0.6, glow: { color: "#39ff7a", strength: 1 } },
  { id: "neon-pink", get label() { return t("Luz neón rosa"); }, color: "#1a0b14", metallic: 0, shine: 0.6, glow: { color: "#ff3fa4", strength: 1 } },
  { id: "neon-blue", get label() { return t("Luz neón azul"); }, color: "#0b101a", metallic: 0, shine: 0.6, glow: { color: "#3fa9ff", strength: 1 } },
  { id: "lava", get label() { return t("Lava"); }, color: "#2a0c04", metallic: 0, shine: 0.3, glow: { color: "#ff5a14", strength: 0.8 } },
  { id: "chrome", get label() { return t("Metal pulido"); }, color: "#d9dce0", metallic: 1, shine: 0.92 },
  { id: "brushed", get label() { return t("Metal cepillado"); }, color: "#b5b9bf", metallic: 1, shine: 0.55 },
  { id: "gold", get label() { return t("Oro"); }, color: "#ffc857", metallic: 1, shine: 0.85 },
  { id: "copper", get label() { return t("Cobre"); }, color: "#c8754a", metallic: 1, shine: 0.75 },
  { id: "skin", get label() { return t("Piel"); }, color: "#e0ac8c", metallic: 0, shine: 0.35 },
  { id: "leather", get label() { return t("Cuero"); }, color: "#6b3f26", metallic: 0, shine: 0.4 },
  { id: "dirt", get label() { return t("Tierra"); }, color: "#5a4632", metallic: 0, shine: 0.05 },
  { id: "stone", get label() { return t("Piedra"); }, color: "#8a8a86", metallic: 0, shine: 0.15 },
  { id: "wood", get label() { return t("Madera"); }, color: "#9a6b3c", metallic: 0, shine: 0.3 },
  { id: "bone", get label() { return t("Hueso"); }, color: "#e8dfc8", metallic: 0, shine: 0.3 },
  { id: "plastic", get label() { return t("Plástico brillante"); }, color: "#d6453d", metallic: 0, shine: 0.85 },
  { id: "rubber", get label() { return t("Goma"); }, color: "#2b2b2e", metallic: 0, shine: 0.1 },
  { id: "cloth", get label() { return t("Tela"); }, color: "#4a5f8a", metallic: 0, shine: 0 },
  { id: "flat", get label() { return t("Color plano"); }, color: "#7aa2f7", metallic: 0, shine: 0, flat: true },
];
