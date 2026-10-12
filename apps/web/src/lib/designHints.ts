// Atajos del visor de Diseñar según lo que se está haciendo, para la barra de
// estado (teclas a la izquierda, qué hacen a la derecha)

import type { CadStore } from "./cad";
import { t as tr } from "./i18n.ts";
import type { CadUi, SketchTool } from "./cadUi";

export interface KeyHint {
  keys: string;
  label: string;
}

const viewHints = (): KeyHint[] => [
  { keys: tr("Botón del medio"), label: tr("girar") },
  { keys: tr("Botón derecho"), label: tr("mover") },
  { keys: tr("Rueda"), label: tr("acercar") },
];

/** Qué hace el clic en las herramientas del sketch que no dibujan */
const SKETCH_CLICK: Partial<Record<SketchTool, string>> = {
  get select() { return tr("elegir"); },
  get move() { return tr("punto base y destino"); },
  get copy() { return tr("punto base y destino"); },
  get rotate() { return tr("centro, desde y hasta"); },
  get scale() { return tr("punto base, desde y hasta"); },
  get split() { return tr("donde se parte"); },
  get offset() { return tr("la curva, después el lado"); },
  get paste() { return tr("dónde va lo pegado"); },
  get trim() { return tr("el tramo que se quita"); },
  get stretch() { return tr("caja sobre las puntas, punto base y destino"); },
  get block_insert() { return tr("dónde va el origen de la copia (Esc termina)"); },
  get calibrate() { return tr("dos puntos de la imagen a una distancia conocida"); },
  get extend() { return tr("cerca del extremo"); },
  get circle_tan() { return tr("cada línea o curva, cerca de donde toca"); },
};

export function designHints(store: CadStore, ui: CadUi): KeyHint[] {
  if (ui.drawingOpen() || ui.flatOpen()) return [];
  if (ui.session())
    return [
      { keys: tr("Clic"), label: SKETCH_CLICK[ui.tool()] ?? tr("dibujar") },
      ...(ui.tool() === "select"
        ? [
            { keys: tr("Doble clic"), label: tr("cadena") },
            { keys: tr("Ctrl+arrastrar"), label: tr("lazo") },
            { keys: tr("Ctrl+Mayús+arrastrar"), label: tr("arrastre libre") },
          ]
        : ui.tool() === "trim"
          ? [
              { keys: tr("Arrastrar"), label: tr("recortar todo lo que cruza") },
              { keys: tr("Ctrl+clic"), label: tr("con qué se corta") },
              { keys: tr("Mayús+clic"), label: tr("extender") },
            ]
          : ui.tool() === "extend"
            ? [
                { keys: tr("Ctrl+clic"), label: tr("hasta dónde") },
                { keys: tr("Mayús+clic"), label: tr("recortar") },
              ]
            : []),
      { keys: tr("Mayús"), label: tr("sin anclajes") },
      { keys: "Q", label: tr("construcción") },
      { keys: "Ctrl+C / Ctrl+V", label: tr("copiar y pegar") },
      { keys: "Ctrl+Z", label: tr("deshacer") },
      { keys: tr("Supr"), label: tr("borrar lo elegido") },
      { keys: "Esc", label: tr("cortar la herramienta") },
      { keys: "Enter", label: tr("terminar") },
      { keys: tr("Botón del medio"), label: tr("girar") },
    ];
  if (ui.pick().kind !== "none") return [{ keys: tr("Clic"), label: tr("elegir") }, { keys: "Esc", label: tr("cancelar") }, ...viewHints()];
  if (store.draft())
    return [
      { keys: "Enter", label: tr("aceptar") },
      { keys: "Esc", label: tr("cancelar") },
      { keys: tr("Arrastrar el borde de arriba"), label: tr("mover el diálogo") },
      ...viewHints(),
    ];
  if (ui.picks().length)
    return [
      { keys: tr("Mayús+clic"), label: tr("sumar o quitar") },
      { keys: "F", label: tr("acercar a lo elegido") },
      { keys: tr("Mayús+S"), label: tr("sketch encima") },
      { keys: tr("Clic derecho"), label: tr("menú") },
      { keys: "Esc", label: tr("limpiar") },
    ];
  return [
    { keys: tr("Clic"), label: tr("elegir") },
    { keys: tr("Arrastrar"), label: tr("caja") },
    ...viewHints(),
    { keys: tr("Mayús+1…7"), label: tr("vistas") },
    { keys: tr("Inicio"), label: tr("ver todo") },
  ];
}
