// Barra de herramientas de Diseñar, arriba del visor: lo más usado a la vista
// (ícono y texto) y el resto en menús por categoría. Durante un sketch la
// reemplaza la barra del sketch.

import { Component, For, JSX, Show, createSignal, onCleanup } from "solid-js";
import { clsx } from "clsx";
import type { CadStore } from "../../lib/cad";
import type { CadUi } from "../../lib/cadUi";
import type { DesignActions } from "../../lib/designActions";
import * as D from "../icons/design";
import * as Icons from "../icons";
import { Tooltip } from "../ui";

type IconC = (p: { size?: number }) => JSX.Element;

interface Tool {
  label: string;
  icon: IconC;
  run: () => void;
  /** Hace falta un sólido */
  needsBody?: boolean;
  tip?: string;
}

const ToolButton: Component<{ tool: Tool; disabled: boolean }> = (props) => (
  <Tooltip content={props.tool.tip ?? props.tool.label}>
    <button
      class="flex items-center gap-1.5 h-7 px-2 rounded text-xs text-text hover:bg-surface disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
      disabled={props.disabled}
      onClick={() => props.tool.run()}
    >
      <props.tool.icon size={16} />
      <span>{props.tool.label}</span>
    </button>
  </Tooltip>
);

/** Menú desplegable: un botón con el nombre de la categoría y sus herramientas debajo */
const ToolMenu: Component<{ label: string; tools: (Tool | "-")[]; hasBody: boolean }> = (props) => {
  const [open, setOpen] = createSignal(false);
  let root!: HTMLDivElement;
  const close = (e: PointerEvent) => {
    if (!root.contains(e.target as Node)) setOpen(false);
  };
  const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
  document.addEventListener("pointerdown", close, true);
  document.addEventListener("keydown", onKey);
  onCleanup(() => {
    document.removeEventListener("pointerdown", close, true);
    document.removeEventListener("keydown", onKey);
  });
  return (
    <div
      ref={root}
      class="relative"
      data-toolbar-menu={props.label}
      data-items={JSON.stringify(props.tools.flatMap((t) => (t === "-" ? [] : [t.label])))}
    >
      <button
        class={clsx(
          "flex items-center gap-1 h-7 px-2 rounded text-xs whitespace-nowrap",
          open() ? "bg-surface text-text" : "text-text-muted hover:text-text hover:bg-surface",
        )}
        aria-haspopup="menu"
        aria-expanded={open()}
        onClick={() => setOpen(!open())}
      >
        {props.label}
        <Icons.CaretDown size={10} />
      </button>
      <Show when={open()}>
        <div role="menu" class="absolute left-0 top-full mt-1 z-20 min-w-44 rounded-md border border-border bg-bg-lighter py-1 shadow-lg">
          <For each={props.tools}>
            {(t) =>
              t === "-" ? (
                <div class="my-1 h-px bg-border" />
              ) : (
                <button
                  class="flex w-full items-center gap-2 px-2.5 py-1.5 text-xs text-left text-text hover:bg-surface disabled:opacity-40 disabled:pointer-events-none"
                  disabled={!!t.needsBody && !props.hasBody}
                  title={t.tip}
                  onClick={() => {
                    setOpen(false);
                    t.run();
                  }}
                >
                  <t.icon size={16} />
                  {t.label}
                </button>
              )
            }
          </For>
        </div>
      </Show>
    </div>
  );
};

const Sep = () => <div class="w-px h-5 bg-border mx-1 shrink-0" />;

export const DesignToolbar: Component<{ store: CadStore; ui: CadUi; actions: DesignActions }> = (props) => {
  const a = props.actions;
  const ui = props.ui;
  const store = props.store;
  const hasBody = () => !!store.result()?.body;

  const main: Tool[] = [
    { label: "Sketch", icon: D.Sketch, run: () => void a.startSketch(), tip: "En el plano o la cara elegida; si no hay nada elegido, se elige en el visor" },
    { label: "Extrusión", icon: D.Extrude, run: a.addExtrude, tip: "Las regiones elegidas en el visor; si no hay, todo el último sketch" },
    { label: "Revolución", icon: D.Revolve, run: a.addRevolve, tip: "Las regiones elegidas; eje: la primera línea de construcción del sketch, o Z" },
    { label: "Redondeo", icon: D.Fillet, run: () => void a.startEdges("fillet"), needsBody: true, tip: "Las aristas elegidas (o se eligen en el diálogo)" },
    { label: "Chaflán", icon: D.Chamfer, run: () => void a.startEdges("chamfer"), needsBody: true, tip: "Las aristas elegidas (o se eligen en el diálogo)" },
    { label: "Agujero", icon: D.Hole, run: a.addHole, needsBody: true, tip: "En los puntos (o círculos) del sketch elegido o el último: simple, con caja o avellanado" },
  ];

  const solids: (Tool | "-")[] = [
    { label: "Barrido", icon: D.Sweep, run: a.addSweep, tip: "Un perfil a lo largo de un camino dibujado en otro sketch" },
    { label: "Transición", icon: D.Loft, run: a.addLoft, tip: "Un sólido que pasa por regiones de varios sketches" },
    "-",
    { label: "Caja", icon: D.BoxShape, run: () => a.addPrimitive({ type: "box", dx: 20, dy: 20, dz: 20, centered: true, centered_z: true }) },
    { label: "Cilindro", icon: D.Cylinder, run: () => a.addPrimitive({ type: "cylinder", radius: 10, height: 20 }) },
    { label: "Esfera", icon: D.Sphere, run: () => a.addPrimitive({ type: "sphere", radius: 10 }) },
    { label: "Cono", icon: D.Cone, run: () => a.addPrimitive({ type: "cone", r1: 10, r2: 0, height: 20 }) },
    { label: "Toro", icon: D.Torus, run: () => a.addPrimitive({ type: "torus", major: 15, minor: 4 }) },
    "-",
    { label: "Tornillo", icon: D.Bolt, run: () => void a.addStandard("bolt"), tip: "Tornillo métrico ISO; con el borde de un agujero elegido se coloca ahí con la medida y el largo que calzan" },
    { label: "Tuerca", icon: D.Nut, run: () => void a.addStandard("nut"), tip: "Tuerca hexagonal ISO 4032; en el borde de un agujero elegido" },
    { label: "Arandela", icon: D.Washer, run: () => void a.addStandard("washer"), tip: "Arandela plana ISO 7089; en el borde de un agujero elegido" },
    "-",
    { label: "STEP...", icon: D.Import, run: () => void a.importStep(), tip: "Traer un sólido de un archivo STEP" },
  ];
  const modify: (Tool | "-")[] = [
    { label: "Vaciado", icon: D.Shell, run: () => void a.startFaces("shell"), needsBody: true, tip: "Ahueca el sólido; las caras elegidas quedan abiertas" },
    { label: "Desmolde", icon: D.Draft, run: () => void a.startFaces("draft"), needsBody: true, tip: "Inclina las caras elegidas" },
    { label: "Engrosar", icon: D.Thicken, run: () => void a.startThicken(), needsBody: true, tip: "Da espesor a las caras elegidas" },
    { label: "Mover cara", icon: D.MoveFace, run: () => void a.startMoveFace(), needsBody: true, tip: "Lleva caras planas hacia afuera o hacia adentro" },
    { label: "Reemplazar cara", icon: D.ReplaceFace, run: () => void a.startReplaceFace(), needsBody: true, tip: "Lleva caras planas hasta un plano o hasta otra cara (elegir las caras y al final la de destino)" },
    { label: "Nervio", icon: D.Rib, run: a.startRib, needsBody: true, tip: "Pared desde las líneas de un sketch hasta el sólido" },
    { label: "Rosca", icon: D.Thread, run: () => void a.startThread(), needsBody: true, tip: "Rosca modelada sobre una cara cilíndrica: exterior en un eje, interior en un agujero" },
    { label: "Escala", icon: D.Scale, run: a.addScale, needsBody: true, tip: "Escala las piezas alrededor de un punto" },
  ];
  const patterns: (Tool | "-")[] = [
    { label: "Patrón lineal", icon: D.PatternLinear, run: () => a.addPattern("linear"), needsBody: true },
    { label: "Patrón circular", icon: D.PatternCircular, run: () => a.addPattern("circular"), needsBody: true },
    { label: "Simetría", icon: D.Mirror, run: a.addMirror, needsBody: true },
    { label: "Patrón en curva", icon: D.CurvePattern, run: a.addCurvePattern, needsBody: true, tip: "Copias a lo largo de un camino (sketch o hélice)" },
  ];
  const parts: (Tool | "-")[] = [
    { label: "Booleana", icon: D.Boolean, run: a.addBoolean, needsBody: true, tip: "Unir, restar o intersecar piezas entre sí" },
    { label: "Separar", icon: D.SplitParts, run: a.addSplitParts, needsBody: true, tip: "Cada sólido suelto de una pieza pasa a ser una pieza" },
    { label: "Cortar por plano", icon: D.SplitPlane, run: a.addSplit, needsBody: true },
    { label: "Borrar pieza", icon: D.DeleteParts, run: a.addDeleteParts, needsBody: true },
  ];
  const sheet: (Tool | "-")[] = [
    { label: "Chapa", icon: D.SheetMetal, run: a.addSheetMetal, tip: "Las regiones elegidas (o el último sketch) como chapa: espesor, radio de doblez y factor K" },
    { label: "Pestaña", icon: D.Flange, run: () => void a.startFlange(), needsBody: true, tip: "Pared doblada desde una arista del borde de la chapa (elegir la arista)" },
    "-",
    { label: "Desarrollo", icon: D.FlatPattern, run: () => ui.setFlatOpen(true), needsBody: true, tip: "La chapa plana para cortar, con las líneas de doblez; DXF 1:1" },
  ];
  const surfaces: (Tool | "-")[] = [
    { label: "Superficie extruida", icon: D.SurfaceExtrude, run: a.addSurfaceExtrude, tip: "Las curvas de un sketch (abiertas o cerradas) empujadas: una superficie sin espesor" },
    { label: "Superficie de revolución", icon: D.SurfaceRevolve, run: a.addSurfaceRevolve, tip: "Las curvas de un sketch giradas alrededor de un eje" },
    { label: "Relleno", icon: D.Fill, run: () => void a.startFill(), needsBody: true, tip: "Superficie que cierra un borde (elegir las aristas); puede seguir tangente a las caras vecinas" },
    { label: "Coser", icon: D.Sew, run: a.addSew, needsBody: true, tip: "Une superficies por sus bordes; si cierran un volumen, queda un sólido" },
    "-",
    { label: "Engrosar", icon: D.Thicken, run: () => void a.startThicken(), needsBody: true, tip: "Da espesor a una superficie (o a caras de un sólido)" },
  ];
  const refs: (Tool | "-")[] = [
    { label: "Plano", icon: D.Plane, run: () => void a.addReference("plane"), tip: "Plano de referencia: desplazado, en ángulo, medio o por tres puntos" },
    { label: "Eje", icon: D.Axis, run: () => void a.addReference("axis"), tip: "Eje de referencia: por dos puntos, arista, cilindro o cruce de planos" },
    { label: "Punto", icon: D.Point, run: () => void a.addReference("point") },
    { label: "Hélice", icon: D.Helix, run: a.addHelix, tip: "Camino para resortes y roscas" },
  ];

  return (
    <div class="flex items-center gap-0.5 rounded-md border border-border bg-bg-lighter/95 px-1.5 py-1 pointer-events-auto shadow-sm">
      {/* Diseño de las piezas o su ensamble */}
      <div role="tablist" aria-label="Modo" class="flex items-center gap-0.5 rounded bg-surface/40 p-0.5">
        <For each={[["design", "Diseño"], ["assembly", "Ensamble"]] as const}>
          {([id, label]) => (
            <button
              role="tab"
              aria-selected={(id === "assembly") === ui.assemblyMode()}
              class={clsx(
                "rounded px-2 py-0.5 text-xs",
                (id === "assembly") === ui.assemblyMode() ? "bg-accent text-bg" : "text-text-muted hover:text-text",
              )}
              onClick={() => {
                if (store.draft()) void store.acceptDraft();
                ui.clearPicks();
                ui.setAssemblyMode(id === "assembly");
              }}
            >
              {label}
            </button>
          )}
        </For>
      </div>
      <Sep />
      <Show when={!ui.assemblyMode()}>
        <div class="flex items-center gap-0.5 flex-wrap">
          <For each={main}>{(t) => <ToolButton tool={t} disabled={!!t.needsBody && !hasBody()} />}</For>
          <Sep />
          <ToolMenu label="Sólidos" tools={solids} hasBody={hasBody()} />
          <ToolMenu label="Modificar" tools={modify} hasBody={hasBody()} />
          <ToolMenu label="Patrones" tools={patterns} hasBody={hasBody()} />
          <ToolMenu label="Piezas" tools={parts} hasBody={hasBody()} />
          <ToolMenu label="Chapa" tools={sheet} hasBody={hasBody()} />
          <ToolMenu label="Superficies" tools={surfaces} hasBody={hasBody()} />
          <ToolMenu label="Referencias" tools={refs} hasBody={hasBody()} />
          <Sep />
          <ToolButton tool={{ label: "Plano 2D", icon: D.Drawing, run: () => ui.setDrawingOpen(true), needsBody: true, tip: "Vistas, líneas ocultas, cotas y cajetín" }} disabled={!hasBody()} />
        </div>
        <Sep />
      </Show>
      <Tooltip content="Deshacer (Ctrl+Z)">
        <button class="p-1 rounded text-text-muted hover:text-text hover:bg-surface disabled:opacity-40" disabled={!store.canUndo()} onClick={() => store.undo()}>
          <Icons.ArrowCounterClockwise size={15} />
          <span class="sr-only">Deshacer</span>
        </button>
      </Tooltip>
      <Tooltip content="Rehacer (Ctrl+Shift+Z)">
        <button class="p-1 rounded text-text-muted hover:text-text hover:bg-surface disabled:opacity-40" disabled={!store.canRedo()} onClick={() => store.redo()}>
          <Icons.ArrowClockwise size={15} />
          <span class="sr-only">Rehacer</span>
        </button>
      </Tooltip>
    </div>
  );
};
