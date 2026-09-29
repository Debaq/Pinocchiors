/**
 * Tema de la app. Los colores están todos en styles/app.css; acá solo se elige
 * cuál aplicar (`data-theme` en <html>) y se leen para lo que no es CSS
 * (el visor 3D, el cubo de vistas, los lienzos).
 */

import { createSignal } from "solid-js";

export type ThemeSetting = "dark" | "light" | "system";
export type Theme = "dark" | "light";

export const THEME_OPTIONS: { value: ThemeSetting; label: string }[] = [
  { value: "dark", label: "Oscuro" },
  { value: "light", label: "Claro" },
  { value: "system", label: "Según el sistema" },
];

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
  return setting;
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
      if (value === "dark" || value === "light" || value === "system") return value;
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

/** Color de una variable del tema (`name` sin `--color-`), como texto CSS */
export function themeColor(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--color-${name}`).trim();
}

/** Igual que `themeColor` pero como número 0xRRGGBB (ignora el alfa) */
export function themeHex(name: string): number {
  const value = themeColor(name);
  const hex = value.startsWith("#") ? value.slice(1, 7) : "";
  return hex.length === 6 ? parseInt(hex, 16) : 0xff00ff;
}
