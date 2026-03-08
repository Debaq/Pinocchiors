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
    // Ignore when typing in inputs
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    if ((e.target as HTMLElement).isContentEditable) return;

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
