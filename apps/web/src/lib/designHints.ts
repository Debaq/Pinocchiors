// Atajos del visor de Diseñar según lo que se está haciendo, para la barra de
// estado (teclas a la izquierda, qué hacen a la derecha)

import type { CadStore } from "./cad";
import type { CadUi } from "./cadUi";

export interface KeyHint {
  keys: string;
  label: string;
}

const VIEW: KeyHint[] = [
  { keys: "Botón del medio", label: "girar" },
  { keys: "Botón derecho", label: "mover" },
  { keys: "Rueda", label: "acercar" },
];

export function designHints(store: CadStore, ui: CadUi): KeyHint[] {
  if (ui.drawingOpen()) return [];
  if (ui.session())
    return [
      { keys: "Clic", label: "dibujar" },
      { keys: "Mayús", label: "sin anclajes" },
      { keys: "Q", label: "construcción" },
      { keys: "Supr", label: "borrar lo elegido" },
      { keys: "Esc", label: "cortar la herramienta" },
      { keys: "Enter", label: "terminar" },
      { keys: "Botón del medio", label: "girar" },
    ];
  if (ui.pick().kind !== "none") return [{ keys: "Clic", label: "elegir" }, { keys: "Esc", label: "cancelar" }, ...VIEW];
  if (store.draft())
    return [
      { keys: "Enter", label: "aceptar" },
      { keys: "Esc", label: "cancelar" },
      { keys: "Arrastrar el borde de arriba", label: "mover el diálogo" },
      ...VIEW,
    ];
  if (ui.picks().length)
    return [
      { keys: "Mayús+clic", label: "sumar o quitar" },
      { keys: "F", label: "acercar a lo elegido" },
      { keys: "Mayús+S", label: "sketch encima" },
      { keys: "Clic derecho", label: "menú" },
      { keys: "Esc", label: "limpiar" },
    ];
  return [
    { keys: "Clic", label: "elegir" },
    { keys: "Arrastrar", label: "caja" },
    ...VIEW,
    { keys: "Mayús+1…7", label: "vistas" },
    { keys: "Inicio", label: "ver todo" },
  ];
}
