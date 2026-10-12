import type { LightSettings } from "./lightRig";
import { t } from "./i18n.ts";

/** Iluminación del render: "viewer" usa las luces del visor; las demás se aplican solo mientras se renderiza */
export interface RenderLight {
  id: string;
  label: () => string;
  lights: LightSettings | null;
}

const base = { color: "#ffffff", headlight: false };

export const RENDER_LIGHTS: RenderLight[] = [
  { id: "viewer", label: () => t("Las del visor"), lights: null },
  { id: "studio", label: () => t("Estudio"), lights: { ...base, azimuth: 40, elevation: 38, intensity: 2.2, fill: 0.9, ambient: 0.35, environment: 0.8 } },
  { id: "soft", label: () => t("Suave"), lights: { ...base, azimuth: 30, elevation: 60, intensity: 1.2, fill: 1.1, ambient: 0.7, environment: 1 } },
  { id: "backlight", label: () => t("Contraluz"), lights: { ...base, azimuth: 160, elevation: 30, intensity: 2.6, fill: 0.6, ambient: 0.25, environment: 0.5 } },
  { id: "dramatic", label: () => t("Dramática"), lights: { ...base, azimuth: 80, elevation: 22, intensity: 3, fill: 0.08, ambient: 0.1, environment: 0.2 } },
];

export const renderLightById = (id: string): RenderLight | undefined => RENDER_LIGHTS.find((l) => l.id === id);
