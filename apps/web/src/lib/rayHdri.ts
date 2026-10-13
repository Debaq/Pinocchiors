import { t } from "./i18n";

/** Entornos HDR que trae la app (Poly Haven, CC0); el motor los lee de dentro del binario */
export const BUILTIN_HDRIS: { path: string; label: () => string }[] = [
  { path: "builtin:studio", label: () => t("Estudio") },
  { path: "builtin:loft", label: () => t("Loft de fotografía") },
  { path: "builtin:sky", label: () => t("Cielo con nubes") },
  { path: "builtin:overcast", label: () => t("Cielo nublado") },
  { path: "builtin:sunset", label: () => t("Atardecer") },
  { path: "builtin:warehouse", label: () => t("Bodega") },
];

/** Valores del selector que no son un entorno incluido */
export const HDRI_NONE = "none";
export const HDRI_FILE = "file";

/** Valor del selector para lo que hay guardado: un incluido, ninguno, o un archivo externo */
export function hdriSelectValue(path: string): string {
  if (!path) return HDRI_NONE;
  return BUILTIN_HDRIS.some((h) => h.path === path) ? path : HDRI_FILE;
}
