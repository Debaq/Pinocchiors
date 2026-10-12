import type { Catalog } from "../i18n";

/** Configuración: pestañas y opciones nuevas */
export const en_settings: Catalog = {
  "General": "General",
  "Visor y navegación": "Viewer and navigation",
  "Atajos de teclado": "Keyboard shortcuts",
  "Archivos y exportación": "Files and export",
  "Acerca de": "About",
  "Tamaño de la interfaz": "Interface size",
  "Agranda o achica todo: texto, botones y paneles.": "Scales everything: text, buttons and panels.",
  "Navegación": "Navigation",
  "Navegación de trackpad": "Trackpad navigation",
  "Dos dedos (o la rueda) giran la vista y con Mayús la desplazan; Ctrl o el pellizco hacen zoom. Sin esto, la rueda hace zoom.":
    "Two fingers (or the wheel) rotate the view and with Shift pan it; Ctrl or pinch zooms. Without this, the wheel zooms.",
  "Pantalla": "Display",
  "Mostrar los FPS en la cabecera": "Show FPS in the header",
  "Teclado": "Keyboard",
  "Mouse en el visor": "Mouse in the viewer",
  "Formato inicial": "Starting format",
  "Con el que arranca Exportar en un proyecto nuevo. Cada proyecto recuerda el suyo.":
    "What Export starts with in a new project. Each project remembers its own.",
  "Disposición": "Layout",
  "Olvida los anchos de paneles, las pestañas abiertas y las alturas recordadas, y recarga la interfaz. No toca tus preferencias ni tus proyectos.":
    "Forgets panel widths, open tabs and remembered heights, and reloads the interface. Your preferences and projects are untouched.",
  "Restablecer la disposición": "Reset the layout",
  "Sí, restablecer": "Yes, reset",
};

Object.assign(en_settings, {
  "Sin atajo": "No shortcut",
  "Pulsa una combinación para cambiarla. Esc cancela; Supr deja el atajo sin tecla.":
    "Click a shortcut, then press the new keys. Esc cancels; Delete leaves it without a key.",
  "Volver al atajo de fábrica": "Back to the default shortcut",
  "Restablecer": "Reset",
  "Pulsa las teclas…": "Press the keys…",
  "Ya la usa «{name}». Cámbiala o desactívala primero.": "Already used by “{name}”. Change or disable it first.",
  "Restablecer todos los atajos": "Reset all shortcuts",
  "Los gestos del mouse y las teclas dentro de cada herramienta (Sketch, pincel…) no se pueden cambiar.":
    "Mouse gestures and keys inside each tool (Sketch, brush…) can't be changed.",
});
