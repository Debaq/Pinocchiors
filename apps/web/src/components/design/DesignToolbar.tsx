// Barras de herramientas de Diseñar: a la izquierda lo más usado (como las
// herramientas de los otros espacios) y arriba el modo y el resto en menús por
// categoría. Durante un sketch las reemplazan las del sketch.

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

/** Botón de la barra izquierda: solo el ícono; el nombre en la ayuda */
export const SideButton: Component<{ label: string; icon: IconC; tip?: string; shortcut?: string; active?: boolean; disabled?: boolean; onClick: () => void }> = (props) => (
  <Tooltip content={`${props.shortcut ? `${props.label} (${props.shortcut})` : props.label}${props.tip ? `: ${props.tip}` : ""}`} placement="right">
    <button
      aria-label={props.label}
      class={clsx(
        "flex items-center justify-center w-8 h-7 shrink-0 rounded disabled:opacity-40 disabled:pointer-events-none",
        props.active ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface",
      )}
      disabled={props.disabled}
      onClick={() => props.onClick()}
    >
      <props.icon size={18} />
      {/* El nombre para lectores de pantalla (y las pruebas que buscan por texto) */}
      <span class="sr-only">{props.label}</span>
    </button>
  </Tooltip>
);

/** Separador de la barra izquierda */
export const SideSep = () => <div class="w-6 h-px bg-border my-0.5 shrink-0" />;

/**
 * Botón de la barra izquierda con variantes: la esquina ▸ abre la lista a la
 * derecha (como las familias de herramientas del sketch)
 */
const SideFamily: Component<{ name: string; tools: Tool[]; hasBody: boolean }> = (props) => {
  const [open, setOpen] = createSignal(false);
  let row!: HTMLDivElement;
  const close = (e: PointerEvent) => {
    if (!row.contains(e.target as Node) && !(e.target as HTMLElement).closest?.("[data-family-list]")) setOpen(false);
  };
  document.addEventListener("pointerdown", close, true);
  onCleanup(() => document.removeEventListener("pointerdown", close, true));
  const main = props.tools[0];
  return (
    <div ref={row} class="relative">
      <SideButton label={main.label} icon={main.icon} tip={main.tip} disabled={!!main.needsBody && !props.hasBody} onClick={main.run} />
      <div class="absolute right-0 bottom-0" data-toolbar-menu={props.name} data-items={JSON.stringify(props.tools.map((t) => t.label))}>
        <button class="flex items-end justify-end w-3 h-3 text-text-dim hover:text-text" title={props.name} onClick={() => setOpen(!open())}>
          <Icons.CaretRight size={8} />
          <span class="sr-only">{props.name}</span>
        </button>
      </div>
      <Show when={open()}>
        <div
          role="menu"
          data-family-list
          class="fixed z-30 min-w-44 rounded-md border border-border bg-bg-lighter shadow-lg py-1"
          style={{ left: `${row.getBoundingClientRect().right + 6}px`, top: `${row.getBoundingClientRect().top}px` }}
        >
          <For each={props.tools}>
            {(t) => (
              <button
                class="w-full flex items-center gap-2 px-2 py-1 text-xs text-left text-text-muted hover:text-text hover:bg-surface disabled:opacity-40 disabled:pointer-events-none"
                title={t.tip}
                disabled={!!t.needsBody && !props.hasBody}
                onClick={() => {
                  setOpen(false);
                  t.run();
                }}
              >
                <t.icon size={16} />
                {t.label}
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

/** Herramientas de Diseñar en la barra izquierda (sin sketch abierto) */
export const DesignSideTools: Component<{ store: CadStore; ui: CadUi; actions: DesignActions }> = (props) => {
  const a = props.actions;
  const ui = props.ui;
  const hasBody = () => !!props.store.result()?.body;
  const sketches: Tool[] = [
    { label: "Sketch", icon: D.Sketch, run: () => void a.startSketch(), tip: "En el plano o la cara elegida (o sobre otro sketch); si no hay nada elegido, se elige en el visor" },
    { label: "Sketch 3D", icon: D.Sketch3d, run: a.addSketch3d, tip: "Líneas, arcos y splines en el espacio (caminos de barrido): clic en un plano o en un vértice del sólido" },
    { label: "Sketch envuelto", icon: D.SurfaceSketch, run: () => void a.startSurfaceSketch(), needsBody: true, tip: "Un sketch dibujado en el desarrollo de una cara y llevado sobre ella (cilindros, conos…)" },
  ];
  const main: Tool[] = [
    { label: "Extrusión", icon: D.Extrude, run: a.addExtrude, tip: "Las regiones elegidas en el visor o una cara plana del sólido; si no hay nada elegido, todo el último sketch" },
    { label: "Revolución", icon: D.Revolve, run: a.addRevolve, tip: "Las regiones elegidas; eje: la primera línea de construcción del sketch, o Z" },
    { label: "Redondeo", icon: D.Fillet, run: () => void a.startEdges("fillet"), needsBody: true, tip: "Las aristas elegidas (o se eligen en el diálogo)" },
    { label: "Chaflán", icon: D.Chamfer, run: () => void a.startEdges("chamfer"), needsBody: true, tip: "Las aristas elegidas (o se eligen en el diálogo)" },
    { label: "Agujero", icon: D.Hole, run: a.addHole, needsBody: true, tip: "En los puntos (o círculos) del sketch elegido o el último: simple, con caja o avellanado" },
  ];
  const button = (t: Tool) => <SideButton label={t.label} icon={t.icon} tip={t.tip} disabled={!!t.needsBody && !hasBody()} onClick={t.run} />;
  return (
    <Show when={!ui.assemblyMode()}>
      <SideFamily name="Más sketches" tools={sketches} hasBody={hasBody()} />
      <For each={main}>{button}</For>
      <Show when={ui.sketchCopy()}>
        {button({ label: "Pegar sketch", icon: D.Sketch, run: a.pasteSketch, tip: "El sketch copiado, como uno nuevo en el plano o la cara elegida" })}
      </Show>
      <SideSep />
      {button({ label: "Plano 2D", icon: D.Drawing, run: () => ui.setDrawingOpen(true), needsBody: true, tip: "Vistas, líneas ocultas, cotas y cajetín" })}
    </Show>
  );
};

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
    { label: "Eliminar cara", icon: D.DeleteParts, run: () => void a.startFaces("delete_face"), needsBody: true, tip: "Saca caras (un agujero, un redondeo, un resalte) y cierra el hueco con las vecinas" },
    { label: "Mover cara", icon: D.MoveFace, run: () => void a.startMoveFace(), needsBody: true, tip: "Lleva caras planas hacia afuera o hacia adentro" },
    { label: "Reemplazar cara", icon: D.ReplaceFace, run: () => void a.startReplaceFace(), needsBody: true, tip: "Lleva caras planas hasta un plano o hasta otra cara (elegir las caras y al final la de destino)" },
    { label: "Cara al escaneo", icon: D.ReplaceFace, run: () => void a.startFaceToScan(), needsBody: true, tip: "Lleva caras planas hasta la piel del escaneo, aunque sea curva (suma o resta según dónde esté)" },
    { label: "Nervio", icon: D.Rib, run: a.startRib, needsBody: true, tip: "Pared desde las líneas de un sketch hasta el sólido" },
    { label: "Rosca", icon: D.Thread, run: () => void a.startThread(), needsBody: true, tip: "Rosca modelada sobre una cara cilíndrica: exterior en un eje, interior en un agujero" },
    { label: "Escala", icon: D.Scale, run: a.addScale, needsBody: true, tip: "Escala las piezas alrededor de un punto" },
    "-",
    { label: "Mover cuerpo", icon: D.MoveFace, run: () => a.addTransform("translate"), needsBody: true, tip: "Mueve piezas enteras (las elegidas, o todas): un desplazamiento o de un punto a otro" },
    { label: "Girar cuerpo", icon: D.PatternCircular, run: () => a.addTransform("rotate"), needsBody: true, tip: "Gira piezas enteras alrededor de un eje" },
    { label: "Copiar cuerpo", icon: D.PatternLinear, run: () => a.addTransform("translate", true), needsBody: true, tip: "Copia piezas enteras corridas o giradas; las copias quedan como piezas nuevas" },
  ];
  const patterns: (Tool | "-")[] = [
    { label: "Patrón lineal", icon: D.PatternLinear, run: () => a.addPattern("linear"), needsBody: true },
    { label: "Patrón circular", icon: D.PatternCircular, run: () => a.addPattern("circular"), needsBody: true },
    { label: "Simetría", icon: D.Mirror, run: a.addMirror, needsBody: true },
    { label: "Patrón en curva", icon: D.CurvePattern, run: a.addCurvePattern, needsBody: true, tip: "Copias a lo largo de un camino (sketch o hélice)" },
    { label: "Patrón por tabla", icon: D.PatternLinear, run: () => a.addPattern("table"), needsBody: true, tip: "Copias en los desplazamientos de una tabla (X, Y, Z)" },
    { label: "Patrón de relleno", icon: D.PatternLinear, run: () => a.addPattern("fill"), needsBody: true, tip: "Copias en grilla que llenan una región de un sketch (rejillas, perforados)" },
    { label: "Patrón por puntos", icon: D.PatternLinear, run: () => a.addPattern("points"), needsBody: true, tip: "Una copia en cada punto suelto de un sketch" },
  ];
  const parts: (Tool | "-")[] = [
    { label: "Booleana", icon: D.Boolean, run: a.addBoolean, needsBody: true, tip: "Unir, restar o intersecar piezas entre sí" },
    { label: "Separar", icon: D.SplitParts, run: a.addSplitParts, needsBody: true, tip: "Cada sólido suelto de una pieza pasa a ser una pieza" },
    { label: "Cortar por plano", icon: D.SplitPlane, run: a.addSplit, needsBody: true },
    { label: "Partir", icon: D.SplitParts, run: a.addSplitBy, needsBody: true, tip: "Parte piezas con un plano o con otra pieza (una superficie curva): cada pedazo queda como pieza" },
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
    { label: "Superficie plana", icon: D.Fill, run: a.addSurfacePlanar, tip: "Las regiones cerradas de un sketch como superficie" },
    { label: "Superficie desfasada", icon: D.Thicken, run: a.addSurfaceOffset, needsBody: true, tip: "Copia de superficies (o de la piel de una pieza) a una distancia" },
    { label: "Relleno", icon: D.Fill, run: () => void a.startFill(), needsBody: true, tip: "Superficie que cierra un borde (elegir las aristas); puede seguir tangente a las caras vecinas" },
    { label: "Coser", icon: D.Sew, run: a.addSew, needsBody: true, tip: "Une superficies por sus bordes; si cierran un volumen, queda un sólido" },
    { label: "Recortar con plano", icon: D.SplitPlane, run: () => void a.trimWithPlane(), needsBody: true, tip: "Deja de las superficies elegidas (o todas) el lado de un plano" },
    { label: "Recortar con sólido", icon: D.Boolean, run: a.trimWithSolid, needsBody: true, tip: "Quita de las superficies lo que queda dentro de un sólido (o deja solo eso, con Intersecar)" },
    "-",
    { label: "Engrosar", icon: D.Thicken, run: () => void a.startThicken(), needsBody: true, tip: "Da espesor a una superficie (o a caras de un sólido)" },
  ];
  const refs: (Tool | "-")[] = [
    { label: "Plano", icon: D.Plane, run: () => void a.addReference("plane"), tip: "Plano de referencia: desplazado, en ángulo, medio o por tres puntos" },
    { label: "Eje", icon: D.Axis, run: () => void a.addReference("axis"), tip: "Eje de referencia: por dos puntos, arista, cilindro o cruce de planos" },
    { label: "Punto", icon: D.Point, run: () => void a.addReference("point") },
    { label: "Hélice", icon: D.Helix, run: a.addHelix, tip: "Camino para resortes y roscas" },
    { label: "Espiral", icon: D.Helix, run: a.addSpiral, tip: "Espiral plana o hélice cónica (con dos radios)" },
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
      <Show when={!ui.assemblyMode()}>
        <Sep />
        <div class="flex items-center gap-0.5 flex-wrap">
          <ToolMenu label="Sólidos" tools={solids} hasBody={hasBody()} />
          <ToolMenu label="Modificar" tools={modify} hasBody={hasBody()} />
          <ToolMenu label="Patrones" tools={patterns} hasBody={hasBody()} />
          <ToolMenu label="Piezas" tools={parts} hasBody={hasBody()} />
          <ToolMenu label="Chapa" tools={sheet} hasBody={hasBody()} />
          <ToolMenu label="Superficies" tools={surfaces} hasBody={hasBody()} />
          <ToolMenu label="Referencias" tools={refs} hasBody={hasBody()} />
        </div>
      </Show>
    </div>
  );
};
