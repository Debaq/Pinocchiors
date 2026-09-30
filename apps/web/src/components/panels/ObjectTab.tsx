import { Component, Show } from "solid-js";
import * as THREE from "three";
import { PlacementPanel, type PlacementPanelProps } from "./PlacementPanel";

/** Nodo del archivo elegido en el Outliner: su transformación local */
export interface ObjectTabNode {
  name: string;
  hint: string;
  translation: [number, number, number];
  rotation: [number, number, number, number];
  scale: [number, number, number];
  hidden: boolean;
}

export interface ObjectTabProps {
  meshInfo?: { vertices: number; faces: number; format: string };
  node?: ObjectTabNode;
  onToggleNode?: () => void;
  onDeselectNode?: () => void;
  /** Transformar el modelo: piso, frente, girar, espejar, escalar, mover, origen */
  placement?: PlacementPanelProps;
}

/** Pestaña Objeto: el modelo y sus transformaciones, disponible en cualquier espacio */
export const ObjectTab: Component<ObjectTabProps> = (props) => (
  <Show
    when={props.meshInfo}
    fallback={<p class="text-xs text-text-muted">Importa un modelo (Archivo → Importar modelo) para transformarlo.</p>}
  >
    {(info) => (
      <div class="space-y-4">
        <div class="flex justify-between gap-2 text-xs px-3 py-2 rounded-md bg-surface/30 border border-border">
          <span class="text-text font-mono uppercase">{info().format}</span>
          <span class="text-text-muted">
            <span class="text-text font-mono">{info().vertices.toLocaleString()}</span> vértices ·{" "}
            <span class="text-text font-mono">{info().faces.toLocaleString()}</span> caras
          </span>
        </div>
        <Show when={props.node}>
          {(node) => {
            const euler = () => {
              const [x, y, z, w] = node().rotation;
              const e = new THREE.Euler().setFromQuaternion(new THREE.Quaternion(x, y, z, w), "XYZ");
              return [e.x, e.y, e.z].map((r) => THREE.MathUtils.radToDeg(r));
            };
            const row = (label: string, values: number[], digits: number, suffix = "") => (
              <div class="flex justify-between gap-2">
                <span class="text-text-muted">{label}</span>
                <span class="font-mono text-text">{values.map((v) => `${v.toFixed(digits)}${suffix}`).join("  ")}</span>
              </div>
            );
            return (
              <div class="space-y-2 text-xs px-3 py-2 rounded-md bg-surface/30 border border-border">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-text font-medium truncate">{node().name}</span>
                  <span class="text-text-dim truncate">{node().hint}</span>
                </div>
                {row("Posición", node().translation, 3)}
                {row("Giro (XYZ)", euler(), 1, "°")}
                {row("Escala", node().scale, 3)}
                <p class="text-[11px] text-text-dim leading-relaxed">
                  Transformación local del archivo (respecto de su padre), solo lectura. Para mover el modelo entero usa las
                  herramientas de abajo.
                </p>
                <div class="flex gap-3">
                  <button class="text-accent hover:underline" onClick={() => props.onToggleNode?.()}>
                    {node().hidden ? "Mostrar" : "Ocultar"}
                  </button>
                  <button class="text-text-muted hover:text-text" onClick={() => props.onDeselectNode?.()}>
                    Quitar selección
                  </button>
                </div>
              </div>
            );
          }}
        </Show>
        <Show when={props.placement}>{(placement) => <PlacementPanel {...placement()} />}</Show>
      </div>
    )}
  </Show>
);
