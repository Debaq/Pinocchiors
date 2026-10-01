export interface ShortcutDef {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  action: () => void;
  description: string;
}

export interface ShortcutManager {
  register: (shortcuts: ShortcutDef[]) => void;
  attach: () => void;
  detach: () => void;
}

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
      const keyMatch = e.key.toLowerCase() === shortcut.key.toLowerCase();
      const ctrlMatch = (shortcut.ctrl ?? false) === (e.ctrlKey || e.metaKey);
      const shiftMatch = (shortcut.shift ?? false) === e.shiftKey;
      const altMatch = (shortcut.alt ?? false) === e.altKey;

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
