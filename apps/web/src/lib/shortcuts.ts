import { createSignal } from "solid-js";
import { t } from "./i18n.ts";
import { createPersisted } from "./ui-state.ts";

export interface ShortcutDef {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  action: () => void;
  description: string;
}

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
export const shortcutId = (d: Binding): string =>
  [d.ctrl && "ctrl", d.shift && "shift", d.alt && "alt", d.key.toLowerCase()].filter(Boolean).join("+");

/** `null` = el usuario lo desactivó; sin entrada = vale la de fábrica */
type Overrides = Record<string, Binding | null>;
const [overrides, setOverrides] = createPersisted<Overrides>("settings.shortcuts", {});
const [registry, setRegistry] = createSignal<ShortcutDef[]>([]);

/** Combinación vigente de un atajo (la reasignada o la de fábrica); `null` si está desactivado */
export function bindingOf(d: ShortcutDef): Binding | null {
  const o = overrides()[shortcutId(d)];
  return o === undefined ? d : o;
}

/** "Ctrl+Shift+S", "Espacio", "Flecha izquierda"… (`null` = sin atajo) */
export function formatBinding(b: Binding | null): string {
  if (!b) return t("Sin atajo");
  const key = b.key === " " ? t("Espacio") : b.key.length === 1 ? b.key.toUpperCase() : b.key;
  return [b.ctrl && "Ctrl", b.shift && "Shift", b.alt && "Alt", key].filter(Boolean).join("+");
}

export interface ShortcutRow {
  id: string;
  description: string;
  keys: string;
  /** Distinto del de fábrica */
  changed: boolean;
}

/** Atajos registrados con su combinación vigente, para la pantalla de Configuración */
export const shortcutRows = (): ShortcutRow[] =>
  registry().map((d) => ({
    id: shortcutId(d),
    description: d.description,
    keys: formatBinding(bindingOf(d)),
    changed: overrides()[shortcutId(d)] !== undefined,
  }));

/** Otro atajo que ya usa esa combinación (para no pisarlo en silencio) */
export function conflictFor(id: string, b: Binding): ShortcutDef | undefined {
  const same = (x: Binding | null) =>
    x !== null && x.key.toLowerCase() === b.key.toLowerCase() && !!x.ctrl === !!b.ctrl && !!x.shift === !!b.shift && !!x.alt === !!b.alt;
  return registry().find((d) => shortcutId(d) !== id && same(bindingOf(d)));
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

    for (const shortcut of shortcuts) {
      const b = bindingOf(shortcut);
      if (!b) continue;
      const keyMatch = e.key.toLowerCase() === b.key.toLowerCase();
      const ctrlMatch = (b.ctrl ?? false) === (e.ctrlKey || e.metaKey);
      const shiftMatch = (b.shift ?? false) === e.shiftKey;
      const altMatch = (b.alt ?? false) === e.altKey;

      if (keyMatch && ctrlMatch && shiftMatch && altMatch) {
        e.preventDefault();
        shortcut.action();
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
