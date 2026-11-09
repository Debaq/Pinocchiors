// Barras del sketch 3D: a la izquierda las herramientas de dibujo (como las
// del sketch en un plano) y arriba el plano activo, a qué se pega el clic y
// los botones para cortar el trazo y cerrar el sketch.

import { Component, For, JSX, Show } from "solid-js";
import { clsx } from "clsx";
import type { CadUi, Hit3d, Plane3dKey, Tool3d } from "../../lib/cadUi";
import * as SketchIcons from "../icons/sketch";
import * as Icons from "../icons";
import { SideButton, SideSep } from "./DesignToolbar";
import { Tooltip } from "../ui";
import { t } from "../../lib/i18n";
import { sketchKey } from "../../lib/shortcuts";

type IconC = (p: { size?: number }) => JSX.Element;

/** Herramientas del sketch 3D; las teclas son las mismas que en el sketch en un plano */
export const TOOL3D: { id: Tool3d; short: string; prompt: string; icon: IconC; keyId: string; key: string }[] = [
  {
    id: "select",
    short: "Elegir",
    get prompt() { return t("Clic en puntos y curvas para elegirlos y restringirlos (Mayús suma)"); },
    icon: SketchIcons.Select,
    keyId: "select",
    key: "S",
  },
  {
    id: "line",
    short: "Línea",
    get prompt() { return t("Clic sobre lo que tienes delante (el modelo o el sólido), en un punto o en un vértice; en el vacío cae en el plano activo. Cada clic sigue la línea; Esc la corta"); },
    icon: SketchIcons.Line,
    keyId: "line",
    key: "L",
  },
  {
    id: "arc",
    short: "Arco 3 p.",
    get prompt() { return t("Arco por tres puntos: comienzo, uno del medio y fin"); },
    icon: SketchIcons.Arc3,
    keyId: "arc3",
    key: "B",
  },
  {
    id: "spline",
    short: "Spline",
    get prompt() { return t("Clic en cada punto de paso; Esc o Enter la termina"); },
    icon: SketchIcons.Spline,
    keyId: "spline",
    key: "N",
  },
  {
    id: "point",
    short: "Punto",
    get prompt() { return t("Clic donde va el punto"); },
    icon: SketchIcons.Point,
    keyId: "point",
    key: "O",
  },
];

/** Tecla de la herramienta (la que se reasignó en Atajos, o la de fábrica) */
export const tool3dKey = (tool: (typeof TOOL3D)[number]) => sketchKey(tool.keyId, tool.key);

/** Qué hay bajo el cursor, para la etiqueta junto al puntero */
export function hit3dLabel(h: Hit3d, plane: Plane3dKey): { glyph: string; label: string } {
  if (h.point !== undefined) return { glyph: "●", label: t("Punto") };
  if (h.entity !== undefined) return { glyph: "∕", label: t("Sobre la curva") };
  if (h.vertex) return { glyph: "●", label: t("Vértice del sólido") };
  if (h.surface === "scan") return { glyph: "◈", label: t("Sobre el modelo") };
  if (h.surface === "face") return { glyph: "◈", label: t("Sobre la cara") };
  return { glyph: "▱", label: t("Plano {p}", { p: plane.toUpperCase() }) };
}

/** Barra izquierda con el sketch 3D abierto */
export const Sketch3dSideTools: Component<{ ui: CadUi }> = (props) => {
  const ui = props.ui;
  return (
    <>
      <For each={TOOL3D}>
        {(tool, i) => (
          <>
            <Show when={i() === 1}>
              <SideSep />
            </Show>
            <SideButton
              label={tool.short}
              shortcut={tool3dKey(tool)?.toUpperCase()}
              tip={tool.prompt}
              icon={tool.icon}
              active={ui.tool3d() === tool.id}
              onClick={() => ui.setTool3d(tool.id)}
            />
          </>
        )}
      </For>
      <SideSep />
      <SideButton
        label="Construcción"
        shortcut="Q"
        tip={t("con algo elegido lo pasa a construcción o de vuelta; sin nada elegido, lo que se dibuja sale de construcción")}
        icon={SketchIcons.Construction}
        active={ui.construction3d()}
        onClick={() => (ui.sel3d().length ? ui.sketch3dApi()?.toggleConstruction() : ui.setConstruction3d(!ui.construction3d()))}
      />
    </>
  );
};

/** Barra de arriba con el sketch 3D abierto */
export const Sketch3dTopBar: Component<{ ui: CadUi; name: string; onDone: () => void }> = (props) => {
  const ui = props.ui;
  const chip = (on: boolean) => clsx("px-1.5 py-0.5 rounded text-[11px]", on ? "bg-accent text-bg" : "text-text-muted hover:text-text hover:bg-surface");
  return (
    <div class="flex flex-wrap items-center gap-1 rounded-md border border-border bg-bg-lighter/90 px-1.5 py-1 pointer-events-auto" data-sketch3d-bar>
      <span class="px-1 text-xs font-medium text-text whitespace-nowrap">{props.name}</span>
      <div class="w-px h-5 bg-border mx-0.5" />
      <Tooltip content={t("Pegarse a lo que está delante: las caras del sólido y la piel del modelo (escaneo, STL). Apagado, el clic cae siempre en el plano activo")}>
        <button aria-pressed={ui.snapSurface3d()} class={chip(ui.snapSurface3d())} onClick={() => ui.setSnapSurface3d(!ui.snapSurface3d())}>
          ◈ {t("Pegar a superficies")}
        </button>
      </Tooltip>
      <div class="w-px h-5 bg-border mx-0.5" />
      <span class="text-[11px] text-text-muted whitespace-nowrap">{t("Plano activo")}</span>
      <div role="radiogroup" aria-label={t("Plano activo")} class="flex items-center gap-0.5">
        <For each={["xy", "xz", "yz"] as const}>
          {(k) => (
            <button role="radio" aria-checked={ui.plane3d() === k} class={chip(ui.plane3d() === k)} onClick={() => ui.setPlane3d(k)}>
              {k.toUpperCase()}
            </button>
          )}
        </For>
      </div>
      <label class="flex items-center gap-1 text-[11px] text-text-muted" title={t("Distancia del plano activo al origen")}>
        <input
          type="number"
          step="1"
          value={ui.offset3d()}
          aria-label={t("Desplazado")}
          class="w-14 px-1 py-0.5 rounded bg-surface/40 border border-border text-xs text-text font-mono outline-none focus:border-accent"
          onChange={(e) => ui.setOffset3d(parseFloat(e.currentTarget.value) || 0)}
        />
        mm
      </label>
      <div class="w-px h-5 bg-border mx-0.5" />
      <Tooltip content={t("Corta lo que se está dibujando (Esc); una spline se crea")}>
        <button class={chip(false)} disabled={ui.trace3d().length === 0} classList={{ "opacity-40": ui.trace3d().length === 0 }} onClick={() => ui.sketch3dApi()?.endTrace()}>
          {t("Terminar trazo")}
        </button>
      </Tooltip>
      <button
        class="flex items-center gap-1 px-2 py-0.5 rounded text-xs bg-accent text-bg hover:opacity-90"
        aria-label={t("Cerrar el sketch 3D")}
        onClick={() => props.onDone()}
      >
        <Icons.Check size={12} />
        {t("Listo")}
      </button>
    </div>
  );
};
