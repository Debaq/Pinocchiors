/**
 * Tema de la app. Los colores están todos en styles/app.css; acá solo se elige
 * cuál aplicar (`data-theme` en <html>) y se leen para lo que no es CSS
 * (el visor 3D, el cubo de vistas, los lienzos).
 */

import { createSignal } from "solid-js";
import { t } from "./i18n.ts";

export type Theme = "dark" | "graphite" | "nord" | "warm" | "blender" | "light" | "sepia";
export type ThemeSetting = Theme | "system";

/** Temas, con sus colores en styles/app.css (`:root[data-theme="…"]`) */
export const THEMES: { value: Theme; label: string }[] = [
  { value: "dark", get label() { return t("Drácula (oscuro)"); } },
  { value: "graphite", get label() { return t("Grafito (oscuro neutro)"); } },
  { value: "nord", get label() { return t("Ártico (oscuro frío)"); } },
  { value: "warm", get label() { return t("Brasa (oscuro cálido)"); } },
  { value: "blender", get label() { return t("Blender (oscuro gris)"); } },
  { value: "light", get label() { return t("Claro"); } },
  { value: "sepia", get label() { return t("Papel (claro cálido)"); } },
];

export const THEME_OPTIONS: { value: ThemeSetting; label: string }[] = [
  ...THEMES.map((th) => ({ value: th.value, get label() { return th.label; } })),
  { value: "system", get label() { return t("Según el sistema"); } },
];

const isSetting = (v: unknown): v is ThemeSetting => v === "system" || THEMES.some((t) => t.value === v);

/** Misma clave que usa `createPersisted("settings.theme")` */
const STORAGE_KEY = "pinocchio.ui.settings.theme";
/** Evento en `window` cuando cambia el tema aplicado */
export const THEME_EVENT = "pinocchio-theme";

const [applied, setApplied] = createSignal<Theme>("dark");
/** Tema aplicado, reactivo: los lienzos que pintan con `themeColor` lo leen para redibujarse */
export const currentTheme = applied;

const systemLight = () => window.matchMedia?.("(prefers-color-scheme: light)").matches ?? false;

export function resolveTheme(setting: ThemeSetting): Theme {
  if (setting === "system") return systemLight() ? "light" : "dark";
  // Uno guardado que ya no existe: el de por defecto
  return isSetting(setting) ? setting : "dark";
}

/** Aplica el tema y avisa a quien pinta fuera del CSS */
export function applyTheme(setting: ThemeSetting): void {
  const theme = resolveTheme(setting);
  const root = document.documentElement;
  if (root.dataset.theme === theme) return;
  // Sin transiciones durante el cambio: los colores que dependen de variables
  // quedarían a medio camino (o pegados en el tema anterior)
  root.classList.add("theme-switching");
  root.dataset.theme = theme;
  void root.offsetHeight;
  setTimeout(() => root.classList.remove("theme-switching"), 50);
  setApplied(theme);
  window.dispatchEvent(new Event(THEME_EVENT));
}

/** Tema guardado, para aplicarlo antes del primer cuadro (sin parpadeo) */
export function storedTheme(): ThemeSetting {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw !== null) {
      const value = JSON.parse(raw);
      if (isSetting(value)) return value;
    }
  } catch {
    // Sin almacenamiento: vale el oscuro
  }
  return "dark";
}

/** Con "Según el sistema", sigue los cambios del sistema operativo. Devuelve cómo dejar de escuchar */
export function followSystemTheme(setting: () => ThemeSetting): () => void {
  const media = window.matchMedia?.("(prefers-color-scheme: light)");
  if (!media) return () => {};
  const onChange = () => setting() === "system" && applyTheme("system");
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

let lightProbe: HTMLElement | undefined;

/**
 * Color de una variable del tema (`name` sin `--color-`), como texto CSS. Con
 * `light`, el de la paleta clara aunque la app esté en oscuro (lo que se
 * dibuja sobre la hoja blanca del plano)
 */
export function themeColor(name: string, light = false): string {
  let el = document.documentElement;
  if (light) {
    if (!lightProbe?.isConnected) {
      lightProbe = document.createElement("div");
      lightProbe.dataset.palette = "light";
      lightProbe.hidden = true;
      document.body.append(lightProbe);
    }
    el = lightProbe;
  }
  return getComputedStyle(el).getPropertyValue(`--color-${name}`).trim();
}

let probe: CanvasRenderingContext2D | null | undefined;

/**
 * Color CSS como 0xRRGGBB y alfa 0–1. El build minifica los valores
 * (`#ffffff` → `#fff`, a veces nombres o `rgb()`), así que se acepta
 * cualquier forma: hex de 3/4/6/8 dígitos directo y el resto vía canvas
 */
export function parseCssColor(value: string): { hex: number; alpha: number } | null {
  let h = value.trim().toLowerCase();
  if (h.startsWith("#") && /^#[0-9a-f]+$/.test(h)) {
    h = h.slice(1);
    if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join("");
    if (h.length === 6) return { hex: parseInt(h, 16), alpha: 1 };
    if (h.length === 8) return { hex: parseInt(h.slice(0, 6), 16), alpha: parseInt(h.slice(6), 16) / 255 };
    return null;
  }
  if (!h) return null;
  probe ??= document.createElement("canvas").getContext("2d");
  if (!probe) return null;
  // Un valor inválido deja fillStyle como estaba: se parte de un centinela
  probe.fillStyle = "#010203";
  probe.fillStyle = h;
  const norm = String(probe.fillStyle);
  if (norm.startsWith("#")) return norm === "#010203" && h !== "#010203" ? null : parseCssColor(norm);
  const m = norm.match(/rgba?\(\s*(\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?/);
  if (!m) return null;
  return { hex: (+m[1] << 16) | (+m[2] << 8) | +m[3], alpha: m[4] === undefined ? 1 : +m[4] };
}

/** Igual que `themeColor` pero como número 0xRRGGBB (ignora el alfa) */
export function themeHex(name: string, light = false): number {
  return parseCssColor(themeColor(name, light))?.hex ?? 0x808080;
}

/** Alfa (0–1) de una variable del tema; 1 si no trae */
export function themeAlpha(name: string): number {
  return parseCssColor(themeColor(name))?.alpha ?? 1;
}
