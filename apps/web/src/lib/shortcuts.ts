import { createSignal } from "solid-js";
import { t } from "./i18n.ts";
import { createPersisted } from "./ui-state.ts";

export interface ShortcutDef {
  /** Identificador propio, para los que nacen sin tecla (`key: ""`) */
  id?: string;
  /** Vacío = sin tecla de fábrica; se asigna en Configuración */
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  action: (e: KeyboardEvent) => void;
  description: string;
  /** Dónde se muestra en Configuración (por defecto "general") */
  area?: AreaId;
  /** Solo vale cuando esto es verdadero; si no, la tecla sigue a los demás atajos (y al navegador) */
  when?: () => boolean;
  /** Shift y Alt no cuentan para reconocer la tecla: la acción los lee del evento (pasos grandes y finos) */
  looseMods?: boolean;
}

/** Áreas de la lista de atajos, en el orden en que se muestran */
export type AreaId = "file" | "tools" | "view" | "workspace" | "rig" | "scan" | "design" | "sketch";
export const AREAS: { id: AreaId; label: () => string }[] = [
  { id: "file", label: () => t("Archivo y edición") },
  { id: "workspace", label: () => t("Espacios de trabajo") },
  { id: "tools", label: () => t("Herramientas del modelo") },
  { id: "view", label: () => t("Vista") },
  { id: "rig", label: () => t("Rig y animación") },
  { id: "scan", label: () => t("Escaneo") },
  { id: "design", label: () => t("Diseñar: Operaciones") },
  { id: "sketch", label: () => t("Diseñar: Sketch") },
];

/** Una combinación de teclas (sin la acción) */
export interface Binding {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
}

export interface ShortcutManager {
  register: (shortcuts: ShortcutDef[]) => void;
  attach: () => void;
  detach: () => void;
}

/**
 * Identificador estable de un atajo: su combinación de fábrica. Es única entre
 * los atajos registrados y no cambia cuando el usuario lo reasigna
 */
export const shortcutId = (d: Binding & { id?: string }): string =>
  d.id ?? [d.ctrl && "ctrl", d.shift && "shift", d.alt && "alt", d.key.toLowerCase()].filter(Boolean).join("+");

/** `null` = el usuario lo desactivó; sin entrada = vale la de fábrica */
type Overrides = Record<string, Binding | null>;
const [overrides, setOverrides] = createPersisted<Overrides>("settings.shortcuts", {});
const [registry, setRegistry] = createSignal<ShortcutDef[]>([]);

/** Combinación vigente de un atajo (la reasignada o la de fábrica); `null` si está desactivado */
export function bindingOf(d: ShortcutDef): Binding | null {
  const o = overrides()[shortcutId(d)];
  return o === undefined ? (d.key === "" ? null : d) : o;
}

/** "Ctrl+Shift+S", "Espacio", "Flecha izquierda"… (`null` = sin atajo) */
export function formatBinding(b: Binding | null): string {
  if (!b) return t("Sin atajo");
  const key = b.key === " " ? t("Espacio") : b.key.length === 1 ? b.key.toUpperCase() : b.key;
  return [b.ctrl && "Ctrl", b.shift && "Shift", b.alt && "Alt", key].filter(Boolean).join("+");
}

/**
 * Herramientas del Sketch: una tecla suelta cada una (sin Ctrl/Alt) que solo
 * vale dentro del sketch, así que no choca con los atajos generales. Las
 * registra CadView; `id` es el de la herramienta
 */
interface SketchKey {
  id: string;
  description: () => string;
  key?: string;
}
const [sketchKeys, setSketchKeys] = createSignal<SketchKey[]>([]);
export const registerSketchKeys = (tools: SketchKey[]) => setSketchKeys(tools);
const sketchId = (toolId: string) => `sketch.${toolId}`;
/** Teclas que el sketch usa por su cuenta y no se pueden asignar a una herramienta */
const SKETCH_RESERVED: Record<string, () => string> = {
  q: () => t("Construcción"),
};

/** Tecla vigente de una herramienta del Sketch (`undefined` = sin tecla), en minúscula */
export function sketchKey(toolId: string, fallback?: string): string | undefined {
  const o = overrides()[sketchId(toolId)];
  return (o === undefined ? fallback : o?.key)?.toLowerCase();
}

export interface ShortcutRow {
  id: string;
  area: AreaId;
  description: string;
  keys: string;
  /** Distinto del de fábrica */
  changed: boolean;
  /** Solo una tecla suelta (sin Ctrl, Shift ni Alt) */
  plainKeyOnly: boolean;
}

/** Atajos con su combinación vigente, para la pantalla de Configuración */
export const shortcutRows = (): ShortcutRow[] => [
  ...registry().map((d) => ({
    id: shortcutId(d),
    area: d.area ?? "tools",
    description: d.description,
    keys: formatBinding(bindingOf(d)),
    changed: overrides()[shortcutId(d)] !== undefined,
    plainKeyOnly: false,
  })),
  ...sketchKeys().map((k) => {
    const key = sketchKey(k.id, k.key);
    return {
      id: sketchId(k.id),
      area: "sketch" as AreaId,
      description: k.description(),
      keys: formatBinding(key ? { key } : null),
      changed: overrides()[sketchId(k.id)] !== undefined,
      plainKeyOnly: true,
    };
  }),
];

const same = (x: Binding | null, b: Binding) =>
  x !== null && x.key.toLowerCase() === b.key.toLowerCase() && !!x.ctrl === !!b.ctrl && !!x.shift === !!b.shift && !!x.alt === !!b.alt;

/** Nombre del atajo que ya usa esa combinación en su misma zona (para no pisarlo en silencio) */
export function conflictFor(id: string, b: Binding): string | undefined {
  if (id.startsWith("sketch.")) {
    const reserved = SKETCH_RESERVED[b.key.toLowerCase()];
    if (reserved) return reserved();
    return sketchKeys().find((k) => sketchId(k.id) !== id && sketchKey(k.id, k.key) === b.key.toLowerCase())?.description();
  }
  return registry().find((d) => shortcutId(d) !== id && same(bindingOf(d), b))?.description;
}

/** Reasigna un atajo; `null` lo desactiva y `undefined` vuelve al de fábrica */
export function rebind(id: string, b: Binding | null | undefined): void {
  const next = { ...overrides() };
  if (b === undefined) delete next[id];
  else next[id] = b && { key: b.key, ...(b.ctrl && { ctrl: true }), ...(b.shift && { shift: true }), ...(b.alt && { alt: true }) };
  setOverrides(next);
}

export const resetShortcuts = () => setOverrides({});
export const hasShortcutChanges = () => Object.keys(overrides()).length > 0;

export function createShortcutManager(): ShortcutManager {
  let shortcuts: ShortcutDef[] = [];
  let handler: ((e: KeyboardEvent) => void) | null = null;

  const onKeyDown = (e: KeyboardEvent) => {
    // Ignore when typing in inputs. Deslizadores, casillas y botones no
    // escriben: con el foco en ellos (después de usarlos) los atajos siguen
    const target = e.target as HTMLElement;
    const tag = target.tagName;
    const typing =
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      (tag === "INPUT" && !["range", "checkbox", "radio", "button", "color"].includes((target as HTMLInputElement).type));
    if (typing) return;
    if (target.isContentEditable) return;
    // Ctrl+C con texto seleccionado (chat, mensajes) copia: no es un atajo
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && window.getSelection()?.toString()) return;

    for (const shortcut of shortcuts) {
      const b = bindingOf(shortcut);
      if (!b || (shortcut.when && !shortcut.when())) continue;
      const keyMatch = e.key.toLowerCase() === b.key.toLowerCase();
      const ctrlMatch = (b.ctrl ?? false) === (e.ctrlKey || e.metaKey);
      const shiftMatch = shortcut.looseMods || (b.shift ?? false) === e.shiftKey;
      const altMatch = shortcut.looseMods || (b.alt ?? false) === e.altKey;

      if (keyMatch && ctrlMatch && shiftMatch && altMatch) {
        e.preventDefault();
        shortcut.action(e);
        return;
      }
    }
  };

  return {
    register(defs: ShortcutDef[]) {
      shortcuts = defs;
      setRegistry(defs);
    },
    attach() {
      if (handler) return;
      handler = onKeyDown;
      window.addEventListener("keydown", handler);
    },
    detach() {
      if (handler) {
        window.removeEventListener("keydown", handler);
        handler = null;
      }
    },
  };
}
